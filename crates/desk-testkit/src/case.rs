// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The cases of the corpus.
//!
//! A case is a folder with a `case.toml` that describes a short document,
//! and, once the reference virtual machine has made them, the `.pages`
//! file that Pages built from the same description and the PDF it
//! exported, `reference.pdf`. The description is the source of both: Desk
//! builds its document from it, and the script of the virtual machine
//! builds the Pages document from it.
//!
//! ```toml
//! title = "Espaço entre parágrafos"
//! rows = ["FMT-15"]
//! description = "O maior entre o espaço depois e o espaço antes vale."
//!
//! [page]
//! size = "a4"
//!
//! [defaults]
//! font = "Liberation Serif"
//! size = 12
//!
//! [[paragraphs]]
//! text = "Primeiro parágrafo."
//! space_after = 18
//!
//! [[paragraphs]]
//! filler = 120
//! space_before = 6
//! align = "justify"
//!
//! [[paragraphs]]
//! runs = [{ text = "Texto em " }, { text = "negrito", bold = true }]
//! ```
//!
//! Lengths are in points. A paragraph has `text`, `runs` or `filler`, a
//! number of words of fixed filler text, and `repeat` makes copies of it.

use std::fmt;
use std::path::{Path, PathBuf};

use desk_model::{
    Alignment, Baseline, Capitalization, CharAttrs, Document, LINE_BREAK, Length, LineSpacing,
    LineStyle, Metadata, ModelError, Op, PageSetup, ParaAttrs, Paragraph, ParagraphStyle, Rgba,
    Run,
};
use serde::Deserialize;

/// The name of the description file of a case.
pub const CASE_FILE: &str = "case.toml";

/// The name of the PDF that Pages exported for a case.
pub const REFERENCE_PDF: &str = "reference.pdf";

/// The document that Pages saved for a case, a package folder.
pub const PAGES_DOCUMENT: &str = "case.pages";

/// Why a case could not be loaded.
#[derive(Debug)]
pub enum CaseError {
    /// The file could not be read.
    Io(PathBuf, std::io::Error),
    /// The description is not valid.
    Toml(PathBuf, Box<toml::de::Error>),
    /// The description is valid TOML but not a valid case.
    Invalid(PathBuf, String),
    /// The document could not be built.
    Model(ModelError),
}

impl fmt::Display for CaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(path, error) => write!(f, "{}: {error}", path.display()),
            Self::Toml(path, error) => write!(f, "{}: {error}", path.display()),
            Self::Invalid(path, message) => write!(f, "{}: {message}", path.display()),
            Self::Model(error) => write!(f, "the document could not be built: {error:?}"),
        }
    }
}

impl std::error::Error for CaseError {}

impl From<ModelError> for CaseError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

/// A case of the corpus.
#[derive(Clone, Debug)]
pub struct Case {
    /// The name of its folder, which names the case.
    pub name: String,
    /// Its folder.
    pub dir: PathBuf,
    /// Its description.
    pub spec: CaseSpec,
}

impl Case {
    /// Loads the case in a folder.
    pub fn load(dir: impl AsRef<Path>) -> Result<Self, CaseError> {
        let dir = dir.as_ref().to_path_buf();
        let path = dir.join(CASE_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| CaseError::Io(path.clone(), e))?;
        let spec = CaseSpec::parse(&text).map_err(|e| match e {
            SpecError::Toml(e) => CaseError::Toml(path.clone(), e),
            SpecError::Invalid(m) => CaseError::Invalid(path.clone(), m),
        })?;
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Self { name, dir, spec })
    }

    /// Loads every case in the folders under `root`, sorted by name.
    pub fn load_all(root: impl AsRef<Path>) -> Result<Vec<Self>, CaseError> {
        let root = root.as_ref();
        let entries = std::fs::read_dir(root).map_err(|e| CaseError::Io(root.into(), e))?;
        let mut dirs = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| CaseError::Io(root.into(), e))?;
            if entry.path().join(CASE_FILE).is_file() {
                dirs.push(entry.path());
            }
        }
        dirs.sort();
        dirs.into_iter().map(Self::load).collect()
    }

    /// The PDF that Pages exported, if the virtual machine has made it.
    pub fn reference(&self) -> Option<PathBuf> {
        Some(self.dir.join(REFERENCE_PDF)).filter(|p| p.is_file())
    }

    /// The document that Pages saved, if the virtual machine has made it.
    pub fn pages_document(&self) -> Option<PathBuf> {
        Some(self.dir.join(PAGES_DOCUMENT)).filter(|p| p.exists())
    }
}

