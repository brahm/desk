// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: MPL-2.0

//! Charts: the embedded objects (`draw:object`) whose package entry holds
//! an `office:chart`, with their data in a table of their own.

use crate::budget::{self, Cost};
use crate::error::OdfError;
use crate::ns::{CHART, DESK, DRAW, OFFICE, STYLE, SVG, TABLE, TEXT};
use crate::units::{Color, Length};
use crate::write::VERSION;
use crate::xml::{Element, Writer, parse};

/// The media type of a chart object.
pub const CHART_TYPE: &str = "application/vnd.oasis.opendocument.chart";

/// A chart, as the standard keeps it: what other programs read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartDocument {
    /// `chart:class`, without its prefix: `bar`, `line`, `area`,
    /// `circle`, `ring`, `scatter`, `bubble`, `radar` or `stock`.
    pub class: String,
    /// `svg:width` of the chart.
    pub width: Option<Length>,
    /// `svg:height` of the chart.
    pub height: Option<Length>,
    /// `chart:title`.
    pub title: Option<String>,
    /// `chart:legend-position` of its legend, when it has one: `start`,
    /// `end`, `top` or `bottom`.
    pub legend: Option<String>,
    /// `chart:vertical`: the bars lie across.
    pub horizontal: bool,
    /// `chart:stacked`.
    pub stacked: bool,
    /// `chart:percentage`.
    pub percentage: bool,
    /// `chart:three-dimensional`.
    pub three_d: bool,
    /// The title of the axis of categories.
    pub x_title: Option<String>,
    /// The title of the axis of values.
    pub y_title: Option<String>,
    /// The names of the categories.
    pub categories: Vec<String>,
    /// The series.
    pub series: Vec<ChartSeries>,
}

/// A series of a chart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartSeries {
    /// Its name.
    pub name: String,
    /// `chart:class` of the series, when it differs from the chart's.
    pub class: Option<String>,
    /// The values, one per category.
    pub values: Vec<Option<f64>>,
    /// The values along x of a scatter or bubble chart (`chart:domain`).
    pub x_values: Option<Vec<Option<f64>>>,
    /// The color it is filled with.
    pub color: Option<Color>,
}

/// A cell of the data of a chart: its text or its number.
#[derive(Clone, Debug, PartialEq)]
enum Datum {
    Text(String),
    Number(f64),
    Empty,
}

/// Parses `$B$2`, as a column and a row from 0.
fn cell_ref(s: &str) -> Option<(usize, usize)> {
    let s = s.trim().trim_start_matches('.').replace('$', "");
    let letters: String = s.chars().take_while(char::is_ascii_alphabetic).collect();
    let digits = &s[letters.len()..];
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let col = letters
        .to_ascii_uppercase()
        .bytes()
        .fold(0usize, |acc, b| acc * 26 + usize::from(b - b'A' + 1))
        - 1;
    let row = digits.parse::<usize>().ok()?.checked_sub(1)?;
    Some((col, row))
}

/// The cells of a range of the data table such as
/// `local-table.$B$2:.$B$5`, by column and row, in order: cut to the grid
/// that holds the data, and to at most `limit` cells.
fn range(address: &str, grid: &[Vec<Datum>], limit: usize) -> Vec<(usize, usize)> {
    let address = address.split_whitespace().next().unwrap_or_default();
    let mut parts = address.split(':').map(|p| match p.rfind('.') {
        Some(i) => &p[i + 1..],
        None => p,
    });
    let Some(start) = parts.next().and_then(cell_ref) else {
        return Vec::new();
    };
    let end = parts.next().and_then(cell_ref).unwrap_or(start);
    // Past the grid every cell is empty: a range is read as far as the
    // grid goes, and at least its first cell.
    let last_row = end.1.min(grid.len().saturating_sub(1)).max(start.1);
    let width = grid.iter().map(Vec::len).max().unwrap_or(0);
    let last_col = end.0.min(width.saturating_sub(1)).max(start.0);
    let mut out = Vec::new();
    for row in start.1..=last_row {
        for col in start.0..=last_col {
            if out.len() >= limit {
                return out;
            }
            out.push((col, row));
        }
    }
    out
}

