// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paragraph layout, named by the IDs of the parity matrix. The tests use
//! Liberation Sans, which CI installs, so the widths are the same
//! everywhere.

use std::sync::Arc;

use desk_model::{
    Alignment, Baseline, Capitalization, CharFormat, LINE_BREAK, Length, LineColor, LineSpacing,
    LineStyle, NO_BREAK_SPACE, ParaFormat, Rgba,
};
use desk_text::{
    Column, LineSpace, ParagraphLayout, ParagraphText, SCRIPT_SCALE, SOFT_HYPHEN, TextSystem,
};

const TEXT: &str = "Desk lays out text the way Pages does, one line at a time, \
                    and decides itself where every line of the paragraph ends.";

fn sans() -> CharFormat {
    CharFormat {
        font_family: Arc::from("Liberation Sans"),
        ..CharFormat::default()
    }
}

fn lay(
    system: &mut TextSystem,
    paragraph: ParaFormat,
    runs: &[(&str, CharFormat)],
) -> ParagraphLayout {
    lay_in(system, paragraph, runs, &Column::new(200.0))
}

fn lay_in(
    system: &mut TextSystem,
    paragraph: ParaFormat,
    runs: &[(&str, CharFormat)],
    space: &dyn LineSpace,
) -> ParagraphLayout {
    let text =
        ParagraphText::from_runs(paragraph, sans(), runs.iter().map(|(t, f)| (*t, f.clone())));
    system.layout(&text, space, 0.0)
}

fn plain(system: &mut TextSystem, paragraph: ParaFormat, text: &str) -> ParagraphLayout {
    lay(system, paragraph, &[(text, sans())])
}

fn line_texts<'a>(layout: &ParagraphLayout, text: &'a str) -> Vec<&'a str> {
    layout
        .lines
        .iter()
        .map(|l| &text[l.range.clone()])
        .collect()
}

#[test]
fn lines_cover_the_text_and_fit_the_column() {
    let mut system = TextSystem::new();
    let layout = plain(&mut system, ParaFormat::default(), TEXT);
    assert!(layout.lines.len() > 2);
    assert_eq!(layout.lines.first().unwrap().range.start, 0);
    assert_eq!(layout.lines.last().unwrap().range.end, TEXT.len());
    for pair in layout.lines.windows(2) {
        assert_eq!(pair[0].range.end, pair[1].range.start);
        assert!(pair[1].top >= pair[0].top + pair[0].height - 0.01);
    }
    for line in &layout.lines {
        assert!(line.advance <= 200.0 + 0.01, "{} > 200", line.advance);
    }
    assert!((layout.bottom - layout.lines.len() as f32 * layout.lines[0].height).abs() < 0.1);
}

#[test]
fn empty_paragraph_has_one_line() {
    let mut system = TextSystem::new();
    let layout = lay(&mut system, ParaFormat::default(), &[]);
    assert_eq!(layout.lines.len(), 1);
    assert_eq!(layout.lines[0].range, 0..0);
    assert!(layout.bottom > 10.0);
}

#[test]
fn fmt_01_bold_and_italic_pick_their_faces() {
    let mut system = TextSystem::new();
    let bold = CharFormat {
        bold: true,
        ..sans()
    };
    let italic = CharFormat {
        italic: true,
        ..sans()
    };
    let layout = lay(
        &mut system,
        ParaFormat::default(),
        &[("plain ", sans()), ("bold ", bold), ("italic", italic)],
    );
    let runs = &layout.lines[0].runs;
    assert_eq!(runs.len(), 3);
    let fonts: Vec<_> = runs
        .iter()
        .map(|r| (r.font.data.id(), r.font.index))
        .collect();
    assert_ne!(fonts[0], fonts[1]);
    assert_ne!(fonts[0], fonts[2]);
    assert_ne!(fonts[1], fonts[2]);
}

