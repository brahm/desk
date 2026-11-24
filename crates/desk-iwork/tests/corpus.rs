// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Reads the Pages documents of the corpus, which Pages 4.3 saved on the
//! reference machine.

use std::path::{Path, PathBuf};

use desk_iwork::pages::{LINE_BREAK, PagesDocument};
use desk_iwork::sf::{Family, LineSpacingMode, Value};
use desk_iwork::{IworkError, read_pages, read_pages_at};

fn case(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/writer")
        .join(name)
        .join("case.pages")
}

fn read(name: &str) -> PagesDocument {
    read_pages_at(&case(name)).unwrap()
}

fn texts(document: &PagesDocument) -> Vec<String> {
    document
        .body
        .iter()
        .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect())
        .collect()
}

/// The value that a style or its ancestors set.
fn inherited<'a>(document: &'a PagesDocument, style: &str, name: &str) -> Option<&'a Value> {
    let sheet = &document.stylesheet;
    sheet
        .ancestry(sheet.find(style)?)
        .into_iter()
        .find_map(|s| s.properties.get(name))
}

#[test]
fn doc_22_reads_paragraphs_spans_and_their_styles() {
    let document = read("fmt-01-02-emphasis");
    assert_eq!(
        texts(&document),
        [
            "Texto em negrito no meio da frase.",
            "Texto em itálico no meio da frase.",
            "Texto em negrito e itálico.",
            "Sublinhado simples e duplo.",
            "Tachado simples e duplo.",
        ]
    );
    let first = &document.body[0];
    let body = document
        .stylesheet
        .find(first.style.as_deref().unwrap())
        .unwrap();
    assert_eq!(body.name.as_deref(), Some("Body"));
    assert_eq!(body.family, Family::Paragraph);
    assert_eq!(body.properties.string("fontName"), Some("LiberationSans"));
    // A property saved empty is not set, unless Pages starts it from a
    // value of its own, as the font size from 12 points.
    assert_eq!(body.properties.get("lineSpacing"), None);
    assert_eq!(body.properties.number("fontSize"), Some(12.0));

    let bold = first.runs[1].style.as_deref().unwrap();
    assert_eq!(first.runs[1].text, "negrito");
    let bold = document.stylesheet.find(bold).unwrap();
    assert_eq!(bold.family, Family::Character);
    assert_eq!(bold.properties.number("bold"), Some(1.0));
    let double = document.body[3].runs[3].style.as_deref().unwrap();
    assert_eq!(
        inherited(&document, double, "underline"),
        Some(&Value::Number(2.0))
    );
}

#[test]
fn doc_22_reads_the_page_and_the_information() {
    let document = read("est-02-03-paper-and-margins");
    let page = document.page;
    assert_eq!((page.width, page.height), (792.0, 612.0));
    assert_eq!((page.top, page.left), (36.0, 108.0));
    let a4 = read("fmt-01-02-emphasis").page;
    assert_eq!((a4.width, a4.height), (595.0, 842.0));
    assert_eq!(read("fmt-01-02-emphasis").metadata.copyright, "(c)");
}

#[test]
fn doc_22_reads_line_spacing_and_inherited_paragraph_properties() {
    let document = read("fmt-14-line-spacing");
    let modes: Vec<(LineSpacingMode, f64)> = document.body[1..]
        .iter()
        .map(
            |p| match inherited(&document, p.style.as_deref().unwrap(), "lineSpacing") {
                Some(Value::LineSpacing { mode, amount }) => (mode.clone(), *amount),
                other => panic!("{other:?}"),
            },
        )
        .collect();
    assert_eq!(
        modes,
        [
            (LineSpacingMode::Relative, 2.0),
            (LineSpacingMode::Relative, 1.5),
            (LineSpacingMode::Minimum, 24.0),
            (LineSpacingMode::Absolute, 18.0),
            (LineSpacingMode::Between, 8.0),
        ]
    );
    // The anonymous style inherits the space after of Body.
    let style = document.body[1].style.as_deref().unwrap();
    assert_eq!(
        inherited(&document, style, "spaceAfter"),
        Some(&Value::Number(12.0))
    );
}

#[test]
fn doc_22_reads_line_breaks_and_colors() {
    let document = read("est-06-txt-08-breaks-and-spaces");
    assert_eq!(document.body[0].runs[0].text.matches(LINE_BREAK).count(), 2);
    let document = read("fmt-11-text-color");
    let red = document.body[0].runs[0].style.as_deref().unwrap();
    let Some(Value::Color(red)) = inherited(&document, red, "fontColor") else {
        panic!();
    };
    assert!((red.r - 0.8).abs() < 1e-6 && red.g == 0.0 && red.b == 0.0);
}

#[test]
fn every_case_of_the_corpus_reads() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/writer");
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path().join("case.pages");
        if path.exists() {
            let document = read_pages_at(&path).unwrap();
            assert!(!document.body.is_empty(), "{}", path.display());
            assert!(
                document.skipped.is_empty(),
                "{}: {:?}",
                path.display(),
                document.skipped
            );
        }
    }
}

