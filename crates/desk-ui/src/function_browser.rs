// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Function Browser of iWork (TAB-32, IOP-05): the functions of the
//! formulas by category, with a search, how each is written and what it
//! gives. Insert puts the function in the cell being edited, or starts a
//! formula with it in the selected cell.

use adw::prelude::*;
use desk_model::table::{FUNCTIONS, FunctionInfo};

use crate::i18n::{n_, tr};

/// The categories, by their keys, in the order of iWork.
const CATEGORIES: [(&str, &str); 9] = [
    ("", n_("All")),
    ("date", n_("Date and Time")),
    ("financial", n_("Financial")),
    ("logical", n_("Logical and Information")),
    ("lookup", n_("Reference")),
    ("numeric", n_("Numeric")),
    ("statistical", n_("Statistical")),
    ("text", n_("Text")),
    ("trigonometric", n_("Trigonometric")),
];

fn matches(f: &FunctionInfo, category: &str, search: &str) -> bool {
    (category.is_empty() || f.category == category)
        && (search.is_empty()
            || f.name.to_lowercase().contains(search)
            || f.summary.to_lowercase().contains(search))
}

/// Opens the browser; `insert` gets the name of the function chosen with
/// its parentheses.
pub fn open(parent: Option<&gtk::Window>, insert: impl Fn(&str) + 'static) {
    let search = gtk::SearchEntry::builder()
        .placeholder_text(tr("Search Functions"))
        .hexpand(true)
        .build();
    let names: Vec<String> = CATEGORIES.iter().map(|(_, n)| tr(n)).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let category = gtk::DropDown::from_strings(&names);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(["navigation-sidebar"])
        .build();
    let syntax = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .css_classes(["monospace", "heading"])
        .build();
    let summary = gtk::Label::builder().xalign(0.0).wrap(true).build();
    let detail = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    detail.append(&syntax);
    detail.append(&summary);
    let fill = {
        let list = list.clone();
        let category = category.clone();
        let search = search.clone();
        move || {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let key = CATEGORIES
                .get(category.selected() as usize)
                .map_or("", |(k, _)| *k);
            let text = search.text().to_lowercase();
            let mut shown: Vec<&FunctionInfo> = FUNCTIONS
                .iter()
                .filter(|f| matches(f, key, &text))
                .collect();
            shown.sort_by_key(|f| f.name);
            for f in shown {
                let label = gtk::Label::builder()
                    .label(f.name)
                    .xalign(0.0)
                    .margin_start(6)
                    .build();
                let row = gtk::ListBoxRow::builder().child(&label).build();
                row.set_widget_name(f.name);
                list.append(&row);
            }
            if let Some(first) = list.row_at_index(0) {
                list.select_row(Some(&first));
            }
        }
    };
    fill();
    {
        let fill = fill.clone();
        search.connect_search_changed(move |_| fill());
    }
    category.connect_selected_notify(move |_| fill());
    {
        let syntax = syntax.clone();
        let summary = summary.clone();
        list.connect_row_selected(move |_, row| {
            let info = row.and_then(|r| {
                desk_model::table::FUNCTIONS
                    .iter()
                    .find(|f| f.name == r.widget_name())
            });
            syntax.set_label(info.map_or("", |f| f.syntax));
            summary.set_label(info.map_or("", |f| f.summary));
        });
    }
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .min_content_height(240)
        .build();
    let top = gtk::Box::builder()
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .build();
    top.append(&category);
    top.append(&search);
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    content.append(&top);
    content.append(&scrolled);
    content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    content.append(&detail);
    let header = adw::HeaderBar::new();
    let button = gtk::Button::builder()
        .label(tr("Insert"))
        .css_classes(["suggested-action"])
        .build();
    header.pack_end(&button);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .title(tr("Function Browser"))
        .content_width(420)
        .content_height(520)
        .child(&view)
        .build();
    let closing = dialog.downgrade();
    let choose = move |list: &gtk::ListBox| {
        if let Some(row) = list.selected_row() {
            insert(&format!("{}(", row.widget_name()));
            if let Some(dialog) = closing.upgrade() {
                dialog.close();
            }
        }
    };
    let choose = std::rc::Rc::new(choose);
    {
        let choose = choose.clone();
        let list = list.clone();
        button.connect_clicked(move |_| choose(&list));
    }
    list.connect_row_activated(move |list, _| choose(list));
    dialog.present(parent);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_32_iop_05_every_function_is_in_a_category_of_the_browser() {
        for f in FUNCTIONS {
            assert!(
                CATEGORIES
                    .iter()
                    .any(|(k, _)| !k.is_empty() && *k == f.category),
                "{} is in {}",
                f.name,
                f.category
            );
        }
    }
}
