// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Tables (`table:table`): those in the text of a document, and those in
//! a frame, as Desk writes the tables that float on a page.

use crate::ns::{DESK, OFFICE, STYLE, TABLE, TEXT};
use crate::text::{Border, Paragraph};
use crate::units::{Color, Length};
use crate::xml::{Element, Writer};

/// The most columns read, so that a hostile file cannot take all the
/// memory with repeated columns.
pub const MAX_COLUMNS: usize = 1024;

/// The most rows read.
pub const MAX_ROWS: usize = 10_000;

/// The most cells read in the tables of one document, all together. The
/// limits of rows and columns hold for each table, and a few hundred bytes
/// that repeat a row of repeated cells in many tables would still clone
/// millions of cells.
pub const MAX_DOCUMENT_CELLS: usize = 1 << 20;

thread_local! {
    /// The cells the tables of the document read now may still clone.
    static CELLS_LEFT: std::cell::Cell<usize> = const { std::cell::Cell::new(MAX_DOCUMENT_CELLS) };
}

/// Starts the count of cells of a new document.
pub(crate) fn start_document() {
    CELLS_LEFT.set(MAX_DOCUMENT_CELLS);
}

/// How many of `wanted` copies of a row of `cells` cells the document may
/// still read, which are then counted.
fn take_rows(wanted: usize, cells: usize) -> usize {
    let cells = cells.max(1);
    let left = CELLS_LEFT.get();
    let n = wanted.min(left / cells);
    CELLS_LEFT.set(left - n * cells);
    n
}

/// A table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// `table:name`.
    pub name: Option<String>,
    /// `table:style-name`.
    pub style: Option<String>,
    /// The columns, with their repetitions written out.
    pub columns: Vec<TableColumn>,
    /// How many of the first columns are in `table:table-header-columns`.
    pub header_columns: usize,
    /// How many of the first rows are in `table:table-header-rows`.
    pub header_rows: usize,
    /// The rows, with their repetitions written out.
    pub rows: Vec<TableRow>,
    /// `desk:table`: all that Desk keeps of the table, which the rest
    /// shows to other programs.
    pub desk: Option<String>,
}

/// A column of a table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableColumn {
    /// `table:style-name`, of the family `table-column`.
    pub style: Option<String>,
    /// `table:default-cell-style-name`.
    pub default_cell_style: Option<String>,
}

/// A row of a table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableRow {
    /// `table:style-name`, of the family `table-row`.
    pub style: Option<String>,
    /// The cells, with their repetitions written out.
    pub cells: Vec<TableCell>,
}

/// A cell of a table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableCell {
    /// Whether it is a `table:covered-table-cell`: one that a merged cell
    /// covers.
    pub covered: bool,
    /// `table:style-name`, of the family `table-cell`.
    pub style: Option<String>,
    /// `office:value-type`: `float`, `percentage`, `currency`, `date`,
    /// `time`, `boolean` or `string`.
    pub value_type: Option<String>,
    /// The value of the attribute that the type names: `office:value`,
    /// `office:date-value`, `office:time-value`, `office:boolean-value`
    /// or `office:string-value`.
    pub value: Option<String>,
    /// `office:currency`, as `BRL`.
    pub currency: Option<String>,
    /// `table:formula`, as written, as `of:=SUM([.A1:.A3])`.
    pub formula: Option<String>,
    /// `table:number-columns-spanned`.
    pub columns_spanned: u32,
    /// `table:number-rows-spanned`.
    pub rows_spanned: u32,
    /// The text.
    pub paragraphs: Vec<Paragraph>,
    /// Its comment, its `office:annotation`, and the replies to it, each a
    /// `desk:annotation` of the cell that names the comment in
    /// `desk:parent-name`, since a cell has one `office:annotation`
    /// (odf-extensions.md).
    pub annotations: Vec<crate::Annotation>,
}

