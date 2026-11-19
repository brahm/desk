// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chart styles of a document, as Pages 15.4 keeps them (W080): the
//! looks that the Chart button shows when inserting a chart and the Chart
//! tab of the Format pane shows for the selected one. A new document has
//! six; Create Chart Style adds the look of a chart after them, and Copy
//! and Paste Chart Style pass the look of one chart to another. As in
//! Pages, a style has no name: each shows as a picture.
//!
//! A style is the fills of the series, in order, and the text of the
//! chart; applying it keeps the kind, the data, the axes and the rest of
//! the look of each series.

use std::sync::Arc;

use crate::error::ModelError;
use crate::id::NodeId;
use crate::object::ObjectKind;
use crate::transaction::Edit;
use desk_chart::{Chart, ChartText, Collection, SeriesFill, SeriesStyle, darker};
use desk_table::Color;

/// The fills that a style keeps; series past them take them again from
/// the first.
const FILLS: usize = 8;

/// A look of charts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChartStyle {
    /// The fills of the series, in order; never empty.
    pub fills: Vec<SeriesFill>,
    /// The text.
    pub text: ChartText,
}

impl ChartStyle {
    /// The six styles of a new document: the colors of new charts first,
    /// then pastel, gray, gradients, textures and dark colors.
    pub fn library() -> [Self; 6] {
        let of = |collection: Collection| Self {
            fills: (0..FILLS).map(|n| collection.fill(n)).collect(),
            text: ChartText::default(),
        };
        let dark = Self {
            fills: (0..FILLS)
                .map(|n| match Collection::Default.fill(n) {
                    SeriesFill::Color(c) => SeriesFill::Color(darker(c)),
                    fill => fill,
                })
                .collect(),
            text: ChartText::default(),
        };
        [
            of(Collection::Default),
            of(Collection::Pastel),
            of(Collection::Gray),
            of(Collection::Gradients),
            of(Collection::Textures),
            dark,
        ]
    }

    /// The fill of the nth series.
    pub fn fill(&self, n: usize) -> SeriesFill {
        self.fills
            .get(n % self.fills.len().max(1))
            .cloned()
            .unwrap_or(SeriesFill::Color(Color::rgb(0x35, 0x84, 0xe4)))
    }

    /// How many series of a chart have a look of their own or data.
    fn series_of(chart: &Chart) -> usize {
        chart.series_count().max(chart.series.len()).max(1)
    }

    /// The look of a chart: the fills of its series and its text.
    pub fn of(chart: &Chart) -> Self {
        let fill = |n: usize| {
            chart
                .series
                .get(n)
                .map_or_else(|| SeriesStyle::nth(n).fill, |s| s.fill.clone())
        };
        Self {
            fills: (0..Self::series_of(chart)).map(fill).collect(),
            text: chart.text.clone(),
        }
    }

    /// Gives a chart this look, keeping the rest.
    pub fn apply_to(&self, chart: &mut Chart) {
        for n in 0..Self::series_of(chart) {
            if n >= chart.series.len() {
                chart.series.push(SeriesStyle::nth(n));
            }
            chart.series[n].fill = self.fill(n);
        }
        chart.text = self.text.clone();
    }

    /// Whether a chart has this look, which the Chart tab shows as chosen.
    pub fn fits(&self, chart: &Chart) -> bool {
        let look = Self::of(chart);
        look.text == self.text
            && look
                .fills
                .iter()
                .enumerate()
                .all(|(n, fill)| *fill == self.fill(n))
    }
}

impl crate::Document {
    /// The chart styles, in the order of their pictures.
    pub fn chart_styles(&self) -> &[ChartStyle] {
        &self.text_settings().chart_styles
    }
}

impl Edit {
    /// Sets the chart styles.
    pub fn set_chart_styles(&mut self, styles: Vec<ChartStyle>) -> Result<(), ModelError> {
        let settings = crate::TextSettings {
            chart_styles: styles,
            ..self.document().text_settings().clone()
        };
        self.set_text_settings(settings)
    }

