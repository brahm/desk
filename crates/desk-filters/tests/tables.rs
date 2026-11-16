// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables and charts saved as ODT and read back (TAB-01, GRF-01,
//! IOP-01, IOP-15).

use std::sync::Arc;

use desk_chart::{Chart, ChartKind};
use desk_filters::{read_odt, write_odt};
use desk_model::{Anchor, Document, Editor, Frame, Object, ObjectKind};
use desk_odf as odf;
use desk_table::{Area, CellFill, Color, Content, Format, Negative, Num};

fn document() -> Document {
    let mut editor = Editor::new(Document::new_word_processing());
    let mut table = desk_model::Table::new(4, 3);
    table.name = "Vendas".into();
    for (r, row) in [
        ["", "Jan", "Fev"],
        ["Norte", "10", "12,5"],
        ["Sul", "7", "9"],
    ]
    .iter()
    .enumerate()
    {
        for (c, text) in row.iter().enumerate() {
            table.type_text(r as u32, c as u32, text, &desk_table::Locale::pt_br());
        }
    }
    table.set_content(3, 1, Content::Formula("=SUM(B2:B3)".into()));
    table.set_content(3, 2, Content::Formula("=SUM(C2:C3)".into()));
    table.cell_mut(1, 1).format = Format::Number {
        decimals: Some(2),
        thousands: true,
        negative: Negative::Minus,
    };
    table.cell_mut(1, 0).style.fill = Some(CellFill::Color(Color::rgb(200, 30, 30)));
    table.cell_mut(1, 0).style.text.bold = true;
    table.cell_mut(2, 2).comment = Some("Conferir".into());
    table
        .merge(Area {
            row: 0,
            col: 1,
            rows: 1,
            cols: 2,
        })
        .unwrap();
    let mut chart = Chart::new(ChartKind::StackedBar);
    chart.title = Some("Vendas por região".into());
    chart.three_d = true;
    editor
        .edit("Insert", |edit| {
            let mut o = Object::new(
                edit.new_id(),
                ObjectKind::Table(Arc::new(table)),
                Frame::from_pt(72.0, 72.0, 216.0, 80.0),
            );
            o.anchor = Anchor::Page(0);
            edit.add_object(o)?;
            let mut o = Object::new(
                edit.new_id(),
                ObjectKind::Chart(Arc::new(chart)),
                Frame::from_pt(72.0, 200.0, 300.0, 200.0),
            );
            o.anchor = Anchor::Page(0);
            edit.add_object(o)
        })
        .unwrap();
    editor.document().clone()
}

fn table_of(document: &Document) -> desk_model::Table {
    document
        .objects()
        .iter()
        .find_map(|o| match &o.kind {
            ObjectKind::Table(t) => Some((**t).clone()),
            _ => None,
        })
        .expect("a table")
}

fn chart_of(document: &Document) -> Chart {
    document
        .objects()
        .iter()
        .find_map(|o| match &o.kind {
            ObjectKind::Chart(c) => Some((**c).clone()),
            _ => None,
        })
        .expect("a chart")
}

#[test]
fn tab_01_grf_01_tables_and_charts_survive_odt() {
    let before = document();
    let bytes = write_odt(&before).unwrap();
    let after = read_odt(&bytes).unwrap();
    assert_eq!(table_of(&after), table_of(&before));
    assert_eq!(chart_of(&after), chart_of(&before));
}

/// The objects of a document of ODF, those in groups among them.
fn objects(document: &mut odf::TextDocument) -> Vec<&mut odf::DrawObject> {
    let mut out: Vec<&mut odf::DrawObject> = document.page_objects.iter_mut().collect();
    for p in &mut document.body {
        out.extend(p.frames.iter_mut());
    }
    out
}

#[test]
fn iop_01_other_programs_read_the_cells_and_the_chart() {
    let bytes = write_odt(&document()).unwrap();
    let mut plain = odf::read_odt(&bytes).unwrap();
    // As another program keeps it: without what only Desk reads.
    for o in objects(&mut plain) {
        match &mut o.kind {
            odf::DrawKind::Table(t) => t.desk = None,
            odf::DrawKind::Object { desk, .. } => *desk = None,
            _ => {}
        }
    }
    assert_eq!(plain.charts.len(), 1);
    let chart = &plain.charts[0].1;
    assert_eq!(chart.class, "bar");
    assert!(chart.horizontal && chart.stacked && chart.three_d);
    assert_eq!(chart.series.len(), 2);
    assert_eq!(chart.categories, ["2007", "2008", "2009", "2010"]);
    let after = read_odt(&odf::write_odt(&plain).unwrap()).unwrap();
    let table = table_of(&after);
    assert_eq!(table.name, "Vendas");
    assert_eq!(table.cell(1, 0).content, Content::Text("Norte".into()));
    assert_eq!(table.cell(1, 2).content, Content::Number(Num(12.5)));
    assert_eq!(
        table.cell(3, 1).content,
        Content::Formula("=SUM(B2:B3)".into())
    );
    assert_eq!(
        table.span(0, 1),
        Area {
            row: 0,
            col: 1,
            rows: 1,
            cols: 2
        }
    );
    assert!(table.cell(1, 0).style.text.bold);
    assert_eq!(
        table.cell(1, 0).style.fill,
        Some(CellFill::Color(Color::rgb(200, 30, 30)))
    );
    assert_eq!(table.cell(2, 2).comment.as_deref(), Some("Conferir"));
    assert_eq!(table.header_rows(), 1);
    let chart = chart_of(&after);
    assert_eq!(chart.kind, ChartKind::StackedBar);
    assert_eq!(chart.title.as_deref(), Some("Vendas por região"));
    assert_eq!(chart.series_count(), 2);
    assert_eq!(chart.value(1, 3), Some(58.0));
}

/// The table of [`document`] with a footer that sums its columns by the
/// names of their headers.
fn by_header_names() -> Document {
    let document = document();
    let mut object = document
        .objects()
        .iter()
        .find(|o| matches!(o.kind, ObjectKind::Table(_)))
        .cloned()
        .expect("a table");
    let mut table = table_of(&document);
    // The headers of their own, not merged as in the other tests.
    table.unmerge(0, 1);
    let pt = desk_table::Locale::pt_br();
    table.type_text(0, 1, "Jan", &pt);
    table.type_text(0, 2, "Fev", &pt);
    table.set_footer_rows(1).unwrap();
    table.set_content(3, 1, Content::Formula("=SUM(Jan)".into()));
    table.set_content(3, 2, Content::Formula("=SUM('fev')".into()));
    object.kind = ObjectKind::Table(Arc::new(table));
    let mut editor = Editor::new(document);
    editor
        .edit("Formulas", |edit| edit.replace_object(object))
        .unwrap();
    editor.document().clone()
}

#[test]
fn iop_15_formulas_that_name_headers_keep_the_names_and_compute() {
    let before = by_header_names();
    let after = read_odt(&write_odt(&before).unwrap()).unwrap();
    let table = table_of(&after);
    // The names are kept as they were typed, not turned into ranges,
    // and are found whatever their case.
    assert_eq!(
        table.cell(3, 1).content,
        Content::Formula("=SUM(Jan)".into())
    );
    assert_eq!(
        table.cell(3, 2).content,
        Content::Formula("=SUM('fev')".into())
    );
    let values = table.values(0.0);
    assert_eq!(values.get(3, 1), &desk_table::Value::Number(17.0));
    assert_eq!(values.get(3, 2), &desk_table::Value::Number(21.5));
}