/// Why a description is not a valid case.
#[derive(Debug)]
pub enum SpecError {
    /// It is not valid TOML, or has fields of the wrong type.
    Toml(Box<toml::de::Error>),
    /// It breaks a rule that the types cannot express.
    Invalid(String),
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(error) => error.fmt(f),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for SpecError {}

/// The description of a case, as `case.toml` holds it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseSpec {
    /// What the case shows, in a few words.
    pub title: String,
    /// The rows of the parity matrix that the case checks.
    pub rows: Vec<String>,
    /// What to look at in the result.
    #[serde(default)]
    pub description: String,
    /// How far, in points, a word may be from where Pages put it. The
    /// default is [`DEFAULT_POSITION_TOLERANCE`].
    #[serde(default)]
    pub position_tolerance: Option<f64>,
    /// The share of the pixels of a page that may differ from the
    /// reference when both are rasterized, in percent. Without it the
    /// share is reported but does not fail the case.
    #[serde(default)]
    pub pixel_tolerance: Option<f64>,
    /// The paper and margins.
    #[serde(default)]
    pub page: PageSpec,
    /// The format of the default paragraph style. It takes the format
    /// fields of a paragraph, and no text.
    #[serde(default)]
    pub defaults: ParagraphSpec,
    /// The paragraphs of the body.
    pub paragraphs: Vec<ParagraphSpec>,
}

/// How far a word may be from where Pages put it by default, in points.
pub const DEFAULT_POSITION_TOLERANCE: f64 = 1.5;

/// The paper and margins of a case. Pages '09 starts the blank template
/// with margins of one inch.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageSpec {
    /// `a4`, the default, or `letter`.
    pub size: Option<PaperSize>,
    /// Turns the paper.
    #[serde(default)]
    pub landscape: bool,
    /// The top margin.
    pub margin_top: Option<f64>,
    /// The bottom margin.
    pub margin_bottom: Option<f64>,
    /// The left margin.
    pub margin_left: Option<f64>,
    /// The right margin.
    pub margin_right: Option<f64>,
}

/// A paper size.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PaperSize {
    /// ISO A4.
    A4,
    /// US Letter.
    Letter,
}

/// One paragraph of a case, with its format and that of all its text.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParagraphSpec {
    /// The text, with `\n` for a line break.
    pub text: Option<String>,
    /// Runs of text with their own format.
    pub runs: Option<Vec<RunSpec>>,
    /// A number of words of filler text.
    pub filler: Option<usize>,
    /// How many copies of the paragraph, 1 by default.
    pub repeat: Option<usize>,
    /// `left`, `center`, `right` or `justify`.
    pub align: Option<AlignSpec>,
    /// The spacing of the lines.
    pub line_spacing: Option<LineSpacingSpec>,
    /// The space above.
    pub space_before: Option<f64>,
    /// The space below.
    pub space_after: Option<f64>,
    /// The indent of the first line, from the left indent.
    pub first_line_indent: Option<f64>,
    /// The left indent.
    pub left_indent: Option<f64>,
    /// The right indent.
    pub right_indent: Option<f64>,
    /// Starts a new page.
    pub page_break_before: Option<bool>,
    /// Stays on the page of the next paragraph.
    pub keep_with_next: Option<bool>,
    /// Keeps its lines on one page.
    pub keep_lines_together: Option<bool>,
    /// Leaves no line alone at the top or bottom of a page.
    pub widow_control: Option<bool>,
    /// The font family.
    pub font: Option<String>,
    /// The size, in points.
    pub size: Option<f64>,
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// `single` or `double`.
    pub underline: Option<LineSpec>,
    /// `single` or `double`.
    pub strikethrough: Option<LineSpec>,
    /// The color, as `#rrggbb`.
    pub color: Option<String>,
    /// `superscript` or `subscript`.
    pub baseline: Option<BaselineSpec>,
    /// `all_caps`, `small_caps` or `title`.
    pub capitalization: Option<CapitalizationSpec>,
}

