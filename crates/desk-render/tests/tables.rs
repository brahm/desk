// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables and charts on the pages, drawn and written as PDF.

use std::process::Command;
use std::sync::Arc;

use desk_model::table::{Area, Border, BorderSet, Color, Format, Locale, Negative};
use desk_model::{
    Anchor, Chart, Document, Editor, Frame, Metadata, Object, ObjectKind, PageSetup, Table, Wrap,
};
use desk_render::{DisplayList, Painter};
use desk_text::TextSystem;

fn text_of(pdf: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!("desk-render-tables-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.pdf");
    std::fs::write(&path, pdf).unwrap();
    let output = Command::new("pdftotext")
        .arg(&path)
        .arg("-")
        .output()
        .expect("pdftotext is needed for this test (poppler-utils)");
    String::from_utf8(output.stdout).unwrap()
}

fn document() -> Editor {
    let pt = Locale::pt_br();
    let mut table = Table::new(4, 3);
    table.type_text(0, 1, "2008", &pt);
    table.type_text(0, 2, "2009", &pt);
    table.type_text(1, 0, "Norte", &pt);
    table.type_text(2, 0, "Sul", &pt);
    table.type_text(3, 0, "Total", &pt);
    table.type_text(1, 1, "1234,5", &pt);
    table.type_text(2, 1, "-20", &pt);
    table.type_text(1, 2, "R$ 10,50", &pt);
    table.type_text(2, 2, "3%", &pt);
    table.type_text(3, 1, "=SUM(B2:B3)", &pt);
    table.set_footer_rows(1).unwrap();
    table.cell_mut(1, 1).format = Format::Number {
        decimals: Some(2),
        thousands: true,
        negative: Negative::Parentheses,
    };
    table.cell_mut(2, 1).format = table.cell(1, 1).format.clone();
    table.cell_mut(3, 1).format = table.cell(1, 1).format.clone();
    table.set_borders(
        Area::between((0, 0), (3, 2)),
        BorderSet::Outside,
        Some(Border::solid(2_000, Color::BLACK)),
    );
    table.style.alternating = Some(Color::rgb(0xee, 0xee, 0xee));
    let mut chart = Chart::new(desk_model::chart::ChartKind::Column);
    chart.title = Some("Vendas por região".into());
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Table and chart", |edit| {
            edit.set_page_setup(PageSetup::a4())?;
            let mut t = Object::new(
                edit.new_id(),
                ObjectKind::Table(Arc::new(table.clone())),
                Frame::from_pt(72.0, 72.0, 216.0, 80.0),
            );
            t.anchor = Anchor::Page(0);
            t.wrap = Wrap::Through;
            edit.add_object(t)?;
            let mut c = Object::new(
                edit.new_id(),
                ObjectKind::Chart(Arc::new(chart.clone())),
                Frame::from_pt(72.0, 200.0, 320.0, 220.0),
            );
            c.anchor = Anchor::Page(0);
            c.wrap = Wrap::Through;
            edit.add_object(c)
        })
        .unwrap();
    editor
}

#[test]
fn tab_01_tab_28_grf_01_tables_and_charts_are_drawn() {
    let editor = document();
    let mut system = TextSystem::new();
    let layout = desk_layout::paginate(editor.document(), &mut system);
    let page = &layout.pages[0];
    let table = page
        .objects
        .iter()
        .find_map(|o| o.table.clone())
        .expect("the table is laid out");
    assert_eq!(table.cols.len(), 4);
    assert!((table.width() - 216.0).abs() < 0.01);
    assert!(table.height() >= 80.0);
    assert!(page.objects.iter().any(|o| o.chart.is_some()));
    let pages = DisplayList::pages(&layout);
    let pdf = Painter::new()
        .unwrap()
        .pdf_bytes(&pages, &Metadata::default())
        .unwrap();
    let text = text_of(&pdf);
    if let Ok(dir) = std::env::var("DESK_SNAPSHOTS") {
        let png = Painter::new()
            .unwrap()
            .thumbnail_png(&pages[0], 1200)
            .unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("tables-and-charts.png"),
            png,
        )
        .unwrap();
    }
    // Numbers are written as the language of the document says.
    let locale = desk_layout::document_locale(editor.document());
    let shown = |v: f64| {
        Format::Number {
            decimals: Some(2),
            thousands: true,
            negative: Negative::Parentheses,
        }
        .show(&desk_model::table::Value::Number(v), &locale)
        .text
    };
    let (a, b, total) = (shown(1234.5), shown(-20.0), shown(1214.5));
    for expected in [
        "Norte",
        &a,
        &b,
        &total,
        "R$",
        "3%",
        "Vendas por região",
        "Região 1",
        "2010",
    ] {
        if !text.contains(expected) {
            eprintln!("TEXT<<{text}>>");
        }
        assert!(text.contains(expected), "{expected} missing");
    }
}

#[test]
fn grf_01_grf_20_every_kind_of_chart_draws_in_2d_and_3d() {
    use desk_model::chart::ChartKind;
    let mut editor = Editor::new(Document::new_word_processing());
    editor
        .edit("Charts", |edit| {
            edit.set_page_setup(PageSetup {
                width: desk_model::Length::from_points(900),
                height: desk_model::Length::from_points(1200),
                ..PageSetup::default()
            })?;
            for (i, kind) in ChartKind::ALL.iter().enumerate() {
                for three_d in [false, true] {
                    if three_d && !kind.has_3d() {
                        continue;
                    }
                    let mut chart = Chart::new(*kind);
                    chart.three_d = three_d;
                    chart.title = Some(format!("{kind:?}"));
                    chart.series[0].labels.show = true;
                    let x =
                        if three_d { 450.0 } else { 0.0 } + if i % 2 == 0 { 0.0 } else { 225.0 };
                    let y = (i / 2) as f64 * 165.0;
                    let mut c = Object::new(
                        edit.new_id(),
                        ObjectKind::Chart(Arc::new(chart)),
                        Frame::from_pt(x + 5.0, y + 5.0, 215.0, 155.0),
                    );
                    c.anchor = Anchor::Page(0);
                    c.wrap = Wrap::Through;
                    edit.add_object(c)?;
                }
            }
            Ok(())
        })
        .unwrap();
    let layout = desk_layout::paginate(editor.document(), &mut TextSystem::new());
    let pages = DisplayList::pages(&layout);
    let charts = layout.pages[0]
        .objects
        .iter()
        .filter(|o| o.chart.is_some())
        .count();
    // Each kind in 2D, and in 3D those that have it.
    let kinds = ChartKind::ALL.len() + ChartKind::ALL.iter().filter(|k| k.has_3d()).count();
    assert_eq!(charts, kinds);
    assert_eq!(kinds, 22);
    if let Ok(dir) = std::env::var("DESK_SNAPSHOTS") {
        let png = Painter::new()
            .unwrap()
            .thumbnail_png(&pages[0], 1800)
            .unwrap();
        std::fs::write(std::path::Path::new(&dir).join("charts.png"), png).unwrap();
    }
}
