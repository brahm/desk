// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Chart model and drawing, including 3D charts.
//!
//! A chart keeps its own data, as the Chart Data Editor of iWork shows it,
//! and draws itself as shapes and lines of text in its frame, which the
//! display list of `desk-render` turns into what a page draws. The crate
//! does not depend on the document model, which holds charts.

mod draw;
mod model;
mod stats;

pub use draw::{
    Align, Drawing, Hit, Mark, Measure, Paint, PathOp, draw, rough_width, value_axis_values,
};
pub use model::{
    CategoryAxis, Chart, ChartData, ChartKind, ChartText, Collection, DataLink, ErrorAmount,
    ErrorBars, ErrorDirection, LabelPosition, Lighting, NumberStyle, PieOptions, ReferenceKind,
    ReferenceLine, Scale, Scene, SeriesFill, SeriesKind, SeriesStyle, Symbol, Texture, TrendKind,
    Trendline, ValueAxis, ValueLabels, darker, lighter,
};
pub use stats::{Fit, error_lengths, fit, moving_average};
