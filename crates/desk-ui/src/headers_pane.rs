// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The headers and footers in the Inspector (EST-01, EST-12, EST-19,
//! EST-27): whether pages show them, how far they are from the edges of
//! the paper, and their text.
//!
//! Each header and footer is a row of text. Its fields are written as
//! `{page}`, `{pages}`, `{date}` and `{file}`, and its paragraphs are
//! separated by ` ¶ `. A changed text keeps the formatting of the first
//! paragraph and of the first text of the header or footer, and its date
//! and file name fields, whose values stay.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{
    CharAttrs, Edit, FIELD_CHARACTER, Field, HeadersFooters, MarginKind, MarginPages, MarginPlace,
    ModelError, PageSetup, Paragraph, Run, Section, StyleFamily,
};

use crate::editing::Format;
use crate::formatting::EditorOf;
use crate::i18n::tr;
use crate::preferences::Units;
use crate::tab_stops;

/// What separates the paragraphs of a header or footer in its row.
const PARAGRAPHS: &str = " ¶ ";

/// The text of each field in a row.
fn token(field: &Field) -> &'static str {
    match field {
        Field::PageNumber { .. } => "{page}",
        Field::PageCount { .. } => "{pages}",
        Field::DateTime { .. } => "{date}",
        Field::FileName { .. } => "{file}",
        Field::NoteMark { .. } | Field::PageOf { .. } | Field::ObjectMark { .. } => "",
    }
}

/// The text of a header or footer, as its row shows it.
pub fn text(paragraphs: &[Paragraph]) -> String {
    let mut out = Vec::new();
    for paragraph in paragraphs {
        let mut line = String::new();
        for run in paragraph.runs() {
            match &run.attrs.field {
                Some(Some(field)) => {
                    for _ in run.text().chars() {
                        line.push_str(token(field));
                    }
                }
                _ => line.push_str(&run.text().replace(desk_model::LINE_BREAK, PARAGRAPHS)),
            }
        }
        out.push(line);
    }
    out.join(PARAGRAPHS)
}

/// The paragraphs of a header or footer for the text of its row, with the
/// formatting of the old ones. An empty text gives none.
pub fn paragraphs(
    edit: &mut Edit,
    text: &str,
    old: &[Paragraph],
    file_name: &str,
) -> Result<Vec<Paragraph>, ModelError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let styles = edit.document().styles();
    let first = old.first();
    let style = first.map(Paragraph::style).unwrap_or_else(|| {
        ["Header & Footer", "Header", "Footer"]
            .iter()
            .find_map(|name| styles.by_name(StyleFamily::Paragraph, name))
            .map_or(styles.default_paragraph(), |s| s.id())
    });
    let attrs = first
        .map(|p| p.attrs().without_structure())
        .unwrap_or_default();
    let runs: Vec<&Run> = old.iter().flat_map(Paragraph::runs).collect();
    let base = runs
        .iter()
        .find(|r| r.attrs.field.is_none())
        .map(|r| (r.style, r.attrs.clone()))
        .unwrap_or_default();
    // The date and file name keep what they showed.
    let kept = |kind: &str| {
        runs.iter()
            .filter_map(|r| r.attrs.field.clone().flatten())
            .find(|f| token(f) == kind)
    };
    let date = kept("{date}").unwrap_or_else(crate::editing::today);
    let file = kept("{file}").unwrap_or_else(|| Field::FileName {
        shown: desk_model::file_shown(file_name, false, true).into(),
        path: false,
        extension: true,
    });
    // Numbers of pages keep their format.
    let page = kept("{page}").unwrap_or(Field::PAGE_NUMBER);
    let pages = kept("{pages}").unwrap_or(Field::PAGE_COUNT);
    let fields = [
        ("{page}", page),
        ("{pages}", pages),
        ("{date}", date),
        ("{file}", file),
    ];
    let mut out = Vec::new();
    for piece in text.split('¶') {
        let piece = piece.strip_prefix(' ').unwrap_or(piece);
        let piece = piece.strip_suffix(' ').unwrap_or(piece);
        let mut runs = Vec::new();
        let mut rest = piece;
        while !rest.is_empty() {
            let next = fields
                .iter()
                .filter_map(|(t, f)| rest.find(t).map(|at| (at, *t, f)))
                .min_by_key(|(at, _, _)| *at);
            let Some((at, name, field)) = next else {
                runs.push(Run::with_attrs(rest, base.0, base.1.clone()));
                break;
            };
            if at > 0 {
                runs.push(Run::with_attrs(&rest[..at], base.0, base.1.clone()));
            }
            let attrs = CharAttrs {
                field: Some(Some(field.clone())),
                ..base.1.clone()
            };
            runs.push(Run::with_attrs(&FIELD_CHARACTER.to_string(), base.0, attrs));
            rest = &rest[at + name.len()..];
        }
        let id = edit.new_id();
        out.push(Paragraph::from_runs(id, style, attrs.clone(), runs)?);
    }
    Ok(out)
}

