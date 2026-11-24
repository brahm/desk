// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! The tables of a Pages document (`sf:tabular-info`, with its
//! `sf:tabular-model`), as the documents that the project made with Pages
//! '09 save them (B-048, TAB-01).
//!
//! The grid (`sf:grid`) gives the number of rows and columns, the width
//! of each column and the height of each row. Its cells (`sf:datasource`)
//! come row by row, one element for each: `sf:t` text, `sf:n` a number,
//! `sf:d` a date, `sf:du` a length of time, `sf:f` a formula with its
//! result, `sf:g` an empty cell and `sf:s` a cell that a merged cell
//! covers. A cell is in the column after the one before it, unless its
//! `sf:ct` says how many columns to go, as a byte: 254 is two columns
//! back, and a cell in a column at or before the one of the cell before it
//! is in the next row. The cells after the last are empty.

use std::collections::HashMap;

use crate::ns::{SF, SFA, SL};
use crate::sf::{Color, Ids, Properties, Stroke, Value};
use crate::xml::Element;

/// The styles that tables refer to by identifier, by the local name of
/// their element and their identifier: cell, paragraph, layout, vector
/// and table styles.
pub(super) type Styles<'a> = HashMap<(&'a str, &'a str), &'a Element>;

/// How many parent styles are followed, so that a cycle ends.
const MAX_PARENTS: usize = 16;

/// Seconds from 1 January 2001, from which Pages counts dates, to the
/// days from 30 December 1899, from which tables count them.
const DAYS_TO_2001: f64 = 36_892.0;

/// What a cell of a table holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CellContent {
    /// Nothing (`sf:g`), or a cell that a merged cell covers (`sf:s`).
    #[default]
    Empty,
    /// Text (`sf:t`), its paragraphs joined by line feeds.
    Text(String),
    /// A number (`sf:n`).
    Number(f64),
    /// A date (`sf:d`), in days from 30 December 1899.
    Date(f64),
    /// A length of time (`sf:du`), in days.
    Duration(f64),
    /// A formula (`sf:f`), as written, with the `=`.
    Formula(String),
}

/// A cell of a table that holds something or starts a merged cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableCell {
    /// The row, from 0.
    pub row: u32,
    /// The column, from 0.
    pub col: u32,
    /// What it holds.
    pub content: CellContent,
    /// `sf:row-span`: how many rows it takes, merged.
    pub rows: u32,
    /// `sf:col-span`: how many columns it takes, merged.
    pub cols: u32,
}

/// A table (`sf:tabular-model`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    /// `sf:name`.
    pub name: String,
    /// `sf:name-is-visible`.
    pub name_shown: bool,
    /// `sf:num-header-rows`.
    pub header_rows: u32,
    /// `sf:num-header-columns`.
    pub header_cols: u32,
    /// `sf:num-footer-rows`.
    pub footer_rows: u32,
    /// The width of each column, in points.
    pub widths: Vec<f64>,
    /// The height of each row, in points.
    pub heights: Vec<f64>,
    /// The cells that hold something or are merged, row by row.
    pub cells: Vec<TableCell>,
    /// The cells with comments (`sl:cell-annotations`): the row, the
    /// column and the object ID that the comment names.
    pub comments: Vec<(u32, u32, String)>,
    /// The elements of cells that the reader does not read, by name.
    pub skipped: Vec<String>,
    /// The looks of the cells, each once (B-049).
    pub looks: Vec<CellLook>,
    /// The look of each cell, row by row, by its place in `looks`.
    pub cell_looks: Vec<Option<usize>>,
    /// The looks of the parts of the table that its table style gives,
    /// by their place in `looks`: the header rows, the header columns,
    /// the footer rows and the body.
    pub part_looks: [Option<usize>; 4],
    /// The lines across, above each row and below the last, a line for
    /// each column: none where no line is drawn. Empty for a table
    /// without a table style, whose lines are those of a new table.
    pub h_lines: Vec<Option<Stroke>>,
    /// The lines down, left of each column and right of the last, a line
    /// for each row.
    pub v_lines: Vec<Option<Stroke>>,
    /// The fill of every other body row, when the rows alternate
    /// (`SFTTableBandedRowsProperty`, TAB-14).
    pub banded: Option<Color>,
}

