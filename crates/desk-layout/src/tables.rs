// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables and charts on the pages: where the rows, columns and the text
//! of the cells go, and what a chart draws, with its text laid out
//! (TAB-01, TAB-04, TAB-14, TAB-20, TAB-21, GRF-01, GRF-08).

use std::cell::RefCell;
use std::sync::Arc;

use desk_model::table::{Content, HAlign, Locale, Region, VAlign};
use desk_model::{
    Alignment, CharAttrs, Document, FontWeight, LineStyle, NodeId, ParaAttrs, Paragraph, Rgba, Run,
    Table,
};
use desk_text::{Column, ParagraphText, TextSystem};

use crate::pages::PlacedLine;

/// A cell of a table, laid out.
#[derive(Clone, Debug)]
pub struct CellLayout {
    /// The row of the cell, or of the first cell of merged cells.
    pub row: u32,
    /// The column.
    pub col: u32,
    /// Its box, from the top left corner of the table's frame: left,
    /// top, width and height.
    pub rect: (f32, f32, f32, f32),
    /// Its fill: of the cell, of its rules, of alternating rows or of its
    /// part of the table.
    pub fill: Option<Rgba>,
    /// Its text, from the top left corner of the frame.
    pub lines: Vec<PlacedLine>,
    /// Whether its text does not fit and is cut, with the + mark shown
    /// in the corner (TAB-20).
    pub overflow: bool,
    /// Whether it has a comment (TAB-27).
    pub comment: bool,
}

/// A table laid out: the edges of its columns and rows and its cells.
#[derive(Clone, Debug, Default)]
pub struct TableLayout {
    /// The left edge of each column and the right edge of the last, from
    /// the left of the frame.
    pub cols: Vec<f32>,
    /// The top edge of each row and the bottom edge of the last, from the
    /// top of the frame, below the name.
    pub rows: Vec<f32>,
    /// The visible cells: those not hidden under merged cells.
    pub cells: Vec<CellLayout>,
    /// The name above the table, when it shows.
    pub name: Vec<PlacedLine>,
    /// The line around the name, when Outline Table Title is on: its
    /// left, top, width and height from the top left of the frame
    /// (PXV-15).
    pub name_outline: Option<(f32, f32, f32, f32)>,
}

impl TableLayout {
    /// The width of the table.
    pub fn width(&self) -> f32 {
        self.cols.last().copied().unwrap_or(0.0)
    }

    /// The height of the table, with its name.
    pub fn height(&self) -> f32 {
        self.rows.last().copied().unwrap_or(0.0)
    }

    /// The cell at a point of the frame, as the first cell of merged
    /// cells; none outside the grid.
    pub fn cell_at(&self, x: f32, y: f32) -> Option<(u32, u32)> {
        self.cells
            .iter()
            .find(|c| {
                x >= c.rect.0 && x < c.rect.0 + c.rect.2 && y >= c.rect.1 && y < c.rect.1 + c.rect.3
            })
            .map(|c| (c.row, c.col))
    }

    /// The box of a cell, or of the merged cells it is part of.
    pub fn cell_rect(&self, row: u32, col: u32) -> Option<(f32, f32, f32, f32)> {
        let r = row as usize;
        let c = col as usize;
        if r + 1 >= self.rows.len() || c + 1 >= self.cols.len() {
            return None;
        }
        self.cells
            .iter()
            .find(|cell| cell.row == row && cell.col == col)
            .map(|cell| cell.rect)
            .or(Some((
                self.cols[c],
                self.rows[r],
                self.cols[c + 1] - self.cols[c],
                self.rows[r + 1] - self.rows[r],
            )))
    }
}

fn rgba(c: desk_model::table::Color) -> Rgba {
    Rgba {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

/// The locale that tables write numbers in: that of the language of the
/// text of the document.
pub fn document_locale(document: &Document) -> Locale {
    let styles = document.styles();
    let format = styles.resolve_character(styles.default_paragraph(), None, &CharAttrs::default());
    let tag = format.language.tag().to_owned();
    if tag.is_empty() || tag == "und" {
        Locale::from_tag(&std::env::var("LANG").unwrap_or_default())
    } else {
        Locale::from_tag(&tag)
    }
}

/// The day now, in days from 30 December 1899, for TODAY and NOW.
pub fn today() -> f64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    25_569.0 + secs as f64 / 86_400.0
}

/// The character attributes of text of a table: the font of the table,
/// the formatting of the cell and the changes of its rules.
fn char_attrs(
    table: &Table,
    style: &desk_model::table::TextStyle,
    rule: &desk_model::table::RuleStyle,
    red: bool,
) -> CharAttrs {
    let font: Arc<str> = style.font.as_deref().unwrap_or(&table.style.font).into();
    let size = style.size.unwrap_or(table.style.font_size);
    let bold = rule.bold.unwrap_or(style.bold);
    let color = rule
        .text_color
        .map(rgba)
        .or(red.then_some(Rgba::rgb(0xe0, 0x1b, 0x24)))
        .or(style.color.map(rgba))
        .unwrap_or(Rgba::BLACK);
    CharAttrs {
        font_family: Some(font),
        font_size: Some(desk_model::Length::from_millipoints(size as i32)),
        weight: Some(if bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        }),
        italic: Some(rule.italic.unwrap_or(style.italic)),
        underline: Some(if rule.underline.unwrap_or(style.underline) {
            LineStyle::Single
        } else {
            LineStyle::None
        }),
        strikethrough: Some(if rule.strike.unwrap_or(style.strike) {
            LineStyle::Single
        } else {
            LineStyle::None
        }),
        color: Some(color),
        ..CharAttrs::default()
    }
}

