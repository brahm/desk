// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Search Commands (GNM-10): every command of the menus, found by name,
//! as the search of the Help menu of iWork found the items of the menu
//! bar. Ctrl+/ and the main menu open it.

use adw::prelude::*;
use gtk::{gio, glib};

use crate::i18n::tr;

/// A command that the search can find.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Command {
    /// Its name, without the underline of the mnemonic.
    pub name: String,
    /// The menus it is in, as "Insert › Break".
    pub path: String,
    /// The action, with its prefix: "win.insert-page-break".
    pub action: String,
    pub target: Option<glib::Variant>,
}

/// The name of a menu item without the underline of its mnemonic; a
/// doubled underline stands for one.
pub(super) fn plain(label: &str) -> String {
    label
        .replace("__", "\u{0}")
        .replace('_', "")
        .replace('\u{0}', "_")
}

/// The commands of `menu`, under the menus of `path`, in their order.
pub(super) fn commands(menu: &gio::MenuModel, path: &str, out: &mut Vec<Command>) {
    for i in 0..menu.n_items() {
        let label = menu
            .item_attribute_value(i, "label", Some(glib::VariantTy::STRING))
            .and_then(|v| v.get::<String>())
            .map(|l| plain(&l));
        if let Some(action) = menu
            .item_attribute_value(i, "action", Some(glib::VariantTy::STRING))
            .and_then(|v| v.get::<String>())
            && let Some(name) = label.clone()
        {
            let target = menu.item_attribute_value(i, "target", None);
            let known = out.iter().any(|c| c.action == action && c.target == target);
            if !known {
                out.push(Command {
                    name,
                    path: path.to_owned(),
                    action,
                    target,
                });
            }
        }
        if let Some(section) = menu.item_link(i, "section") {
            commands(&section, path, out);
        }
        if let Some(submenu) = menu.item_link(i, "submenu") {
            let name = label.unwrap_or_default();
            let inner = if path.is_empty() {
                name
            } else {
                format!("{path} › {name}")
            };
            commands(&submenu, &inner, out);
        }
    }
}

/// Folds a text for matching: lower case, without accents, so that
/// "pagina" finds "Página".
pub(super) fn fold(text: &str) -> String {
    glib::normalize(text.to_lowercase(), glib::NormalizeMode::All)
        .chars()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect()
}