/// The Headers and Footers group of the Inspector.
pub struct HeadersGroup {
    group: adw::PreferencesGroup,
    headers: adw::SwitchRow,
    footers: adw::SwitchRow,
    header_margin: adw::SpinRow,
    footer_margin: adw::SpinRow,
    /// The rows of the text of each header and footer, as
    /// [`MarginPlace::ALL`] lists them.
    texts: Vec<adw::EntryRow>,
    units: Cell<Units>,
    shown: RefCell<(PageSetup, Section, HeadersFooters)>,
    updating: Cell<bool>,
    editor: EditorOf,
}

fn title(place: MarginPlace) -> String {
    match (place.kind, place.pages) {
        (MarginKind::Header, MarginPages::Default) => tr("Header"),
        (MarginKind::Header, MarginPages::First) => tr("First Page Header"),
        (MarginKind::Header, MarginPages::Left) => tr("Left Page Header"),
        (MarginKind::Footer, MarginPages::Default) => tr("Footer"),
        (MarginKind::Footer, MarginPages::First) => tr("First Page Footer"),
        (MarginKind::Footer, MarginPages::Left) => tr("Left Page Footer"),
    }
}

impl HeadersGroup {
    /// The group for the editor of a window.
    pub fn new(editor: EditorOf, units: Units) -> Rc<Self> {
        let headers = adw::SwitchRow::builder().title(tr("Headers")).build();
        let footers = adw::SwitchRow::builder().title(tr("Footers")).build();
        let header_margin = adw::SpinRow::with_range(0.0, 1.0, 1.0);
        header_margin.set_title(&tr("Header Margin"));
        header_margin.set_subtitle(&tr("From the top of the paper"));
        let footer_margin = adw::SpinRow::with_range(0.0, 1.0, 1.0);
        footer_margin.set_title(&tr("Footer Margin"));
        footer_margin.set_subtitle(&tr("From the bottom of the paper"));
        let group = adw::PreferencesGroup::builder()
            .title(tr("Headers and Footers"))
            .description(tr(
                "Fields: {page} for the page number, {pages} for the page count, {date} and {file}",
            ))
            .build();
        group.add(&headers);
        group.add(&header_margin);
        group.add(&footers);
        group.add(&footer_margin);
        let texts: Vec<adw::EntryRow> = MarginPlace::ALL
            .iter()
            .map(|&place| {
                let row = adw::EntryRow::builder()
                    .title(title(place))
                    .show_apply_button(true)
                    .build();
                group.add(&row);
                row
            })
            .collect();
        let this = Rc::new(Self {
            group,
            headers,
            footers,
            header_margin,
            footer_margin,
            texts,
            units: Cell::new(units),
            shown: RefCell::new(Default::default()),
            updating: Cell::new(false),
            editor,
        });
        this.set_units(units);
        this.connect();
        this
    }

