// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tables and charts in ODF (TAB-01, GRF-01, IOP-01).
//!
//! A table is written as a `table:table` that other programs read, with
//! its cells, values, formulas, merges, widths and look, and with all that
//! Desk keeps of it in `desk:table`, which Desk reads back first. A chart
//! is an object of its own in the package, an `office:chart` with its
//! data, and Desk keeps all of it in `desk:chart`.

use desk_chart::{Chart, ChartData, ChartKind};
use desk_model::{CharAttrs, Document};
use desk_odf as odf;
use desk_table::{
    Area, Border, CellFill, CellText, Color, Content, Format, HAlign, LineStyle, Locale, Num,
    VAlign,
};

/// A table of the model.
pub(crate) type Table = desk_model::Table;

/// The language of the text of a document, which numbers are written in.
pub(crate) fn locale(document: &Document) -> Locale {
    let styles = document.styles();
    let format = styles.resolve_character(styles.default_paragraph(), None, &CharAttrs::default());
    let tag = format.language.tag().to_owned();
    if tag.is_empty() || tag == "und" {
        Locale::from_tag(&std::env::var("LANG").unwrap_or_default())
    } else {
        Locale::from_tag(&tag)
    }
}

/// The language of the text of an ODF document, from its default style,
/// which the numbers of its merged cells are written in; without one, the
/// language of the system, as [`locale`] takes.
fn odf_locale(source: &odf::TextDocument) -> Locale {
    let text = &source.default_style.text;
    match text.language.as_deref() {
        Some(language) if !language.is_empty() && !matches!(language, "zxx" | "und") => {
            let tag = text
                .country
                .as_deref()
                .map_or_else(|| language.to_owned(), |c| format!("{language}-{c}"));
            Locale::from_tag(&tag)
        }
        _ => Locale::from_tag(&std::env::var("LANG").unwrap_or_default()),
    }
}

/// The day now, for TODAY and NOW.
fn today() -> f64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    25_569.0 + secs as f64 / 86_400.0
}

fn pt(thousandths: u32) -> odf::Length {
    odf::Length(f64::from(thousandths) / 1000.0)
}

fn thousandths(length: odf::Length) -> u32 {
    (length.0 * 1000.0).round().clamp(0.0, f64::from(u32::MAX)) as u32
}

fn odf_color(c: Color) -> odf::Color {
    odf::Color {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

fn model_color(c: odf::Color) -> Color {
    Color::rgb(c.r, c.g, c.b)
}

fn odf_border(b: Option<Border>) -> Option<odf::Border> {
    let b = b?;
    Some(odf::Border::Line {
        width: pt(b.width),
        style: match b.style {
            LineStyle::Solid => odf::BorderLine::Solid,
            LineStyle::Dashed => odf::BorderLine::Dashed,
            LineStyle::Dotted => odf::BorderLine::Dotted,
        },
        color: odf_color(b.color),
    })
}

fn model_border(b: Option<odf::Border>) -> Option<Border> {
    match b? {
        odf::Border::None => None,
        odf::Border::Line {
            width,
            style,
            color,
        } => Some(Border {
            width: thousandths(width),
            color: model_color(color),
            style: match style {
                odf::BorderLine::Solid => LineStyle::Solid,
                odf::BorderLine::Dashed => LineStyle::Dashed,
                odf::BorderLine::Dotted => LineStyle::Dotted,
            },
        }),
    }
}

fn text_paragraphs(text: &str) -> Vec<odf::Paragraph> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|line| odf::Paragraph {
            spans: vec![odf::Span {
                text: line.to_owned(),
                ..odf::Span::default()
            }],
            ..odf::Paragraph::default()
        })
        .collect()
}

/// An ISO 8601 date and time of a day count from 30 December 1899.
fn iso_date(days: f64) -> String {
    let whole = days.floor();
    let (y, m, d) = desk_formula::date_from_days(whole as i64);
    let secs = ((days - whole) * 86_400.0).round() as i64;
    if secs == 0 {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
            secs / 3600,
            secs / 60 % 60,
            secs % 60
        )
    }
}