#[test]
fn other_documents_are_refused() {
    let keynote =
        r#"<key:presentation xmlns:key="http://developer.apple.com/namespaces/keynote2"/>"#;
    assert!(matches!(
        read_pages(keynote.as_bytes()),
        Err(IworkError::WrongType(_))
    ));
    assert!(read_pages(b"not xml <").is_err());
}

/// The table of a fixture of Pages '09, in the line of the text.
fn fixture_table(name: &str) -> desk_iwork::pages::Table {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/pages")
        .join(name);
    let document = read_pages_at(&path).unwrap();
    document
        .attachments
        .iter()
        .find_map(|(_, d)| match &d.kind {
            desk_iwork::pages::DrawableKind::Table(t) => Some((**t).clone()),
            _ => None,
        })
        .expect("a table")
}

#[test]
fn tab_01_reads_the_cells_of_a_table_row_by_row() {
    use desk_iwork::pages::CellContent;
    let table = fixture_table("tab-01-basic.pages");
    assert_eq!((table.rows(), table.cols()), (5, 3));
    assert_eq!(table.header_rows, 1);
    assert!((table.widths[0] - 160.2047).abs() < 0.001);
    let at = |r, c| {
        table
            .cells
            .iter()
            .find(|cell| (cell.row, cell.col) == (r, c))
            .map(|cell| cell.content.clone())
    };
    assert_eq!(at(0, 2), Some(CellContent::Text("Price".into())));
    assert_eq!(at(1, 0), Some(CellContent::Text("Apple".into())));
    assert_eq!(at(3, 1), Some(CellContent::Number(7.0)));
    assert_eq!(at(4, 0), None);
    assert!(table.skipped.is_empty());
}

#[test]
fn tab_23_32_reads_merged_cells_skipped_cells_and_formulas() {
    use desk_iwork::pages::CellContent;
    let merged = fixture_table("tab-23-24-merge-split.pages");
    let spans: Vec<(u32, u32, u32, u32)> = merged
        .cells
        .iter()
        .filter(|c| c.rows > 1 || c.cols > 1)
        .map(|c| (c.row, c.col, c.rows, c.cols))
        .collect();
    assert_eq!(
        spans,
        [
            (0, 0, 1, 2),
            (1, 0, 1, 2),
            (2, 0, 1, 3),
            (3, 0, 1, 2),
            (3, 2, 2, 1),
            (3, 3, 2, 1)
        ]
    );
    let formulas = fixture_table("tab-32-iop-05-formulas.pages");
    let at = |r, c| {
        formulas
            .cells
            .iter()
            .find(|cell| (cell.row, cell.col) == (r, c))
            .map(|cell| cell.content.clone())
    };
    // A cell skipped: the 6 is in the second column of the last row.
    assert_eq!(at(3, 1), Some(CellContent::Number(6.0)));
    assert_eq!(at(3, 0), None);
    assert_eq!(at(3, 2), Some(CellContent::Formula("=SUM(A4:B4)".into())));
}

#[test]
fn tab_27_reads_the_cells_that_have_comments() {
    let table = fixture_table("tab-27-cell-comment.pages");
    assert_eq!(
        table.comments,
        [(0, 0, "SLAnnotationTableCell-0".to_owned())]
    );
}

/// The look of a cell of a table.
fn look_of(table: &desk_iwork::pages::Table, r: u32, c: u32) -> &desk_iwork::pages::CellLook {
    let index = table.cell_looks[(r * table.cols() + c) as usize].expect("a look");
    &table.looks[index]
}

#[test]
fn tab_04_14_reads_the_fills_and_lines_of_cells_and_the_alternating_rows() {
    use desk_iwork::sf::Value;
    let table = fixture_table("tab-04-22-25-appearance.pages");
    let (rows, cols) = (table.rows(), table.cols());
    // The header row of the table style: grey, and the gradient of the
    // third header cell over it.
    let header = &table.looks[table.part_looks[0].unwrap()];
    match header.cell.get("fill") {
        Some(Value::Color(c)) => assert!((c.r - 0.690).abs() < 0.001, "{c:?}"),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        look_of(&table, 0, 2).cell.get("fill"),
        Some(Value::Gradient { stops, .. }) if stops.len() == 2
    ));
    // Its text: bold Helvetica, centred.
    let text = &look_of(&table, 0, 0).text;
    assert_eq!(text.string("fontName"), Some("Helvetica-Bold"));
    assert_eq!(text.number("alignment"), Some(2.0));
    // Every line is drawn, and those around the third header cell are
    // 3 points wide, by the runs of its gridlines.
    assert_eq!(table.h_lines.len(), ((rows + 1) * cols) as usize);
    assert_eq!(table.v_lines.len(), (rows * (cols + 1)) as usize);
    let width = |line: &Option<desk_iwork::sf::Stroke>| line.as_ref().map(|s| s.width);
    assert!(
        table
            .h_lines
            .iter()
            .chain(&table.v_lines)
            .all(Option::is_some)
    );
    assert_eq!(width(&table.h_lines[2]), Some(3.0));
    assert_eq!(width(&table.h_lines[(cols + 2) as usize]), Some(3.0));
    assert_eq!(width(&table.v_lines[2]), Some(3.0));
    assert_eq!(width(&table.v_lines[3]), Some(3.0));
    assert_eq!(width(&table.h_lines[(cols + 1) as usize]), Some(1.0));
    let banded = fixture_table("tab-14-alternating.pages")
        .banded
        .expect("banded");
    assert!(banded.r > 0.85 && banded.r < 1.0, "{banded:?}");
    assert!(fixture_table("tab-01-basic.pages").banded.is_none());
}