fn column_name(mut col: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

fn address(col: usize, row: usize) -> String {
    format!("${}${}", column_name(col), row + 1)
}

fn local_range(from: (usize, usize), to: (usize, usize)) -> String {
    format!(
        "local-table.{}:.{}",
        address(from.0, from.1),
        address(to.0, to.1)
    )
}

/// The rows of the data table of a chart, within what the charts of the
/// document may still read.
fn grid(table: &Element) -> Vec<Vec<Datum>> {
    let mut rows = Vec::new();
    collect_rows(table, &mut rows);
    rows
}

fn datum_bytes(datum: &Datum) -> usize {
    match datum {
        Datum::Text(text) => text.len(),
        _ => 0,
    }
}

fn collect_rows(e: &Element, rows: &mut Vec<Vec<Datum>>) {
    for child in e.elements() {
        if child.name.is(TABLE, "table-row") {
            if rows.len() >= 10_000 {
                return;
            }
            let mut row: Vec<Datum> = Vec::new();
            for cell in child.elements() {
                if !cell.name.is(TABLE, "table-cell") && !cell.name.is(TABLE, "covered-table-cell")
                {
                    continue;
                }
                let datum = match cell.attr(OFFICE, "value-type") {
                    Some("float" | "percentage" | "currency") => cell
                        .attr(OFFICE, "value")
                        .and_then(|v| v.trim().parse().ok())
                        .map_or(Datum::Empty, Datum::Number),
                    _ => {
                        let text: Vec<String> =
                            cell.children_named(TEXT, "p").map(Element::text).collect();
                        if text.is_empty() {
                            Datum::Empty
                        } else {
                            Datum::Text(text.join("\n"))
                        }
                    }
                };
                let n = cell
                    .attr(TABLE, "number-columns-repeated")
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 1024);
                let cost = Cost {
                    cells: 1,
                    bytes: datum_bytes(&datum),
                };
                let wanted = n.min(1024 - row.len().min(1024));
                // The cell as written costs a cell; only its copies cost
                // its bytes again.
                let n = match budget::take_chart(wanted.min(1), Cost { cells: 1, bytes: 0 }) {
                    0 => 0,
                    _ => 1 + budget::take_chart(wanted - 1, cost),
                };
                for _ in 0..n {
                    row.push(datum.clone());
                }
                // A cell refused ends the row, or the next would move
                // into its column.
                if n < wanted {
                    break;
                }
            }
            // A row written once for several the same, as ODF allows.
            let repeated = child
                .attr(TABLE, "number-rows-repeated")
                .and_then(|n| n.trim().parse::<usize>().ok())
                .unwrap_or(1)
                .clamp(1, 10_000 - rows.len());
            // The row is counted once read; each other copy costs it again.
            let cost = Cost {
                cells: row.len().max(1),
                bytes: row.iter().map(datum_bytes).sum(),
            };
            let repeated = 1 + budget::take_chart(repeated - 1, cost);
            for _ in 0..repeated {
                rows.push(row.clone());
            }
        } else if child.name.ns == TABLE {
            collect_rows(child, rows);
        }
    }
}

/// The values of a range, within what the charts of the document may
/// still read; the text they copy is counted too.
fn range_values<T>(address: &str, grid: &[Vec<Datum>], most: usize, of: fn(&Datum) -> T) -> Vec<T> {
    let cells = range(address, grid, most.min(budget::chart_values_left()));
    let mut left = budget::bytes_left();
    let mut out = Vec::with_capacity(cells.len());
    for at in cells {
        let d = datum(grid, at);
        let bytes = datum_bytes(d);
        if bytes > left {
            break;
        }
        left -= bytes;
        out.push(of(d));
    }
    budget::spend_chart_values(out.len(), budget::bytes_left() - left);
    out
}

fn datum(grid: &[Vec<Datum>], (col, row): (usize, usize)) -> &Datum {
    grid.get(row)
        .and_then(|r| r.get(col))
        .unwrap_or(&Datum::Empty)
}

fn text_of(d: &Datum) -> String {
    match d {
        Datum::Text(t) => t.clone(),
        Datum::Number(n) => n.to_string(),
        Datum::Empty => String::new(),
    }
}

fn number_of(d: &Datum) -> Option<f64> {
    match d {
        Datum::Number(n) => Some(*n),
        Datum::Text(t) => t.trim().parse().ok(),
        Datum::Empty => None,
    }
}