#[test]
fn fmt_02_underline_and_strikethrough_are_decorations() {
    let mut system = TextSystem::new();
    let red = Rgba::rgb(255, 0, 0);
    let marked = CharFormat {
        underline: LineStyle::Single,
        strikethrough: LineStyle::Double,
        strikethrough_color: LineColor::Custom(red),
        ..sans()
    };
    let layout = lay(
        &mut system,
        ParaFormat::default(),
        &[("a ", sans()), ("marked", marked)],
    );
    let line = &layout.lines[0];
    assert_eq!(line.decorations.len(), 2);
    let (under, strike) = (&line.decorations[0], &line.decorations[1]);
    assert_eq!(under.style, LineStyle::Single);
    assert_eq!(under.color, Rgba::BLACK);
    assert_eq!(strike.style, LineStyle::Double);
    assert_eq!(strike.color, red);
    // Underline below the baseline, strikethrough above it.
    assert!(under.y > line.baseline);
    assert!(strike.y < line.baseline);
    let run = &line.runs[1];
    assert!((under.x0 - run.x).abs() < 0.01);
    assert!((under.x1 - (run.x + run.advance)).abs() < 0.01);
}

#[test]
fn fmt_05_font_size_scales_width_and_height() {
    let mut system = TextSystem::new();
    let big = CharFormat {
        font_size: Length::from_points(24),
        ..sans()
    };
    let small = lay(&mut system, ParaFormat::default(), &[("Size", sans())]);
    let large = lay(&mut system, ParaFormat::default(), &[("Size", big)]);
    let ratio = large.lines[0].advance / small.lines[0].advance;
    assert!((ratio - 2.0).abs() < 0.02, "{ratio}");
    assert!((large.lines[0].height / small.lines[0].height - 2.0).abs() < 0.05);
    assert_eq!(large.lines[0].runs[0].size, 24.0);
}

#[test]
fn fmt_06_superscript_is_smaller_and_raised() {
    let mut system = TextSystem::new();
    let sup = CharFormat {
        baseline: Baseline::Superscript,
        ..sans()
    };
    let sub = CharFormat {
        baseline: Baseline::Subscript,
        ..sans()
    };
    let layout = lay(
        &mut system,
        ParaFormat::default(),
        &[("x", sans()), ("2", sup), ("y", sans()), ("i", sub)],
    );
    let runs = &layout.lines[0].runs;
    assert_eq!(runs.len(), 4);
    assert_eq!(runs[1].size, 12.0 * SCRIPT_SCALE);
    assert!(runs[1].baseline < runs[0].baseline - 2.0);
    assert!(runs[3].baseline > runs[2].baseline + 1.0);
}

#[test]
fn fmt_07_capitalization_changes_glyphs_not_offsets() {
    let mut system = TextSystem::new();
    let caps = CharFormat {
        capitalization: Capitalization::AllCaps,
        ..sans()
    };
    let small = CharFormat {
        capitalization: Capitalization::SmallCaps,
        ..sans()
    };
    let lower = lay(&mut system, ParaFormat::default(), &[("straße", sans())]);
    let upper = lay(&mut system, ParaFormat::default(), &[("straße", caps)]);
    let smallcaps = lay(&mut system, ParaFormat::default(), &[("straße", small)]);
    // "STRASSE" has seven letters where "straße" has six.
    assert_eq!(upper.lines[0].runs[0].glyphs.len(), 7);
    assert_eq!(upper.lines[0].range, 0.."straße".len());
    assert!(upper.lines[0].advance > lower.lines[0].advance);
    assert!(smallcaps.lines[0].advance < upper.lines[0].advance);
    assert!(smallcaps.lines[0].advance > lower.lines[0].advance);
}

#[test]
fn fmt_11_color_travels_with_the_run() {
    let mut system = TextSystem::new();
    let blue = CharFormat {
        color: Rgba::rgb(0, 0, 255),
        ..sans()
    };
    let text = ParagraphText::from_runs(
        ParaFormat::default(),
        sans(),
        [("black ", sans()), ("blue", blue)],
    );
    let layout = system.layout(&text, &Column::new(200.0), 0.0);
    let runs = &layout.lines[0].runs;
    assert_eq!(text.formats()[runs[0].format as usize].color, Rgba::BLACK);
    assert_eq!(
        text.formats()[runs[1].format as usize].color,
        Rgba::rgb(0, 0, 255)
    );
}