/// The look of a cell: the properties of its cell style (`fill`,
/// `SFTCellTextWrapProperty`), of the paragraph style of its text and of
/// its layout style (`padding`, `verticalAlignment`), each with those
/// that its parents give.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellLook {
    /// The cell style.
    pub cell: Properties,
    /// The paragraph style.
    pub text: Properties,
    /// The layout style.
    pub layout: Properties,
    /// How the value is written (`SFTCellStylePropertyFormatType` and the
    /// format it names, TAB-28).
    pub format: CellFormat,
    /// The rules of conditional formatting
    /// (`SFTCellStylePropertyConditionalFormat`, TAB-21).
    pub rules: Vec<CellRule>,
}

/// A number format (`sf:number-format`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NumberFormat {
    /// `sf:format-string`, in the syntax of spreadsheets.
    pub pattern: String,
    /// `sf:format-decimal-places`: none for as many as the number has
    /// (Pages writes 65534 for that).
    pub decimals: Option<u8>,
    /// `sf:format-currency-code`, as BRL.
    pub currency: String,
    /// `sf:format-negative-style`: 0 a minus sign, 1 red, 2 parentheses,
    /// 3 red parentheses.
    pub negative: u8,
    /// `sf:format-show-thousands-separator`.
    pub thousands: bool,
    /// `sf:format-use-accounting-style`.
    pub accounting: bool,
    /// `sf:format-name`, for a format that a person made.
    pub name: Option<String>,
    /// `sf:format-base`, for a number in another base.
    pub base: Option<u8>,
    /// `sf:format-base-places`.
    pub base_places: Option<u8>,
    /// `sf:format-base-use-minus-sign`.
    pub base_minus: Option<bool>,
}

/// How a cell writes its value, by the format type of its style
/// (`SFTCellStylePropertyFormatType`). The codes are those that the
/// project's files show (tab-28, tab-29): 256 automatic, 257 currency, 258
/// percentage, 259 scientific, 260 text, 261 date and time, 268
/// duration, 269 numeral system and 270 a format that a person made.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CellFormat {
    /// Automatic (256), or no format type.
    #[default]
    Automatic,
    /// Currency (257).
    Currency(NumberFormat),
    /// Percentage (258).
    Percent(NumberFormat),
    /// Scientific (259).
    Scientific(NumberFormat),
    /// Text (260).
    Text,
    /// Date and time (261), by its pattern (`sf:date-format sf:fmt`), as
    /// `dd/MM/yy HH:mm`.
    DateTime(String),
    /// Duration (268), by its pattern (`sf:duration-format sf:fmt`), as
    /// `hhh mmm sss`.
    Duration(String),
    /// Numeral system (269).
    Base(NumberFormat),
    /// A number format that a person made (270).
    Custom(NumberFormat),
    /// Another format type, by its code.
    Other(u32),
}

/// A rule of conditional formatting (`sf:cfo`): a condition
/// (`sf:conditional-format-condition-specs`) and what it changes
/// (`sf:conditional-format-format`), each a dictionary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellRule {
    /// `SFTConditionalFormatConditionSpecType`. The project's file
    /// (tab-21) shows 6 for not equal to.
    pub kind: u32,
    /// `SFTConditionalFormatConditionSpecParameter1` and `…2`: numbers,
    /// or text.
    pub operands: Vec<RuleOperand>,
    /// Bold, when the rule sets it.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// Underline.
    pub underline: Option<bool>,
    /// Strikethrough.
    pub strike: Option<bool>,
    /// The color of the text; none when the rule leaves it, as Pages
    /// writes a clear color for that.
    pub font_color: Option<Color>,
    /// The fill of the cell, likewise.
    pub fill: Option<Color>,
}

/// An operand of a rule.
#[derive(Clone, Debug, PartialEq)]
pub enum RuleOperand {
    /// A number.
    Number(f64),
    /// Text.
    Text(String),
}

