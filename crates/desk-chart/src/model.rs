// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The chart: its kind, its data and the look of its parts (GRF-01 to
//! GRF-20).

use std::sync::Arc;

use desk_table::{Color, Num};

/// What a chart draws (GRF-01).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ChartKind {
    /// Columns side by side.
    #[default]
    Column,
    /// Columns stacked.
    StackedColumn,
    /// Bars side by side.
    Bar,
    /// Bars stacked.
    StackedBar,
    /// Lines.
    Line,
    /// Areas.
    Area,
    /// Areas stacked.
    StackedArea,
    /// A pie, of the first value of each series.
    Pie,
    /// Points by pairs of values: x and y.
    Scatter,
    /// Two series, each a column, a line or an area.
    Mixed,
    /// Like Mixed, with the second series on a second value axis on the
    /// right.
    TwoAxis,
}

impl ChartKind {
    /// All the kinds, in the order of the chart menu.
    pub const ALL: [Self; 11] = [
        Self::Column,
        Self::StackedColumn,
        Self::Bar,
        Self::StackedBar,
        Self::Line,
        Self::Area,
        Self::StackedArea,
        Self::Pie,
        Self::Scatter,
        Self::Mixed,
        Self::TwoAxis,
    ];

    /// Whether the kind can be drawn in 3D: all but scatter, mixed and 2
    /// axes.
    pub fn has_3d(self) -> bool {
        !matches!(self, Self::Scatter | Self::Mixed | Self::TwoAxis)
    }

    /// Whether the series are stacked.
    pub fn is_stacked(self) -> bool {
        matches!(
            self,
            Self::StackedColumn | Self::StackedBar | Self::StackedArea
        )
    }

    /// Whether the categories go down and the values across.
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Bar | Self::StackedBar)
    }

    /// Whether the kind has error bars and trendlines: the 2D charts
    /// other than pie; trendlines only when not stacked (GRF-13, GRF-14).
    pub fn has_error_bars(self) -> bool {
        self != Self::Pie
    }

    /// Whether the kind takes trendlines (GRF-14).
    pub fn has_trendlines(self) -> bool {
        self != Self::Pie && !self.is_stacked()
    }
}

/// What a series of a mixed or 2-axis chart draws (GRF-19).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum SeriesKind {
    /// Columns.
    #[default]
    Column,
    /// A line.
    Line,
    /// An area.
    Area,
}

/// The data of a chart: a table of values with names for its rows and
/// columns, as the Chart Data Editor shows it (GRF-02, GRF-03).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ChartData {
    /// The names of the rows.
    pub rows: Vec<String>,
    /// The names of the columns.
    pub cols: Vec<String>,
    /// The values, by rows; none for an empty cell.
    pub values: Vec<Vec<Option<Num>>>,
}

impl ChartData {
    /// The data of a new chart, as iWork fills it: two regions over four
    /// years (GRF-03).
    pub fn sample() -> Self {
        let v = |row: [f64; 4]| row.iter().map(|x| Some(Num(*x))).collect();
        Self {
            rows: vec!["Região 1".into(), "Região 2".into()],
            cols: vec!["2007".into(), "2008".into(), "2009".into(), "2010".into()],
            values: vec![v([17.0, 26.0, 53.0, 96.0]), v([55.0, 43.0, 70.0, 58.0])],
        }
    }

    /// Whether there is a row of values for each row name, with a value
    /// for each column name. Data made by Desk always has; data read
    /// from a file may not, and drawing it would index past its rows.
    pub fn is_consistent(&self) -> bool {
        self.values.len() == self.rows.len()
            && self.values.iter().all(|row| row.len() == self.cols.len())
    }

    /// The value at a row and a column.
    pub fn get(&self, row: usize, col: usize) -> Option<f64> {
        self.values
            .get(row)?
            .get(col)
            .copied()
            .flatten()
            .map(|n| n.0)
    }

    /// Sets the value at a row and a column, growing the data as needed.
    pub fn set(&mut self, row: usize, col: usize, value: Option<f64>) {
        while self.rows.len() <= row {
            self.rows.push(String::new());
        }
        while self.cols.len() <= col {
            self.cols.push(String::new());
        }
        self.normalize();
        self.values[row][col] = value.map(Num);
    }

