// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paragraph layout, named by the IDs of the parity matrix. The tests use
//! Liberation Sans, which CI installs, so the widths are the same
//! everywhere.

use desk_model::FontWeight;
use std::sync::Arc;

use desk_model::{
    Alignment, Baseline, Capitalization, CharFormat, LINE_BREAK, Length, LineColor, LineSpacing,
    LineStyle, NO_BREAK_SPACE, ParaFormat, Rgba, TextSettings,
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
        weight: FontWeight::BOLD,
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

    // Weights other than bold pick their own faces, where the family has
    // them (INT-15).
    if &*system.font_choice("Inter").used == "Inter" {
        let inter = |weight| CharFormat {
            font_family: Arc::from("Inter"),
            weight,
            ..CharFormat::default()
        };
        let layout = lay(
            &mut system,
            ParaFormat::default(),
            &[
                ("thin ", inter(FontWeight::THIN)),
                ("light ", inter(FontWeight::LIGHT)),
                ("regular", inter(FontWeight::NORMAL)),
            ],
        );
        let fonts: Vec<_> = layout.lines[0]
            .runs
            .iter()
            .map(|r| (r.font.data.id(), r.font.index))
            .collect();
        assert_eq!(fonts.len(), 3);
        assert_ne!(fonts[0], fonts[1]);
        assert_ne!(fonts[1], fonts[2]);
    }
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
    let blue = Rgba::rgb(0, 0, 255);
    let colored = lay(
        &mut system,
        ParaFormat::default(),
        &[(
            "marked",
            CharFormat {
                underline_color: LineColor::Custom(blue),
                ..marked.clone()
            },
        )],
    );
    // FMT-01: the underline has a color of its own too.
    assert_eq!(colored.lines[0].decorations[0].color, blue);
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
    // As in Pages: the top of a superscript meets the top of the text and
    // the bottom of a subscript its bottom. Liberation Sans rises 0.905 em
    // and falls 0.212 em, and 4 pt are lost to the smaller size.
    let raise = runs[0].baseline - runs[1].baseline;
    let drop = runs[3].baseline - runs[2].baseline;
    assert!((raise - 0.905 * 4.0).abs() < 0.01, "{raise}");
    assert!((drop - 0.212 * 4.0).abs() < 0.01, "{drop}");
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
    // As Pages lays out lines, measured on the reference machine: the
    // ascent, descent and leading of Liberation Sans at 12 pt round to 11,
    // 3 and 0, so a single line is 14 pt.
    let mut system = TextSystem::new();
    let with = |line_spacing| ParaFormat {
        line_spacing,
        ..ParaFormat::default()
    };
    let pitch = |layout: &ParagraphLayout| layout.lines[1].top - layout.lines[0].top;
    let layout = |system: &mut TextSystem, spacing| plain(system, with(spacing), TEXT);
    let single = layout(&mut system, LineSpacing::SINGLE);
    assert!((pitch(&single) - 14.0).abs() < 0.01, "{}", pitch(&single));
    // The text sits at the top of the line in every mode but one, and the
    // extra space goes below it.
    let baseline = single.lines[0].baseline;
    let check = |layout: ParagraphLayout, expected: f32, shift: f32| {
        assert!(
            (pitch(&layout) - expected).abs() < 0.01,
            "pitch {} instead of {expected}",
            pitch(&layout)
        );
        assert!(
            (layout.lines[0].baseline - (baseline + shift)).abs() < 0.01,
            "baseline {} instead of {}",
            layout.lines[0].baseline,
            baseline + shift
        );
    };
    check(
        layout(&mut system, LineSpacing::Multiple { percent: 150 }),
        21.0,
        0.0,
    );
    check(layout(&mut system, LineSpacing::DOUBLE), 28.0, 0.0);
    let pt = Length::from_points;
    check(layout(&mut system, LineSpacing::AtLeast(pt(30))), 30.0, 0.0);
    check(layout(&mut system, LineSpacing::AtLeast(pt(5))), 14.0, 0.0);
    check(layout(&mut system, LineSpacing::Between(pt(6))), 20.0, 0.0);
    // Exact spacing puts the text on the bottom of the line, less its
    // descent and leading.
    check(
        layout(&mut system, LineSpacing::Exactly(pt(30))),
        30.0,
        16.0,
    );
    check(layout(&mut system, LineSpacing::Exactly(pt(8))), 8.0, -6.0);
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

#[test]
fn txt_09_automatic_hyphenation_breaks_words_by_their_language() {
    let mut system = TextSystem::new();
    let text = "The hyphenation of representative words";
    let english = CharFormat {
        language: desk_model::Language::new("en-US"),
        ..sans()
    };
    let column = Column::new(70.0);
    let whole = lay_in(
        &mut system,
        ParaFormat::default(),
        &[(text, english.clone())],
        &column,
    );
    assert!(whole.lines.iter().all(|l| !l.hyphenated));
    let hyphenating = ParaFormat {
        hyphenate: true,
        ..ParaFormat::default()
    };
    // Text in the undetermined language is not hyphenated.
    let undetermined = lay_in(&mut system, hyphenating.clone(), &[(text, sans())], &column);
    assert!(undetermined.lines.iter().all(|l| !l.hyphenated));
    if desk_lang::patterns("en-US").is_none() {
        return;
    }
    let broken = lay_in(&mut system, hyphenating, &[(text, english)], &column);
    let first = broken
        .lines
        .iter()
        .find(|l| l.hyphenated)
        .expect("a hyphen");
    // The lines keep the offsets of the model, without the hyphens.
    let lines = line_texts(&broken, text);
    assert_eq!(lines.concat().replace(' ', ""), text.replace(' ', ""));
    assert!(
        text[first.range.clone()].ends_with("hy") || lines.iter().any(|l| l.ends_with("hyphen"))
    );
    // Where a word may break, tracking adds no space.
    let tracked = CharFormat {
        tracking: 200,
        ..english_format()
    };
    let wide = Column::new(400.0);
    let width = |system: &mut TextSystem, hyphenate: bool| {
        let paragraph = ParaFormat {
            hyphenate,
            ..ParaFormat::default()
        };
        lay_in(
            system,
            paragraph,
            &[("representative", tracked.clone())],
            &wide,
        )
        .lines[0]
            .advance
    };
    assert_eq!(width(&mut system, true), width(&mut system, false));
    // The text that a PDF keeps to copy has no marks where words may break.
    for line in &broken.lines {
        for run in &line.runs {
            assert!(!run.text.contains('\u{ad}'), "{:?}", run.text);
        }
    }

    // A document that is not hyphenated has no hyphens, and words never
    // hyphenated are kept whole.
    let lay_with = |system: &mut TextSystem, settings: &TextSettings| {
        let text = ParagraphText::from_runs_with(
            settings,
            ParaFormat {
                hyphenate: true,
                ..ParaFormat::default()
            },
            sans(),
            [(text, english_format())],
        );
        system.layout(&text, &column, 0.0)
    };
    let off = TextSettings {
        hyphenation: false,
        ..TextSettings::default()
    };
    assert!(
        lay_with(&mut system, &off)
            .lines
            .iter()
            .all(|l| !l.hyphenated)
    );
    let never = TextSettings {
        never_hyphenate: ["hyphenation", "representative"].map(str::to_owned).into(),
        ..TextSettings::default()
    };
    let kept = lay_with(&mut system, &never);
    for line in &kept.lines {
        let words = &text[line.range.clone()];
        assert!(
            !line.hyphenated || !(words.ends_with("hy") || words.ends_with("repre")),
            "{words}"
        );
    }
}

fn english_format() -> CharFormat {
    CharFormat {
        language: desk_model::Language::new("en-US"),
        ..sans()
    }
}

#[test]
fn fmt_09_a_document_without_ligatures_has_none() {
    let mut system = TextSystem::new();
    let all = CharFormat {
        ligatures: desk_model::Ligatures::All,
        ..sans()
    };
    let glyphs = |system: &mut TextSystem, settings: &TextSettings| {
        let text = ParagraphText::from_runs_with(
            settings,
            ParaFormat::default(),
            sans(),
            [("office", all.clone())],
        );
        let laid = system.layout(&text, &Column::new(300.0), 0.0);
        laid.lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.glyphs.len())
            .sum::<usize>()
    };
    let off = TextSettings {
        ligatures: false,
        ..TextSettings::default()
    };
    // One glyph for each letter, and no more than that with ligatures.
    assert_eq!(glyphs(&mut system, &off), "office".len());
    assert!(glyphs(&mut system, &TextSettings::default()) <= "office".len());
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

/// A column with an object on its right half from 21pt down.
struct Below;

impl LineSpace for Below {
    fn width(&self) -> f32 {
        200.0
    }

    fn segments(&self, top: f32, height: f32) -> Vec<(f32, f32)> {
        if top + height > 21.0 {
            vec![(0.0, 90.0)]
        } else {
            vec![(0.0, 200.0)]
        }
    }
}

#[test]
fn obj_17_a_line_shorter_than_its_text_takes_only_its_own_height_from_an_object() {
    let mut system = TextSystem::new();
    let paragraph = ParaFormat {
        line_spacing: LineSpacing::Exactly(Length::from_points(10)),
        ..ParaFormat::default()
    };
    let layout = lay_in(&mut system, paragraph, &[(TEXT, sans())], &Below);
    // The second line, from 10 to 20 points, is clear of the object,
    // though its text would be taller.
    let tops: Vec<(f32, f32)> = layout.lines.iter().map(|l| (l.top, l.width)).collect();
    assert_eq!(tops[1], (10.0, 200.0), "{tops:?}");
    assert_eq!(tops[2], (20.0, 90.0), "{tops:?}");
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

#[test]
fn int_02_the_text_of_a_line_ends_above_its_leading() {
    // Liberation Serif at 12 pt, as Pages rounds it: 11 pt above the
    // baseline, 3 below and 1 of leading, which a page need not hold.
    let mut system = TextSystem::new();
    let serif = CharFormat {
        font_family: Arc::from("Liberation Serif"),
        ..CharFormat::default()
    };
    let layout = lay(&mut system, ParaFormat::default(), &[("Lorem", serif)]);
    let line = &layout.lines[0];
    assert_eq!(line.height, 15.0);
    assert_eq!(line.baseline - line.top, 11.0);
    assert_eq!(line.text_bottom - line.top, 14.0);
}

#[test]
fn doc_05_missing_fonts_are_laid_out_with_their_substitutes() {
    let mut system = TextSystem::new();
    let helvetica = system.font_choice("Helvetica");
    if helvetica.is_substitution() {
        // Liberation Sans has the metrics of Helvetica.
        assert_eq!(&*helvetica.used, "Liberation Sans");
        assert_eq!(helvetica.kind, desk_text::FontMatch::Metric);
        let format = |family: &str| CharFormat {
            font_family: Arc::from(family),
            ..CharFormat::default()
        };
        let text = "Lines break where they did on the Mac.";
        let a = lay(
            &mut system,
            ParaFormat::default(),
            &[(text, format("Helvetica"))],
        );
        let b = lay(
            &mut system,
            ParaFormat::default(),
            &[(text, format("Liberation Sans"))],
        );
        assert_eq!(a.lines[0].advance, b.lines[0].advance);
    }
    let missing = system.font_choice("Desk Missing Serif");
    assert_eq!(&*missing.used, "Liberation Serif");
    assert_eq!(missing.kind, desk_text::FontMatch::Fallback);
    let listed: Vec<_> = system
        .substitutions()
        .into_iter()
        .map(|c| c.requested)
        .collect();
    assert!(
        listed.iter().any(|r| &**r == "Desk Missing Serif"),
        "{listed:?}"
    );
    assert!(!listed.iter().any(|r| &**r == "Liberation Sans"));
}

#[test]
fn doc_05_fonts_of_the_mac_keep_the_lines_and_ligatures_of_pages() {
    let mut system = TextSystem::new();
    let format = |family: &str| CharFormat {
        font_family: Arc::from(family),
        font_size: Length::from_points(12),
        ..CharFormat::default()
    };
    // As Pages measured on the reference machine: Helvetica at 12 pt has
    // lines 14 pt tall, with the leading over the text, and Palatino 16 pt,
    // whatever font stands for them.
    for (family, height, baseline) in [("Helvetica", 14.0, 11.0), ("Palatino", 16.0, 13.0)] {
        let layout = lay(
            &mut system,
            ParaFormat::default(),
            &[("Line", format(family))],
        );
        let line = &layout.lines[0];
        assert_eq!(line.height, height, "{family}");
        assert_eq!(line.baseline - line.top, baseline, "{family}");
    }
    // Pages joins fi and fl in Palatino, and nothing else: the ligature
    // draws the letters it joins, and offsets stay those of the model.
    let text = "office fly";
    let layout = lay(
        &mut system,
        ParaFormat::default(),
        &[(text, format("Palatino"))],
    );
    let line = &layout.lines[0];
    assert_eq!(line.range, 0..text.len());
    let run = &line.runs[0];
    assert_eq!(run.text, text);
    assert_eq!(run.glyphs.len(), text.chars().count() - 2);
    assert_eq!(
        run.clusters.iter().map(|c| c.1).sum::<u32>() as usize,
        run.glyphs.len()
    );
}

#[test]
fn doc_05_a_ligature_of_the_font_reads_as_the_letters_it_joins() {
    // Helvetica Neue, as TeX Gyre Heros stands for it, joins fi itself:
    // the glyph of the ligature carries both letters, so that a PDF reads
    // "Definir" and not "Defnir".
    let mut system = TextSystem::new();
    let format = CharFormat {
        font_family: Arc::from("Helvetica Neue"),
        font_size: Length::from_points(12),
        ..CharFormat::default()
    };
    for text in ["Definir", "fluxo", "office"] {
        let layout = lay(
            &mut system,
            ParaFormat::default(),
            &[(text, format.clone())],
        );
        let run = &layout.lines[0].runs[0];
        assert_eq!(run.text, text);
        assert!(!run.clusters.is_empty(), "{text}");
        assert!(
            run.clusters.iter().all(|c| c.1 > 0),
            "{text}: {:?}",
            run.clusters
        );
        assert_eq!(
            run.clusters.iter().map(|c| c.0).sum::<u32>() as usize,
            text.len()
        );
    }
}

#[test]
fn doc_05_helvetica_neue_lays_out_as_pages_lays_out_each_face() {
    // As measured on the reference machine: Pages rounds the ascent, the
    // descent and the line gap of each face to whole points, and puts the
    // gap below the text.
    let mut system = TextSystem::new();
    let neue = |weight: u16, size: i32| CharFormat {
        font_family: Arc::from("Helvetica Neue"),
        font_size: Length::from_points(size),
        weight: FontWeight::new(weight),
        ..CharFormat::default()
    };
    let text = "Line one\u{2028}Line two\u{2028}Line three";
    for (weight, size, baseline, pitch) in [
        (400, 12, 11.0, 14.0),
        (300, 12, 12.0, 15.0),
        (100, 12, 11.0, 14.0),
        (500, 12, 12.0, 15.0),
        (700, 12, 12.0, 15.0),
        // At 72 pt the gap rounds to 2 pt, all of it under the text.
        (400, 72, 69.0, 69.0 + 15.0 + 2.0),
    ] {
        let layout = lay(
            &mut system,
            ParaFormat::default(),
            &[(text, neue(weight, size))],
        );
        let lines = &layout.lines;
        assert_eq!(lines[0].baseline, baseline, "{weight} at {size} pt");
        assert_eq!(lines[1].top - lines[0].top, pitch, "{weight} at {size} pt");
    }

    // A multiple of the line rounds to a whole point: 130% of the 11 pt
    // line of Light at 9 pt is 14 pt, as in Pages.
    let spaced = ParaFormat {
        line_spacing: LineSpacing::Multiple { percent: 130 },
        ..ParaFormat::default()
    };
    let layout = lay(&mut system, spaced, &[(text, neue(300, 9))]);
    assert_eq!(layout.lines[1].top - layout.lines[0].top, 14.0);

    // Its letters are spaced so that a line is as wide as in Pages, where
    // this one is 310.75 pt in Regular and 301.45 pt in Light.
    let fox = "The quick brown fox jumps over the lazy dog 0123456789";
    let width = |system: &mut TextSystem, weight| {
        let layout = lay_in(
            system,
            ParaFormat::default(),
            &[(fox, neue(weight, 12))],
            &Column::new(500.0),
        );
        let run = layout.lines[0].runs.last().unwrap();
        run.x + run.advance
    };
    let regular = width(&mut system, 400);
    assert!((309.0..312.5).contains(&regular), "{regular}");
    let light = width(&mut system, 300);
    assert!((298.0..303.0).contains(&light), "{light}");

    // Regular and Bold join ff and ffi as well as fi and fl; the other
    // faces join only fi and fl. The text of the PDF keeps the letters.
    for (weight, glyphs) in [(400, 4), (700, 4), (300, 5)] {
        let layout = lay(
            &mut system,
            ParaFormat::default(),
            &[("office", neue(weight, 12))],
        );
        let run = &layout.lines[0].runs[0];
        assert_eq!(run.text, "office");
        assert_eq!(run.glyphs.len(), glyphs, "{weight}");
    }
}

#[test]
fn txt_03_a_cursor_stops_at_every_character_of_a_line() {
    let mut system = TextSystem::new();
    let layout = plain(&mut system, ParaFormat::default(), TEXT);
    for line in &layout.lines {
        // From the start of the line to its end, left to right.
        assert_eq!(line.carets.first().unwrap().0, line.range.start);
        assert_eq!(line.carets.last().unwrap().0, line.range.end);
        assert!(
            line.carets
                .windows(2)
                .all(|w| w[0].0 < w[1].0 && w[0].1 <= w[1].1)
        );
        for &(offset, x) in &line.carets {
            assert_eq!(line.caret_x(offset), x);
            assert_eq!(line.offset_at(x + 0.1), offset);
        }
    }
    // Ligatures and the capitals of small caps keep a stop per letter of
    // the model.
    let first = &layout.lines[0];
    assert_eq!(
        first.carets.len(),
        TEXT[first.range.clone()].chars().count() + 1
    );
    // An empty paragraph has one place, at the left of a left aligned
    // line and in the middle of a centred one.
    let empty = plain(&mut system, ParaFormat::default(), "");
    assert_eq!(empty.lines[0].carets, vec![(0, 0.0)]);
    let centred = ParaFormat {
        alignment: Alignment::Center,
        ..ParaFormat::default()
    };
    let empty = plain(&mut system, centred, "");
    assert_eq!(empty.lines[0].carets, vec![(0, 100.0)]);
}

#[test]
fn fmt_16_text_after_a_right_tab_in_another_format_moves_with_it() {
    let mut system = TextSystem::new();
    let paragraph = ParaFormat {
        tab_stops: desk_model::TabStops::new([desk_model::TabStop {
            alignment: desk_model::TabAlignment::Right,
            ..desk_model::TabStop::left(Length::from_points(180))
        }]),
        ..ParaFormat::default()
    };
    let bold = CharFormat {
        weight: FontWeight::BOLD,
        ..sans()
    };
    let layout = lay(&mut system, paragraph, &[("Entry\t", sans()), ("12", bold)]);
    let line = &layout.lines[0];
    let last = line.runs.last().unwrap();
    // The number ends at the stop.
    assert!(
        (last.x + last.advance - 180.0).abs() < 0.5,
        "{:?}",
        line.runs
            .iter()
            .map(|r| (r.x, r.advance))
            .collect::<Vec<_>>()
    );
    let first_glyph = last.glyphs[0].x;
    assert!(
        (first_glyph - last.x).abs() < 0.5,
        "{first_glyph} {}",
        last.x
    );
}

#[test]
fn est_22_a_field_after_a_right_tab_moves_with_it() {
    let mut system = TextSystem::new();
    let paragraph = ParaFormat {
        tab_stops: desk_model::TabStops::new([desk_model::TabStop {
            alignment: desk_model::TabAlignment::Right,
            ..desk_model::TabStop::left(Length::from_points(180))
        }]),
        ..ParaFormat::default()
    };
    let field = CharFormat {
        field: Some(desk_model::Field::PAGE_NUMBER),
        ..sans()
    };
    let text = ParagraphText::from_runs_in(
        &TextSettings::default(),
        desk_text::FieldValues {
            page: 12,
            ..desk_text::FieldValues::default()
        },
        paragraph,
        sans(),
        [("Entry\t", sans()), ("\u{fffc}", field)],
    );
    let layout = system.layout(&text, &Column::new(200.0), 0.0);
    let line = &layout.lines[0];
    let last = line.runs.last().unwrap();
    assert!(
        (last.x + last.advance - 180.0).abs() < 0.5 && (last.glyphs[0].x - last.x).abs() < 0.5,
        "{:?}",
        line.runs
            .iter()
            .map(|r| (
                r.x,
                r.advance,
                r.glyphs.iter().map(|g| g.x).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn obj_01_an_object_in_the_text_takes_its_room_in_the_line() {
    let mut system = TextSystem::new();
    let object = desk_model::NodeId::new(99).unwrap();
    let mark = CharFormat {
        field: Some(desk_model::Field::ObjectMark { object }),
        ..sans()
    };
    let sizes = std::collections::HashMap::from([(
        object,
        (Length::from_points(50), Length::from_points(40)),
    )]);
    let lay_with = |system: &mut TextSystem, values: desk_text::FieldValues| {
        let text = ParagraphText::from_runs_in(
            &TextSettings::default(),
            values,
            ParaFormat::default(),
            sans(),
            [("ab", sans()), ("\u{fffc}", mark.clone()), ("cd", sans())],
        );
        system.layout(&text, &Column::new(200.0), 0.0)
    };
    let plain = lay_with(&mut system, desk_text::FieldValues::default());
    let layout = lay_with(
        &mut system,
        desk_text::FieldValues {
            objects: Some(Arc::new(sizes)),
            ..desk_text::FieldValues::default()
        },
    );
    let (line, without) = (&layout.lines[0], &plain.lines[0]);
    assert_eq!(layout.lines.len(), 1);
    // As wide as the object, between "ab" and "cd".
    let before = line.caret_x(2);
    let after = line.caret_x(2 + '\u{fffc}'.len_utf8());
    assert!((after - before - 50.0).abs() < 0.1, "{before} {after}");
    assert!((line.advance - without.advance - 50.0).abs() < 0.1);
    // As tall: the object stands on the baseline.
    assert!((line.baseline - line.top - 40.0).abs() < 0.01, "{line:?}");
    let under = without.height - (without.baseline - without.top);
    assert!((line.height - 40.0 - under).abs() < 0.01, "{}", line.height);
}

#[test]
fn doc_05_a_multiple_spacing_scales_the_text_and_not_the_object() {
    // A picture as tall as most of the page, in a paragraph spaced at
    // 130 %: Pages makes the line as tall as the picture plus the text
    // below the baseline, so that the caption after it fits on the page.
    let mut system = TextSystem::new();
    let object = desk_model::NodeId::new(99).unwrap();
    let mark = CharFormat {
        field: Some(desk_model::Field::ObjectMark { object }),
        ..sans()
    };
    let sizes = std::collections::HashMap::from([(
        object,
        (Length::from_points(400), Length::from_points(600)),
    )]);
    let paragraph = ParaFormat {
        line_spacing: LineSpacing::Multiple { percent: 130 },
        ..ParaFormat::default()
    };
    let values = desk_text::FieldValues {
        objects: Some(Arc::new(sizes)),
        ..desk_text::FieldValues::default()
    };
    let text = ParagraphText::from_runs_in(
        &TextSettings::default(),
        values,
        paragraph.clone(),
        sans(),
        [("\u{fffc}", mark)],
    );
    let layout = system.layout(&text, &Column::new(500.0), 0.0);
    let line = &layout.lines[0];
    let spaced = plain(&mut system, paragraph, "Legenda");
    let text_line = &spaced.lines[0];
    let under = text_line.height - (text_line.baseline - text_line.top);
    assert!((line.baseline - line.top - 600.0).abs() < 0.01, "{line:?}");
    assert!(
        (line.height - 600.0 - under).abs() < 0.51,
        "{} {under}",
        line.height
    );
}

#[test]
fn doc_05_an_object_alone_on_a_line_keeps_the_text_below_it_at_any_spacing() {
    // The line of a picture keeps the part of its text below the
    // baseline, as with a multiple, when the spacing is at least or
    // exactly a height: the picture stands on the baseline of the text.
    let mut system = TextSystem::new();
    let object = desk_model::NodeId::new(99).unwrap();
    let mark = CharFormat {
        field: Some(desk_model::Field::ObjectMark { object }),
        ..sans()
    };
    let sizes = std::collections::HashMap::from([(
        object,
        (Length::from_points(100), Length::from_points(60)),
    )]);
    let values = desk_text::FieldValues {
        objects: Some(Arc::new(sizes)),
        ..desk_text::FieldValues::default()
    };
    let lay_out = |system: &mut TextSystem, spacing: LineSpacing| {
        let paragraph = ParaFormat {
            line_spacing: spacing,
            ..ParaFormat::default()
        };
        let text = ParagraphText::from_runs_in(
            &TextSettings::default(),
            values.clone(),
            paragraph,
            sans(),
            [("\u{fffc}", mark.clone())],
        );
        system.layout(&text, &Column::new(500.0), 0.0).lines[0].clone()
    };
    let words = plain(&mut system, ParaFormat::default(), "Legenda");
    let word = &words.lines[0];
    let under = word.height - (word.baseline - word.top);
    assert!(under > 1.0);
    // At least 14 points: the picture and the text below it.
    let line = lay_out(&mut system, LineSpacing::AtLeast(Length::from_points(14)));
    assert!((line.baseline - line.top - 60.0).abs() < 0.01, "{line:?}");
    assert!(
        (line.height - 60.0 - under).abs() < 0.51,
        "{} {under}",
        line.height
    );
    // At least more than that: the height asked for.
    let line = lay_out(&mut system, LineSpacing::AtLeast(Length::from_points(90)));
    assert!((line.height - 90.0).abs() < 0.01, "{line:?}");
    // Exactly 80 points: the picture stands on the baseline, as high
    // above the bottom of the line as the text below it.
    let line = lay_out(&mut system, LineSpacing::Exactly(Length::from_points(80)));
    assert!((line.height - 80.0).abs() < 0.01, "{line:?}");
    assert!(
        (line.top + line.height - line.baseline - under).abs() < 0.51,
        "{line:?} {under}"
    );
}

#[test]
fn fmt_12_a_justified_line_with_few_spaces_spreads_its_letters_as_pages_does() {
    let mut system = TextSystem::new();
    let text = "Desk lays outtextandgoeson";
    let lay_out = |system: &mut TextSystem, alignment, width| {
        let paragraph = ParaFormat {
            alignment,
            ..ParaFormat::default()
        };
        lay_in(system, paragraph, &[(text, sans())], &Column::new(width))
    };
    let xs = |layout: &ParagraphLayout| -> Vec<f32> {
        layout.lines[0]
            .runs
            .iter()
            .flat_map(|r| r.glyphs.iter().map(|g| g.x))
            .collect()
    };
    let left = lay_out(&mut system, Alignment::Left, 140.0);
    let natural = xs(&left);
    // "Desk lays" at 12 points: six gaps between letters and a space.
    let advance = left.lines[0].advance;
    let check = |system: &mut TextSystem, width: f32, between: f32, space: f32| {
        let justified = lay_out(system, Alignment::Justify, width);
        assert_eq!(line_texts(&justified, text)[0], "Desk lays ");
        let spread = xs(&justified);
        let grown: Vec<f32> = (1..9)
            .map(|i| (spread[i] - spread[i - 1]) - (natural[i] - natural[i - 1]))
            .collect();
        for (i, g) in grown.iter().enumerate() {
            let want = match i {
                3 => 0.0,
                4 => space,
                _ => between,
            };
            assert!((g - want).abs() < 0.01, "{width}: {grown:?}");
        }
    };
    // 20 points to fill: the space grows by half an em and half the gap
    // between the letters, which share the rest.
    let between = (20.0 - 6.0) / 6.5;
    check(&mut system, advance + 20.0, between, 6.0 + between / 2.0);
    // 80 points: the letters grow by 0.2892 em at most, and the space
    // takes the rest.
    let between = 0.2892 * 12.0;
    check(&mut system, advance + 80.0, between, 80.0 - 6.0 * between);
}