impl Table {
    /// The number of rows.
    pub fn rows(&self) -> u32 {
        self.heights.len() as u32
    }

    /// The number of columns.
    pub fn cols(&self) -> u32 {
        self.widths.len() as u32
    }
}

/// The most rows or columns a table is read with, so that a damaged file
/// cannot ask for a grid that does not fit in memory.
const MAX_LINES: usize = 10_000;

/// The most cells a table is read with, for the same reason; a table of
/// Pages '09 in the text of a document is far smaller.
const MAX_GRID: usize = 100_000;

/// Reads a table from its `sf:tabular-info` or `sf:tabular-model`.
pub(super) fn read(ids: &Ids<'_>, styles: &Styles<'_>, element: &Element) -> Option<Table> {
    let element = ids.deref(element);
    let model = if element.name.is(SF, "tabular-model") {
        element
    } else {
        ids.deref(element.child(SF, "tabular-model")?)
    };
    let grid = model.child(SF, "grid")?;
    let number = |e: &Element, name: &str| -> Option<f64> {
        e.attr(SF, name)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    };
    let count = |name: &str| -> u32 {
        model
            .attr(SF, name)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    let widths: Vec<f64> = grid
        .child(SF, "columns")?
        .children_named(SF, "grid-column")
        .take(MAX_LINES)
        .map(|c| number(c, "width").unwrap_or(72.0).max(1.0))
        .collect();
    let heights: Vec<f64> = grid
        .child(SF, "rows")?
        .children_named(SF, "grid-row")
        .take(MAX_LINES)
        .map(|r| number(r, "height").unwrap_or(18.0).max(1.0))
        .collect();
    if widths.is_empty() || heights.is_empty() || widths.len() * heights.len() > MAX_GRID {
        return None;
    }
    let mut table = Table {
        name: model.attr(SF, "name").unwrap_or_default().to_owned(),
        name_shown: model.attr(SF, "name-is-visible") == Some("true"),
        header_rows: count("num-header-rows"),
        header_cols: count("num-header-columns"),
        footer_rows: count("num-footer-rows"),
        widths,
        heights,
        ..Table::default()
    };
    let (rows, cols) = (table.rows(), table.cols());
    let mut cell_styles: Vec<Option<&Element>> = vec![None; (rows * cols) as usize];
    let (mut row, mut col): (u32, i64) = (0, 0);
    for (n, cell) in grid
        .child(SF, "datasource")
        .into_iter()
        .flat_map(Element::elements)
        .enumerate()
    {
        let step = match cell
            .attr(SF, "ct")
            .and_then(|v| v.trim().parse::<u8>().ok())
        {
            Some(byte) => i64::from(byte as i8),
            None if n == 0 => 0,
            None => 1,
        };
        let next = col + step;
        if n > 0 && next <= col {
            row += 1;
        }
        if next < 0 || next >= i64::from(cols) {
            table.skipped.push(cell.name.local.clone());
            break;
        }
        col = next;
        if row >= rows {
            break;
        }
        if let Some(style) = cell_styles.get_mut((row * cols + col as u32) as usize) {
            *style = cell.attr(SF, "s").and_then(|id| ids.get(id));
        }
        let content = match cell.name.local.as_str() {
            "g" | "s" => CellContent::Empty,
            "t" => CellContent::Text(cell_text(cell)),
            "n" => CellContent::Number(number(cell, "v").unwrap_or(0.0)),
            "d" => CellContent::Date(
                number(cell, "cell-date").unwrap_or(0.0) / 86_400.0 + DAYS_TO_2001,
            ),
            "du" => CellContent::Duration(number(cell, "du").unwrap_or(0.0) / 86_400.0),
            "f" => match cell
                .child(SF, "fo")
                .and_then(|f| f.attr(SF, "fs"))
                .filter(|f| f.starts_with('='))
            {
                Some(formula) => CellContent::Formula(formula.to_owned()),
                None => {
                    table.skipped.push("f".to_owned());
                    CellContent::Empty
                }
            },
            other => {
                table.skipped.push(other.to_owned());
                CellContent::Empty
            }
        };
        let span = |name: &str| -> u32 {
            cell.attr(SF, name)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(1)
                .max(1)
        };
        let (rows_spanned, cols_spanned) = (span("row-span"), span("col-span"));
        if content != CellContent::Empty || rows_spanned > 1 || cols_spanned > 1 {
            table.cells.push(TableCell {
                row,
                col: col as u32,
                content,
                rows: rows_spanned.min(rows - row),
                cols: cols_spanned.min(cols - col as u32),
            });
        }
    }
    for a in model
        .child(SL, "cell-annotations")
        .into_iter()
        .flat_map(|c| c.children_named(SL, "a-cell"))
    {
        let at = |name: &str| a.attr(SF, name).and_then(|v| v.trim().parse::<u32>().ok());
        if let (Some(r), Some(c), Some(id)) = (at("row"), at("column"), a.attr(SFA, "ID"))
            && r < rows
            && c < cols
        {
            table.comments.push((r, c, id.to_owned()));
        }
    }
    read_looks(ids, styles, model, &mut table, &cell_styles);
    Some(table)
}

/// The first element of a property of a style or of its parents, as the
/// reference to a style that the property holds.
fn property_element<'a>(
    ids: &Ids<'a>,
    styles: &Styles<'a>,
    style: &'a Element,
    name: &str,
) -> Option<&'a Element> {
    let mut current = Some(ids.deref(style));
    for _ in 0..MAX_PARENTS {
        let style = current?;
        if let Some(value) = style
            .child(SF, "property-map")
            .and_then(|m| m.child(SF, name))
            .and_then(|p| p.elements().next())
        {
            return Some(ids.deref(value));
        }
        current = parent(styles, style);
    }
    None
}