#[test]
fn tab_21_28_29_reads_the_formats_and_the_rules_of_cells() {
    use desk_iwork::pages::{CellFormat, CellRule, RuleOperand};
    let table = fixture_table("tab-28-cell-formats.pages");
    let format = |r, c| look_of(&table, r, c).format.clone();
    assert_eq!(format(0, 0), CellFormat::Automatic);
    assert!(
        matches!(format(0, 1), CellFormat::Currency(n) if n.currency == "BRL" && n.decimals == Some(2))
    );
    assert!(matches!(format(0, 2), CellFormat::Percent(n) if n.decimals == Some(2)));
    assert_eq!(format(1, 0), CellFormat::DateTime("dd/MM/yy HH:mm".into()));
    assert_eq!(format(1, 1), CellFormat::Duration("hhh mmm sss".into()));
    assert!(matches!(format(2, 0), CellFormat::Base(_)));
    assert!(matches!(format(2, 1), CellFormat::Scientific(n) if n.decimals == Some(2)));
    assert_eq!(format(2, 2), CellFormat::Text);
    let custom = fixture_table("tab-29-30-custom-format.pages");
    match &look_of(&custom, 0, 0).format {
        CellFormat::Custom(n) => {
            assert_eq!(n.name.as_deref(), Some("Custom Format 1"));
            assert_eq!(n.pattern, "#,###");
        }
        other => panic!("{other:?}"),
    }
    // The rule of tab-21, on the second value only: italic when not 50.
    let rules = fixture_table("tab-21-conditional.pages");
    assert_eq!(
        look_of(&rules, 2, 1).rules,
        [CellRule {
            kind: 6,
            operands: vec![RuleOperand::Number(50.0)],
            italic: Some(true),
            ..CellRule::default()
        }]
    );
    assert!(look_of(&rules, 1, 1).rules.is_empty());
}

#[test]
fn a_table_reads_runs_past_its_lines_and_cells_without_a_table_style() {
    use std::io::Read;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/pages/tab-04-22-25-appearance.pages/index.xml.gz");
    let mut xml = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap())
        .read_to_string(&mut xml)
        .unwrap();
    let table_of = |xml: &str| {
        let document = desk_iwork::pages::parse_pages(xml).unwrap();
        document
            .attachments
            .iter()
            .find_map(|(_, d)| match &d.kind {
                desk_iwork::pages::DrawableKind::Table(t) => Some((**t).clone()),
                _ => None,
            })
            .expect("a table")
    };
    // A run whose end is far past the line stops at the line, at once.
    let runs = r#"sf:start-index="2" sf:stop-index="3""#;
    assert!(xml.contains(runs));
    let far = table_of(&xml.replace(runs, r#"sf:start-index="2" sf:stop-index="4294967295""#));
    assert_eq!(far.h_lines.len(), ((far.rows() + 1) * far.cols()) as usize);
    assert_eq!(far.h_lines[2].as_ref().map(|s| s.width), Some(3.0));
    // Without the table style, the cells keep their own styles.
    let style = r#"<sf:tabular-style-ref sfa:IDREF="SFTTableStyle-1"/>"#;
    assert!(xml.contains(style));
    let bare = table_of(&xml.replace(style, ""));
    assert_eq!(bare.part_looks, [None; 4]);
    assert!(bare.h_lines.is_empty() && bare.banded.is_none());
    assert!(matches!(
        look_of(&bare, 0, 2).cell.get("fill"),
        Some(Value::Gradient { .. })
    ));
}

#[test]
fn every_table_of_the_fixtures_reads_every_cell() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/fixtures/pages");
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "pages") {
            continue;
        }
        let document = read_pages_at(&path).unwrap();
        for (_, drawable) in &document.attachments {
            if let desk_iwork::pages::DrawableKind::Table(table) = &drawable.kind {
                assert!(table.skipped.is_empty(), "{path:?}: {:?}", table.skipped);
            }
        }
    }
}
