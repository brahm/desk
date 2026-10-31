// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Comparing what Desk renders with what Pages exported.
//!
//! Both PDFs are read back with the Poppler tools. The layout is compared
//! word by word: the same words must fall on the same pages, each within a
//! tolerance of where Pages put it. Then both are rasterized, and the
//! share of pixels that differ is measured and drawn in a diff image: in
//! red what only Pages drew, in blue what only Desk drew, and in grey what
//! both drew.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use desk_render::cairo::{Format, ImageSurface};
use serde::Serialize;

/// The resolution of the rasterized pages, in dots per inch.
pub const RASTER_DPI: u32 = 72;

/// How different two grey levels must be, out of 255, to count as
/// different pixels, so that anti-aliasing alone does not count.
const PIXEL_THRESHOLD: u8 = 96;

/// Why a comparison could not be made.
#[derive(Debug)]
pub enum CompareError {
    /// A Poppler tool could not run, or failed.
    Tool(String),
    /// A file could not be read or written.
    Io(PathBuf, std::io::Error),
    /// A rasterized page could not be read.
    Image(String),
}

impl fmt::Display for CompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tool(message) => f.write_str(message),
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
            Self::Image(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CompareError {}

/// A word on a page of a PDF, in points from the top left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    /// The text.
    pub text: String,
    /// The left edge.
    pub x: f64,
    /// The bottom edge, near the baseline.
    pub y: f64,
}

/// The words of each page of a PDF.
pub fn words(pdf: &Path) -> Result<Vec<Vec<Word>>, CompareError> {
    let output = run(Command::new("pdftotext").arg("-bbox").arg(pdf).arg("-"))?;
    Ok(parse_bbox(&output))
}

/// Reads the output of `pdftotext -bbox`.
fn parse_bbox(html: &str) -> Vec<Vec<Word>> {
    let mut pages = Vec::new();
    for line in html.lines() {
        let line = line.trim();
        if line.starts_with("<page ") {
            pages.push(Vec::new());
        } else if let Some(rest) = line.strip_prefix("<word ") {
            let attr = |name: &str| {
                let start = rest.find(&format!("{name}=\""))? + name.len() + 2;
                let end = rest[start..].find('"')? + start;
                rest[start..end].parse::<f64>().ok()
            };
            let text = rest
                .split_once('>')
                .and_then(|(_, t)| t.strip_suffix("</word>"))
                .map(unescape)
                .unwrap_or_default();
            if let (Some(page), Some(x), Some(y)) = (pages.last_mut(), attr("xMin"), attr("yMax")) {
                page.push(Word { text, x, y });
            }
        }
    }
    pages
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// How the words of Desk compare with those of Pages.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LayoutDiff {
    /// The pages of the reference.
    pub reference_pages: usize,
    /// The pages Desk made.
    pub desk_pages: usize,
    /// The first word that is not on the same page, or not the same, as
    /// its page and index on the reference, and both words.
    pub first_mismatch: Option<Mismatch>,
    /// The largest distance between a word and its place on the
    /// reference, in points, over the words that match.
    pub max_offset: f64,
    /// The word with that distance, as its page and index.
    pub max_offset_at: Option<(usize, usize)>,
}

/// Two words that differ.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Mismatch {
    /// The page, from 1.
    pub page: usize,
    /// The index of the word on the page, from 0.
    pub index: usize,
    /// The word of the reference, if there is one there.
    pub reference: Option<String>,
    /// The word of Desk, if there is one there.
    pub desk: Option<String>,
}

impl LayoutDiff {
    /// Whether the words are the same, on the same pages, within
    /// `tolerance` points of their place.
    pub fn passes(&self, tolerance: f64) -> bool {
        self.reference_pages == self.desk_pages
            && self.first_mismatch.is_none()
            && self.max_offset <= tolerance
    }
}