/// A run of text with its own format, over that of the paragraph.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSpec {
    /// The text, with `\n` for a line break.
    pub text: String,
    /// The font family.
    pub font: Option<String>,
    /// The size, in points.
    pub size: Option<f64>,
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// `single` or `double`.
    pub underline: Option<LineSpec>,
    /// `single` or `double`.
    pub strikethrough: Option<LineSpec>,
    /// The color, as `#rrggbb`.
    pub color: Option<String>,
    /// `superscript` or `subscript`.
    pub baseline: Option<BaselineSpec>,
    /// `all_caps`, `small_caps` or `title`.
    pub capitalization: Option<CapitalizationSpec>,
}

/// The character format of a paragraph or a run.
#[derive(Clone, Copy, Debug)]
pub struct CharSpec<'a> {
    font: Option<&'a str>,
    size: Option<f64>,
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<LineSpec>,
    strikethrough: Option<LineSpec>,
    color: Option<&'a str>,
    baseline: Option<BaselineSpec>,
    capitalization: Option<CapitalizationSpec>,
}

macro_rules! char_spec {
    ($ty:ty) => {
        impl $ty {
            /// Its character format.
            pub fn character(&self) -> CharSpec<'_> {
                CharSpec {
                    font: self.font.as_deref(),
                    size: self.size,
                    bold: self.bold,
                    italic: self.italic,
                    underline: self.underline,
                    strikethrough: self.strikethrough,
                    color: self.color.as_deref(),
                    baseline: self.baseline,
                    capitalization: self.capitalization,
                }
            }
        }
    };
}

char_spec!(ParagraphSpec);
char_spec!(RunSpec);

/// An alignment.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlignSpec {
    Left,
    Center,
    Right,
    Justify,
}

/// A line spacing, as one of its fields: `{ multiple = 150 }` in percent,
/// or `{ at_least = 14 }`, `{ exactly = 14 }` or `{ between = 4 }` in
/// points.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LineSpacingSpec {
    Multiple(u32),
    AtLeast(f64),
    Exactly(f64),
    Between(f64),
}

/// A line under or through text.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineSpec {
    Single,
    Double,
}

/// A raised or lowered baseline.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSpec {
    Superscript,
    Subscript,
}

/// A capitalization.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapitalizationSpec {
    AllCaps,
    SmallCaps,
    Title,
}

/// The distance between default tab stops, in points, of Pages on the
/// reference machine: 1.25 cm, as Pages saves it.
pub const PAGES_TAB_INTERVAL: f64 = 35.43307;

impl CaseSpec {
    /// Reads a description.
    pub fn parse(text: &str) -> Result<Self, SpecError> {
        let spec: Self = toml::from_str(text).map_err(|e| SpecError::Toml(Box::new(e)))?;
        spec.check().map_err(SpecError::Invalid)?;
        Ok(spec)
    }

