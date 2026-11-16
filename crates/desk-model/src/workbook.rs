// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sheets of a Desk Tables document (NTAB-01 to NTAB-04).
//!
//! A [`Workbook`] is a list of [`Sheet`]s, and each sheet holds the tables
//! and charts on it as [`SheetItem`]s, in the order that the Sheets page of
//! the sidebar lists them. That order is not where they are on the sheet:
//! moving an item in the list leaves it where it was (NTAB-03).
//!
//! The cells of a table belong to the table engine of `desk-table`, which
//! the tables of Desk Writer bring. Until a sheet holds those tables, an
//! item keeps what the window needs to show one: its name, its frame and
//! the outline of its grid. The workbook then becomes the container of a
//! [`crate::Document`] for spreadsheets, and is saved with it.
//!
//! The names that the workbook gives new sheets and tables are made from
//! a stem that the caller passes in its language, such as "Planilha" or
//! "Tabela", so that the model knows nothing of translations.

use std::fmt;

/// The size of a column of a new table, in points: an inch, as in
/// Numbers '09.
pub const COLUMN_WIDTH: f64 = 72.0;
/// The height of a row of a new table, in points: a fifth of an inch, as in
/// Numbers '09.
pub const ROW_HEIGHT: f64 = 14.4;
/// Where the first table of a new sheet is, from the top left corner of
/// the sheet, in points.
pub const FIRST_TABLE_ORIGIN: (f64, f64) = (36.0, 36.0);

/// Why a change of the sheets was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SheetError {
    /// A name with nothing but spaces.
    EmptyName,
    /// Another sheet, or another item of the same sheet, has the name.
    NameTaken,
    /// The only sheet of a document cannot be deleted.
    LastSheet,
    /// No sheet or item at the index.
    OutOfRange,
}

impl fmt::Display for SheetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyName => "the name is empty",
            Self::NameTaken => "the name is taken",
            Self::LastSheet => "a document keeps at least one sheet",
            Self::OutOfRange => "there is nothing at that index",
        })
    }
}

impl std::error::Error for SheetError {}

/// The outline of the grid of a table: how many rows and columns it has,
/// and how many of them are headers and footers (NTAB-06, NTAB-09).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    /// Rows, headers and footers included.
    pub rows: u32,
    /// Columns, headers included.
    pub columns: u32,
    /// Header rows at the top, 0 to 5.
    pub header_rows: u32,
    /// Header columns at the start, 0 to 5.
    pub header_columns: u32,
    /// Footer rows at the bottom, 0 to 5.
    pub footer_rows: u32,
}

impl Grid {
    /// The table that the Blank template of Numbers '09 puts on a new
    /// sheet: 13 columns by 45 rows, with one header row and one header
    /// column.
    pub const BLANK: Self = Self {
        rows: 45,
        columns: 13,
        header_rows: 1,
        header_columns: 1,
        footer_rows: 0,
    };

    /// The width and the height of the grid, in points.
    pub fn size(&self) -> (f64, f64) {
        (
            f64::from(self.columns) * COLUMN_WIDTH,
            f64::from(self.rows) * ROW_HEIGHT,
        )
    }
}

/// What an item of a sheet is.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    /// A table, with the outline of its grid.
    Table(Grid),
    /// A chart.
    Chart,
}

/// A table or a chart on a sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetItem {
    /// Its name, unique on the sheet ("Tabela 1").
    pub name: String,
    /// Whether the name shows above the item on the sheet (NTAB-09).
    pub name_shown: bool,
    /// What it is.
    pub kind: ItemKind,
    /// Its top left corner on the sheet, in points.
    pub origin: (f64, f64),
}

impl SheetItem {
    /// A table called `name` with the grid `grid`, at `origin`.
    pub fn table(name: &str, grid: Grid, origin: (f64, f64)) -> Self {
        Self {
            name: name.to_owned(),
            name_shown: false,
            kind: ItemKind::Table(grid),
            origin,
        }
    }