    /// Makes every row as long as the names of the columns.
    pub fn normalize(&mut self) {
        let cols = self.cols.len();
        self.values.resize_with(self.rows.len(), Vec::new);
        for row in &mut self.values {
            row.resize(cols, None);
        }
    }

    /// Adds a row before `at`.
    pub fn insert_row(&mut self, at: usize, name: String) {
        let at = at.min(self.rows.len());
        self.rows.insert(at, name);
        self.values.insert(at, vec![None; self.cols.len()]);
    }

    /// Adds a column before `at`.
    pub fn insert_col(&mut self, at: usize, name: String) {
        let at = at.min(self.cols.len());
        self.cols.insert(at, name);
        for row in &mut self.values {
            row.insert(at.min(row.len()), None);
        }
    }

    /// Deletes a row.
    pub fn delete_row(&mut self, at: usize) {
        if at < self.rows.len() {
            self.rows.remove(at);
            self.values.remove(at);
        }
    }

    /// Deletes a column.
    pub fn delete_col(&mut self, at: usize) {
        if at < self.cols.len() {
            self.cols.remove(at);
            for row in &mut self.values {
                if at < row.len() {
                    row.remove(at);
                }
            }
        }
    }

    /// Moves a row to another place, as dragging it in the Data Editor
    /// does (GRF-03).
    pub fn move_row(&mut self, from: usize, to: usize) {
        if from < self.rows.len() && to < self.rows.len() {
            let name = self.rows.remove(from);
            let row = self.values.remove(from);
            self.rows.insert(to, name);
            self.values.insert(to, row);
        }
    }

    /// Moves a column to another place.
    pub fn move_col(&mut self, from: usize, to: usize) {
        if from < self.cols.len() && to < self.cols.len() {
            let name = self.cols.remove(from);
            self.cols.insert(to, name);
            for row in &mut self.values {
                if from < row.len() && to < row.len() {
                    let v = row.remove(from);
                    row.insert(to, v);
                }
            }
        }
    }

    /// The data from text pasted from a spreadsheet: a tab between
    /// cells and a line between rows, with names in the first row and
    /// column (GRF-03).
    pub fn from_text(text: &str) -> Self {
        let lines: Vec<Vec<&str>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split('\t').collect())
            .collect();
        let Some(first) = lines.first() else {
            return Self {
                rows: Vec::new(),
                cols: Vec::new(),
                values: Vec::new(),
            };
        };
        let cols: Vec<String> = first.iter().skip(1).map(|s| s.trim().to_owned()).collect();
        let mut data = Self {
            rows: Vec::new(),
            cols,
            values: Vec::new(),
        };
        for line in &lines[1..] {
            data.rows
                .push(line.first().map_or(String::new(), |s| s.trim().to_owned()));
            data.values.push(
                line.iter()
                    .skip(1)
                    .map(|s| {
                        let t = s.trim().replace(' ', "");
                        // With both marks, the last is the decimal one, as
                        // in 1,234.56 and 1.234,56; a comma alone is one.
                        let t = match (t.rfind(','), t.rfind('.')) {
                            (Some(comma), Some(dot)) if comma > dot => {
                                t.replace('.', "").replace(',', ".")
                            }
                            (Some(_), Some(_)) => t.replace(',', ""),
                            (Some(_), None) => t.replace(',', "."),
                            _ => t,
                        };
                        t.parse::<f64>().ok().map(Num)
                    })
                    .collect(),
            );
        }
        data.normalize();
        data
    }
}

/// A fill of a series: a color, a gradient or a texture of the
/// collections of iWork, such as marble and wood (GRF-11).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SeriesFill {
    /// One color.
    Color(Color),
    /// A gradient from the top to the bottom of the bar or slice.
    Gradient(Color, Color),
    /// A texture, by name, drawn in its main color with its grain.
    Texture(Texture),
}

/// The textures of the fill collections (GRF-11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Texture {
    /// Marble.
    Marble,
    /// Wood.
    Wood,
    /// Brushed metal.
    Metal,
    /// Stone.
    Stone,
    /// Fabric.
    Fabric,
}

