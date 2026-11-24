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