    fn check(&self) -> Result<(), String> {
        if self.rows.is_empty() {
            return Err("a case checks at least one row of the matrix".into());
        }
        let d = &self.defaults;
        if d.text.is_some() || d.runs.is_some() || d.filler.is_some() || d.repeat.is_some() {
            return Err("the defaults take format, not text".into());
        }
        for (i, paragraph) in self.paragraphs.iter().enumerate() {
            let sources = [
                paragraph.text.is_some(),
                paragraph.runs.is_some(),
                paragraph.filler.is_some(),
            ];
            if sources.iter().filter(|&&s| s).count() != 1 {
                return Err(format!(
                    "paragraph {} needs exactly one of text, runs and filler",
                    i + 1
                ));
            }
        }
        for color in self.colors() {
            parse_color(color)?;
        }
        Ok(())
    }

    fn colors(&self) -> impl Iterator<Item = &str> {
        let paragraphs = std::iter::once(&self.defaults).chain(&self.paragraphs);
        let runs = self.paragraphs.iter().flat_map(|p| p.runs.iter().flatten());
        paragraphs
            .filter_map(|p| p.color.as_deref())
            .chain(runs.filter_map(|r| r.color.as_deref()))
    }

    /// The page setup.
    pub fn page_setup(&self) -> PageSetup {
        let mut setup = match self.page.size.unwrap_or(PaperSize::A4) {
            PaperSize::A4 => PageSetup::a4(),
            PaperSize::Letter => PageSetup::letter(),
        };
        if self.page.landscape {
            setup = setup.rotated();
        }
        let set = |field: &mut Length, value: Option<f64>| {
            if let Some(value) = value {
                *field = Length::from_pt(value);
            }
        };
        set(&mut setup.margin_top, self.page.margin_top);
        set(&mut setup.margin_bottom, self.page.margin_bottom);
        set(&mut setup.margin_left, self.page.margin_left);
        set(&mut setup.margin_right, self.page.margin_right);
        setup
    }

    /// The paragraphs as they are laid out, each with its runs of text,
    /// with copies and filler expanded.
    pub fn expanded(&self) -> Vec<(&ParagraphSpec, Vec<RunSpec>)> {
        let mut out = Vec::new();
        let mut filler_start = 0;
        for paragraph in &self.paragraphs {
            for _ in 0..paragraph.repeat.unwrap_or(1) {
                let runs = if let Some(text) = &paragraph.text {
                    vec![RunSpec {
                        text: text.clone(),
                        ..RunSpec::default()
                    }]
                } else if let Some(runs) = &paragraph.runs {
                    runs.clone()
                } else {
                    let words = paragraph.filler.unwrap_or(0);
                    let text = filler(filler_start, words);
                    filler_start += words;
                    vec![RunSpec {
                        text,
                        ..RunSpec::default()
                    }]
                };
                out.push((paragraph, runs));
            }
        }
        out
    }

    /// Builds the document that the case describes.
    pub fn document(&self) -> Result<Document, CaseError> {
        let mut document = Document::new_word_processing();
        let default = document.styles().default_paragraph();
        let mut style = ParagraphStyle::new(default, "Body");
        style.paragraph = para_attrs(&self.defaults);
        // Pages on the reference machine, in Portuguese, puts the default
        // tab stops 1.25 cm apart.
        style.paragraph.default_tab_interval = Some(Length::from_pt(PAGES_TAB_INTERVAL));
        style.character = char_attrs(self.defaults.character());
        document.put_style(style)?;

        let first = document
            .body()
            .get(0)
            .expect("a new document has a paragraph")
            .id();
        let paragraphs = self.expanded();
        for (index, (format, runs)) in paragraphs.iter().enumerate() {
            let base = char_attrs(format.character());
            let runs = runs.iter().map(|run| {
                let mut attrs = base.clone();
                attrs.merge(&char_attrs(run.character()));
                let text = run.text.replace('\n', &LINE_BREAK.to_string());
                Run::with_attrs(&text, None, attrs)
            });
            let id = document.new_id();
            let paragraph = Paragraph::from_runs(id, default, para_attrs(format), runs)?;
            document.apply(Op::InsertParagraph {
                index: index + 1,
                paragraph,
            })?;
        }
        if !paragraphs.is_empty() {
            document.apply(Op::RemoveParagraph { paragraph: first })?;
        }
        document.apply(Op::SetPageSetup {
            page: self.page_setup(),
        })?;
        document.apply(Op::SetMetadata {
            metadata: Metadata {
                title: self.title.clone(),
                ..Metadata::default()
            },
        })?;
        Ok(document)
    }
}