/// The lines of text laid out in a column of a width, from its top left
/// corner: one paragraph for each line of the text.
pub(crate) fn text_lines(
    document: &Document,
    text: &str,
    attrs: &CharAttrs,
    alignment: Alignment,
    width: f32,
    system: &mut TextSystem,
) -> Vec<PlacedLine> {
    let styles = document.styles();
    let settings = document.text_settings();
    let mut out = Vec::new();
    let mut y = 0.0;
    let id = NodeId::new(u64::MAX).expect("not zero");
    for (i, line_text) in text.split('\n').enumerate() {
        let paragraph = Paragraph::from_runs(
            id,
            styles.default_paragraph(),
            ParaAttrs {
                alignment: Some(alignment),
                space_before: Some(desk_model::Length::ZERO),
                space_after: Some(desk_model::Length::ZERO),
                first_line_indent: Some(desk_model::Length::ZERO),
                left_indent: Some(desk_model::Length::ZERO),
                right_indent: Some(desk_model::Length::ZERO),
                line_spacing: Some(desk_model::LineSpacing::SINGLE),
                ..ParaAttrs::default()
            },
            [Run::with_attrs(line_text, None, attrs.clone())],
        );
        let Ok(paragraph) = paragraph else { continue };
        let prepared = ParagraphText::new_in_document_with(
            styles,
            settings,
            &paragraph,
            None,
            Default::default(),
        );
        let laid = system.layout(&prepared, &Column::new(width.max(1.0)), 0.0);
        let first_top = laid.lines.first().map_or(0.0, |l| l.top);
        let mut bottom = y;
        for (n, line) in laid.lines.into_iter().enumerate() {
            bottom = y + line.top - first_top + line.height;
            out.push(PlacedLine {
                paragraph: i,
                index: n,
                origin: (0.0, y - first_top),
                line,
            });
        }
        y = bottom;
    }
    out
}

fn lines_height(lines: &[PlacedLine]) -> f32 {
    lines.iter().map(PlacedLine::bottom).fold(0.0, f32::max)
}

fn lines_width(lines: &[PlacedLine]) -> f32 {
    lines
        .iter()
        .map(|l| l.line.x + l.line.advance)
        .fold(0.0, f32::max)
}

fn shift(lines: &mut [PlacedLine], dx: f32, dy: f32) {
    for l in lines {
        l.origin.0 += dx;
        l.origin.1 += dy;
    }
}