/// Whether `command` matches every word of `query`, in its name or in its
/// menus.
pub(super) fn matches(command: &Command, query: &str) -> bool {
    let haystack = fold(&format!("{} {}", command.name, command.path));
    fold(query)
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

/// Whether the window or the app can run the action now.
fn enabled(window: &adw::ApplicationWindow, action: &str) -> bool {
    let found = if let Some(name) = action.strip_prefix("win.") {
        window.lookup_action(name)
    } else if let Some(name) = action.strip_prefix("app.") {
        window.application().and_then(|app| app.lookup_action(name))
    } else {
        None
    };
    found.is_some_and(|a| a.is_enabled())
}

/// Shows the search over `window`, with the commands of `menus`: each a
/// menu and the name of where it is.
pub(crate) fn present(window: &adw::ApplicationWindow, menus: &[(String, gio::MenuModel)]) {
    let mut found = Vec::new();
    for (path, menu) in menus {
        commands(menu, path, &mut found);
    }
    let app = window.application();

    let dialog = adw::Dialog::builder()
        .title(tr("Search Commands"))
        .content_width(460)
        .content_height(420)
        .build();
    let entry = gtk::SearchEntry::builder()
        .placeholder_text(tr("Search commands…"))
        .hexpand(true)
        .build();
    let header = adw::HeaderBar::builder().title_widget(&entry).build();
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(["navigation-sidebar"])
        .build();
    list.update_property(&[gtk::accessible::Property::Label(&tr("Commands"))]);
    let mut rows = Vec::new();
    for command in found {
        if !enabled(window, &command.action) {
            continue;
        }
        let row = adw::ActionRow::builder()
            .title(&command.name)
            .subtitle(&command.path)
            .activatable(true)
            .build();
        let detailed = gio::Action::print_detailed_name(&command.action, command.target.as_ref());
        if let Some(accel) = app
            .as_ref()
            .and_then(|app| app.accels_for_action(&detailed).first().cloned())
        {
            let label = adw::ShortcutLabel::new(&accel);
            label.set_valign(gtk::Align::Center);
            row.add_suffix(&label);
        }
        list.append(&row);
        rows.push((row, command));
    }
    let rows = std::rc::Rc::new(rows);
    let empty = adw::StatusPage::builder()
        .icon_name("edit-find-symbolic")
        .title(tr("No Commands Found"))
        .css_classes(["compact"])
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let stack = gtk::Stack::new();
    stack.add_named(&scrolled, Some("list"));
    stack.add_named(&empty, Some("empty"));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&stack));
    dialog.set_child(Some(&view));

    // Runs a command once the dialog has gone, so that what it opens
    // shows over the window.
    let run = {
        let dialog = dialog.downgrade();
        let window = window.downgrade();
        move |command: &Command| {
            let (Some(dialog), Some(window)) = (dialog.upgrade(), window.upgrade()) else {
                return;
            };
            dialog.close();
            let command = command.clone();
            glib::idle_add_local_once(move || {
                let _ =
                    WidgetExt::activate_action(&window, &command.action, command.target.as_ref());
            });
        }
    };
    {
        let rows = rows.clone();
        let run = run.clone();
        list.connect_row_activated(move |_, row| {
            if let Some((_, command)) = rows
                .iter()
                .find(|(r, _)| r.upcast_ref::<gtk::ListBoxRow>() == row)
            {
                run(command);
            }
        });
    }
    let first_shown = {
        let rows = rows.clone();
        move || rows.iter().find(|(row, _)| row.is_visible()).cloned()
    };
    {
        let list = list.clone();
        let stack = stack.clone();
        let rows = rows.clone();
        let first_shown = first_shown.clone();
        entry.connect_search_changed(move |entry| {
            let query = entry.text();
            for (row, command) in rows.iter() {
                row.set_visible(matches(command, &query));
            }
            match first_shown() {
                Some((row, _)) => {
                    list.select_row(Some(&row));
                    stack.set_visible_child_name("list");
                }
                None => stack.set_visible_child_name("empty"),
            }
        });
    }
    {
        let list = list.clone();
        let first_shown = first_shown.clone();
        let run = run.clone();
        let rows = rows.clone();
        entry.connect_activate(move |_| {
            let selected = list.selected_row().and_then(|selected| {
                rows.iter()
                    .find(|(r, _)| r.upcast_ref::<gtk::ListBoxRow>() == &selected && r.is_visible())
                    .cloned()
            });
            if let Some((_, command)) = selected.or_else(&first_shown) {
                run(&command);
            }
        });
    }
    {
        // The arrows move through the list while the keyboard stays in
        // the field.
        let list = list.clone();
        let rows = rows.clone();
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(move |_, key, _, _| {
            let step: i32 = match key {
                gtk::gdk::Key::Down => 1,
                gtk::gdk::Key::Up => -1,
                _ => return glib::Propagation::Proceed,
            };
            let shown: Vec<_> = rows
                .iter()
                .filter(|(r, _)| r.is_visible())
                .map(|(r, _)| r.clone())
                .collect();
            if shown.is_empty() {
                return glib::Propagation::Stop;
            }
            let at = list.selected_row().and_then(|selected| {
                shown
                    .iter()
                    .position(|r| r.upcast_ref::<gtk::ListBoxRow>() == &selected)
            });
            let next = match at {
                Some(at) => (at as i32 + step).clamp(0, shown.len() as i32 - 1) as usize,
                None => 0,
            };
            list.select_row(Some(&shown[next]));
            shown[next].grab_focus();
            glib::Propagation::Stop
        });
        entry.add_controller(keys);
    }
    {
        // Escape in the field closes the search, as it would the dialog.
        let dialog = dialog.downgrade();
        entry.connect_stop_search(move |_| {
            if let Some(dialog) = dialog.upgrade() {
                dialog.close();
            }
        });
    }
    if let Some((row, _)) = first_shown() {
        list.select_row(Some(&row));
    }
    dialog.present(Some(window));
    entry.grab_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str, path: &str) -> Command {
        Command {
            name: name.to_owned(),
            path: path.to_owned(),
            action: "win.x".to_owned(),
            target: None,
        }
    }

    #[test]
    fn gnm_10_menu_names_lose_the_mnemonic() {
        assert_eq!(plain("Page _Thumbnails"), "Page Thumbnails");
        assert_eq!(plain("Save _As…"), "Save As…");
        assert_eq!(plain("A__B"), "A_B");
    }

    #[test]
    fn gnm_10_the_search_matches_every_word_without_accents() {
        let page = command("Quebra de página", "Inserir › Quebra");
        assert!(matches(&page, "pagina"));
        assert!(matches(&page, "QUEBRA ins"));
        assert!(matches(&page, ""));
        assert!(!matches(&page, "coluna"));
    }

    #[test]
    fn gnm_10_commands_come_from_sections_and_submenus_once() {
        let menu = gio::Menu::new();
        let section = gio::Menu::new();
        section.append(Some("_Ruler"), Some("win.ruler"));
        menu.append_section(None, &section);
        let breaks = gio::Menu::new();
        breaks.append(Some("_Page Break"), Some("win.insert-page-break"));
        breaks.append(Some("Again"), Some("win.ruler"));
        menu.append_submenu(Some("_Break"), &breaks);
        let mut found = Vec::new();
        commands(menu.upcast_ref(), "Insert", &mut found);
        let names: Vec<_> = found
            .iter()
            .map(|c| (c.name.as_str(), c.path.as_str()))
            .collect();
        assert_eq!(
            names,
            [("Ruler", "Insert"), ("Page Break", "Insert › Break")]
        );
    }
}