/// The parent of a style, by its `sf:parent-ident`.
fn parent<'a>(styles: &Styles<'a>, style: &Element) -> Option<&'a Element> {
    let ident = style.attr(SF, "parent-ident")?;
    styles.get(&(style.name.local.as_str(), ident)).copied()
}

/// The properties of a style and of its parents, its own first.
fn properties<'a>(ids: &Ids<'a>, styles: &Styles<'a>, style: &'a Element) -> Properties {
    let mut out = Properties::default();
    let mut current = Some(ids.deref(style));
    for _ in 0..MAX_PARENTS {
        let Some(style) = current else { break };
        if let Some(map) = style.child(SF, "property-map") {
            for (name, value) in Properties::read(ids, map).0 {
                if out.get(&name).is_none() {
                    out.0.push((name, value));
                }
            }
        }
        current = parent(styles, style);
    }
    out
}

/// The look of a cell style.
fn look<'a>(ids: &Ids<'a>, styles: &Styles<'a>, style: &'a Element) -> CellLook {
    let of = |name: &str| {
        property_element(ids, styles, style, name)
            .filter(|e| e.name.local != "null")
            .map(|e| properties(ids, styles, e))
            .unwrap_or_default()
    };
    let cell = properties(ids, styles, style);
    let element =
        |name: &str| property_element(ids, styles, style, name).filter(|e| e.name.local != "null");
    let number_format = || {
        element("SFTCellStylePropertyNumberFormat")
            .map(number_format)
            .unwrap_or_default()
    };
    let pattern = |name: &str| {
        element(name)
            .and_then(|e| e.attr(SF, "fmt"))
            .unwrap_or_default()
            .to_owned()
    };
    let format = match cell
        .number("SFTCellStylePropertyFormatType")
        .map(|n| n as u32)
    {
        None | Some(256) => CellFormat::Automatic,
        Some(257) => CellFormat::Currency(number_format()),
        Some(258) => CellFormat::Percent(number_format()),
        Some(259) => CellFormat::Scientific(number_format()),
        Some(260) => CellFormat::Text,
        Some(261) => CellFormat::DateTime(pattern("SFTCellStylePropertyDateTimeFormat")),
        Some(268) => CellFormat::Duration(pattern("SFTCellStylePropertyDurationFormat")),
        Some(269) => CellFormat::Base(number_format()),
        Some(270) => CellFormat::Custom(number_format()),
        Some(other) => CellFormat::Other(other),
    };
    let rules = element("SFTCellStylePropertyConditionalFormat")
        .map(rules)
        .unwrap_or_default();
    CellLook {
        cell,
        text: of("SFTCellStylePropertyParagraphStyle"),
        layout: of("SFTCellStylePropertyLayoutStyle"),
        format,
        rules,
    }
}