fn para_attrs(f: &ParagraphSpec) -> ParaAttrs {
    let pt = |v: Option<f64>| v.map(Length::from_pt);
    ParaAttrs {
        alignment: f.align.map(|a| match a {
            AlignSpec::Left => Alignment::Left,
            AlignSpec::Center => Alignment::Center,
            AlignSpec::Right => Alignment::Right,
            AlignSpec::Justify => Alignment::Justify,
        }),
        line_spacing: f.line_spacing.map(|s| match s {
            LineSpacingSpec::Multiple(percent) => LineSpacing::Multiple { percent },
            LineSpacingSpec::AtLeast(v) => LineSpacing::AtLeast(Length::from_pt(v)),
            LineSpacingSpec::Exactly(v) => LineSpacing::Exactly(Length::from_pt(v)),
            LineSpacingSpec::Between(v) => LineSpacing::Between(Length::from_pt(v)),
        }),
        space_before: pt(f.space_before),
        space_after: pt(f.space_after),
        first_line_indent: pt(f.first_line_indent),
        left_indent: pt(f.left_indent),
        right_indent: pt(f.right_indent),
        page_break_before: f.page_break_before,
        keep_with_next: f.keep_with_next,
        keep_lines_together: f.keep_lines_together,
        widow_control: f.widow_control,
        ..ParaAttrs::default()
    }
}

fn char_attrs(c: CharSpec<'_>) -> CharAttrs {
    let line = |l: Option<LineSpec>| {
        l.map(|l| match l {
            LineSpec::Single => LineStyle::Single,
            LineSpec::Double => LineStyle::Double,
        })
    };
    CharAttrs {
        font_family: c.font.map(Into::into),
        font_size: c.size.map(Length::from_pt),
        bold: c.bold,
        italic: c.italic,
        underline: line(c.underline),
        strikethrough: line(c.strikethrough),
        // Checked when the case was read.
        color: c.color.and_then(|c| parse_color(c).ok()),
        baseline: c.baseline.map(|b| match b {
            BaselineSpec::Superscript => Baseline::Superscript,
            BaselineSpec::Subscript => Baseline::Subscript,
        }),
        capitalization: c.capitalization.map(|c| match c {
            CapitalizationSpec::AllCaps => Capitalization::AllCaps,
            CapitalizationSpec::SmallCaps => Capitalization::SmallCaps,
            CapitalizationSpec::Title => Capitalization::Title,
        }),
        ..CharAttrs::default()
    }
}

fn parse_color(color: &str) -> Result<Rgba, String> {
    let hex = color
        .strip_prefix('#')
        .filter(|h| h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| format!("{color:?} is not a color like #1a2b3c"))?;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).expect("checked as hex");
    Ok(Rgba::rgb(byte(0), byte(2), byte(4)))
}

/// Filler words, the same on every run so that Desk and Pages lay out the
/// same text. The words vary in length so that lines break in different
/// places, and every tenth ends a sentence.
const FILLER: &[&str] = &[
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "ut",
    "labore",
    "et",
    "dolore",
    "magna",
    "aliqua",
    "enim",
    "ad",
    "minim",
    "veniam",
    "quis",
    "nostrud",
    "exercitation",
    "ullamco",
    "laboris",
    "nisi",
    "aliquip",
    "ex",
    "ea",
    "commodo",
    "consequat",
    "duis",
    "aute",
    "irure",
    "in",
    "reprehenderit",
    "voluptate",
    "velit",
    "esse",
    "cillum",
    "fugiat",
    "nulla",
    "pariatur",
    "excepteur",
    "sint",
    "occaecat",
    "cupidatat",
    "non",
    "proident",
    "sunt",
    "culpa",
    "qui",
    "officia",
    "deserunt",
    "mollit",
    "anim",
    "id",
    "est",
    "laborum",
];

