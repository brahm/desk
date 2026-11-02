// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Warnings window (INT-22): what an import left out and the fonts
//! that were replaced, which can be copied as text, and a missing font
//! replaced by another one.

use std::rc::Rc;

use crate::Warning;
use crate::i18n::tr;
use adw::prelude::*;

/// Replaces a missing font family with another one, and says whether it
/// did.
pub(crate) type ReplaceFont = Rc<dyn Fn(&str, &str) -> bool>;

/// Shows the warnings of a document over `parent`. `replace` replaces a
/// missing font family with another one chosen here, and says whether it
/// did.
pub(crate) fn show(
    parent: &impl IsA<gtk::Widget>,
    document: &str,
    warnings: &[Warning],
    replace: ReplaceFont,
) {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    for warning in warnings {
        let row = adw::ActionRow::builder()
            .title(glib_escape(&warning.text))
            .title_lines(0)
            .build();
        row.add_prefix(&gtk::Image::from_icon_name("dialog-warning-symbolic"));
        if let Some(font) = &warning.font {
            let button = gtk::Button::builder()
                .label(tr("_Replace…"))
                .use_underline(true)
                .valign(gtk::Align::Center)
                .tooltip_text(tr("Use another font in its place"))
                .build();
            let font = font.clone();
            let replace = replace.clone();
            let row_weak = row.downgrade();
            button.connect_clicked(move |button| {
                let window = button.root().and_downcast::<gtk::Window>();
                let font = font.clone();
                let replace = replace.clone();
                let row_weak = row_weak.clone();
                gtk::FontDialog::builder()
                    .title(tr("Replace {font}").replace("{font}", &font))
                    .build()
                    .choose_family(
                        window.as_ref(),
                        None::<&gtk::pango::FontFamily>,
                        gtk::gio::Cancellable::NONE,
                        move |chosen| {
                            let Ok(chosen) = chosen else { return };
                            if replace(&font, &chosen.name())
                                && let Some(row) = row_weak.upgrade()
                                && let Some(list) = row.parent().and_downcast::<gtk::ListBox>()
                            {
                                list.remove(&row);
                            }
                        },
                    );
            });
            row.add_suffix(&button);
        }
        list.append(&row);
    }

    let text = warnings
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let copy = gtk::Button::builder()
        .label(tr("_Copy"))
        .use_underline(true)
        .tooltip_text(tr("Copy the warnings as text"))
        .build();
    copy.connect_clicked(move |button| {
        button.clipboard().set_text(&text);
    });

    let header = adw::HeaderBar::new();
    header.pack_start(&copy);

    let description = gtk::Label::builder()
        .label(
            tr("{document} opened with these changes. Saving it keeps them.")
                .replace("{document}", document),
        )
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&description);
    content.append(&list);
    let clamp = adw::Clamp::builder().child(&content).build();
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&clamp)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .build();

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&scrolled));

    let dialog = adw::Dialog::builder()
        .title(tr("Document Warnings"))
        .content_width(480)
        .content_height(400)
        .child(&view)
        .build();
    dialog.present(Some(parent));
}

/// The text of a warning as Pango markup, which an action row reads.
fn glib_escape(text: &str) -> String {
    gtk::glib::markup_escape_text(text).to_string()
}