/// Compares the words of two PDFs, page by page.
pub fn compare_words(reference: &[Vec<Word>], desk: &[Vec<Word>]) -> LayoutDiff {
    let mut diff = LayoutDiff {
        reference_pages: reference.len(),
        desk_pages: desk.len(),
        first_mismatch: None,
        max_offset: 0.0,
        max_offset_at: None,
    };
    let empty = Vec::new();
    for page in 0..reference.len().max(desk.len()) {
        let r = reference.get(page).unwrap_or(&empty);
        let d = desk.get(page).unwrap_or(&empty);
        for index in 0..r.len().max(d.len()) {
            match (r.get(index), d.get(index)) {
                (Some(a), Some(b)) if a.text == b.text => {
                    let offset = (a.x - b.x).abs().max((a.y - b.y).abs());
                    if offset > diff.max_offset {
                        diff.max_offset = offset;
                        diff.max_offset_at = Some((page + 1, index));
                    }
                }
                (a, b) => {
                    if diff.first_mismatch.is_none() {
                        diff.first_mismatch = Some(Mismatch {
                            page: page + 1,
                            index,
                            reference: a.map(|w| w.text.clone()),
                            desk: b.map(|w| w.text.clone()),
                        });
                    }
                    break;
                }
            }
        }
    }
    diff
}

/// How the pixels of two PDFs compare.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PixelDiff {
    /// For each page both have, the share of its pixels that differ, in
    /// percent.
    pub pages: Vec<f64>,
    /// The diff images, one per page both have.
    pub images: Vec<PathBuf>,
}

impl PixelDiff {
    /// The largest share of differing pixels of a page, in percent.
    pub fn worst(&self) -> f64 {
        self.pages.iter().copied().fold(0.0, f64::max)
    }
}

/// Rasterizes both PDFs into `out` and compares their pixels, writing a
/// diff image of each page into `out`.
pub fn compare_pixels(
    reference: &Path,
    desk: &Path,
    out: &Path,
) -> Result<PixelDiff, CompareError> {
    std::fs::create_dir_all(out).map_err(|e| CompareError::Io(out.into(), e))?;
    let reference_pages = rasterize(reference, &out.join("reference"))?;
    let desk_pages = rasterize(desk, &out.join("desk"))?;
    let mut diff = PixelDiff {
        pages: Vec::new(),
        images: Vec::new(),
    };
    for (i, (a, b)) in reference_pages.iter().zip(&desk_pages).enumerate() {
        let image = out.join(format!("diff-{}.png", i + 1));
        diff.pages.push(diff_images(a, b, &image)?);
        diff.images.push(image);
    }
    Ok(diff)
}

/// Rasterizes a PDF in grey, as `<prefix>-<page>.png`, and gives the
/// files in page order.
fn rasterize(pdf: &Path, prefix: &Path) -> Result<Vec<PathBuf>, CompareError> {
    run(Command::new("pdftoppm")
        .args(["-gray", "-png", "-r", &RASTER_DPI.to_string()])
        .arg(pdf)
        .arg(prefix))?;
    let dir = prefix.parent().unwrap_or(Path::new("."));
    let stem = prefix
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut pages: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)
        .map_err(|e| CompareError::Io(dir.into(), e))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let number = name
                .strip_prefix(&format!("{stem}-"))?
                .strip_suffix(".png")?
                .parse()
                .ok()?;
            Some((number, entry.path()))
        })
        .collect();
    pages.sort();
    Ok(pages.into_iter().map(|(_, path)| path).collect())
}

fn load(path: &Path) -> Result<ImageSurface, CompareError> {
    let mut file = std::fs::File::open(path).map_err(|e| CompareError::Io(path.into(), e))?;
    ImageSurface::create_from_png(&mut file)
        .map_err(|e| CompareError::Image(format!("{}: {e}", path.display())))
}

/// The grey level of a pixel of an image that Cairo read.
fn grey(data: &[u8], stride: usize, format: Format, x: usize, y: usize) -> u8 {
    match format {
        Format::A8 => 255 - data[y * stride + x],
        _ => {
            // Native-endian ARGB32 or RGB24: average the three colors.
            let p = &data[y * stride + x * 4..y * stride + x * 4 + 4];
            let (r, g, b) = if cfg!(target_endian = "little") {
                (p[2], p[1], p[0])
            } else {
                (p[1], p[2], p[3])
            };
            ((u16::from(r) + u16::from(g) + u16::from(b)) / 3) as u8
        }
    }
}