/// Lays out a table: the heights of its rows, which grow to fit the text
/// of their cells, and the text of each cell, with the values of its
/// formulas, in its format and the locale.
pub fn lay_out_table(
    document: &Document,
    table: &Table,
    system: &mut TextSystem,
    locale: &Locale,
    now: f64,
) -> TableLayout {
    let values = table.values(now);
    let pad = table.style.padding as f32 / 1000.0;
    let mut cols = vec![0.0f32];
    for c in 0..table.cols() {
        let last = *cols.last().unwrap_or(&0.0);
        cols.push(last + table.width(c) as f32 / 1000.0);
    }
    // The name above the table.
    let mut name = Vec::new();
    let mut name_outline = None;
    let mut top = 0.0;
    if table.options.show_name && !table.name.is_empty() {
        let attrs = CharAttrs {
            font_family: Some(table.style.font.as_str().into()),
            font_size: Some(desk_model::Length::from_millipoints(
                (table.style.font_size + 2_000) as i32,
            )),
            weight: Some(FontWeight::BOLD),
            ..CharAttrs::default()
        };
        let width = cols[cols.len() - 1];
        let align = if table.options.name_centered {
            Alignment::Center
        } else {
            Alignment::Left
        };
        name = text_lines(document, &table.name, &attrs, align, width, system);
        top = lines_height(&name) + 4.0;
        if table.options.name_outline {
            name_outline = Some((0.0, 0.0, width, top - 2.0));
        }
    }
    // The text of each visible cell, laid out at the width of its column
    // or its merged columns.
    struct Prepared {
        row: u32,
        col: u32,
        rows: u32,
        lines: Vec<PlacedLine>,
        width: f32,
        align: HAlign,
        numeric: bool,
        valign: VAlign,
        wrap: bool,
        fill: Option<Rgba>,
        comment: bool,
        symbol: Vec<PlacedLine>,
    }
    let mut prepared = Vec::new();
    for r in 0..table.rows() {
        for c in 0..table.cols() {
            if table.is_covered(r, c) {
                continue;
            }
            let span = table.span(r, c);
            let cell = table.cell(r, c);
            let width = cols[span.col_end() as usize] - cols[span.col as usize];
            let shown = table.shown(&values, r, c, locale);
            let rule = table.rule_style(&values, r, c);
            let attrs = char_attrs(table, &cell.style.text, &rule, shown.red);
            let text = match &cell.content {
                Content::Text(t) => t.clone(),
                _ => shown.text.clone(),
            };
            let numeric = shown.numeric;
            let alignment = match (cell.style.h_align, numeric) {
                (HAlign::Left, _) | (HAlign::Auto, false) => Alignment::Left,
                (HAlign::Center, _) => Alignment::Center,
                (HAlign::Right, _) | (HAlign::Auto, true) => Alignment::Right,
                (HAlign::Justify, _) => Alignment::Justify,
            };
            let inner = (width - 2.0 * pad).max(1.0);
            let wrap = cell.style.wrap;
            let lines = if text.is_empty() {
                Vec::new()
            } else if wrap {
                text_lines(document, &text, &attrs, alignment, inner, system)
            } else {
                text_lines(document, &text, &attrs, Alignment::Left, 100_000.0, system)
            };
            let symbol = match &shown.left_symbol {
                Some(s) => text_lines(document, s, &attrs, Alignment::Left, inner, system),
                None => Vec::new(),
            };
            // The fill: the cell's own, its rules', alternating rows, or
            // that of its part of the table (TAB-14, TAB-21, TAB-22).
            let region = table.region(r, c);
            let alternate = region == Region::Body
                && table.style.alternating.is_some()
                && (r - table.body_start()) % 2 == 1;
            let fill = rule
                .fill
                .map(rgba)
                .or(match &cell.style.fill {
                    Some(desk_model::table::CellFill::Color(c)) => Some(rgba(*c)),
                    Some(desk_model::table::CellFill::Gradient { start, .. }) => Some(rgba(*start)),
                    _ => None,
                })
                .or(if alternate {
                    table.style.alternating.map(rgba)
                } else {
                    None
                })
                .or(match region {
                    Region::HeaderRow => table.style.header_row_fill.map(rgba),
                    Region::HeaderColumn => table.style.header_column_fill.map(rgba),
                    Region::Footer => table.style.footer_fill.map(rgba),
                    Region::Body => table.style.body_fill.map(rgba),
                });
            prepared.push(Prepared {
                row: r,
                col: c,
                rows: span.rows,
                lines,
                width,
                align: match alignment {
                    Alignment::Center => HAlign::Center,
                    Alignment::Right => HAlign::Right,
                    _ => HAlign::Left,
                },
                numeric,
                valign: cell.style.v_align,
                wrap,
                fill,
                comment: cell.comment.is_some(),
                symbol,
            });
        }
    }
    // Row heights: at least the height set, and enough for the text of
    // the cells that start in the row and take one row; merged cells
    // taller than their rows make the last of them grow.
    let mut heights: Vec<f32> = (0..table.rows())
        .map(|r| table.height(r) as f32 / 1000.0)
        .collect();
    for p in prepared.iter().filter(|p| p.rows == 1) {
        let need = lines_height(&p.lines) + 2.0 * pad;
        let h = &mut heights[p.row as usize];
        *h = h.max(need);
    }
    for p in prepared.iter().filter(|p| p.rows > 1) {
        let need = lines_height(&p.lines) + 2.0 * pad;
        let span: f32 = heights[p.row as usize..(p.row + p.rows) as usize]
            .iter()
            .sum();
        if need > span {
            heights[(p.row + p.rows - 1) as usize] += need - span;
        }
    }
    let mut rows = vec![top];
    for h in &heights {
        let last = *rows.last().unwrap_or(&top);
        rows.push(last + h);
    }
    let filled: Vec<Vec<bool>> = (0..table.rows())
        .map(|r| {
            (0..table.cols())
                .map(|c| !table.cell(r, c).content.is_empty())
                .collect()
        })
        .collect();
    let mut cells = Vec::with_capacity(prepared.len());
    for mut p in prepared {
        let span = table.span(p.row, p.col);
        let x = cols[span.col as usize];
        let y = rows[span.row as usize];
        let h = rows[span.row_end() as usize] - y;
        let inner_w = (p.width - 2.0 * pad).max(1.0);
        let text_h = lines_height(&p.lines);
        let dy = match p.valign {
            VAlign::Top => pad,
            VAlign::Middle => (h - text_h) / 2.0,
            VAlign::Bottom => h - pad - text_h,
        };
        let mut overflow = false;
        if p.wrap {
            shift(&mut p.lines, x + pad, y + dy);
        } else {
            // Text that does not wrap runs into the empty cells beside
            // it, or is cut with the + mark when they are not empty
            // (TAB-20).
            let natural = lines_width(&p.lines);
            let dx = match p.align {
                HAlign::Center => (inner_w - natural) / 2.0,
                HAlign::Right => inner_w - natural,
                _ => 0.0,
            };
            shift(&mut p.lines, x + pad + dx, y + dy);
            if natural > inner_w {
                let next = span.col_end() as usize;
                let blocked = next >= table.cols() as usize || filled[span.row as usize][next];
                overflow = blocked || p.align == HAlign::Right;
            }
        }
        if text_h > h - pad {
            overflow = true;
        }
        let _ = p.numeric;
        let symbol_h = lines_height(&p.symbol);
        shift(&mut p.symbol, x + pad, y + (h - symbol_h).max(0.0) / 2.0);
        let mut lines = p.lines;
        lines.extend(p.symbol);
        cells.push(CellLayout {
            row: p.row,
            col: p.col,
            rect: (x, y, p.width, h),
            fill: p.fill,
            lines,
            overflow,
            comment: p.comment,
        });
    }
    TableLayout {
        cols,
        rows,
        cells,
        name,
        name_outline,
    }
}

