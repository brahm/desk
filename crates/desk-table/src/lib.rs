// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Table model shared by the three apps.
//!
//! Cells, cell formats, conditional formatting, the header, footer and
//! body areas, sorting, filling, merging and splitting cells. A table is
//! the same object in a document, on a sheet or on a slide. Formulas are
//! evaluated by `desk-formula`.
//!
//! The text of a cell is a type of the app: plain text in Desk Tables and
//! formatted paragraphs in Desk Writer and Desk Slides, through the
//! [`CellText`] trait, so that this crate does not depend on the document
//! model, which holds tables.

mod cell;
mod conditional;
mod copy;
mod eval;
mod format;
mod nav;
mod ops;
mod table;

pub use cell::{
    Cell, CellFill, CellStyle, CellText, Color, Content, HAlign, Num, TextStyle, VAlign,
};
pub use conditional::{Condition, Operand, Rule, RuleStyle};
pub use eval::{Values, header_range};
pub use format::{
    CustomFormat, DateStyle, DurationStyle, Format, FractionPrecision, Locale, Negative, Shown,
    TimeStyle, parse_date, parse_input, parse_number,
};
pub use nav::{Landing, Step};
pub use ops::{FillDirection, Split};
pub use table::{
    Area, Border, BorderSet, DEFAULT_BORDER, LastCellTab, LineStyle, MAX_CELLS, MAX_HEADERS,
    Region, Table, TableError, TableOptions, TableStyle,
};

pub use desk_formula::{CellRef, ErrorKind, FUNCTIONS, Formula, FunctionInfo, Value};
