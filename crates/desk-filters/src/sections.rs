// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Sections and layouts of the model in ODF (EST-07 to EST-11, EST-16,
//! EST-18).
//!
//! A layout is a `text:section` whose section style has the columns and
//! the margins of the layout, with its space above and below and whether
//! it starts a page as `desk:` attributes. A section of the model is a
//! layout whose style also says which page it starts on, and whose first
//! paragraph breaks the page and sets the number of the page, which other
//! programs read. The first layout of the document, when it is not the
//! plain one, is a section around the first paragraphs.

use desk_model::{Columns, Layout, Length, MAX_COLUMNS, PARTS, PageSetup, Section, SectionStart};
use desk_odf as odf;

/// The width of the text of a layout on the pages of a setup, in points.
fn layout_width(setup: &PageSetup, margin_left: Length, margin_right: Length) -> f64 {
    let width = setup.width - setup.margin_left - setup.margin_right - margin_left - margin_right;
    width.to_pt().max(1.0)
}

fn odf_length(value: Length) -> odf::Length {
    odf::Length(value.to_pt())
}

fn length(value: odf::Length) -> Length {
    Length::from_pt(value.0.max(0.0))
}

/// The columns of ODF for columns of the model `width` points wide.
pub fn odf_columns(columns: &Columns, width: f64) -> Option<odf::Columns> {
    if columns.count < 2 {
        return None;
    }
    let spans = columns.spans(f64::from(PARTS));
    let gaps: Vec<f64> = spans
        .windows(2)
        .map(|pair| pair[1].0 - (pair[0].0 + pair[0].1))
        .collect();
    let to_points = |parts: f64| odf::Length(parts / f64::from(PARTS) * width);
    let columns_out = spans
        .iter()
        .enumerate()
        .map(|(i, &(_, w))| {
            let before = if i == 0 { 0.0 } else { gaps[i - 1] / 2.0 };
            let after = gaps.get(i).map_or(0.0, |g| g / 2.0);
            odf::Column {
                rel_width: (w + before + after).round().max(1.0) as u32,
                start_indent: to_points(before),
                end_indent: to_points(after),
            }
        })
        .collect();
    Some(odf::Columns {
        count: columns.count,
        gap: columns
            .equal
            .then(|| to_points(gaps.first().copied().unwrap_or(0.0))),
        columns: columns_out,
    })
}

/// The columns of the model for columns of ODF `width` points wide.
pub fn columns(columns: &odf::Columns, width: f64) -> Columns {
    let count = usize::from(columns.count).clamp(1, MAX_COLUMNS);
    let parts = |points: f64| ((points / width).clamp(0.0, 1.0) * f64::from(PARTS)).round() as u32;
    let given = &columns.columns;
    if given.len() < 2 {
        return Columns::equal(count as u8, columns.gap.map_or(0, |g| parts(g.0)));
    }
    let count = given.len().min(MAX_COLUMNS);
    let total: f64 = given
        .iter()
        .map(|c| f64::from(c.rel_width))
        .sum::<f64>()
        .max(1.0);
    let mut widths = [0u32; MAX_COLUMNS];
    let mut gaps = [0u32; MAX_COLUMNS];
    for (i, column) in given.iter().take(count).enumerate() {
        let outer = width * f64::from(column.rel_width) / total;
        widths[i] = parts(outer - column.start_indent.0 - column.end_indent.0);
        if let Some(next) = given.get(i + 1).filter(|_| i + 1 < count) {
            gaps[i] = parts(column.end_indent.0 + next.start_indent.0);
        }
    }
    // Columns within a thousandth of each other are as wide as each other.
    let near = |a: u32, b: u32| a.abs_diff(b) <= PARTS / 1000;
    let equal = widths[..count].iter().all(|&w| near(w, widths[0]))
        && gaps[..count - 1].iter().all(|&g| near(g, gaps[0]));
    if equal {
        return Columns::equal(count as u8, gaps[0]);
    }
    Columns {
        count: count as u8,
        equal,
        widths,
        gaps,
    }
}

/// The section style of ODF for a layout of the model, and the section
/// it starts when it starts one.
pub fn section_style(
    name: String,
    setup: &PageSetup,
    layout: &Layout,
    section: Option<&Section>,
) -> odf::SectionStyle {
    let width = layout_width(setup, layout.left, layout.right);
    let some = |l: Length| (l != Length::ZERO).then(|| odf_length(l));
    odf::SectionStyle {
        name,
        margin_left: some(layout.left),
        margin_right: some(layout.right),
        columns: odf_columns(&layout.columns, width),
        space_before: some(layout.before),
        space_after: some(layout.after),
        new_page: layout.new_page.then_some(true),
        section_start: section.map(|s| match s.start {
            SectionStart::AnyPage => odf::SectionStart::Any,
            SectionStart::LeftPage => odf::SectionStart::Left,
            SectionStart::RightPage => odf::SectionStart::Right,
        }),
        first_page_different: section.and_then(|s| s.first_page_different.then_some(true)),
        left_right_different: section.and_then(|s| s.left_right_different.then_some(true)),
    }
}

/// The layout of the model for a section style of ODF.
pub fn layout(style: &odf::SectionStyle, setup: &PageSetup) -> Layout {
    let left = style.margin_left.map_or(Length::ZERO, length);
    let right = style.margin_right.map_or(Length::ZERO, length);
    let width = layout_width(setup, left, right);
    Layout {
        columns: style
            .columns
            .as_ref()
            .map_or(Columns::ONE, |c| columns(c, width)),
        before: style.space_before.map_or(Length::ZERO, length),
        after: style.space_after.map_or(Length::ZERO, length),
        left,
        right,
        new_page: style.new_page.unwrap_or(false),
    }
}

/// The section of the model that a section style of ODF starts, if it
/// starts one, with the number of its first page from its first
/// paragraph.
pub fn section(style: &odf::SectionStyle, first_number: Option<u32>) -> Option<Section> {
    let start = style.section_start?;
    Some(Section {
        first_number,
        first_page_different: style.first_page_different.unwrap_or(false),
        left_right_different: style.left_right_different.unwrap_or(false),
        // A master page of its own says so.
        own_margins: false,
        start: match start {
            odf::SectionStart::Any => SectionStart::AnyPage,
            odf::SectionStart::Left => SectionStart::LeftPage,
            odf::SectionStart::Right => SectionStart::RightPage,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_07_columns_go_to_odf_and_back() {
        let mut uneven = Columns::equal(3, PARTS / 20);
        uneven.equal = false;
        uneven.widths[..3].copy_from_slice(&[PARTS / 2, PARTS / 5, PARTS / 5]);
        uneven.gaps[..2].copy_from_slice(&[PARTS / 20, PARTS / 20]);
        for set in [Columns::equal(2, PARTS / 20), uneven] {
            let odf = odf_columns(&set, 400.0).unwrap();
            let back = columns(&odf, 400.0);
            assert_eq!(back.count, set.count);
            assert_eq!(back.equal, set.equal);
            for (a, b) in back.spans(400.0).iter().zip(set.spans(400.0)) {
                assert!(
                    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01,
                    "{a:?} {b:?}"
                );
            }
        }
        assert!(odf_columns(&Columns::ONE, 400.0).is_none());
        // Columns as LibreOffice writes them, without one by one widths.
        let plain = odf::Columns {
            count: 2,
            gap: Some(odf::Length(20.0)),
            columns: Vec::new(),
        };
        let spans = columns(&plain, 400.0).spans(400.0);
        assert!((spans[1].0 - 210.0).abs() < 0.01, "{spans:?}");
    }
}
