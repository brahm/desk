// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables laid out: the fills of the parts of a table and of alternating
//! rows, text that does not fit in its cell, comments and sizes.

use desk_layout::{TableLayout, lay_out_table, table_size};
use desk_model::table::{Color, Locale};
use desk_model::{Document, Table};
use desk_text::TextSystem;

fn lay_out(table: &Table) -> TableLayout {
    let document = Document::new_word_processing();
    lay_out_table(
        &document,
        table,
        &mut TextSystem::new(),
        &Locale::pt_br(),
        0.0,
    )
}

fn cell(layout: &TableLayout, row: u32, col: u32) -> &desk_layout::CellLayout {
    layout
        .cells
        .iter()
        .find(|c| c.row == row && c.col == col)
        .expect("the cell is laid out")
}

#[test]
fn tab_14_alternating_rows_take_their_color() {
    let mut table = Table::new(5, 3);
    let gray = Color::rgb(0xee, 0xee, 0xee);
    table.style.alternating = Some(gray);
    let layout = lay_out(&table);
    // The first row of the body keeps its fill, the second takes the
    // color, the third does not.
    let fills: Vec<bool> = (1..4)
        .map(|r| {
            cell(&layout, r, 1)
                .fill
                .is_some_and(|f| f.r == 0xee && f.g == 0xee)
        })
        .collect();
    assert_eq!(fills, [false, true, false]);
}

#[test]
fn tab_20_text_too_long_for_its_cell_is_marked() {
    let mut table = Table::new(3, 3);
    let pt = Locale::pt_br();
    table.type_text(1, 1, &"palavra ".repeat(40), &pt);
    // Text that does not wrap runs into the empty cell beside it, and is
    // cut with the mark when that cell is not empty.
    let mut cut = table.clone();
    cut.cell_mut(1, 1).style.wrap = false;
    assert!(!cell(&lay_out(&cut), 1, 1).overflow);
    cut.type_text(1, 2, "7", &pt);
    assert!(cell(&lay_out(&cut), 1, 1).overflow);
    // With the text wrapped the row grows to hold it.
    let wrapped = lay_out(&table);
    let short = lay_out(&Table::new(3, 3));
    let height = |l: &TableLayout| l.rows[2] - l.rows[1];
    assert!(height(&wrapped) > height(&short) * 2.0);
    assert!(!cell(&wrapped, 1, 1).overflow);
}

#[test]
fn tab_27_cells_with_a_comment_show_its_mark() {
    let mut table = Table::new(3, 3);
    table.cell_mut(2, 2).comment = Some("Conferir com o balanço".into());
    let layout = lay_out(&table);
    assert!(cell(&layout, 2, 2).comment);
    assert!(!cell(&layout, 1, 1).comment);
}

#[test]
fn tab_03_a_table_scaled_by_its_handle_keeps_its_proportions() {
    let document = Document::new_word_processing();
    let mut system = TextSystem::new();
    let mut table = Table::new(3, 3);
    table.set_width(0, 40_000);
    let (w, _) = table_size(&document, &table, &mut system);
    table.scale_width((w * 2000.0) as u32);
    let (w2, _) = table_size(&document, &table, &mut system);
    assert!((w2 - 2.0 * w).abs() < 1.0, "{w} {w2}");
    assert_eq!(table.width(1), table.width(2));
    assert!(table.width(0) < table.width(1));
}

#[test]
fn tab_04_each_part_of_the_table_takes_the_fill_of_its_style() {
    let mut table = Table::new(5, 3);
    table.set_footer_rows(1).unwrap();
    let (header_row, header_col, footer, body) = (
        Color::rgb(0x10, 0x20, 0x30),
        Color::rgb(0x40, 0x50, 0x60),
        Color::rgb(0x70, 0x80, 0x90),
        Color::rgb(0xa0, 0xb0, 0xc0),
    );
    table.style.header_row_fill = Some(header_row);
    table.style.header_column_fill = Some(header_col);
    table.style.footer_fill = Some(footer);
    table.style.body_fill = Some(body);
    // A cell with a fill of its own keeps it over the style.
    let own = Color::rgb(0xff, 0x00, 0x00);
    table.cell_mut(2, 2).style.fill = Some(desk_model::table::CellFill::Color(own));
    let layout = lay_out(&table);
    let fill = |r, c| cell(&layout, r, c).fill.map(|f| (f.r, f.g, f.b));
    let rgb = |c: Color| Some((c.r, c.g, c.b));
    assert_eq!(fill(0, 1), rgb(header_row));
    assert_eq!(fill(2, 0), rgb(header_col));
    assert_eq!(fill(4, 1), rgb(footer));
    assert_eq!(fill(1, 1), rgb(body));
    assert_eq!(fill(2, 2), rgb(own));
}

#[test]
fn pxv_15_the_title_shows_centered_and_outlined() {
    let mut table = Table::new(3, 3);
    table.name = "Table 1".into();
    table.options.show_name = true;
    // As the name of Desk Tables: at the left, with no line around it.
    let left = lay_out(&table);
    let first = &left.name[0];
    assert!(first.origin.0 + first.line.x < 1.0);
    assert_eq!(left.name_outline, None);
    // As the title of Pages 15: in the middle, the line around it with
    // Outline Table Title, and the cells below both.
    table.options.name_centered = true;
    table.options.name_outline = true;
    let titled = lay_out(&table);
    let line = &titled.name[0];
    let (start, end) = (
        line.origin.0 + line.line.x,
        line.origin.0 + line.line.x + line.line.width,
    );
    let width = titled.width();
    assert!(((start + end) / 2.0 - width / 2.0).abs() < 2.0);
    let (x, y, w, h) = titled.name_outline.expect("a line around the title");
    assert_eq!((x, y, w), (0.0, 0.0, width));
    assert!(h > 0.0 && h <= titled.rows[0]);
}