    /// Whether the item is a table.
    pub fn is_table(&self) -> bool {
        matches!(self.kind, ItemKind::Table(_))
    }

    /// Its width and height on the sheet, in points.
    pub fn size(&self) -> (f64, f64) {
        match &self.kind {
            ItemKind::Table(grid) => grid.size(),
            ItemKind::Chart => (360.0, 216.0),
        }
    }
}

/// A sheet: a named canvas with tables and charts (NTAB-01, NTAB-04).
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    /// Its name, unique in the document ("Planilha 1").
    pub name: String,
    /// Its tables and charts, in the order that the sidebar lists them.
    pub items: Vec<SheetItem>,
}

impl Sheet {
    /// A sheet called `name` with nothing on it.
    pub fn empty(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            items: Vec::new(),
        }
    }

    /// How many tables the sheet has.
    pub fn tables(&self) -> usize {
        self.items.iter().filter(|i| i.is_table()).count()
    }

    /// How many charts the sheet has.
    pub fn charts(&self) -> usize {
        self.items.len() - self.tables()
    }

    /// The width and height that the items of the sheet take from its top
    /// left corner, in points.
    pub fn extent(&self) -> (f64, f64) {
        self.items.iter().fold((0.0, 0.0), |(w, h), item| {
            let (iw, ih) = item.size();
            (w.max(item.origin.0 + iw), h.max(item.origin.1 + ih))
        })
    }

    /// Renames the item at `index`. Names of items are unique on a sheet,
    /// as Numbers asks.
    pub fn rename_item(&mut self, index: usize, name: &str) -> Result<(), SheetError> {
        let name = checked_name(name)?;
        if index >= self.items.len() {
            return Err(SheetError::OutOfRange);
        }
        if self
            .items
            .iter()
            .enumerate()
            .any(|(i, item)| i != index && same_name(&item.name, name))
        {
            return Err(SheetError::NameTaken);
        }
        self.items[index].name = name.to_owned();
        Ok(())
    }

    /// Moves the item at `from` so that it lands at `to` in the list,
    /// without moving it on the sheet (NTAB-03).
    pub fn move_item(&mut self, from: usize, to: usize) -> Result<(), SheetError> {
        if from >= self.items.len() || to >= self.items.len() {
            return Err(SheetError::OutOfRange);
        }
        let item = self.items.remove(from);
        self.items.insert(to, item);
        Ok(())
    }
}

/// The sheets of a document, never fewer than one.
#[derive(Clone, Debug, PartialEq)]
pub struct Workbook {
    sheets: Vec<Sheet>,
}

impl Workbook {
    /// The document that the Blank template starts: one sheet, called
    /// `sheet` and the number 1, with a table called `table` and the number
    /// 1, of the grid of [`Grid::BLANK`], whose name is hidden.
    pub fn blank(sheet: &str, table: &str) -> Self {
        let mut first = Sheet::empty(&format!("{sheet} 1"));
        first.items.push(SheetItem::table(
            &format!("{table} 1"),
            Grid::BLANK,
            FIRST_TABLE_ORIGIN,
        ));
        Self {
            sheets: vec![first],
        }
    }

    /// A workbook with these sheets, or none when there are none.
    pub fn from_sheets(sheets: Vec<Sheet>) -> Option<Self> {
        (!sheets.is_empty()).then_some(Self { sheets })
    }

    /// The sheets, in order.
    pub fn sheets(&self) -> &[Sheet] {
        &self.sheets
    }

    /// The sheet at `index`.
    pub fn sheet(&self, index: usize) -> Option<&Sheet> {
        self.sheets.get(index)
    }

    /// The sheet at `index`, to change.
    pub fn sheet_mut(&mut self, index: usize) -> Option<&mut Sheet> {
        self.sheets.get_mut(index)
    }

    /// How many sheets there are.
    pub fn len(&self) -> usize {
        self.sheets.len()
    }