impl Texture {
    /// The main color of the texture.
    pub fn base(self) -> Color {
        match self {
            Self::Marble => Color::rgb(0xe8, 0xe4, 0xdc),
            Self::Wood => Color::rgb(0xa8, 0x6f, 0x3e),
            Self::Metal => Color::rgb(0xb4, 0xb9, 0xbf),
            Self::Stone => Color::rgb(0x8c, 0x87, 0x7e),
            Self::Fabric => Color::rgb(0x4a, 0x6b, 0x8a),
        }
    }

    /// The color of its grain.
    pub fn grain(self) -> Color {
        match self {
            Self::Marble => Color::rgb(0xb8, 0xb0, 0xa2),
            Self::Wood => Color::rgb(0x7a, 0x4a, 0x24),
            Self::Metal => Color::rgb(0x8f, 0x95, 0x9c),
            Self::Stone => Color::rgb(0x6a, 0x65, 0x5e),
            Self::Fabric => Color::rgb(0x33, 0x4d, 0x66),
        }
    }
}

/// A collection of fills that the Chart inspector applies to all the
/// series at once (GRF-11).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Collection {
    /// The colors of new charts.
    #[default]
    Default,
    /// Soft colors.
    Pastel,
    /// Grays.
    Gray,
    /// Gradients of the default colors.
    Gradients,
    /// Textures.
    Textures,
}

impl Collection {
    /// All the collections, in the order of the inspector.
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::Pastel,
        Self::Gray,
        Self::Gradients,
        Self::Textures,
    ];

    /// The fill of the nth series.
    pub fn fill(self, n: usize) -> SeriesFill {
        const DEFAULT: [Color; 8] = [
            Color::rgb(0x35, 0x84, 0xe4),
            Color::rgb(0x33, 0xd1, 0x7a),
            Color::rgb(0xf6, 0xd3, 0x2d),
            Color::rgb(0xff, 0x78, 0x00),
            Color::rgb(0xe0, 0x1b, 0x24),
            Color::rgb(0x91, 0x41, 0xac),
            Color::rgb(0x98, 0x6a, 0x44),
            Color::rgb(0x77, 0x76, 0x7b),
        ];
        const PASTEL: [Color; 6] = [
            Color::rgb(0x99, 0xc1, 0xf1),
            Color::rgb(0x8f, 0xf0, 0xa4),
            Color::rgb(0xf9, 0xf0, 0x6b),
            Color::rgb(0xff, 0xbe, 0x6f),
            Color::rgb(0xf6, 0x61, 0x51),
            Color::rgb(0xdc, 0x8a, 0xdd),
        ];
        const GRAY: [Color; 5] = [
            Color::rgb(0x3d, 0x38, 0x46),
            Color::rgb(0x5e, 0x5c, 0x64),
            Color::rgb(0x77, 0x76, 0x7b),
            Color::rgb(0x9a, 0x99, 0x96),
            Color::rgb(0xc0, 0xbf, 0xbc),
        ];
        const TEXTURES: [Texture; 5] = [
            Texture::Fabric,
            Texture::Wood,
            Texture::Marble,
            Texture::Stone,
            Texture::Metal,
        ];
        match self {
            Self::Default => SeriesFill::Color(DEFAULT[n % DEFAULT.len()]),
            Self::Pastel => SeriesFill::Color(PASTEL[n % PASTEL.len()]),
            Self::Gray => SeriesFill::Color(GRAY[n % GRAY.len()]),
            Self::Gradients => {
                let c = DEFAULT[n % DEFAULT.len()];
                SeriesFill::Gradient(lighter(c), c)
            }
            Self::Textures => SeriesFill::Texture(TEXTURES[n % TEXTURES.len()]),
        }
    }
}

/// A color mixed with white, by a third.
pub fn lighter(c: Color) -> Color {
    let mix = |v: u8| (u16::from(v) + (255 - u16::from(v)) / 3) as u8;
    Color::rgb(mix(c.r), mix(c.g), mix(c.b))
}

/// A color mixed with black, by a quarter.
pub fn darker(c: Color) -> Color {
    let mix = |v: u8| (u16::from(v) * 3 / 4) as u8;
    Color {
        a: c.a,
        ..Color::rgb(mix(c.r), mix(c.g), mix(c.b))
    }
}