fn title(e: Option<&Element>) -> Option<String> {
    let e = e?;
    let text: Vec<String> = e.children_named(TEXT, "p").map(Element::text).collect();
    Some(text.join("\n"))
}

/// Reads the `content.xml` of a chart object.
pub(crate) fn read_chart(content: &str) -> Result<Option<ChartDocument>, OdfError> {
    let root = parse(content)?;
    let Some(chart) = root
        .child(OFFICE, "body")
        .and_then(|b| b.child(OFFICE, "chart"))
        .and_then(|c| c.child(CHART, "chart"))
    else {
        return Ok(None);
    };
    // The properties of the styles that the chart names.
    let styles = root.child(OFFICE, "automatic-styles");
    let style = |name: Option<&str>| -> Option<&Element> {
        let name = name?;
        styles?
            .children_named(STYLE, "style")
            .find(|s| s.attr(STYLE, "name") == Some(name))
    };
    let class = |c: Option<&str>| c.map(|c| c.rsplit(':').next().unwrap_or(c).to_owned());
    let mut out = ChartDocument {
        class: class(chart.attr(CHART, "class")).unwrap_or_else(|| "bar".into()),
        width: chart.attr(SVG, "width").and_then(Length::parse),
        height: chart.attr(SVG, "height").and_then(Length::parse),
        title: title(chart.child(CHART, "title")),
        legend: chart
            .child(CHART, "legend")
            .map(|l| l.attr(CHART, "legend-position").unwrap_or("end").to_owned()),
        ..ChartDocument::default()
    };
    let Some(plot) = chart.child(CHART, "plot-area") else {
        return Ok(Some(out));
    };
    if let Some(props) =
        style(plot.attr(CHART, "style-name")).and_then(|s| s.child(STYLE, "chart-properties"))
    {
        let flag = |name| props.attr(CHART, name) == Some("true");
        out.horizontal = flag("vertical");
        out.stacked = flag("stacked");
        out.percentage = flag("percentage");
        out.three_d = flag("three-dimensional");
    }
    out.three_d |=
        plot.attr(DR3D_NS, "projection").is_some() || plot.child(DR3D_NS, "light").is_some();
    let table = chart.child(TABLE, "table");
    let grid = table.map(grid).unwrap_or_default();
    for axis in plot.children_named(CHART, "axis") {
        let dimension = axis.attr(CHART, "dimension");
        let t = title(axis.child(CHART, "title"));
        if dimension == Some("x") {
            out.x_title = t;
            if let Some(categories) = axis
                .child(CHART, "categories")
                .and_then(|c| c.attr(TABLE, "cell-range-address"))
            {
                out.categories = range_values(categories, &grid, usize::MAX, text_of);
            }
        } else if dimension == Some("y") && out.y_title.is_none() {
            out.y_title = t;
        }
    }
    for series in plot.children_named(CHART, "series") {
        let values: Vec<Option<f64>> = series
            .attr(CHART, "values-cell-range-address")
            .map(|a| range_values(a, &grid, usize::MAX, number_of))
            .unwrap_or_default();
        let name = series
            .attr(CHART, "label-cell-address")
            .and_then(|a| range_values(a, &grid, 1, text_of).into_iter().next())
            .unwrap_or_default();
        let x_values = series
            .child(CHART, "domain")
            .and_then(|d| d.attr(TABLE, "cell-range-address"))
            .map(|a| range_values(a, &grid, usize::MAX, number_of));
        let color = style(series.attr(CHART, "style-name"))
            .and_then(|s| s.child(STYLE, "graphic-properties"))
            .and_then(|g| {
                g.attr(DRAW, "fill-color")
                    .or_else(|| g.attr(SVG, "stroke-color"))
            })
            .and_then(Color::parse);
        out.series.push(ChartSeries {
            name,
            class: class(series.attr(CHART, "class")).filter(|c| *c != out.class),
            values,
            x_values,
            color,
        });
    }
    // Charts with no ranges keep their data in the order of the table:
    // names in the first row, categories in the first column.
    if out.series.is_empty() && grid.len() > 1 {
        let columns = grid.iter().map(Vec::len).max().unwrap_or(0);
        out.categories = grid
            .iter()
            .skip(1)
            .map(|r| text_of(r.first().unwrap_or(&Datum::Empty)))
            .collect();
        for col in 1..columns {
            out.series.push(ChartSeries {
                name: text_of(datum(&grid, (col, 0))),
                values: (1..grid.len())
                    .map(|r| number_of(datum(&grid, (col, r))))
                    .collect(),
                ..ChartSeries::default()
            });
        }
    }
    Ok(Some(out))
}