/// Writes the diff image of two pages and gives the share of pixels that
/// differ, in percent. Pages of different sizes are compared over the
/// part they share.
fn diff_images(reference: &Path, desk: &Path, out: &Path) -> Result<f64, CompareError> {
    let mut a = load(reference)?;
    let mut b = load(desk)?;
    let width = a.width().min(b.width()).max(0) as usize;
    let height = a.height().min(b.height()).max(0) as usize;
    let (format_a, stride_a) = (a.format(), a.stride() as usize);
    let (format_b, stride_b) = (b.format(), b.stride() as usize);
    let mut image = ImageSurface::create(Format::Rgb24, width as i32, height as i32)
        .map_err(|e| CompareError::Image(e.to_string()))?;
    let stride = image.stride() as usize;
    let mut differing = 0usize;
    {
        let data_a = a.data().map_err(|e| CompareError::Image(e.to_string()))?;
        let data_b = b.data().map_err(|e| CompareError::Image(e.to_string()))?;
        let mut data = image
            .data()
            .map_err(|e| CompareError::Image(e.to_string()))?;
        for y in 0..height {
            for x in 0..width {
                let ga = grey(&data_a, stride_a, format_a, x, y);
                let gb = grey(&data_b, stride_b, format_b, x, y);
                let (r, g, bl) = if ga.abs_diff(gb) > PIXEL_THRESHOLD {
                    differing += 1;
                    if ga < gb {
                        (220, 40, 40)
                    } else {
                        (40, 80, 220)
                    }
                } else {
                    // Both drew it, or neither: a light grey copy.
                    let v = 255 - (255 - ga.min(gb)) / 3;
                    (v, v, v)
                };
                let p = &mut data[y * stride + x * 4..y * stride + x * 4 + 4];
                let pixel = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(bl);
                p.copy_from_slice(&pixel.to_ne_bytes());
            }
        }
    }
    let mut file = std::fs::File::create(out).map_err(|e| CompareError::Io(out.into(), e))?;
    image
        .write_to_png(&mut file)
        .map_err(|e| CompareError::Image(format!("{}: {e}", out.display())))?;
    let total = (width * height).max(1);
    Ok(differing as f64 * 100.0 / total as f64)
}

fn run(command: &mut Command) -> Result<String, CompareError> {
    let name = command.get_program().to_string_lossy().into_owned();
    let output = command.output().map_err(|e| {
        CompareError::Tool(format!(
            "{name} could not run ({e}); it comes with the Poppler tools"
        ))
    })?;
    if !output.status.success() {
        return Err(CompareError::Tool(format!(
            "{name} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, x: f64, y: f64) -> Word {
        Word {
            text: text.into(),
            x,
            y,
        }
    }

    #[test]
    fn reads_the_words_of_pdftotext() {
        let html = r#"<doc>
  <page width="595.276000" height="841.890000">
    <word xMin="72.000000" yMin="60.0" xMax="100.5" yMax="75.250000">Ol&amp;á</word>
    <word xMin="104.000000" yMin="60.0" xMax="130.0" yMax="75.250000">mundo</word>
  </page>
  <page width="595.276000" height="841.890000">
  </page>
</doc>"#;
        assert_eq!(
            parse_bbox(html),
            [
                vec![word("Ol&á", 72.0, 75.25), word("mundo", 104.0, 75.25)],
                vec![]
            ]
        );
    }

    #[test]
    fn words_compare_by_page_text_and_place() {
        let reference = vec![vec![word("a", 72.0, 80.0), word("b", 90.0, 80.0)]];
        let same = compare_words(&reference, &reference);
        assert!(same.passes(0.0));

        let moved = vec![vec![word("a", 72.0, 80.0), word("b", 91.0, 80.5)]];
        let diff = compare_words(&reference, &moved);
        assert_eq!(diff.max_offset, 1.0);
        assert_eq!(diff.max_offset_at, Some((1, 1)));
        assert!(diff.passes(1.5));
        assert!(!diff.passes(0.5));

        // "b" went to the next page.
        let broken = vec![vec![word("a", 72.0, 80.0)], vec![word("b", 72.0, 80.0)]];
        let diff = compare_words(&reference, &broken);
        assert_eq!(diff.desk_pages, 2);
        assert_eq!(
            diff.first_mismatch,
            Some(Mismatch {
                page: 1,
                index: 1,
                reference: Some("b".into()),
                desk: None,
            })
        );
        assert!(!diff.passes(100.0));
    }
}