    /// Never true: a workbook has a sheet at least.
    pub fn is_empty(&self) -> bool {
        self.sheets.is_empty()
    }

    /// `stem` and the lowest number from 1 that no sheet is called, as
    /// "Planilha 2".
    pub fn new_sheet_name(&self, stem: &str) -> String {
        first_free(stem, self.sheets.iter().map(|s| s.name.as_str()))
    }

    /// Adds a sheet after the one at `after`, or at the end, with a table
    /// as the template has on a new sheet (NTAB-02), and returns its index.
    /// The sheet is called `sheet` with the first free number, and the
    /// table `table` 1.
    pub fn add_sheet(&mut self, after: Option<usize>, sheet: &str, table: &str) -> usize {
        let mut new = Sheet::empty(&self.new_sheet_name(sheet));
        new.items.push(SheetItem::table(
            &format!("{table} 1"),
            Grid::BLANK,
            FIRST_TABLE_ORIGIN,
        ));
        let index = after.map_or(self.sheets.len(), |a| (a + 1).min(self.sheets.len()));
        self.sheets.insert(index, new);
        index
    }

    /// Puts a copy of the sheet at `index` right after it, with all it
    /// holds (NTAB-02), and returns the index of the copy. The copy is
    /// called as the sheet, with `copy` and a number when that is taken, as
    /// in "Despesas (cópia)" or "Despesas (cópia) 2".
    pub fn duplicate_sheet(&mut self, index: usize, copy: &str) -> Result<usize, SheetError> {
        let mut sheet = self
            .sheets
            .get(index)
            .ok_or(SheetError::OutOfRange)?
            .clone();
        let base = format!("{} {copy}", sheet.name);
        sheet.name = if self.sheets.iter().any(|s| same_name(&s.name, &base)) {
            first_free_from(&base, 2, self.sheets.iter().map(|s| s.name.as_str()))
        } else {
            base
        };
        self.sheets.insert(index + 1, sheet);
        Ok(index + 1)
    }

    /// Deletes the sheet at `index` with what it holds (NTAB-02), unless it
    /// is the only one, and returns it.
    pub fn delete_sheet(&mut self, index: usize) -> Result<Sheet, SheetError> {
        if index >= self.sheets.len() {
            return Err(SheetError::OutOfRange);
        }
        if self.sheets.len() == 1 {
            return Err(SheetError::LastSheet);
        }
        Ok(self.sheets.remove(index))
    }

    /// Renames the sheet at `index` (NTAB-04). Names are unique in the
    /// document, whatever their case, and the spaces around them go.
    pub fn rename_sheet(&mut self, index: usize, name: &str) -> Result<(), SheetError> {
        let name = checked_name(name)?;
        if index >= self.sheets.len() {
            return Err(SheetError::OutOfRange);
        }
        if self
            .sheets
            .iter()
            .enumerate()
            .any(|(i, s)| i != index && same_name(&s.name, name))
        {
            return Err(SheetError::NameTaken);
        }
        self.sheets[index].name = name.to_owned();
        Ok(())
    }

    /// Moves the sheet at `from` so that it lands at `to` (NTAB-03).
    pub fn move_sheet(&mut self, from: usize, to: usize) -> Result<(), SheetError> {
        if from >= self.sheets.len() || to >= self.sheets.len() {
            return Err(SheetError::OutOfRange);
        }
        let sheet = self.sheets.remove(from);
        self.sheets.insert(to, sheet);
        Ok(())
    }