    /// Gives charts a look, keeping their data. Locked charts and other
    /// objects stay as they are.
    pub fn apply_chart_style(
        &mut self,
        ids: &[NodeId],
        style: &ChartStyle,
    ) -> Result<(), ModelError> {
        for id in ids {
            let Some(mut object) = self.document().objects().get(*id).cloned() else {
                continue;
            };
            let ObjectKind::Chart(chart) = &object.kind else {
                continue;
            };
            if object.locked {
                continue;
            }
            let mut chart = (**chart).clone();
            style.apply_to(&mut chart);
            object.kind = ObjectKind::Chart(Arc::new(chart));
            self.replace_object(object)?;
        }
        Ok(())
    }

    /// Adds the look of a chart after the chart styles, as Create Chart
    /// Style does; false when `id` is no chart.
    pub fn create_chart_style(&mut self, id: NodeId) -> Result<bool, ModelError> {
        let Some(ObjectKind::Chart(chart)) =
            self.document().objects().get(id).map(|o| o.kind.clone())
        else {
            return Ok(false);
        };
        let mut styles = self.document().chart_styles().to_vec();
        styles.push(ChartStyle::of(&chart));
        self.set_chart_styles(styles)?;
        Ok(true)
    }

    /// Takes out a chart style; the charts keep their look.
    pub fn delete_chart_style(&mut self, index: usize) -> Result<(), ModelError> {
        let mut styles = self.document().chart_styles().to_vec();
        if index < styles.len() {
            styles.remove(index);
        }
        self.set_chart_styles(styles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{Frame, Object};
    use crate::{Document, Editor};
    use desk_chart::ChartKind;

    fn chart_of(editor: &Editor, id: NodeId) -> Chart {
        match &editor.document().objects().get(id).unwrap().kind {
            ObjectKind::Chart(c) => (**c).clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn w080_charts_take_chart_styles() {
        let mut editor = Editor::new(Document::new_word_processing());
        let styles = editor.document().chart_styles().to_vec();
        assert_eq!(styles, ChartStyle::library());
        let add = |editor: &mut Editor, kind| {
            editor
                .edit("Insert Chart", |edit| {
                    let id = edit.new_id();
                    edit.add_object(Object::new(
                        id,
                        ObjectKind::Chart(Arc::new(Chart::new(kind))),
                        Frame::from_pt(0.0, 0.0, 216.0, 160.0),
                    ))?;
                    Ok(id)
                })
                .unwrap()
        };
        let (a, b) = (
            add(&mut editor, ChartKind::ALL[0]),
            add(&mut editor, ChartKind::ALL[1]),
        );
        // A new chart has the look of the first style.
        assert!(styles[0].fits(&chart_of(&editor, a)));
        assert!(!styles[1].fits(&chart_of(&editor, a)));
        // Applying the pastel style gives its fills to every series and
        // keeps the data.
        let before = chart_of(&editor, a);
        editor
            .edit("Chart Style", |e| e.apply_chart_style(&[a], &styles[1]))
            .unwrap();
        let after = chart_of(&editor, a);
        assert!(styles[1].fits(&after));
        assert_eq!(after.data, before.data);
        assert_eq!(after.kind, before.kind);
        for (n, series) in after.series.iter().enumerate() {
            assert_eq!(series.fill, styles[1].fill(n));
        }
        // Copy and Paste Chart Style pass the look to another chart.
        let copied = ChartStyle::of(&after);
        editor
            .edit("Paste Chart Style", |e| e.apply_chart_style(&[b], &copied))
            .unwrap();
        assert!(styles[1].fits(&chart_of(&editor, b)));
        // Create Chart Style adds the look of a chart after the others.
        let mut bold = chart_of(&editor, b);
        bold.text.bold = true;
        let look = ChartStyle::of(&bold);
        editor
            .edit("Look", |e| e.apply_chart_style(&[b], &look))
            .unwrap();
        assert!(
            editor
                .edit("Create Chart Style", |e| e.create_chart_style(b))
                .unwrap()
        );
        assert_eq!(editor.document().chart_styles().len(), 7);
        assert!(editor.document().chart_styles()[6].fits(&chart_of(&editor, b)));
        editor.edit("Delete", |e| e.delete_chart_style(6)).unwrap();
        assert_eq!(editor.document().chart_styles().len(), 6);
        // Undo brings the style back.
        editor.undo().unwrap();
        assert_eq!(editor.document().chart_styles().len(), 7);
    }
}