fn parse_iso_date(text: &str) -> Option<f64> {
    let (date, time) = text.split_once('T').unwrap_or((text, ""));
    let mut parts = date.split('-').map(|p| p.parse::<i32>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    let mut days = desk_formula::days_from_date(y, m, d) as f64;
    let mut t = time.split(':').map(|p| p.parse::<f64>().unwrap_or(0.0));
    if !time.is_empty() {
        let (h, mi, s) = (
            t.next().unwrap_or(0.0),
            t.next().unwrap_or(0.0),
            t.next().unwrap_or(0.0),
        );
        days += (h * 3600.0 + mi * 60.0 + s) / 86_400.0;
    }
    Some(days)
}

/// `PT12H30M00S` of a length of time in days.
fn iso_duration(days: f64) -> String {
    let secs = (days.abs() * 86_400.0).round() as i64;
    format!(
        "{}PT{}H{:02}M{:02}S",
        if days < 0.0 { "-" } else { "" },
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

fn parse_iso_duration(text: &str) -> Option<f64> {
    let negative = text.starts_with('-');
    let rest = text.trim_start_matches('-').strip_prefix('P')?;
    let mut days = 0.0;
    let mut number = String::new();
    let mut time = false;
    for c in rest.chars() {
        match c {
            'T' => time = true,
            '0'..='9' | '.' | ',' => number.push(if c == ',' { '.' } else { c }),
            _ => {
                let n: f64 = number.parse().ok()?;
                number.clear();
                days += match (c, time) {
                    ('D', _) => n,
                    ('H', true) => n / 24.0,
                    ('M', true) => n / 1440.0,
                    ('S', true) => n / 86_400.0,
                    _ => return None,
                };
            }
        }
    }
    Some(if negative { -days } else { days })
}

/// The automatic styles that the parts of a table take.
pub(crate) trait TableStyles {
    /// The name of an automatic style of a family with these properties.
    fn table_style(&mut self, family: odf::Family, properties: odf::Properties) -> String;
}

/// A table of the model as ODF.
pub(crate) fn export_table(
    table: &Table,
    locale: &Locale,
    styles: &mut dyn TableStyles,
    document: &Document,
) -> odf::Table {
    let values = table.values(today());
    // The comments of the cells are their annotations; the identifiers
    // the cells keep are those of this document only.
    let kept = if table.comments().is_empty() {
        None
    } else {
        let mut kept = table.clone();
        for ((r, c), _) in table.comments() {
            kept.cell_mut(r, c).comment = None;
        }
        Some(kept)
    };
    let mut out = odf::Table {
        name: Some(if table.name.is_empty() {
            "Tabela".to_owned()
        } else {
            table.name.clone()
        }),
        header_columns: table.header_cols() as usize,
        header_rows: table.header_rows() as usize,
        desk: serde_json::to_string(kept.as_ref().unwrap_or(table)).ok(),
        ..odf::Table::default()
    };
    out.style = Some(styles.table_style(
        odf::Family::Table,
        odf::Properties {
            table: odf::TableProperties {
                width: Some(pt(table.total_width())),
                ..odf::TableProperties::default()
            },
            ..odf::Properties::default()
        },
    ));
    for c in 0..table.cols() {
        let style = styles.table_style(
            odf::Family::TableColumn,
            odf::Properties {
                table: odf::TableProperties {
                    column_width: Some(pt(table.width(c))),
                    ..odf::TableProperties::default()
                },
                ..odf::Properties::default()
            },
        );
        out.columns.push(odf::TableColumn {
            style: Some(style),
            default_cell_style: None,
        });
    }
    for r in 0..table.rows() {
        let style = styles.table_style(
            odf::Family::TableRow,
            odf::Properties {
                table: odf::TableProperties {
                    min_row_height: Some(pt(table.height(r))),
                    ..odf::TableProperties::default()
                },
                ..odf::Properties::default()
            },
        );
        let mut row = odf::TableRow {
            style: Some(style),
            cells: Vec::new(),
        };
        for c in 0..table.cols() {
            let span = table.span(r, c);
            if (span.row, span.col) != (r, c) {
                row.cells.push(odf::TableCell {
                    covered: true,
                    ..odf::TableCell::default()
                });
                continue;
            }
            let cell = table.cell(r, c);
            let shown = table.shown(&values, r, c, locale);
            let value = values.get(r, c);
            let mut out_cell = odf::TableCell {
                columns_spanned: span.cols,
                rows_spanned: span.rows,
                paragraphs: text_paragraphs(&shown.text),
                annotations: super::comments::cell_annotations(document, cell.comment),
                ..odf::TableCell::default()
            };
            if let Content::Formula(f) = &cell.content
                && let Ok(parsed) = desk_formula::Formula::parse(f.trim_start_matches('='))
            {
                out_cell.formula = Some(parsed.to_odf());
            }
            use desk_formula::Value;
            let (kind, v) = match value {
                Value::Number(n) => match &cell.format {
                    Format::Percent { .. } => ("percentage", n.to_string()),
                    Format::Currency { .. } => ("currency", n.to_string()),
                    _ => ("float", n.to_string()),
                },
                Value::Bool(b) => ("boolean", b.to_string()),
                Value::Date(d) => ("date", iso_date(*d)),
                Value::Duration(d) => ("time", iso_duration(*d)),
                Value::Text(_) | Value::Error(_) => ("string", String::new()),
                Value::Empty => ("", String::new()),
            };
            if !kind.is_empty() {
                out_cell.value_type = Some(kind.to_owned());
                if !v.is_empty() {
                    out_cell.value = Some(v);
                }
            }
            // ODF allows `office:currency` only on a cell whose value is
            // money: text in a currency cell, as tab-28 has, goes without.
            if let Format::Currency { symbol, .. } = &cell.format
                && kind == "currency"
            {
                out_cell.currency = Some(
                    match symbol.as_str() {
                        "R$" => "BRL",
                        "US$" | "$" => "USD",
                        "€" => "EUR",
                        "£" => "GBP",
                        "¥" => "JPY",
                        other => other,
                    }
                    .to_owned(),
                );
            }
            let style = &cell.style;
            let fill = match &style.fill {
                Some(CellFill::Color(c)) => Some(odf_color(*c)),
                Some(CellFill::Gradient { start, .. }) => Some(odf_color(*start)),
                _ => None,
            };
            let text = &style.text;
            let properties = odf::Properties {
                table: odf::TableProperties {
                    background: fill,
                    border_top: odf_border(table.h_border(r, c)),
                    border_bottom: odf_border(table.h_border(span.row_end(), c)),
                    border_left: odf_border(table.v_border(r, c)),
                    border_right: odf_border(table.v_border(r, span.col_end())),
                    vertical_align: Some(
                        match style.v_align {
                            VAlign::Top => "top",
                            VAlign::Middle => "middle",
                            VAlign::Bottom => "bottom",
                        }
                        .to_owned(),
                    ),
                    ..odf::TableProperties::default()
                },
                paragraph: odf::ParagraphProperties {
                    align: match style.h_align {
                        HAlign::Auto => None,
                        HAlign::Left => Some(odf::TextAlign::Start),
                        HAlign::Center => Some(odf::TextAlign::Center),
                        HAlign::Right => Some(odf::TextAlign::End),
                        HAlign::Justify => Some(odf::TextAlign::Justify),
                    },
                    ..odf::ParagraphProperties::default()
                },
                text: odf::TextProperties {
                    font_family: text.font.clone(),
                    font_size: text.size.map(pt),
                    font_weight: text.bold.then_some(700),
                    italic: text.italic.then_some(true),
                    underline: text.underline.then_some(odf::Line::Single),
                    line_through: text.strike.then_some(odf::Line::Single),
                    color: text.color.map(odf_color),
                    ..odf::TextProperties::default()
                },
                ..odf::Properties::default()
            };
            out_cell.style = Some(styles.table_style(odf::Family::TableCell, properties));
            row.cells.push(out_cell);
        }
        out.rows.push(row);
    }
    out
}

fn style_of<'a>(
    source: &'a odf::TextDocument,
    family: odf::Family,
    name: Option<&str>,
) -> Option<&'a odf::Style> {
    let name = name?;
    source
        .automatic_styles
        .iter()
        .chain(&source.styles)
        .find(|s| s.family == family && s.name == name)
}

/// A table of the model from ODF.
pub(crate) fn import_table(source: &odf::TextDocument, odf_table: &odf::Table) -> Table {
    let rows = odf_table.rows.len().max(1) as u32;
    let cols = odf_table.columns.len().max(1) as u32;
    // What Desk wrote, when the table still has the shape it had and its
    // parts agree with it: the JSON of a damaged file could name more
    // rows than it has cells, and the first cell read would panic.
    if let Some(table) = odf_table
        .desk
        .as_deref()
        .and_then(|d| serde_json::from_str::<Table>(d).ok())
        .filter(|t| t.rows() == rows && t.cols() == cols && t.is_consistent())
    {
        return table;
    }
    let mut table = Table::new(rows, cols);
    table.name = odf_table.name.clone().unwrap_or_default();
    let _ = table.set_header_rows(odf_table.header_rows.min(5) as u32);
    let _ = table.set_header_cols(odf_table.header_columns.min(5) as u32);
    // Tables of other programs have their borders on their cells.
    table.set_borders(
        Area {
            row: 0,
            col: 0,
            rows,
            cols,
        },
        desk_table::BorderSet::All,
        None,
    );
    for (c, column) in odf_table.columns.iter().enumerate() {
        if let Some(width) = style_of(source, odf::Family::TableColumn, column.style.as_deref())
            .and_then(|s| s.properties.table.column_width)
        {
            table.set_width(c as u32, thousandths(width).max(1000));
        }
    }
    let mut merges = Vec::new();
    for (r, row) in odf_table.rows.iter().enumerate() {
        let r = r as u32;
        if let Some(t) = style_of(source, odf::Family::TableRow, row.style.as_deref())
            .map(|s| &s.properties.table)
            && let Some(height) = t.row_height.or(t.min_row_height)
        {
            table.set_height(r, thousandths(height).max(1000));
        }
        for (c, cell) in row.cells.iter().enumerate() {
            let c = c as u32;
            if cell.covered || c >= cols {
                continue;
            }
            if cell.columns_spanned > 1 || cell.rows_spanned > 1 {
                merges.push(Area {
                    row: r,
                    col: c,
                    rows: cell.rows_spanned.min(rows - r),
                    cols: cell.columns_spanned.min(cols - c),
                });
            }
            let text = cell.text();
            let number = || {
                cell.value
                    .as_deref()
                    .and_then(|v| v.trim().parse::<f64>().ok())
            };
            let formula = cell
                .formula
                .as_deref()
                .and_then(|f| desk_formula::Formula::parse_odf(f).ok())
                .map(|f| f.to_string());
            let content = if let Some(f) = formula {
                Content::Formula(f)
            } else {
                match cell.value_type.as_deref() {
                    Some("float" | "percentage" | "currency") => {
                        number().map_or(Content::Empty, |n| Content::Number(Num(n)))
                    }
                    Some("date") => cell
                        .value
                        .as_deref()
                        .and_then(parse_iso_date)
                        .map_or(Content::Empty, |d| Content::Date(Num(d))),
                    Some("time") => cell
                        .value
                        .as_deref()
                        .and_then(parse_iso_duration)
                        .map_or(Content::Empty, |d| Content::Duration(Num(d))),
                    Some("boolean") => Content::Bool(cell.value.as_deref() == Some("true")),
                    _ if text.is_empty() => Content::Empty,
                    _ => Content::Text(String::from_plain(&text)),
                }
            };
            let target = table.cell_mut(r, c);
            target.content = content;
            match cell.value_type.as_deref() {
                Some("percentage") => {
                    target.format = Format::Percent {
                        decimals: None,
                        thousands: false,
                        negative: Default::default(),
                    }
                }
                Some("currency") => {
                    target.format = Format::Currency {
                        symbol: match cell.currency.as_deref() {
                            Some("BRL") | None => "R$",
                            Some("USD") => "US$",
                            Some("EUR") => "€",
                            Some("GBP") => "£",
                            Some("JPY") => "¥",
                            Some(other) => other,
                        }
                        .to_owned(),
                        decimals: 2,
                        thousands: true,
                        negative: Default::default(),
                        accounting: false,
                    }
                }
                _ => {}
            }
            let Some(style) = style_of(source, odf::Family::TableCell, cell.style.as_deref())
                .or_else(|| {
                    let column = odf_table.columns.get(c as usize)?;
                    style_of(
                        source,
                        odf::Family::TableCell,
                        column.default_cell_style.as_deref(),
                    )
                })
            else {
                continue;
            };
            let p = &style.properties;
            let t = &p.text;
            let look = &mut target.style;
            look.fill = p.table.background.map(|c| CellFill::Color(model_color(c)));
            look.v_align = match p.table.vertical_align.as_deref() {
                Some("middle") => VAlign::Middle,
                Some("bottom") => VAlign::Bottom,
                _ => VAlign::Top,
            };
            look.h_align = match p.paragraph.align {
                Some(odf::TextAlign::Start | odf::TextAlign::Left) => HAlign::Left,
                Some(odf::TextAlign::Center) => HAlign::Center,
                Some(odf::TextAlign::End | odf::TextAlign::Right) => HAlign::Right,
                Some(odf::TextAlign::Justify) => HAlign::Justify,
                None => HAlign::Auto,
            };
            look.text.font.clone_from(&t.font_family);
            look.text.size = t.font_size.map(thousandths);
            look.text.bold = t.font_weight.is_some_and(|w| w >= 600);
            look.text.italic = t.italic == Some(true);
            look.text.underline =
                matches!(t.underline, Some(odf::Line::Single | odf::Line::Double));
            look.text.strike =
                matches!(t.line_through, Some(odf::Line::Single | odf::Line::Double));
            look.text.color = t.color.map(model_color);
            let tp = &p.table;
            table.set_h_border(r, c, model_border(tp.border_top));
            table.set_v_border(r, c, model_border(tp.border_left));
            let span_rows = cell.rows_spanned.max(1).min(rows - r);
            let span_cols = cell.columns_spanned.max(1).min(cols - c);
            for cc in c..c + span_cols {
                if tp.border_top.is_some() {
                    table.set_h_border(r, cc, model_border(tp.border_top));
                }
                if tp.border_bottom.is_some() {
                    table.set_h_border(r + span_rows, cc, model_border(tp.border_bottom));
                }
            }
            for rr in r..r + span_rows {
                if tp.border_left.is_some() {
                    table.set_v_border(rr, c, model_border(tp.border_left));
                }
                if tp.border_right.is_some() {
                    table.set_v_border(rr, c + span_cols, model_border(tp.border_right));
                }
            }
        }
    }
    let locale = odf_locale(source);
    for area in merges {
        let _ = table.merge(area, &locale);
    }
    table
}

/// A chart of the model as ODF.
pub(crate) fn export_chart(chart: &Chart, width: f64, height: f64) -> odf::ChartDocument {
    let class = match chart.kind {
        ChartKind::Column | ChartKind::StackedColumn | ChartKind::Bar | ChartKind::StackedBar => {
            "bar"
        }
        ChartKind::Line => "line",
        ChartKind::Area | ChartKind::StackedArea => "area",
        ChartKind::Pie => "circle",
        ChartKind::Scatter => "scatter",
        ChartKind::Mixed | ChartKind::TwoAxis => "bar",
    };
    let mut out = odf::ChartDocument {
        class: class.into(),
        width: Some(odf::Length(width)),
        height: Some(odf::Length(height)),
        title: chart.title.clone(),
        legend: chart.legend.then(|| "end".into()),
        horizontal: chart.kind.is_horizontal(),
        stacked: chart.kind.is_stacked(),
        three_d: chart.three_d,
        categories: (0..chart.category_count())
            .map(|c| chart.category_name(c).to_owned())
            .collect(),
        ..odf::ChartDocument::default()
    };
    let n = chart.series_count();
    let values = |s: usize| -> Vec<Option<f64>> {
        (0..chart.category_count())
            .map(|c| chart.value(s, c))
            .collect()
    };
    if chart.kind == ChartKind::Scatter {
        for s in 0..n / 2 {
            out.series.push(odf::ChartSeries {
                name: chart.series_name(2 * s + 1).to_owned(),
                values: values(2 * s + 1),
                x_values: Some(values(2 * s)),
                color: Some(odf_color(chart.style(2 * s + 1).color())),
                ..odf::ChartSeries::default()
            });
        }
    } else {
        for s in 0..n {
            let class = match (chart.kind, chart.style(s).kind) {
                (ChartKind::Mixed | ChartKind::TwoAxis, desk_chart::SeriesKind::Line) => {
                    Some("line".to_owned())
                }
                (ChartKind::Mixed | ChartKind::TwoAxis, desk_chart::SeriesKind::Area) => {
                    Some("area".to_owned())
                }
                _ => None,
            };
            out.series.push(odf::ChartSeries {
                name: chart.series_name(s).to_owned(),
                class,
                values: values(s),
                color: Some(odf_color(chart.style(s).color())),
                ..odf::ChartSeries::default()
            });
        }
    }
    out
}

/// A chart of the model from ODF: what Desk wrote of it, or what the
/// standard keeps.
pub(crate) fn import_chart(
    desk: Option<&str>,
    odf_chart: Option<&odf::ChartDocument>,
) -> Option<Chart> {
    // What Desk wrote, when its data has a value for each row and column.
    if let Some(chart) = desk
        .and_then(|d| serde_json::from_str::<Chart>(d).ok())
        .filter(|c| c.data.is_consistent())
    {
        return Some(chart);
    }
    let source = odf_chart?;
    let kind = match (source.class.as_str(), source.horizontal, source.stacked) {
        ("bar", false, false) => ChartKind::Column,
        ("bar", false, true) => ChartKind::StackedColumn,
        ("bar", true, false) => ChartKind::Bar,
        ("bar", true, true) => ChartKind::StackedBar,
        ("line", ..) => ChartKind::Line,
        ("area", _, false) => ChartKind::Area,
        ("area", _, true) => ChartKind::StackedArea,
        ("circle" | "ring", ..) => ChartKind::Pie,
        ("scatter" | "bubble", ..) => ChartKind::Scatter,
        _ => ChartKind::Column,
    };
    let mut chart = Chart::new(kind);
    chart.three_d = source.three_d && kind.has_3d();
    chart.title = source.title.clone();
    chart.legend = source.legend.is_some();
    let categories = source
        .series
        .iter()
        .map(|s| s.values.len())
        .max()
        .unwrap_or(0)
        .max(source.categories.len());
    let mut data = ChartData {
        rows: (0..categories)
            .map(|i| {
                source
                    .categories
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| (i + 1).to_string())
            })
            .collect(),
        cols: Vec::new(),
        values: vec![Vec::new(); categories],
    };
    let mut push = |name: String, values: &[Option<f64>]| {
        data.cols.push(name);
        for (i, row) in data.values.iter_mut().enumerate() {
            row.push(values.get(i).copied().flatten().map(Num));
        }
    };
    for s in &source.series {
        if kind == ChartKind::Scatter {
            let x = s
                .x_values
                .clone()
                .unwrap_or_else(|| (1..=s.values.len()).map(|i| Some(i as f64)).collect());
            push(format!("{} X", s.name), &x);
        }
        push(s.name.clone(), &s.values);
    }
    chart.data = data;
    chart.series_in_rows = false;
    let mut series = Vec::new();
    for (i, s) in source.series.iter().enumerate() {
        let index = if kind == ChartKind::Scatter {
            2 * i + 1
        } else {
            i
        };
        while series.len() <= index {
            series.push(chart.style(series.len()));
        }
        if let Some(c) = s.color {
            series[index].fill = desk_chart::SeriesFill::Color(model_color(c));
        }
    }
    chart.series = series;
    Some(chart)
}