/// The mark of the points of a line or a scatter chart (GRF-17).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Symbol {
    /// No mark.
    #[default]
    None,
    /// A circle.
    Circle,
    /// A triangle.
    Triangle,
    /// A square.
    Square,
    /// A diamond.
    Diamond,
}

/// Where the value labels of a series sit (GRF-12).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum LabelPosition {
    /// Above the bar or the point, or outside the end of a bar.
    #[default]
    Outside,
    /// Inside the end of the bar.
    InsideEnd,
    /// In the middle of the bar or the slice.
    Middle,
    /// Inside the base of the bar.
    InsideBase,
}

/// The number format of values: on an axis, in a value label (GRF-10,
/// GRF-12).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NumberStyle {
    /// The decimal places; none for as many as the value has.
    pub decimals: Option<u8>,
    /// Text before the number, as a currency.
    pub prefix: String,
    /// Text after the number, as a unit or %.
    pub suffix: String,
    /// Whether thousands are separated.
    pub thousands: bool,
    /// Whether negatives are written between parentheses, rather than
    /// with a minus sign.
    pub parentheses: bool,
}

impl Default for NumberStyle {
    fn default() -> Self {
        Self {
            decimals: None,
            prefix: String::new(),
            suffix: String::new(),
            thousands: true,
            parentheses: false,
        }
    }
}

impl NumberStyle {
    /// A value written in this style, with the decimal and thousands
    /// separators of the locale.
    pub fn write(&self, value: f64, decimal: char, group: char) -> String {
        let decimals = self.decimals.map_or_else(
            || {
                let s = format!("{:.6}", value.abs());
                let trimmed = s.trim_end_matches('0');
                trimmed.split_once('.').map_or(0, |(_, f)| f.len())
            },
            usize::from,
        );
        let s = format!("{:.*}", decimals, value.abs());
        let (int_part, frac) = s.split_once('.').map_or((s.as_str(), ""), |(a, b)| (a, b));
        let mut out = String::new();
        for (i, c) in int_part.chars().enumerate() {
            if self.thousands && i > 0 && (int_part.len() - i).is_multiple_of(3) {
                out.push(group);
            }
            out.push(c);
        }
        if !frac.is_empty() {
            out.push(decimal);
            out.push_str(frac);
        }
        let body = format!("{}{out}{}", self.prefix, self.suffix);
        if value < 0.0 && out.chars().any(|c| c.is_ascii_digit() && c != '0') {
            if self.parentheses {
                format!("({body})")
            } else {
                format!("-{body}")
            }
        } else {
            body
        }
    }
}

/// The value labels of a series (GRF-12).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ValueLabels {
    /// Whether they show.
    pub show: bool,
    /// Where they sit.
    pub position: LabelPosition,
    /// How they are written.
    pub number: NumberStyle,
}

/// Which way error bars go (GRF-13).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ErrorDirection {
    /// Above and below.
    #[default]
    Both,
    /// Above.
    Positive,
    /// Below.
    Negative,
}

/// How long error bars are (GRF-13).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ErrorAmount {
    /// A fixed value.
    Fixed(Num),
    /// A percentage of each value.
    Percent(Num),
    /// A number of standard deviations of the series.
    StandardDeviation(Num),
    /// The standard error of the series.
    StandardError,
    /// Values of their own for each point: positive and negative.
    Custom(Arc<[(Num, Num)]>),
}

/// The error bars of a series (GRF-13).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ErrorBars {
    /// Which way they go.
    pub direction: ErrorDirection,
    /// How long they are.
    pub amount: ErrorAmount,
    /// For a scatter chart: whether they go along x rather than y.
    pub along_x: bool,
}

/// The kind of a trendline (GRF-14).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum TrendKind {
    /// A straight line.
    Linear,
    /// a + b ln x.
    Logarithmic,
    /// A polynomial of an order from 2 to 6.
    Polynomial(u8),
    /// a x^b.
    Power,
    /// a e^(bx).
    Exponential,
    /// The average of the last points, over a period from 2.
    MovingAverage(u8),
}

/// A trendline of a series (GRF-14).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Trendline {
    /// Its kind.
    pub kind: TrendKind,
    /// Whether the equation shows.
    pub show_equation: bool,
    /// Whether R² shows.
    pub show_r_squared: bool,
    /// A label of its own, shown beside it when not empty.
    pub label: String,
    /// Its color; none for that of the series.
    pub color: Option<Color>,
}