/// A number format (`sf:number-format`).
fn number_format(element: &Element) -> NumberFormat {
    let attr = |name: &str| element.attr(SF, name).map(str::trim);
    let small = |name: &str| attr(name).and_then(|v| v.parse::<u8>().ok());
    let flag = |name: &str| attr(name).map(|v| v == "true" || v == "1");
    NumberFormat {
        pattern: attr("format-string").unwrap_or_default().to_owned(),
        decimals: small("format-decimal-places"),
        currency: attr("format-currency-code").unwrap_or_default().to_owned(),
        negative: small("format-negative-style").unwrap_or(0),
        thousands: flag("format-show-thousands-separator").unwrap_or(false),
        accounting: flag("format-use-accounting-style").unwrap_or(false),
        name: attr("format-name").map(str::to_owned),
        base: small("format-base"),
        base_places: small("format-base-places"),
        base_minus: flag("format-base-use-minus-sign"),
    }
}

/// The pairs of a dictionary (`sfa:pair`, with `sfa:key` and
/// `sfa:value`), by the string of their key.
fn pairs(dictionary: &Element) -> impl Iterator<Item = (&str, &Element)> {
    dictionary.children_named(SFA, "pair").filter_map(|pair| {
        let key = pair.child(SFA, "key")?.attr(SFA, "string")?;
        Some((key, pair.child(SFA, "value")?))
    })
}

/// The rules of conditional formatting of a `sf:cfo`: the conditions and
/// the formats come in two lists, in the same order.
fn rules(cfo: &Element) -> Vec<CellRule> {
    if cfo.attr(SF, "conditional-format-enabled-state") == Some("false") {
        return Vec::new();
    }
    let conditions = cfo
        .child(SF, "conditional-format-condition-specs")
        .into_iter()
        .flat_map(|c| c.children_named(SF, "mutable-dictionary"));
    let formats = cfo
        .child(SF, "conditional-format-formats")
        .into_iter()
        .flat_map(|f| f.children_named(SF, "conditional-format-format"));
    conditions
        .zip(formats)
        .filter_map(|(condition, format)| {
            let mut rule = CellRule::default();
            let mut kind = None;
            let mut operands: [Option<RuleOperand>; 2] = [None, None];
            for (key, value) in pairs(condition) {
                let number = value
                    .attr(SFA, "number")
                    .and_then(|n| n.trim().parse::<f64>().ok());
                let operand = || match (number, value.attr(SFA, "string")) {
                    (Some(n), _) => Some(RuleOperand::Number(n)),
                    (None, Some(t)) => Some(RuleOperand::Text(t.to_owned())),
                    _ => None,
                };
                match key {
                    "SFTConditionalFormatConditionSpecType" => kind = number.map(|n| n as u32),
                    "SFTConditionalFormatConditionSpecParameter1" => operands[0] = operand(),
                    "SFTConditionalFormatConditionSpecParameter2" => operands[1] = operand(),
                    _ => {}
                }
            }
            rule.kind = kind?;
            rule.operands = operands.into_iter().map_while(|o| o).collect();
            let properties = format.child(SF, "conditional-format-format-properties")?;
            for (key, value) in pairs(properties) {
                // The first of the pair is the value; the second, whether
                // the rule sets it.
                let first = value.child(SF, "pair-first");
                let set = value
                    .child(SF, "pair-second")
                    .and_then(|s| s.attr(SFA, "number"))
                    .is_some_and(|n| n.trim() != "0");
                let Some(first) = first.filter(|_| set) else {
                    continue;
                };
                // Only what the rule turns on: Pages lists every
                // attribute, those that the rule leaves as off or clear.
                let on = first
                    .attr(SFA, "number")
                    .and_then(|n| n.trim().parse::<f64>().ok())
                    .is_some_and(|n| n != 0.0)
                    .then_some(true);
                let color = || crate::sf::color(first).filter(|c| c.a > 0.0);
                match key {
                    "bold" => rule.bold = on,
                    "italic" => rule.italic = on,
                    "underline" => rule.underline = on,
                    "strikethru" => rule.strike = on,
                    "fontColor" => rule.font_color = color(),
                    "fill" => rule.fill = color(),
                    _ => {}
                }
            }
            Some(rule)
        })
        .collect()
}