const DR3D_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:dr3d:1.0";

fn root(w: &mut Writer, name: &'static str) {
    w.start(name)
        .attr("xmlns:office", OFFICE)
        .attr("xmlns:style", STYLE)
        .attr("xmlns:text", TEXT)
        .attr("xmlns:table", TABLE)
        .attr("xmlns:draw", DRAW)
        .attr("xmlns:fo", crate::ns::FO)
        .attr("xmlns:svg", SVG)
        .attr("xmlns:chart", CHART)
        .attr("xmlns:desk", DESK)
        .attr("office:version", VERSION);
}

fn text_cell(w: &mut Writer, text: &str) {
    w.start("table:table-cell")
        .attr("office:value-type", "string");
    w.text_element("text:p", text);
    w.end();
}

fn number_cell(w: &mut Writer, value: Option<f64>) {
    w.start("table:table-cell");
    if let Some(v) = value {
        w.attr("office:value-type", "float")
            .attr("office:value", &v.to_string());
        w.text_element("text:p", &v.to_string());
    }
    w.end();
}

/// Writes the `content.xml` and `styles.xml` of a chart object.
pub(crate) fn write_chart(chart: &ChartDocument) -> (String, String) {
    let mut w = Writer::new();
    root(&mut w, "office:document-content");
    w.start("office:automatic-styles");
    w.start("style:style")
        .attr("style:name", "pa")
        .attr("style:family", "chart");
    w.start("style:chart-properties");
    for (name, on) in [
        ("chart:vertical", chart.horizontal),
        ("chart:stacked", chart.stacked),
        ("chart:percentage", chart.percentage),
        ("chart:three-dimensional", chart.three_d),
    ] {
        if on {
            w.attr(name, "true");
        }
    }
    w.end().end();
    for (i, series) in chart.series.iter().enumerate() {
        w.start("style:style")
            .attr("style:name", &format!("s{}", i + 1))
            .attr("style:family", "chart");
        if let Some(color) = series.color {
            w.start("style:graphic-properties")
                .attr("draw:fill", "solid")
                .attr("draw:fill-color", &color.to_string())
                .attr("svg:stroke-color", &color.to_string())
                .end();
        }
        w.end();
    }
    w.end();
    w.start("office:body").start("office:chart");
    w.start("chart:chart")
        .attr("chart:class", &format!("chart:{}", chart.class));
    if let Some(width) = chart.width {
        w.attr("svg:width", &width.to_string());
    }
    if let Some(height) = chart.height {
        w.attr("svg:height", &height.to_string());
    }
    if let Some(title) = &chart.title {
        w.start("chart:title");
        w.text_element("text:p", title);
        w.end();
    }
    if let Some(position) = &chart.legend {
        w.start("chart:legend")
            .attr("chart:legend-position", position)
            .end();
    }
    // The table: names in the first row, categories in the first column,
    // and for each series its values along x, if it has them, and its
    // values.
    let rows = chart
        .series
        .iter()
        .map(|s| s.values.len().max(s.x_values.as_ref().map_or(0, Vec::len)))
        .max()
        .unwrap_or(0)
        .max(chart.categories.len());
    let mut columns = Vec::new();
    let mut col: usize = 1;
    for s in &chart.series {
        let x = s.x_values.as_ref().map(|_| {
            col += 1;
            col - 1
        });
        columns.push((x, col));
        col += 1;
    }
    w.start("chart:plot-area")
        .attr("chart:style-name", "pa")
        .attr(
            "table:cell-range-address",
            &local_range((0, 0), (col.saturating_sub(1), rows)),
        )
        .attr("chart:data-source-has-labels", "both");
    w.start("chart:axis")
        .attr("chart:dimension", "x")
        .attr("chart:name", "primary-x");
    if let Some(t) = &chart.x_title {
        w.start("chart:title");
        w.text_element("text:p", t);
        w.end();
    }
    if rows > 0 && !chart.categories.is_empty() {
        w.start("chart:categories")
            .attr("table:cell-range-address", &local_range((0, 1), (0, rows)))
            .end();
    }
    w.end();
    w.start("chart:axis")
        .attr("chart:dimension", "y")
        .attr("chart:name", "primary-y");
    if let Some(t) = &chart.y_title {
        w.start("chart:title");
        w.text_element("text:p", t);
        w.end();
    }
    w.start("chart:grid").attr("chart:class", "major").end();
    w.end();
    for (i, (s, (x, col))) in chart.series.iter().zip(&columns).enumerate() {
        w.start("chart:series")
            .attr("chart:style-name", &format!("s{}", i + 1))
            .attr(
                "chart:values-cell-range-address",
                &local_range((*col, 1), (*col, rows.max(1))),
            )
            .attr(
                "chart:label-cell-address",
                &local_range((*col, 0), (*col, 0)),
            );
        if let Some(class) = &s.class {
            w.attr("chart:class", &format!("chart:{class}"));
        }
        if let Some(x) = x {
            w.start("chart:domain")
                .attr(
                    "table:cell-range-address",
                    &local_range((*x, 1), (*x, rows.max(1))),
                )
                .end();
        }
        w.end();
    }
    w.end();
    w.start("table:table").attr("table:name", "local-table");
    w.start("table:table-header-columns");
    w.start("table:table-column").end();
    w.end();
    w.start("table:table-columns");
    w.start("table:table-column")
        .attr(
            "table:number-columns-repeated",
            &col.saturating_sub(1).max(1).to_string(),
        )
        .end();
    w.end();
    w.start("table:table-header-rows");
    w.start("table:table-row");
    text_cell(&mut w, "");
    for (s, (x, _)) in chart.series.iter().zip(&columns) {
        if x.is_some() {
            text_cell(&mut w, &format!("{} X", s.name));
        }
        text_cell(&mut w, &s.name);
    }
    w.end();
    w.end();
    w.start("table:table-rows");
    for r in 0..rows {
        w.start("table:table-row");
        text_cell(&mut w, chart.categories.get(r).map_or("", String::as_str));
        for (s, (x, _)) in chart.series.iter().zip(&columns) {
            if x.is_some() {
                number_cell(
                    &mut w,
                    s.x_values
                        .as_ref()
                        .and_then(|v| v.get(r).copied().flatten()),
                );
            }
            number_cell(&mut w, s.values.get(r).copied().flatten());
        }
        w.end();
    }
    w.end();
    w.end();
    w.end().end().end().end();
    let content = w.finish();
    let mut w = Writer::new();
    root(&mut w, "office:document-styles");
    w.start("office:styles").end();
    w.end();
    (content, w.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_survive_the_trip() {
        let chart = ChartDocument {
            class: "bar".into(),
            title: Some("Vendas".into()),
            legend: Some("end".into()),
            stacked: true,
            categories: vec!["Jan".into(), "Fev".into()],
            series: vec![
                ChartSeries {
                    name: "Norte".into(),
                    values: vec![Some(1.0), Some(2.5)],
                    color: Some(Color { r: 255, g: 0, b: 0 }),
                    ..ChartSeries::default()
                },
                ChartSeries {
                    name: "Sul".into(),
                    values: vec![None, Some(4.0)],
                    x_values: Some(vec![Some(1.0), Some(2.0)]),
                    ..ChartSeries::default()
                },
            ],
            ..ChartDocument::default()
        };
        let (content, _) = write_chart(&chart);
        assert_eq!(read_chart(&content).unwrap().unwrap(), chart);
        assert_eq!(column_name(27), "AB");
        assert_eq!(cell_ref("$AB$3"), Some((27, 2)));
    }

    /// The content of a chart with one series over `rows`, whose data
    /// table is `table`.
    fn chart_with(rows: &str, table: &str) -> String {
        format!(
            r#"<office:document-content xmlns:office="{OFFICE}" xmlns:chart="{CHART}" xmlns:table="{TABLE}" xmlns:text="{TEXT}"><office:body><office:chart><chart:chart chart:class="chart:bar"><chart:plot-area><chart:series chart:values-cell-range-address="local-table.$B$2:.$B${rows}"/></chart:plot-area><table:table table:name="local-table">{table}</table:table></chart:chart></office:chart></office:body></office:document-content>"#
        )
    }

    fn row(repeated: usize, value: f64) -> String {
        format!(
            r#"<table:table-row table:number-rows-repeated="{repeated}"><table:table-cell><text:p>x</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="{value}"/></table:table-row>"#
        )
    }

    #[test]
    fn rows_written_once_for_several_are_read_as_several() {
        // Two rows the same, written as one with number-rows-repeated:
        // the rows after it moved up and the series lost a value (review
        // RV, batch 1).
        let header = row(1, 0.0);
        let content = chart_with("4", &format!("{header}{}{}", row(2, 1.0), row(1, 2.0)));
        let chart = read_chart(&content).unwrap().unwrap();
        assert_eq!(chart.series[0].values, [Some(1.0), Some(1.0), Some(2.0)]);
    }

    #[test]
    fn a_chart_that_repeats_its_cells_without_end_is_read_within_bounds() {
        // A row repeated 10,000 times of 1,024 cells, and a series over a
        // range far past the data: each was expanded in full, about ten
        // million cells from a few hundred bytes.
        let wide = r#"<table:table-row table:number-rows-repeated="10000"><table:table-cell table:number-columns-repeated="1024" office:value-type="float" office:value="1"/></table:table-row>"#;
        let content = chart_with("1048576", wide);
        let chart = read_chart(&content).unwrap().unwrap();
        assert!(chart.series[0].values.len() <= budget::MAX_CHART_CELLS);
        let grid = vec![vec![Datum::Empty; 3]; 2];
        assert_eq!(range("$A$1:$AMJ$1048576", &grid, 10).len(), 6);
        assert_eq!(range("$A$1:$AMJ$1048576", &grid, 4).len(), 4);
    }

    #[test]
    fn the_charts_of_a_document_share_what_they_may_read() {
        // Each chart could read a million cells, and a document can hold
        // a thousand charts (review of batch 1).
        let wide = r#"<table:table-row table:number-rows-repeated="10000"><table:table-cell table:number-columns-repeated="1024" office:value-type="float" office:value="1"/></table:table-row>"#;
        let content = chart_with("10000", wide);
        budget::start_document();
        let first = read_chart(&content).unwrap().unwrap();
        let second = read_chart(&content).unwrap().unwrap();
        assert!(!first.series[0].values.is_empty());
        assert!(second.series[0].values.len() <= 1);
    }

    #[test]
    fn a_large_chart_keeps_all_its_series() {
        // The ranges of the series spent the budget of the data table, so
        // a chart of 700 series over 1,000 rows lost series without a
        // word (review of batch 1).
        let cells = r#"<table:table-cell office:value-type="float" office:value="1"/>"#;
        let table = format!(
            r#"<table:table-row table:number-rows-repeated="1000">{}</table:table-row>"#,
            cells.repeat(700)
        );
        let series: String = (0..700)
            .map(|i| {
                let col = column_name(i);
                format!(r#"<chart:series chart:values-cell-range-address="local-table.${col}$1:.${col}$1000"/>"#)
            })
            .collect();
        let content = format!(
            r#"<office:document-content xmlns:office="{OFFICE}" xmlns:chart="{CHART}" xmlns:table="{TABLE}" xmlns:text="{TEXT}"><office:body><office:chart><chart:chart chart:class="chart:bar"><chart:plot-area>{series}</chart:plot-area><table:table table:name="local-table">{table}</table:table></chart:chart></office:chart></office:body></office:document-content>"#
        );
        budget::start_document();
        let chart = read_chart(&content).unwrap().unwrap();
        assert_eq!(chart.series.len(), 700);
        assert!(chart.series.iter().all(|s| s.values.len() == 1000));
    }

    #[test]
    fn a_cell_refused_ends_its_row() {
        // The cells after a refused one moved into its column: the
        // number of a series came out in the column of the text (review
        // of batch 1). Here the copies of the text cannot be paid for.
        budget::start_document();
        budget::take_table(
            1,
            Cost {
                cells: 0,
                bytes: budget::MAX_DOCUMENT_BYTES - 5,
            },
        );
        let row = r#"<table:table-row><table:table-cell table:number-columns-repeated="3"><text:p>xxxxxxxxxx</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="5"/></table:table-row>"#;
        let content = chart_with("1", row).replace("$B$2:.$B$1", "$B$1:.$B$1");
        let chart = read_chart(&content).unwrap().unwrap();
        assert_eq!(chart.series[0].values, [None]);
    }
}