#[test]
fn fmt_12_alignment() {
    let mut system = TextSystem::new();
    let with = |alignment| ParaFormat {
        alignment,
        ..ParaFormat::default()
    };
    let left = plain(&mut system, with(Alignment::Left), TEXT);
    let center = plain(&mut system, with(Alignment::Center), TEXT);
    let right = plain(&mut system, with(Alignment::Right), TEXT);
    let justify = plain(&mut system, with(Alignment::Justify), TEXT);
    let first_x = |layout: &ParagraphLayout, line: usize| layout.lines[line].runs[0].glyphs[0].x;
    assert_eq!(line_texts(&left, TEXT), line_texts(&justify, TEXT));
    for line in 0..left.lines.len() {
        let slack = 200.0 - left.lines[line].advance;
        assert!(first_x(&left, line).abs() < 0.01);
        assert!((first_x(&center, line) - slack / 2.0).abs() < 0.05);
        assert!((first_x(&right, line) - slack).abs() < 0.05);
        assert!((first_x(&right, line) + right.lines[line].advance - 200.0).abs() < 0.05);
    }
    // Justified lines reach the right edge, except the last one.
    let n = justify.lines.len();
    for line in 0..n - 1 {
        // The run ends after the trailing space, which hangs past the edge.
        let end = justify.lines[line].runs.last().unwrap();
        let end = end.x + end.advance;
        assert!((200.0..204.0).contains(&end), "line {line}: {end}");
    }
    let end = justify.lines[n - 1].runs.last().unwrap();
    assert!(end.x + end.advance < 199.0);
}

#[test]
fn fmt_14_six_line_spacing_modes() {
    let mut system = TextSystem::new();
    let with = |line_spacing| ParaFormat {
        line_spacing,
        ..ParaFormat::default()
    };
    let pitch = |layout: &ParagraphLayout| layout.lines[1].top - layout.lines[0].top;
    let single = plain(
        &mut system,
        with(LineSpacing::Multiple { percent: 100 }),
        TEXT,
    );
    let one_half = plain(
        &mut system,
        with(LineSpacing::Multiple { percent: 150 }),
        TEXT,
    );
    let double = plain(
        &mut system,
        with(LineSpacing::Multiple { percent: 200 }),
        TEXT,
    );
    let base = pitch(&single);
    assert!(base > 12.0 && base < 16.0, "{base}");
    assert!((pitch(&one_half) - 1.5 * base).abs() < 0.05);
    assert!((pitch(&double) - 2.0 * base).abs() < 0.05);

    let exactly = plain(
        &mut system,
        with(LineSpacing::Exactly(Length::from_points(30))),
        TEXT,
    );
    assert!((pitch(&exactly) - 30.0).abs() < 0.01);
    let tight = plain(
        &mut system,
        with(LineSpacing::Exactly(Length::from_points(8))),
        TEXT,
    );
    assert!((pitch(&tight) - 8.0).abs() < 0.01);

    let at_least = plain(
        &mut system,
        with(LineSpacing::AtLeast(Length::from_points(30))),
        TEXT,
    );
    assert!((pitch(&at_least) - 30.0).abs() < 0.01);
    // The extra space goes above the text.
    let line = &at_least.lines[0];
    assert!((line.baseline - (single.lines[0].baseline + 30.0 - base)).abs() < 0.05);
    let below = plain(
        &mut system,
        with(LineSpacing::AtLeast(Length::from_points(5))),
        TEXT,
    );
    assert!((pitch(&below) - base).abs() < 0.01);

    let between = plain(
        &mut system,
        with(LineSpacing::Between(Length::from_points(6))),
        TEXT,
    );
    assert!((pitch(&between) - (base + 6.0)).abs() < 0.05);
    assert!((between.lines[0].baseline - single.lines[0].baseline).abs() < 0.01);
}

#[test]
fn fmt_18_indents() {
    let mut system = TextSystem::new();
    let indented = ParaFormat {
        left_indent: Length::from_points(20),
        first_line_indent: Length::from_points(15),
        right_indent: Length::from_points(30),
        ..ParaFormat::default()
    };
    let layout = plain(&mut system, indented, TEXT);
    assert!(layout.lines.len() > 2);
    assert_eq!(layout.lines[0].x, 35.0);
    assert_eq!(layout.lines[0].width, 135.0);
    for line in &layout.lines[1..] {
        assert_eq!(line.x, 20.0);
        assert_eq!(line.width, 150.0);
        assert!(line.advance <= 150.01);
        assert!((line.runs[0].glyphs[0].x - 20.0).abs() < 0.01);
    }

    // A hanging indent starts the first line to the left of the others.
    let hanging = ParaFormat {
        left_indent: Length::from_points(20),
        first_line_indent: Length::from_points(-20),
        ..ParaFormat::default()
    };
    let layout = plain(&mut system, hanging, TEXT);
    assert_eq!(layout.lines[0].x, 0.0);
    assert_eq!(layout.lines[1].x, 20.0);
}