    /// Moves the item at `item` of the sheet at `from` to the end of the
    /// sheet at `to`, where it keeps its place on the sheet (NTAB-03). A
    /// name taken there gets the first free number after it, and the
    /// index of the item on the sheet `to` is returned.
    pub fn move_item_to_sheet(
        &mut self,
        from: usize,
        item: usize,
        to: usize,
    ) -> Result<usize, SheetError> {
        if from >= self.sheets.len()
            || to >= self.sheets.len()
            || item >= self.sheets[from].items.len()
        {
            return Err(SheetError::OutOfRange);
        }
        let mut moved = self.sheets[from].items.remove(item);
        let target = &mut self.sheets[to];
        if target.items.iter().any(|i| same_name(&i.name, &moved.name)) {
            moved.name =
                first_free_from(&moved.name, 2, target.items.iter().map(|i| i.name.as_str()));
        }
        target.items.push(moved);
        Ok(target.items.len() - 1)
    }

    /// How many tables and charts the document has, on all its sheets.
    pub fn counts(&self) -> (usize, usize) {
        self.sheets
            .iter()
            .fold((0, 0), |(t, c), s| (t + s.tables(), c + s.charts()))
    }
}

/// The name without the spaces around it, unless nothing is left.
fn checked_name(name: &str) -> Result<&str, SheetError> {
    let name = name.trim();
    if name.is_empty() {
        Err(SheetError::EmptyName)
    } else {
        Ok(name)
    }
}

/// Whether two names are the same name, whatever their case.
fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// `stem` and the lowest number from 1 that none of `taken` is.
fn first_free<'a>(stem: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    first_free_from(stem, 1, taken)
}