/// The cell style of a part of a table style: its fill, its lines, the
/// room in its cells and the font of its text.
fn template_cell_style(
    name: String,
    fill: Option<Color>,
    look: &desk_model::TableLook,
) -> odf::Style {
    let mut properties = odf::Properties::default();
    let border = odf_border(look.border).or(Some(odf::Border::None));
    let t = &mut properties.table;
    t.background = fill.map(odf_color);
    t.border_top = border;
    t.border_bottom = border;
    t.border_left = border;
    t.border_right = border;
    t.padding = Some(pt(look.style.padding));
    properties.text.font_family = Some(look.style.font.clone());
    properties.text.font_size = Some(pt(look.style.font_size));
    odf::Style {
        name,
        family: odf::Family::TableCell,
        properties,
        ..odf::Style::default()
    }
}

/// The table styles of a document, as the table templates of ODF and the
/// cell styles of their parts (PXV-14). Their names start with
/// `Desk_Table_Style_`, which no other style of Desk takes.
pub(crate) fn export_table_styles(document: &Document, out: &mut odf::TextDocument) {
    for (i, entry) in document.table_styles().iter().enumerate() {
        let look = &entry.look;
        let s = &look.style;
        let prefix = format!("Desk_Table_Style_{}", i + 1);
        let mut part = |suffix: &str, fill: Option<Color>| {
            let name = format!("{prefix}.{suffix}");
            out.styles
                .push(template_cell_style(name.clone(), fill, look));
            Some(name)
        };
        let template = odf::TableTemplate {
            name: entry.name.to_string(),
            first_row: part("FirstRow", s.header_row_fill),
            first_column: part("FirstColumn", s.header_column_fill),
            last_row: part("LastRow", s.footer_fill),
            even_rows: s.alternating.and_then(|c| part("EvenRows", Some(c))),
            body: part("Body", s.body_fill),
        };
        out.table_templates.push(template);
    }
}