#[test]
fn txt_08_no_break_space_keeps_words_together() {
    let mut system = TextSystem::new();
    // In 60pt, "10 km" fits but not with the word before it.
    let text = format!("about 10{NO_BREAK_SPACE}km");
    let layout = lay_in(
        &mut system,
        ParaFormat::default(),
        &[(&text, sans())],
        &Column::new(60.0),
    );
    let lines = line_texts(&layout, &text);
    assert_eq!(lines, ["about ", &format!("10{NO_BREAK_SPACE}km")[..]]);
}

#[test]
fn est_06_line_break_forces_a_new_line() {
    let mut system = TextSystem::new();
    let text = format!("one{LINE_BREAK}two");
    let layout = plain(&mut system, ParaFormat::default(), &text);
    let lines = line_texts(&layout, &text);
    assert_eq!(lines, [&format!("one{LINE_BREAK}")[..], "two"]);

    // A justified line that ends in a line break is not stretched.
    let justified = ParaFormat {
        alignment: Alignment::Justify,
        ..ParaFormat::default()
    };
    let text = format!("a b c{LINE_BREAK}d");
    let layout = plain(&mut system, justified, &text);
    let end = layout.lines[0].runs.last().unwrap();
    assert!(end.x + end.advance < 100.0);
}

#[test]
fn soft_hyphen_draws_a_hyphen_only_at_a_break() {
    let mut system = TextSystem::new();
    let text = format!("extra{SOFT_HYPHEN}ordinary");
    let wide = plain(&mut system, ParaFormat::default(), &text);
    assert_eq!(wide.lines.len(), 1);
    assert!(!wide.lines[0].hyphenated);

    let narrow = lay_in(
        &mut system,
        ParaFormat::default(),
        &[(&text, sans())],
        &Column::new(40.0),
    );
    assert_eq!(narrow.lines.len(), 2);
    let first = &narrow.lines[0];
    assert!(first.hyphenated);
    assert_eq!(&text[first.range.clone()], format!("extra{SOFT_HYPHEN}"));
    let hyphen = first.runs.last().unwrap();
    assert_eq!(hyphen.glyphs.len(), 1);
    assert!(first.advance <= 40.01);
    assert!(!narrow.lines[1].hyphenated);
}

/// A column with an object on its right half, from 15pt to 45pt down.
struct Wrapped;

impl LineSpace for Wrapped {
    fn width(&self) -> f32 {
        200.0
    }

    fn segments(&self, top: f32, height: f32) -> Vec<(f32, f32)> {
        if top < 45.0 && top + height > 15.0 {
            vec![(0.0, 90.0), (150.0, 50.0)]
        } else {
            vec![(0.0, 200.0)]
        }
    }
}

#[test]
fn lines_flow_around_an_object() {
    let mut system = TextSystem::new();
    let layout = lay_in(
        &mut system,
        ParaFormat::default(),
        &[(TEXT, sans())],
        &Wrapped,
    );
    let beside: Vec<_> = layout.lines.iter().filter(|l| l.width < 200.0).collect();
    assert!(beside.len() >= 4, "{}", beside.len());
    for line in &layout.lines {
        assert!(line.advance <= line.width + 0.01);
        for run in &line.runs {
            for glyph in &run.glyphs {
                assert!(glyph.x >= line.x - 0.01 && glyph.x < line.x + line.width);
            }
        }
    }
    // Two pieces at the same height share a top.
    let pair = beside
        .windows(2)
        .find(|p| p[0].x == 0.0 && p[1].x == 150.0)
        .unwrap();
    assert_eq!(pair[0].top, pair[1].top);
    assert_eq!(layout.lines.last().unwrap().range.end, TEXT.len());
}

#[test]
fn indents_wider_than_the_column_overflow_instead_of_hanging() {
    let mut system = TextSystem::new();
    let squeezed = ParaFormat {
        left_indent: Length::from_points(150),
        right_indent: Length::from_points(150),
        ..ParaFormat::default()
    };
    let layout = plain(&mut system, squeezed, "word");
    assert_eq!(layout.lines.len(), 1);
}