/// The look of a series (GRF-11, GRF-12, GRF-16, GRF-17).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SeriesStyle {
    /// How bars, areas and slices are filled.
    pub fill: SeriesFill,
    /// How opaque, in percent.
    pub opacity: u8,
    /// The outline: width in thousandths of a point and color; none for
    /// no outline.
    pub stroke: Option<(u32, Color)>,
    /// Whether it casts a shadow.
    pub shadow: bool,
    /// The mark of its points.
    pub symbol: Symbol,
    /// The size of the mark, in points.
    pub symbol_size: u8,
    /// Whether the line between points is a curve.
    pub curved: bool,
    /// The width of its line, in thousandths of a point.
    pub line_width: u32,
    /// For a scatter chart: the color of the points, apart from the line
    /// (GRF-18); none for that of the fill.
    pub point_color: Option<Color>,
    /// Whether points of a scatter chart are joined by a line.
    pub connect: bool,
    /// The value labels.
    pub labels: ValueLabels,
    /// The error bars.
    pub errors: Option<ErrorBars>,
    /// The trendline.
    pub trend: Option<Trendline>,
    /// What it draws in a mixed or 2-axis chart.
    pub kind: SeriesKind,
    /// How far the slice of a pie stands out, in percent of the radius
    /// (GRF-15).
    pub explode: u8,
}

impl SeriesStyle {
    /// The look of the nth series of a new chart.
    pub fn nth(n: usize) -> Self {
        Self {
            fill: Collection::Default.fill(n),
            opacity: 100,
            stroke: None,
            shadow: false,
            symbol: Symbol::Circle,
            symbol_size: 6,
            curved: false,
            line_width: 2_000,
            point_color: None,
            connect: false,
            labels: ValueLabels::default(),
            errors: None,
            trend: None,
            kind: if n == 0 {
                SeriesKind::Column
            } else {
                SeriesKind::Line
            },
            explode: 0,
        }
    }

    /// The main color of the fill.
    pub fn color(&self) -> Color {
        match &self.fill {
            SeriesFill::Color(c) => *c,
            SeriesFill::Gradient(_, c) => *c,
            SeriesFill::Texture(t) => t.base(),
        }
    }
}

/// The scale of a value axis (GRF-10).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Scale {
    /// Linear.
    #[default]
    Linear,
    /// Logarithmic, in powers of 10.
    Logarithmic,
    /// Percent: each category of stacked series adds up to 100%.
    Percent,
}

/// A value axis (GRF-10).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ValueAxis {
    /// The least value; none for automatic.
    pub min: Option<Num>,
    /// The greatest value; none for automatic.
    pub max: Option<Num>,
    /// How many steps between the least and the greatest; none for
    /// automatic.
    pub steps: Option<u8>,
    /// Whether the labels show.
    pub labels: bool,
    /// Whether the label of the least value shows.
    pub min_label: bool,
    /// Whether the major gridlines show.
    pub gridlines: bool,
    /// Whether the minor gridlines show.
    pub minor_gridlines: bool,
    /// Whether tick marks show.
    pub ticks: bool,
    /// Whether the axis line shows.
    pub line: bool,
    /// The title; none when it does not show.
    pub title: Option<String>,
    /// The scale.
    pub scale: Scale,
    /// How the labels are written.
    pub number: NumberStyle,
}

impl Default for ValueAxis {
    fn default() -> Self {
        Self {
            min: None,
            max: None,
            steps: None,
            labels: true,
            min_label: true,
            gridlines: true,
            minor_gridlines: false,
            ticks: false,
            line: false,
            title: None,
            scale: Scale::Linear,
            number: NumberStyle::default(),
        }
    }
}

/// The category axis (GRF-10).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CategoryAxis {
    /// Whether the labels show.
    pub labels: bool,
    /// The labels show every so many categories, from 1.
    pub every: u16,
    /// Whether the last label shows even when it is not one of those.
    pub last: bool,
    /// Whether gridlines between the categories show.
    pub gridlines: bool,
    /// Whether tick marks show.
    pub ticks: bool,
    /// Whether the axis line shows.
    pub line: bool,
    /// The title; none when it does not show.
    pub title: Option<String>,
}