/// The size of a table laid out: its width and its height with its name,
/// which its frame takes.
pub fn table_size(document: &Document, table: &Table, system: &mut TextSystem) -> (f32, f32) {
    let layout = lay_out_table(document, table, system, &document_locale(document), today());
    (layout.width(), layout.height())
}

/// A line of text of a chart, laid out.
#[derive(Clone, Debug)]
pub struct ChartLabel {
    /// The lines, placed so that the point of the label is at (0, 0).
    pub lines: Vec<PlacedLine>,
    /// The point of the label, from the top left corner of the frame.
    pub at: (f32, f32),
    /// The angle, counterclockwise in degrees.
    pub angle: f32,
}

/// A chart laid out: its shapes, and its text as lines.
#[derive(Clone, Debug, Default)]
pub struct ChartLayout {
    /// What the chart draws; its text marks are also in `labels`.
    pub drawing: desk_model::chart::Drawing,
    /// The text, laid out, in the order of the text marks.
    pub labels: Vec<ChartLabel>,
}

/// Lays out a chart in a frame of a size, measuring its text with the
/// fonts of the document.
pub fn lay_out_chart(
    document: &Document,
    chart: &desk_model::Chart,
    width: f32,
    height: f32,
    system: &mut TextSystem,
    locale: &Locale,
) -> ChartLayout {
    let attrs = |size: f32, bold: bool, italic: bool, color: Rgba| CharAttrs {
        font_family: Some(chart.text.font.as_str().into()),
        font_size: Some(desk_model::Length::from_pt(f64::from(size))),
        weight: Some(if bold {
            FontWeight::BOLD
        } else {
            FontWeight::NORMAL
        }),
        italic: Some(italic),
        color: Some(color),
        ..CharAttrs::default()
    };
    let cell = RefCell::new(&mut *system);
    let measure = |text: &str, size: f32| -> f32 {
        let lines = text_lines(
            document,
            text,
            &attrs(size, chart.text.bold, chart.text.italic, Rgba::BLACK),
            Alignment::Left,
            100_000.0,
            &mut cell.borrow_mut(),
        );
        lines_width(&lines)
    };
    let drawing =
        desk_model::chart::draw(chart, width, height, &measure, locale.decimal, locale.group);
    let system = cell.into_inner();
    let mut labels = Vec::new();
    for mark in &drawing.marks {
        let desk_model::chart::Mark::Text {
            x,
            y,
            text,
            size,
            color,
            bold,
            italic,
            align,
            angle,
            ..
        } = mark
        else {
            continue;
        };
        let mut lines = text_lines(
            document,
            text,
            &attrs(*size, *bold, *italic, rgba(*color)),
            Alignment::Left,
            100_000.0,
            system,
        );
        let w = lines_width(&lines);
        let baseline = lines.first().map_or(0.0, PlacedLine::baseline);
        let dx = match align {
            desk_model::chart::Align::Start => 0.0,
            desk_model::chart::Align::Middle => -w / 2.0,
            desk_model::chart::Align::End => -w,
        };
        shift(&mut lines, dx, -baseline);
        labels.push(ChartLabel {
            lines,
            at: (*x, *y),
            angle: *angle,
        });
    }
    ChartLayout { drawing, labels }
}