    fn connect(self: &Rc<Self>) {
        for row in [&self.headers, &self.footers] {
            let weak = Rc::downgrade(self);
            row.connect_active_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    this.setup_changed();
                }
            });
        }
        for row in [&self.header_margin, &self.footer_margin] {
            let weak = Rc::downgrade(self);
            row.connect_value_notify(move |_| {
                if let Some(this) = weak.upgrade()
                    && !this.updating.get()
                {
                    this.setup_changed();
                }
            });
        }
        for (row, place) in self.texts.iter().zip(MarginPlace::ALL) {
            let weak = Rc::downgrade(self);
            row.connect_apply(move |row| {
                if let Some(this) = weak.upgrade()
                    && let Some(editor) = (this.editor)()
                {
                    editor.set_header_footer_text(place, &row.text());
                }
            });
        }
    }

    /// The group.
    pub fn widget(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Measures the margins in other units.
    pub fn set_units(&self, units: Units) {
        self.updating.set(true);
        self.units.set(units);
        for row in [&self.header_margin, &self.footer_margin] {
            tab_stops::set_units(row, units, 0.0, 1584.0);
        }
        self.updating.set(false);
        let shown = self.shown.borrow().clone();
        self.show_parts(shown.0, shown.1, shown.2);
    }

    /// Shows the headers and footers of the document, and those that the
    /// section at the selection uses.
    pub fn show(&self, format: &Format) {
        self.show_parts(format.setup, format.section, format.headers.clone());
    }

    fn show_parts(&self, setup: PageSetup, section: Section, headers: HeadersFooters) {
        *self.shown.borrow_mut() = (setup, section, headers.clone());
        self.updating.set(true);
        let units = self.units.get();
        if self.headers.is_active() != setup.headers {
            self.headers.set_active(setup.headers);
        }
        if self.footers.is_active() != setup.footers {
            self.footers.set_active(setup.footers);
        }
        for (row, value) in [
            (&self.header_margin, setup.header_margin),
            (&self.footer_margin, setup.footer_margin),
        ] {
            let value = tab_stops::in_units(value, units);
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        }
        self.header_margin.set_sensitive(setup.headers);
        self.footer_margin.set_sensitive(setup.footers);
        for (row, place) in self.texts.iter().zip(MarginPlace::ALL) {
            let shown = match place.kind {
                MarginKind::Header => setup.headers,
                MarginKind::Footer => setup.footers,
            };
            // The first and left ones show when the section has them.
            let used = match place.pages {
                MarginPages::Default => true,
                MarginPages::First => section.first_page_different,
                MarginPages::Left => section.left_right_different,
            };
            row.set_visible(used);
            row.set_sensitive(shown);
            let value = text(headers.get(place));
            if row.text() != value {
                row.set_text(&value);
            }
        }
        self.updating.set(false);
    }

    fn setup_changed(&self) {
        let (shown, _, _) = *self.shown.borrow();
        let units = self.units.get();
        let length = |row: &adw::SpinRow| tab_stops::from_units(row.value(), units);
        let setup = PageSetup {
            headers: self.headers.is_active(),
            footers: self.footers.is_active(),
            header_margin: length(&self.header_margin),
            footer_margin: length(&self.footer_margin),
            ..shown
        };
        if setup != shown
            && let Some(editor) = (self.editor)()
        {
            editor.set_page_setup("Headers and Footers", setup);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desk_model::{Document, Editor};

    #[test]
    fn est_12_est_27_the_text_of_a_row_goes_to_paragraphs_and_back() {
        let mut editor = Editor::new(Document::new_word_processing());
        let place = MarginPlace::new(MarginKind::Header, MarginPages::Default);
        let row = "Page {page} of {pages} ¶ {file}, {date}";
        editor
            .edit("Header", |edit| {
                let paragraphs = paragraphs(edit, row, &[], "Notes.odt")?;
                edit.set_header_footer(place, paragraphs)
            })
            .unwrap();
        let header = editor.document().headers_footers().get(place).to_vec();
        assert_eq!(header.len(), 2);
        assert_eq!(header[0].text(), "Page \u{fffc} of \u{fffc}");
        assert_eq!(text(&header), row);
        let file = header[1].runs().next().unwrap();
        assert_eq!(
            file.attrs.field,
            Some(Some(Field::FileName {
                shown: "Notes.odt".into(),
                path: false,
                extension: true,
            }))
        );
        // A new text keeps the date and file name fields as they were.
        editor
            .edit("Header", |edit| {
                let paragraphs = paragraphs(edit, "{file}", &header, "Other.odt")?;
                edit.set_header_footer(place, paragraphs)
            })
            .unwrap();
        let header = editor.document().headers_footers().get(place).to_vec();
        assert_eq!(
            header[0].runs().next().unwrap().attrs.field,
            file.attrs.field
        );
        // An empty row empties the header.
        editor
            .edit("Header", |edit| {
                let paragraphs = paragraphs(edit, "  ", &header, "")?;
                edit.set_header_footer(place, paragraphs)
            })
            .unwrap();
        assert!(editor.document().headers_footers().get(place).is_empty());
    }
}