impl Default for CategoryAxis {
    fn default() -> Self {
        Self {
            labels: true,
            every: 1,
            last: false,
            gridlines: false,
            ticks: true,
            line: true,
            title: None,
        }
    }
}

/// How a 3D chart is lit (GRF-20).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Lighting {
    /// Soft light from above.
    #[default]
    Soft,
    /// Strong light from the left.
    Strong,
    /// Flat: the sides as dark as the front.
    Flat,
}

/// The scene of a 3D chart (GRF-20).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    /// The angle the chart is seen from, left to right, in degrees from
    /// -45 to 45.
    pub yaw: i8,
    /// The angle the chart is seen from, from above, in degrees from 0 to
    /// 45.
    pub pitch: i8,
    /// How deep the bars are, in percent of their width.
    pub depth: u16,
    /// The light.
    pub lighting: Lighting,
    /// Whether the edges of a pie are beveled.
    pub bevel: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            yaw: 20,
            pitch: 15,
            depth: 100,
            lighting: Lighting::Soft,
            bevel: false,
        }
    }
}

/// The options of a pie (GRF-15).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PieOptions {
    /// How far the first slice is turned, clockwise from the top, in
    /// degrees.
    pub rotation: u16,
    /// How far the labels sit from the center, in percent of the radius.
    pub label_distance: u16,
    /// Whether the labels show the names of the series.
    pub series_names: bool,
    /// Whether the value labels show percentages of the whole.
    pub percentages: bool,
    /// Whether the shadow falls from the whole pie rather than from each
    /// slice.
    pub group_shadow: bool,
}

impl Default for PieOptions {
    fn default() -> Self {
        Self {
            rotation: 0,
            label_distance: 70,
            series_names: false,
            percentages: true,
            group_shadow: true,
        }
    }
}

/// The text of a chart (GRF-08).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ChartText {
    /// The font family.
    pub font: String,
    /// The size, in thousandths of a point.
    pub size: u32,
    /// The color.
    pub color: Color,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
}

impl Default for ChartText {
    fn default() -> Self {
        Self {
            font: "Helvetica Neue".to_owned(),
            size: 10_000,
            color: Color::rgb(0x24, 0x1f, 0x31),
            bold: false,
            italic: false,
        }
    }
}

/// Where the data of a chart pasted from Desk Tables comes from: the
/// document and the tables, kept with the data as they were last read, so
/// the chart opens without them (GRF-06, IOP-02, IOP-21).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DataLink {
    /// The path or URI of the spreadsheet.
    pub source: String,
    /// The ranges it reads, as Sheet 1::Table 1::A1:D3.
    pub ranges: Vec<String>,
}

/// A chart (GRF-01 to GRF-20).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Chart {
    /// What it draws.
    pub kind: ChartKind,
    /// Whether it is drawn in 3D.
    pub three_d: bool,
    /// The data.
    pub data: ChartData,
    /// Whether each row of the data is a series, rather than each column
    /// (GRF-02).
    pub series_in_rows: bool,
    /// The title; none when it does not show (GRF-07).
    pub title: Option<String>,
    /// Whether the legend shows (GRF-07).
    pub legend: bool,
    /// The look of each series, in order; series past the end take the
    /// look of a new chart.
    pub series: Vec<SeriesStyle>,
    /// The value axis, the vertical one of columns (Y1).
    pub value_axis: ValueAxis,
    /// The second value axis of a 2-axis chart, on the right (Y2).
    pub value_axis2: ValueAxis,
    /// For a scatter chart: the axis of the x values.
    pub x_axis: ValueAxis,
    /// The category axis.
    pub category_axis: CategoryAxis,
    /// The room between bars of a set, in percent of their width
    /// (GRF-16).
    pub bar_gap: u16,
    /// The room between sets of bars, in percent of the width of a bar.
    pub set_gap: u16,
    /// The options of a pie.
    pub pie: PieOptions,
    /// The scene of a 3D chart.
    pub scene: Scene,
    /// The text.
    pub text: ChartText,
    /// The spreadsheet that the data comes from, when it was pasted from
    /// Desk Tables and stays linked to it (GRF-06).
    pub link: Option<DataLink>,
}