/// `stem` and the lowest number from `start` that none of `taken` is.
fn first_free_from<'a>(
    stem: &str,
    start: u32,
    taken: impl Iterator<Item = &'a str> + Clone,
) -> String {
    (start..)
        .map(|n| format!("{stem} {n}"))
        .find(|name| !taken.clone().any(|t| same_name(t, name)))
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(book: &Workbook) -> Vec<&str> {
        book.sheets().iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn ntab_01_the_blank_template_has_a_sheet_with_a_table() {
        let book = Workbook::blank("Planilha", "Tabela");
        assert_eq!(names(&book), ["Planilha 1"]);
        let sheet = book.sheet(0).unwrap();
        assert_eq!(sheet.items.len(), 1);
        let table = &sheet.items[0];
        assert_eq!(table.name, "Tabela 1");
        assert!(!table.name_shown);
        assert_eq!(table.kind, ItemKind::Table(Grid::BLANK));
        assert_eq!((Grid::BLANK.columns, Grid::BLANK.rows), (13, 45));
        assert_eq!(book.counts(), (1, 0));
        assert!(!book.is_empty());
    }

    #[test]
    fn ntab_02_a_new_sheet_takes_the_first_free_number_and_a_table() {
        let mut book = Workbook::blank("Planilha", "Tabela");
        assert_eq!(book.add_sheet(None, "Planilha", "Tabela"), 1);
        book.rename_sheet(0, "Resumo").unwrap();
        // Planilha 1 is free again, and the sheet goes after the current one.
        assert_eq!(book.add_sheet(Some(0), "Planilha", "Tabela"), 1);
        assert_eq!(names(&book), ["Resumo", "Planilha 1", "Planilha 2"]);
        assert_eq!(book.sheet(1).unwrap().items[0].name, "Tabela 1");
        assert_eq!(book.counts(), (3, 0));
    }

    #[test]
    fn ntab_02_a_copy_holds_what_the_sheet_holds_and_names_itself() {
        let mut book = Workbook::blank("Planilha", "Tabela");
        book.rename_sheet(0, "Despesas").unwrap();
        assert_eq!(book.duplicate_sheet(0, "(cópia)"), Ok(1));
        assert_eq!(book.duplicate_sheet(0, "(cópia)"), Ok(1));
        assert_eq!(
            names(&book),
            ["Despesas", "Despesas (cópia) 2", "Despesas (cópia)"]
        );
        assert_eq!(book.sheet(1).unwrap().items, book.sheet(0).unwrap().items);
        assert_eq!(
            book.duplicate_sheet(9, "(cópia)"),
            Err(SheetError::OutOfRange)
        );
    }

    #[test]
    fn ntab_02_the_last_sheet_stays() {
        let mut book = Workbook::blank("Planilha", "Tabela");
        assert_eq!(book.delete_sheet(0), Err(SheetError::LastSheet));
        book.add_sheet(None, "Planilha", "Tabela");
        let gone = book.delete_sheet(0).unwrap();
        assert_eq!(gone.name, "Planilha 1");
        assert_eq!(names(&book), ["Planilha 2"]);
        assert_eq!(book.delete_sheet(4), Err(SheetError::OutOfRange));
        assert!(Workbook::from_sheets(Vec::new()).is_none());
    }

    #[test]
    fn ntab_04_names_are_unique_and_trimmed() {
        let mut book = Workbook::blank("Planilha", "Tabela");
        book.add_sheet(None, "Planilha", "Tabela");
        assert_eq!(book.rename_sheet(1, "  "), Err(SheetError::EmptyName));
        assert_eq!(
            book.rename_sheet(1, "planilha 1"),
            Err(SheetError::NameTaken)
        );
        // A sheet may take its own name in another case.
        book.rename_sheet(0, "PLANILHA 1").unwrap();
        book.rename_sheet(1, " Receitas ").unwrap();
        assert_eq!(names(&book), ["PLANILHA 1", "Receitas"]);
        assert_eq!(book.rename_sheet(5, "x"), Err(SheetError::OutOfRange));
    }

    #[test]
    fn ntab_03_sheets_and_items_move_in_the_list_only() {
        let mut book = Workbook::blank("Planilha", "Tabela");
        book.add_sheet(None, "Planilha", "Tabela");
        book.add_sheet(None, "Planilha", "Tabela");
        book.move_sheet(2, 0).unwrap();
        assert_eq!(names(&book), ["Planilha 3", "Planilha 1", "Planilha 2"]);
        assert_eq!(book.move_sheet(0, 3), Err(SheetError::OutOfRange));

        let sheet = book.sheet_mut(0).unwrap();
        sheet
            .items
            .push(SheetItem::table("Taxas", Grid::BLANK, (500.0, 36.0)));
        let before: Vec<_> = sheet.items.iter().map(|i| i.origin).collect();
        sheet.move_item(1, 0).unwrap();
        assert_eq!(sheet.items[0].name, "Taxas");
        // The order of the list does not move them on the sheet.
        assert_eq!(sheet.items[0].origin, before[1]);
        assert_eq!(sheet.items[1].origin, before[0]);
    }

    #[test]
    fn ntab_03_an_item_moves_to_another_sheet_with_a_free_name() {
        let mut book = Workbook::blank("Planilha", "Tabela");
        book.add_sheet(None, "Planilha", "Tabela");
        assert_eq!(book.move_item_to_sheet(0, 0, 1), Ok(1));
        assert!(book.sheet(0).unwrap().items.is_empty());
        let names: Vec<_> = book
            .sheet(1)
            .unwrap()
            .items
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(names, ["Tabela 1", "Tabela 1 2"]);
        assert_eq!(
            book.move_item_to_sheet(0, 0, 1),
            Err(SheetError::OutOfRange)
        );
    }

    #[test]
    fn ntab_09_table_names_are_unique_on_a_sheet() {
        let mut sheet = Sheet::empty("Planilha 1");
        sheet
            .items
            .push(SheetItem::table("Tabela 1", Grid::BLANK, (0.0, 0.0)));
        sheet
            .items
            .push(SheetItem::table("Tabela 2", Grid::BLANK, (0.0, 900.0)));
        assert_eq!(sheet.rename_item(1, "tabela 1"), Err(SheetError::NameTaken));
        sheet.rename_item(1, "Taxas").unwrap();
        assert_eq!(sheet.items[1].name, "Taxas");
        assert_eq!(
            sheet.extent(),
            (13.0 * COLUMN_WIDTH, 900.0 + 45.0 * ROW_HEIGHT)
        );
    }
}