impl TableCell {
    /// The plain text of the cell, its paragraphs joined by line breaks.
    pub fn text(&self) -> String {
        self.paragraphs
            .iter()
            .map(Paragraph::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The table properties of a style, of the families `table`,
/// `table-column`, `table-row` and `table-cell`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableProperties {
    /// `style:width` of a table.
    pub width: Option<Length>,
    /// `style:column-width` of a column.
    pub column_width: Option<Length>,
    /// `style:row-height` of a row.
    pub row_height: Option<Length>,
    /// `style:min-row-height` of a row.
    pub min_row_height: Option<Length>,
    /// `fo:background-color` of a cell.
    pub background: Option<Color>,
    /// `fo:border-top` of a cell.
    pub border_top: Option<Border>,
    /// `fo:border-bottom` of a cell.
    pub border_bottom: Option<Border>,
    /// `fo:border-left` of a cell.
    pub border_left: Option<Border>,
    /// `fo:border-right` of a cell.
    pub border_right: Option<Border>,
    /// `fo:padding` of a cell.
    pub padding: Option<Length>,
    /// `style:vertical-align` of a cell: `top`, `middle` or `bottom`.
    pub vertical_align: Option<String>,
}

fn count(e: &Element, name: &str) -> usize {
    e.attr(TABLE, name)
        .and_then(|n| n.trim().parse::<usize>().ok())
        .unwrap_or(1)
        .max(1)
}

/// Reads a `table:table`. `paragraph` reads the paragraphs of a cell.
pub(crate) fn read_table(e: &Element, paragraph: &dyn Fn(&Element) -> Paragraph) -> Table {
    let mut table = Table {
        name: e.attr(TABLE, "name").map(str::to_owned),
        style: e.attr(TABLE, "style-name").map(str::to_owned),
        desk: e.attr(DESK, "table").map(str::to_owned),
        ..Table::default()
    };
    read_parts(e, &mut table, paragraph, false, false);
    // Repeated cells and columns past the last that holds anything, as
    // spreadsheets write to fill the sheet, are left out.
    let used = table
        .rows
        .iter()
        .map(|r| r.cells.len())
        .max()
        .unwrap_or(0)
        .max(1);
    table.columns.truncate(used.max(table.header_columns));
    while table.columns.len() < used {
        table.columns.push(TableColumn::default());
    }
    for row in &mut table.rows {
        while row.cells.len() < used {
            row.cells.push(TableCell::default());
        }
    }
    table
}

fn read_parts(
    e: &Element,
    table: &mut Table,
    paragraph: &dyn Fn(&Element) -> Paragraph,
    header_columns: bool,
    header_rows: bool,
) {
    for child in e.elements() {
        if child.name.ns != TABLE {
            continue;
        }
        match child.name.local.as_str() {
            "table-column" => {
                let column = TableColumn {
                    style: child.attr(TABLE, "style-name").map(str::to_owned),
                    default_cell_style: child
                        .attr(TABLE, "default-cell-style-name")
                        .map(str::to_owned),
                };
                let n = count(child, "number-columns-repeated")
                    .min(MAX_COLUMNS - table.columns.len().min(MAX_COLUMNS));
                for _ in 0..n {
                    table.columns.push(column.clone());
                }
                if header_columns {
                    table.header_columns = table.columns.len();
                }
            }
            "table-columns" | "table-column-group" => {
                read_parts(child, table, paragraph, false, header_rows)
            }
            "table-header-columns" => read_parts(child, table, paragraph, true, header_rows),
            "table-rows" | "table-row-group" => {
                read_parts(child, table, paragraph, header_columns, false)
            }
            "table-header-rows" => read_parts(child, table, paragraph, header_columns, true),
            "table-row" => {
                let row = read_row(child, paragraph);
                let empty = row.cells.iter().all(|c| c.paragraphs.is_empty());
                let mut n = count(child, "number-rows-repeated");
                // Spreadsheets fill the sheet with a row repeated a
                // million times.
                if empty && n > 1 {
                    n = n.min(2);
                }
                let n = n.min(MAX_ROWS - table.rows.len().min(MAX_ROWS));
                let n = take_rows(n, row.cells.len());
                for _ in 0..n {
                    table.rows.push(row.clone());
                }
                if header_rows {
                    table.header_rows = table.rows.len();
                }
            }
            _ => {}
        }
    }
}

fn read_row(e: &Element, paragraph: &dyn Fn(&Element) -> Paragraph) -> TableRow {
    let mut row = TableRow {
        style: e.attr(TABLE, "style-name").map(str::to_owned),
        cells: Vec::new(),
    };
    for c in e.elements() {
        let covered = c.name.is(TABLE, "covered-table-cell");
        if !covered && !c.name.is(TABLE, "table-cell") {
            continue;
        }
        let value_type = c
            .attr(OFFICE, "value-type")
            .or_else(|| c.attr(crate::ns::CALCEXT, "value-type"))
            .map(str::to_owned);
        let value = match value_type.as_deref() {
            Some("date") => c.attr(OFFICE, "date-value"),
            Some("time") => c.attr(OFFICE, "time-value"),
            Some("boolean") => c.attr(OFFICE, "boolean-value"),
            Some("string") => c.attr(OFFICE, "string-value"),
            Some(_) => c.attr(OFFICE, "value"),
            None => None,
        }
        .map(str::to_owned);
        let cell = TableCell {
            covered,
            style: c.attr(TABLE, "style-name").map(str::to_owned),
            value_type,
            value,
            currency: c.attr(OFFICE, "currency").map(str::to_owned),
            formula: c.attr(TABLE, "formula").map(str::to_owned),
            columns_spanned: c
                .attr(TABLE, "number-columns-spanned")
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(1),
            rows_spanned: c
                .attr(TABLE, "number-rows-spanned")
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(1),
            paragraphs: c
                .elements()
                .filter(|p| p.name.is(TEXT, "p") || p.name.is(TEXT, "h"))
                .map(paragraph)
                .collect(),
            annotations: c
                .elements()
                .filter(|a| a.name.is(OFFICE, "annotation") || a.name.is(DESK, "annotation"))
                .map(crate::read::read_annotation)
                .collect(),
        };
        let empty = cell.paragraphs.is_empty() && cell.value.is_none();
        let mut n = count(c, "number-columns-repeated");
        if empty && n > 1 {
            n = n.min(2);
        }
        let n = n.min(MAX_COLUMNS - row.cells.len().min(MAX_COLUMNS));
        for _ in 0..n {
            row.cells.push(cell.clone());
        }
    }
    // Empty cells at the end of a row stand for nothing.
    while row
        .cells
        .last()
        .is_some_and(|c| c.paragraphs.is_empty() && c.value.is_none() && c.style.is_none())
        && row.cells.len() > 1
    {
        row.cells.pop();
    }
    row
}

/// Writes a `table:table`. `paragraph` writes the paragraphs of a cell.
pub(crate) fn write_table(
    w: &mut Writer,
    table: &Table,
    paragraph: &dyn Fn(&mut Writer, &Paragraph),
) {
    w.start("table:table");
    w.attr_opt("table:name", table.name.as_deref());
    w.attr_opt("table:style-name", table.style.as_deref());
    w.attr_opt("desk:table", table.desk.as_deref());
    let header_columns = table.header_columns.min(table.columns.len());
    for (i, column) in table.columns.iter().enumerate() {
        if i == 0 && header_columns > 0 {
            w.start("table:table-header-columns");
        }
        w.start("table:table-column");
        w.attr_opt("table:style-name", column.style.as_deref());
        w.attr_opt(
            "table:default-cell-style-name",
            column.default_cell_style.as_deref(),
        );
        w.end();
        if i + 1 == header_columns {
            w.end();
        }
    }
    let header_rows = table.header_rows.min(table.rows.len());
    for (i, row) in table.rows.iter().enumerate() {
        if i == 0 && header_rows > 0 {
            w.start("table:table-header-rows");
        }
        w.start("table:table-row");
        w.attr_opt("table:style-name", row.style.as_deref());
        for cell in &row.cells {
            write_cell(w, cell, paragraph);
        }
        w.end();
        if i + 1 == header_rows {
            w.end();
        }
    }
    w.end();
}

fn write_cell(w: &mut Writer, cell: &TableCell, paragraph: &dyn Fn(&mut Writer, &Paragraph)) {
    w.start(if cell.covered {
        "table:covered-table-cell"
    } else {
        "table:table-cell"
    });
    w.attr_opt("table:style-name", cell.style.as_deref());
    if !cell.covered {
        if cell.columns_spanned > 1 {
            w.attr(
                "table:number-columns-spanned",
                &cell.columns_spanned.to_string(),
            );
        }
        if cell.rows_spanned > 1 {
            w.attr("table:number-rows-spanned", &cell.rows_spanned.to_string());
        }
    }
    w.attr_opt("table:formula", cell.formula.as_deref());
    if let Some(kind) = &cell.value_type {
        w.attr("office:value-type", kind);
        w.attr_opt("office:currency", cell.currency.as_deref());
        if let Some(value) = &cell.value {
            let name = match kind.as_str() {
                "date" => "office:date-value",
                "time" => "office:time-value",
                "boolean" => "office:boolean-value",
                "string" => "office:string-value",
                _ => "office:value",
            };
            w.attr(name, value);
        }
    }
    for (n, annotation) in cell.annotations.iter().enumerate() {
        crate::write::write_annotation(w, n > 0, annotation);
    }
    for p in &cell.paragraphs {
        paragraph(w, p);
    }
    w.end();
}

/// Reads the table properties of a style.
pub(crate) fn table_properties(
    style: &Element,
    border: &dyn Fn(&str) -> Option<Border>,
) -> TableProperties {
    let length =
        |e: Option<&Element>, ns, name| e.and_then(|e| e.attr(ns, name)).and_then(Length::parse);
    let table = style.child(STYLE, "table-properties");
    let column = style.child(STYLE, "table-column-properties");
    let row = style.child(STYLE, "table-row-properties");
    let cell = style.child(STYLE, "table-cell-properties");
    let fo = crate::ns::FO;
    let side = |name: &str| {
        let e = cell?;
        e.attr(fo, &format!("border-{name}"))
            .or_else(|| e.attr(fo, "border"))
            .and_then(border)
    };
    TableProperties {
        width: length(table, STYLE, "width"),
        column_width: length(column, STYLE, "column-width"),
        row_height: length(row, STYLE, "row-height"),
        min_row_height: length(row, STYLE, "min-row-height"),
        background: cell
            .and_then(|c| c.attr(fo, "background-color"))
            .and_then(Color::parse),
        border_top: side("top"),
        border_bottom: side("bottom"),
        border_left: side("left"),
        border_right: side("right"),
        padding: length(cell, fo, "padding").or_else(|| length(cell, fo, "padding-left")),
        vertical_align: cell
            .and_then(|c| c.attr(STYLE, "vertical-align"))
            .map(str::to_owned),
    }
}

/// Writes the table properties of a style, before its paragraph and text
/// properties.
pub(crate) fn write_table_properties(
    w: &mut Writer,
    t: &TableProperties,
    border: &dyn Fn(Border) -> String,
) {
    if let Some(width) = t.width {
        w.start("style:table-properties")
            .attr("style:width", &width.to_string())
            .end();
    }
    if let Some(width) = t.column_width {
        w.start("style:table-column-properties")
            .attr("style:column-width", &width.to_string())
            .end();
    }
    if t.row_height.is_some() || t.min_row_height.is_some() {
        w.start("style:table-row-properties");
        if let Some(height) = t.row_height {
            w.attr("style:row-height", &height.to_string());
        }
        if let Some(height) = t.min_row_height {
            w.attr("style:min-row-height", &height.to_string());
        }
        w.end();
    }
    let sides = [
        ("fo:border-top", t.border_top),
        ("fo:border-bottom", t.border_bottom),
        ("fo:border-left", t.border_left),
        ("fo:border-right", t.border_right),
    ];
    if t.background.is_some()
        || t.padding.is_some()
        || t.vertical_align.is_some()
        || sides.iter().any(|(_, b)| b.is_some())
    {
        w.start("style:table-cell-properties");
        if let Some(color) = t.background {
            w.attr("fo:background-color", &color.to_string());
        }
        for (name, side) in sides {
            if let Some(side) = side {
                w.attr(name, &border(side));
            }
        }
        if let Some(padding) = t.padding {
            w.attr("fo:padding", &padding.to_string());
        }
        w.attr_opt("style:vertical-align", t.vertical_align.as_deref());
        w.end();
    }
}

/// A table template of `office:styles` (`table:table-template`): a named
/// look of tables, as the table styles of Desk keep it, by the cell styles
/// of its parts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableTemplate {
    /// `table:name`.
    pub name: String,
    /// The cell style of the first row (`table:first-row`).
    pub first_row: Option<String>,
    /// The cell style of the last row (`table:last-row`).
    pub last_row: Option<String>,
    /// The cell style of the first column (`table:first-column`).
    pub first_column: Option<String>,
    /// The cell style of the body (`table:body`).
    pub body: Option<String>,
    /// The cell style of every other row (`table:even-rows`).
    pub even_rows: Option<String>,
}

/// The parts of a table template, by element, with how to reach them.
type TemplatePart = (&'static str, fn(&TableTemplate) -> &Option<String>);

/// The parts of a table template that Desk reads and writes, in the
/// order of the schema.
const TEMPLATE_PARTS: [TemplatePart; 5] = [
    ("table:first-row", |t| &t.first_row),
    ("table:last-row", |t| &t.last_row),
    ("table:first-column", |t| &t.first_column),
    ("table:body", |t| &t.body),
    ("table:even-rows", |t| &t.even_rows),
];

/// Reads a `table:table-template`, or none for another element.
pub(crate) fn table_template(e: &Element) -> Option<TableTemplate> {
    if !e.name.is(TABLE, "table-template") {
        return None;
    }
    // ODF 1.2 named the template with text:style-name.
    let name = e
        .attr(TABLE, "name")
        .or_else(|| e.attr(TEXT, "style-name"))?;
    let part = |local: &str| {
        e.child(TABLE, local)
            .and_then(|p| p.attr(TABLE, "style-name"))
            .map(str::to_owned)
    };
    Some(TableTemplate {
        name: name.to_owned(),
        first_row: part("first-row"),
        last_row: part("last-row"),
        first_column: part("first-column"),
        body: part("body"),
        even_rows: part("even-rows"),
    })
}

/// Writes a `table:table-template`.
pub(crate) fn write_table_template(w: &mut Writer, template: &TableTemplate) {
    // The corners belong to the header and footer rows, as Desk draws
    // them; ODF 1.3 asks to say so.
    w.start("table:table-template")
        .attr("table:name", &template.name)
        .attr("table:first-row-start-column", "row")
        .attr("table:first-row-end-column", "row")
        .attr("table:last-row-start-column", "row")
        .attr("table:last-row-end-column", "row");
    for (name, part) in TEMPLATE_PARTS {
        if let Some(style) = part(template) {
            w.start(name).attr("table:style-name", style);
            w.end();
        }
    }
    w.end();
}