impl Chart {
    /// A new chart of a kind, with the data of iWork (GRF-03).
    pub fn new(kind: ChartKind) -> Self {
        Self {
            kind,
            three_d: false,
            data: ChartData::sample(),
            series_in_rows: true,
            title: None,
            legend: true,
            series: (0..2).map(SeriesStyle::nth).collect(),
            value_axis: ValueAxis::default(),
            value_axis2: ValueAxis {
                gridlines: false,
                ..ValueAxis::default()
            },
            x_axis: ValueAxis {
                gridlines: false,
                ..ValueAxis::default()
            },
            category_axis: CategoryAxis::default(),
            bar_gap: 8,
            set_gap: 75,
            pie: PieOptions::default(),
            scene: Scene::default(),
            text: ChartText::default(),
            link: None,
        }
    }

    /// How many series.
    pub fn series_count(&self) -> usize {
        if self.series_in_rows {
            self.data.rows.len()
        } else {
            self.data.cols.len()
        }
    }

    /// How many categories (data sets).
    pub fn category_count(&self) -> usize {
        if self.series_in_rows {
            self.data.cols.len()
        } else {
            self.data.rows.len()
        }
    }

    /// The name of a series.
    pub fn series_name(&self, s: usize) -> &str {
        let names = if self.series_in_rows {
            &self.data.rows
        } else {
            &self.data.cols
        };
        names.get(s).map_or("", String::as_str)
    }

    /// The name of a category.
    pub fn category_name(&self, c: usize) -> &str {
        let names = if self.series_in_rows {
            &self.data.cols
        } else {
            &self.data.rows
        };
        names.get(c).map_or("", String::as_str)
    }

    /// The value of a series in a category.
    pub fn value(&self, s: usize, c: usize) -> Option<f64> {
        if self.series_in_rows {
            self.data.get(s, c)
        } else {
            self.data.get(c, s)
        }
    }

    /// The look of a series.
    pub fn style(&self, s: usize) -> SeriesStyle {
        self.series
            .get(s)
            .cloned()
            .unwrap_or_else(|| SeriesStyle::nth(s))
    }

    /// Changes the kind, as the chart menu does (GRF-04): the axes,
    /// gridlines, number formats, borders, rotation, shadows and 3D
    /// lighting stay; the value labels, the strokes, the shadows and the
    /// marks of the series go back to their defaults, and a scatter
    /// chart drops a last series without a pair.
    pub fn set_kind(&mut self, kind: ChartKind) {
        if kind == self.kind {
            return;
        }
        self.kind = kind;
        if !kind.has_3d() {
            self.three_d = false;
        }
        for (i, s) in self.series.iter_mut().enumerate() {
            let fresh = SeriesStyle::nth(i);
            s.labels = fresh.labels;
            s.stroke = fresh.stroke;
            s.shadow = fresh.shadow;
            s.symbol = fresh.symbol;
            s.symbol_size = fresh.symbol_size;
            if !kind.has_trendlines() {
                s.trend = None;
            }
            if !kind.has_error_bars() {
                s.errors = None;
            }
        }
    }

    /// Sets the fills of every series from a collection (GRF-11).
    pub fn apply_collection(&mut self, collection: Collection) {
        let n = self.series_count().max(self.series.len());
        for i in 0..n {
            if i >= self.series.len() {
                self.series.push(SeriesStyle::nth(i));
            }
            self.series[i].fill = collection.fill(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_numbers_take_the_last_mark_as_the_decimal_one() {
        // Review RV, batch 4: 1.234,56 was read as 1.23456.
        let data = ChartData::from_text("\tA\tB\tC\tD\nx\t1.234,56\t1,234.56\t2,5\t3.5");
        let row: Vec<Option<f64>> = (0..4).map(|c| data.get(0, c)).collect();
        assert_eq!(row, [Some(1234.56), Some(1234.56), Some(2.5), Some(3.5)]);
    }

    #[test]
    fn data_with_a_value_for_each_row_and_column_is_consistent() {
        let mut data = ChartData::sample();
        assert!(data.is_consistent());
        data.values[1].pop();
        assert!(!data.is_consistent());
        let mut data = ChartData::sample();
        data.rows.push("Região 3".into());
        assert!(!data.is_consistent());
    }
}