/// `count` words of filler text, from word `start` of the endless text.
pub fn filler(start: usize, count: usize) -> String {
    let mut out = String::new();
    for i in start..start + count {
        let word = FILLER[(i * 7 + i / FILLER.len()) % FILLER.len()];
        let first = i == start || i % 10 == 0;
        if !out.is_empty() {
            out.push(' ');
        }
        if first {
            let mut chars = word.chars();
            if let Some(c) = chars.next() {
                out.extend(c.to_uppercase());
                out.push_str(chars.as_str());
            }
        } else {
            out.push_str(word);
        }
        if i % 10 == 9 || i + 1 == start + count {
            out.push('.');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "title = \"t\"\nrows = [\"FMT-15\"]\n";

    #[test]
    fn a_description_builds_its_document() {
        let spec = CaseSpec::parse(&format!(
            "{MINIMAL}[page]\nsize = \"letter\"\nmargin_left = 36\n\
             [defaults]\nfont = \"Liberation Sans\"\nspace_after = 6\n\
             [[paragraphs]]\ntext = \"a\\nb\"\nalign = \"center\"\nrepeat = 2\n\
             [[paragraphs]]\nruns = [{{ text = \"x\" }}, {{ text = \"y\", bold = true, color = \"#ff0000\" }}]\n\
             line_spacing = {{ at_least = 14 }}\n"
        ))
        .unwrap();
        let document = spec.document().unwrap();
        assert_eq!(document.body().len(), 3);
        assert_eq!(document.page_setup().margin_left, Length::from_points(36));
        assert_eq!(document.page_setup().width, PageSetup::letter().width);
        let first = document.body().get(0).unwrap();
        assert_eq!(first.text(), format!("a{LINE_BREAK}b"));
        assert_eq!(first.attrs().alignment, Some(Alignment::Center));
        let last = document.body().get(2).unwrap();
        assert_eq!(
            last.attrs().line_spacing,
            Some(LineSpacing::AtLeast(Length::from_points(14)))
        );
        let bold = &last.runs().nth(1).unwrap().attrs;
        assert_eq!(bold.bold, Some(true));
        assert_eq!(bold.color, Some(Rgba::rgb(255, 0, 0)));
        let styles = document.styles();
        let body = styles.get(styles.default_paragraph()).unwrap();
        assert_eq!(document.metadata().title, "t");
        assert_eq!(body.name(), "Body");
    }

    #[test]
    fn bad_descriptions_are_refused() {
        for bad in [
            "title = \"t\"\nrows = []\nparagraphs = []\n".to_owned(),
            format!("{MINIMAL}[[paragraphs]]\ntext = \"a\"\nfiller = 3\n"),
            format!("{MINIMAL}[[paragraphs]]\nalign = \"left\"\n"),
            format!("{MINIMAL}[[paragraphs]]\ntext = \"a\"\ncolor = \"red\"\n"),
            format!("{MINIMAL}[[paragraphs]]\ntext = \"a\"\nbolf = true\n"),
            format!("{MINIMAL}[defaults]\ntext = \"a\"\n[[paragraphs]]\ntext = \"a\"\n"),
        ] {
            let bad = if bad.contains("paragraphs") {
                bad
            } else {
                format!("{bad}paragraphs = []\n")
            };
            assert!(CaseSpec::parse(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn filler_is_fixed_and_continues() {
        assert_eq!(filler(0, 3), filler(0, 3));
        assert_eq!(filler(0, 0), "");
        let ten = filler(0, 10);
        assert_eq!(ten.split(' ').count(), 10);
        assert!(ten.ends_with('.'));
        assert!(ten.starts_with(char::is_uppercase));
        assert_ne!(filler(0, 10), filler(10, 10));
    }
}