/// A line of a vector style: none for a style that draws none.
fn line<'a>(ids: &Ids<'a>, styles: &Styles<'a>, style: Option<&'a Element>) -> Option<Stroke> {
    match properties(ids, styles, style?).get("SFTStrokeProperty") {
        Some(Value::Stroke(stroke)) if stroke.width > 0.0 => Some(stroke.clone()),
        _ => None,
    }
}

/// Reads the looks of the cells and the lines of a table, from its table
/// style and from the styles of its cells and its gridlines (B-049).
fn read_looks<'a>(
    ids: &Ids<'a>,
    styles: &Styles<'a>,
    model: &'a Element,
    table: &mut Table,
    cell_styles: &[Option<&'a Element>],
) {
    let (rows, cols) = (table.rows(), table.cols());
    let table_style = model.child(SF, "tabular-style-ref").map(|r| ids.deref(r));
    // Each style is read once, found again by its element.
    let mut seen: HashMap<*const Element, usize> = HashMap::new();
    let mut index_of = |style: &'a Element, table: &mut Table| -> usize {
        *seen.entry(style as *const Element).or_insert_with(|| {
            table.looks.push(look(ids, styles, style));
            table.looks.len() - 1
        })
    };
    let part = |name: &str| table_style.and_then(|t| property_element(ids, styles, t, name));
    let parts = [
        part("SFTDefaultHeaderRowCellStyleProperty"),
        part("SFTDefaultHeaderColumnCellStyleProperty"),
        part("SFTDefaultFooterRowCellStyleProperty"),
        part("SFTDefaultBodyCellStyleProperty"),
    ];
    for (i, style) in parts.iter().enumerate() {
        let look = style.map(|s| index_of(s, table));
        if let Some(part) = table.part_looks.get_mut(i) {
            *part = look;
        }
    }
    let (header_rows, header_cols) = (table.header_rows.min(rows), table.header_cols.min(cols));
    let footer_start = rows - table.footer_rows.min(rows - header_rows);
    let part_of = |r: u32, c: u32| -> usize {
        if r < header_rows {
            0
        } else if r >= footer_start {
            2
        } else if c < header_cols {
            1
        } else {
            3
        }
    };
    table.cell_looks = Vec::with_capacity(cell_styles.len());
    for r in 0..rows {
        for c in 0..cols {
            let own = cell_styles.get((r * cols + c) as usize).copied().flatten();
            let look = match own {
                Some(style) => Some(index_of(style, table)),
                None => table.part_looks.get(part_of(r, c)).copied().flatten(),
            };
            table.cell_looks.push(look);
        }
    }
    // Without a table style, the cells keep their own styles, and the
    // lines and the alternating rows are those of a new table.
    let Some(table_style) = table_style else {
        return;
    };
    let flag = |name: &str| {
        properties(ids, styles, table_style)
            .number(name)
            .is_some_and(|n| n != 0.0)
    };
    if flag("SFTTableBandedRowsProperty") {
        table.banded =
            match properties(ids, styles, table_style).get("SFTTableBandedCellFillProperty") {
                Some(Value::Color(color)) => Some(*color),
                _ => None,
            };
    }
    // The lines of the parts of the table, from its table style.
    let vector = |name: &str| line(ids, styles, part(name));
    let border = vector("SFTDefaultBorderVectorStyleProperty");
    let header_border = vector("SFTDefaultHeaderBorderVectorStyleProperty");
    let footer_border = vector("SFTDefaultFooterBorderVectorStyleProperty");
    let header_separator = vector("SFTDefaultHeaderSeparatorVectorStyleProperty");
    let footer_separator = vector("SFTDefaultFooterSeparatorVectorStyleProperty");
    let header_body = vector("SFTDefaultHeaderBodyVectorStyleProperty");
    let footer_body = vector("SFTDefaultFooterBodyVectorStyleProperty");
    let body = vector("SFTDefaultBodyVectorStyleProperty");
    let in_header = |r: u32, c: u32| r < header_rows || c < header_cols;
    let in_footer = |r: u32| r >= footer_start;
    // Across: the line above row `i`, at column `c`.
    for i in 0..=rows {
        for c in 0..cols {
            let line = if i == 0 {
                if header_rows > 0 {
                    &header_border
                } else {
                    &border
                }
            } else if i == rows {
                if footer_start < rows {
                    &footer_border
                } else if header_rows == rows {
                    &header_border
                } else {
                    &border
                }
            } else if header_rows > 0 && i == header_rows {
                &header_separator
            } else if footer_start < rows && i == footer_start {
                &footer_separator
            } else if in_header(i, c) {
                &header_body
            } else if in_footer(i) {
                &footer_body
            } else {
                &body
            };
            table.h_lines.push(line.clone());
        }
    }
    // Down: the line left of column `j`, at row `r`.
    for r in 0..rows {
        for j in 0..=cols {
            let line = if j == 0 || j == cols {
                if r < header_rows || (j == 0 && header_cols > 0) {
                    &header_border
                } else if in_footer(r) {
                    &footer_border
                } else {
                    &border
                }
            } else if header_cols > 0 && j == header_cols {
                &header_separator
            } else if in_header(r, j) {
                &header_body
            } else if in_footer(r) {
                &footer_body
            } else {
                &body
            };
            table.v_lines.push(line.clone());
        }
    }
    // The lines given their own style, by runs along each gridline.
    let grid = model.child(SF, "grid");
    for (name, across) in [
        ("horizontal-gridline-styles", true),
        ("vertical-gridline-styles", false),
    ] {
        let runs = grid
            .and_then(|g| g.child(SF, name))
            .into_iter()
            .flat_map(|g| g.children_named(SF, "style-run"));
        for run in runs {
            let number = |e: &Element, name: &str| {
                e.attr(SF, name).and_then(|v| v.trim().parse::<u32>().ok())
            };
            let Some(index) = number(run, "gridline-index") else {
                continue;
            };
            for reference in run.elements() {
                let (Some(start), Some(stop)) = (
                    number(reference, "start-index"),
                    number(reference, "stop-index"),
                ) else {
                    continue;
                };
                let stroke = line(ids, styles, Some(ids.deref(reference)));
                let (lines, along, count) = if across {
                    (&mut table.h_lines, cols, rows + 1)
                } else {
                    (&mut table.v_lines, cols + 1, rows)
                };
                // The run goes along the line: no further than its end,
                // whatever the file says.
                let stop = stop.min(if across { along } else { count });
                for k in start..stop {
                    let at = if across {
                        (index < count && k < along).then(|| (index * along + k) as usize)
                    } else {
                        (index < along && k < count).then(|| (k * along + index) as usize)
                    };
                    if let Some(line) = at.and_then(|at| lines.get_mut(at)) {
                        *line = stroke.clone();
                    }
                }
            }
        }
    }
}

/// The text of a text cell: its plain text (`sf:ct sfa:s`), or the
/// paragraphs of its formatted text (`sf:so`), joined by line feeds.
fn cell_text(cell: &Element) -> String {
    if let Some(text) = cell.child(SF, "ct").and_then(|c| c.attr(SFA, "s")) {
        return text.to_owned();
    }
    let Some(body) = cell.child(SF, "so").and_then(|s| s.child(SF, "text-body")) else {
        return String::new();
    };
    body.descendants()
        .filter(|e| e.name.is(SF, "p"))
        .map(Element::text)
        .collect::<Vec<_>>()
        .join("\n")
}