/// The table styles of a file, from its table templates, in place of those
/// of the document (PXV-14). A style of the library keeps its identifier;
/// a file with none leaves the document with its own.
pub(crate) fn import_table_styles(
    source: &odf::TextDocument,
    document: &mut Document,
) -> Result<(), crate::FilterError> {
    if source.table_templates.is_empty() {
        return Ok(());
    }
    let cell = |name: &Option<String>| {
        let name = name.as_deref()?;
        source
            .styles
            .iter()
            .find(|s| s.family == odf::Family::TableCell && s.name == name)
    };
    let mut entries: Vec<desk_model::TableStyleEntry> = Vec::new();
    for template in &source.table_templates {
        if entries.iter().any(|e| *e.name == *template.name) {
            continue;
        }
        let fill = |name: &Option<String>| {
            cell(name).and_then(|s| s.properties.table.background.map(model_color))
        };
        let mut look = desk_model::TableLook::default();
        let s = &mut look.style;
        s.header_row_fill = fill(&template.first_row);
        s.header_column_fill = fill(&template.first_column);
        s.footer_fill = fill(&template.last_row);
        s.body_fill = fill(&template.body);
        s.alternating = fill(&template.even_rows);
        let body = cell(&template.body).or_else(|| cell(&template.first_row));
        if let Some(body) = body {
            let p = &body.properties;
            if let Some(font) = &p.text.font_family {
                s.font.clone_from(font);
            }
            if let Some(size) = p.text.font_size {
                s.font_size = thousandths(size).clamp(1_000, 1_638_000);
            }
            if let Some(padding) = p.table.padding {
                s.padding = thousandths(padding).min(100_000);
            }
            look.border = model_border(p.table.border_top);
        }
        let own = document
            .table_styles()
            .iter()
            .find(|e| *e.name == *template.name)
            .map(|e| e.id);
        let id = own.unwrap_or_else(|| document.new_id());
        entries.push(desk_model::TableStyleEntry {
            id,
            name: template.name.as_str().into(),
            look,
        });
    }
    document.apply(desk_model::Op::SetTableStyles {
        styles: desk_model::TableStyles::from_entries(entries),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates_and_durations() {
        assert_eq!(iso_date(45_658.5), "2025-01-01T12:00:00");
        assert_eq!(parse_iso_date("2025-01-01T12:00:00"), Some(45_658.5));
        assert_eq!(parse_iso_duration(&iso_duration(1.25)), Some(1.25));
        assert_eq!(parse_iso_duration("PT1H30M"), Some(1.5 / 24.0));
    }

    #[test]
    fn merged_cells_are_joined_in_the_language_of_the_document() {
        let mut source = odf::TextDocument::default();
        source.default_style.text.language = Some("pt".into());
        source.default_style.text.country = Some("BR".into());
        let locale = odf_locale(&source);
        let mut table = Table::new(1, 2);
        table.set_header_rows(0).unwrap();
        table.set_header_cols(0).unwrap();
        table.type_text(0, 0, "Preço", &locale);
        table.type_text(0, 1, "2,5", &locale);
        table
            .merge(desk_model::table::Area::between((0, 0), (0, 1)), &locale)
            .unwrap();
        assert_eq!(table.source_text(0, 0, &locale), "Preço\t2,5");
        // An undetermined language is none, as in the model.
        source.default_style.text.language = Some("und".into());
        let system = Locale::from_tag(&std::env::var("LANG").unwrap_or_default());
        assert_eq!(odf_locale(&source), system);
    }
}
