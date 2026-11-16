// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The actions of the slim format bar and its More menus (INT-12): the
//! character style, the line spacing and the list of text; the shadow,
//! reflection, placement and wrap of objects; Instant Alpha; and Show in
//! Inspector, which opens the Format level of the Inspector or one of its
//! tabs (INT-13).

use std::rc::Rc;

use desk_model::{Anchor, LineSpacing, ObjectShadow, ParaAttrs, Wrap};
use gtk::prelude::*;
use gtk::{gio, glib};

use super::DocumentWindow;
use crate::editing::{Format, ObjectInfo, TextEditor};
use crate::formatting;

/// The actions of the format bar, which need a document.
pub(super) const FORMAT_BAR_ACTIONS: [&str; 16] = [
    "character-style",
    "line-spacing",
    "list-style",
    "show-inspector",
    "inspector-tab",
    "object-shadow",
    "object-reflection",
    "object-placement",
    "object-wrap",
    "instant-alpha",
    "object-style",
    "update-object-style",
    "new-object-style",
    "default-object-style",
    "rename-object-style",
    "delete-object-style",
];

/// How opaque a reflection starts, in percent, as Pages makes it.
const REFLECTION: u8 = 50;

/// The wrap that a target of `win.object-wrap` names.
fn wrap_of(name: &str) -> Option<Wrap> {
    Some(match name {
        "both" => Wrap::Both,
        "largest" => Wrap::Largest,
        "left" => Wrap::Left,
        "right" => Wrap::Right,
        "above-and-below" => Wrap::AboveAndBelow,
        "through" => Wrap::Through,
        "behind" => Wrap::Behind,
        _ => return None,
    })
}

/// The target of `win.object-wrap` for a wrap.
fn wrap_name(wrap: Wrap) -> &'static str {
    match wrap {
        Wrap::Both => "both",
        Wrap::Largest => "largest",
        Wrap::Left => "left",
        Wrap::Right => "right",
        Wrap::AboveAndBelow => "above-and-below",
        Wrap::Through => "through",
        Wrap::Behind => "behind",
    }
}

/// The state of `win.line-spacing` for a line spacing: its percent, or
/// nothing for a spacing that the menu does not have.
fn spacing_name(spacing: LineSpacing) -> String {
    match spacing {
        LineSpacing::Multiple { percent } => percent.to_string(),
        _ => String::new(),
    }
}

impl DocumentWindow {
    pub(super) fn add_format_bar_actions(self: &Rc<Self>) {
        // A stateful action whose state follows the selection, and whose
        // change runs `apply` with the editor.
        fn stateful(
            this: &Rc<DocumentWindow>,
            name: &str,
            initial: glib::Variant,
            apply: impl Fn(&Rc<DocumentWindow>, &TextEditor, &glib::Variant) + 'static,
        ) {
            let parameter = (initial.type_() != glib::VariantTy::BOOLEAN).then(|| initial.type_());
            let action = gio::SimpleAction::new_stateful(name, parameter, &initial);
            let weak = Rc::downgrade(this);
            action.connect_change_state(move |action, value| {
                let (Some(this), Some(value)) = (weak.upgrade(), value) else {
                    return;
                };
                action.set_state(value);
                if let Some(editor) = this.editor() {
                    apply(&this, &editor, value);
                }
            });
            this.actions.add_action(&action);
        }

        stateful(
            self,
            "character-style",
            (-1i32).to_variant(),
            |this, editor, value| {
                if let Some(style) = value
                    .get::<i32>()
                    .and_then(|i| this.format_bar.character_style(i))
                {
                    editor.set_character_style(style);
                    editor.focus();
                }
            },
        );
        stateful(
            self,
            "line-spacing",
            String::new().to_variant(),
            |_, editor, value| {
                if let Some(percent) = value.get::<String>().and_then(|v| v.parse::<u32>().ok()) {
                    editor.set_paragraph_attrs(
                        "Line Spacing",
                        &ParaAttrs {
                            line_spacing: Some(LineSpacing::Multiple { percent }),
                            ..ParaAttrs::default()
                        },
                    );
                    editor.focus();
                }
            },
        );
        stateful(
            self,
            "object-shadow",
            false.to_variant(),
            |_, editor, value| {
                let on = value.get::<bool>().unwrap_or(false);
                editor.style_objects("Shadow", move |style, _| {
                    style.shadow = on.then(ObjectShadow::default);
                });
            },
        );
        stateful(
            self,
            "object-reflection",
            false.to_variant(),
            |_, editor, value| {
                let on = value.get::<bool>().unwrap_or(false);
                editor.style_objects("Reflection", move |style, _| {
                    style.reflection = on.then_some(REFLECTION);
                });
            },
        );
        stateful(
            self,
            "object-placement",
            "floating".to_variant(),
            |_, editor, value| {
                editor.set_objects_inline(value.get::<String>().as_deref() == Some("inline"));
            },
        );
        stateful(
            self,
            "object-wrap",
            "both".to_variant(),
            |_, editor, value| {
                if let Some(wrap) = value.get::<String>().as_deref().and_then(wrap_of) {
                    editor.set_each_object("Wrap", |object, _| object.wrap = wrap);
                }
            },
        );

        let list = gio::SimpleAction::new("list-style", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        list.connect_activate(move |_, value| {
            if let (Some(this), Some(kind)) =
                (weak.upgrade(), value.and_then(|v| v.get::<String>()))
                && let Some(editor) = this.editor()
            {
                crate::lists_pane::set_list(&editor, &kind);
                editor.focus();
            }
        });
        self.actions.add_action(&list);

        let inspector = gio::SimpleAction::new("show-inspector", None);
        let weak = Rc::downgrade(self);
        inspector.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.inspector.show_level(crate::inspector::FORMAT);
                this.actions
                    .change_action_state("inspector", &true.to_variant());
            }
        });
        self.actions.add_action(&inspector);

        // A tab of the Inspector by its name, as Adjust Image opens the
        // Image tab.
        let tab = gio::SimpleAction::new("inspector-tab", Some(glib::VariantTy::STRING));
        let weak = Rc::downgrade(self);
        tab.connect_activate(move |_, value| {
            if let (Some(this), Some(name)) =
                (weak.upgrade(), value.and_then(|v| v.get::<String>()))
            {
                if !this.inspector.show_tab(&name) {
                    this.inspector.show_level(crate::inspector::FORMAT);
                }
                this.actions
                    .change_action_state("inspector", &true.to_variant());
            }
        });
        self.actions.add_action(&tab);

        // The object styles (MOD-04, MOD-07): the style of the selected
        // object is the state, and choosing another applies it.
        stateful(
            self,
            "object-style",
            String::new().to_variant(),
            |_, editor, value| {
                if let Some(style) = value
                    .get::<String>()
                    .as_deref()
                    .and_then(crate::object_styles_pane::style_of_target)
                {
                    editor.apply_object_style(style);
                }
            },
        );
        /// A command on the style of the selected objects.
        type StyleCommand = fn(&TextEditor);
        let style_commands: [(&str, StyleCommand); 4] = [
            ("update-object-style", TextEditor::update_object_style),
            ("new-object-style", TextEditor::new_object_style),
            ("default-object-style", TextEditor::set_default_object_style),
            ("delete-object-style", TextEditor::delete_object_style),
        ];
        for (name, run) in style_commands {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                    run(&editor);
                }
            });
            self.actions.add_action(&action);
        }
        let rename = gio::SimpleAction::new("rename-object-style", None);
        let weak = Rc::downgrade(self);
        rename.connect_activate(move |_, _| {
            if let Some(this) = weak.upgrade() {
                this.rename_object_style();
            }
        });
        self.actions.add_action(&rename);

        let alpha = gio::SimpleAction::new("instant-alpha", None);
        let weak = Rc::downgrade(self);
        alpha.connect_activate(move |_, _| {
            if let Some(editor) = weak.upgrade().and_then(|this| this.editor()) {
                editor.start_instant_alpha();
            }
        });
        self.actions.add_action(&alpha);
    }

    /// Puts the format bar and the states of its actions in step with the
    /// selection.
    pub(super) fn show_format_bar(&self, editor: &TextEditor, format: &Format) {
        self.format_bar.show(format);
        let info: Option<ObjectInfo> = editor
            .has_selected_objects()
            .then(|| editor.object_info())
            .flatten();
        self.format_bar.show_objects(info.as_ref());
        if let Some(variant) = self.table_bars.show(editor) {
            self.format_bar.show_variant(variant);
        }
        let set =
            |name: &str, value: glib::Variant| formatting::set_state(&self.actions, name, value);
        set(
            "character-style",
            self.format_bar
                .character_index(editor.character_style())
                .to_variant(),
        );
        set(
            "line-spacing",
            spacing_name(format.paragraph.line_spacing).to_variant(),
        );
        if let Some(info) = &info {
            let style = &info.object.style;
            set("object-shadow", style.shadow.is_some().to_variant());
            set("object-reflection", style.reflection.is_some().to_variant());
            let inline = info.object.anchor == Anchor::InText;
            set(
                "object-placement",
                if inline { "inline" } else { "floating" }.to_variant(),
            );
            set("object-wrap", wrap_name(info.object.wrap).to_variant());
        }
        // The commands on the style of the object: updating a style asks
        // for an object changed since it took it.
        let styles = info.as_ref().map(|i| &i.styles);
        let current = styles.and_then(|s| s.current);
        set(
            "object-style",
            current
                .map(crate::object_styles_pane::style_target)
                .unwrap_or_default()
                .to_variant(),
        );
        let has_styles = styles.is_some_and(|s| !s.styles.is_empty());
        let locked = info.as_ref().is_some_and(|i| i.object.locked);
        for (name, on) in [
            ("object-style", has_styles && !locked),
            (
                "update-object-style",
                styles.is_some_and(|s| s.modified) && !locked,
            ),
            ("new-object-style", has_styles && !locked),
            ("default-object-style", has_styles),
            ("rename-object-style", current.is_some()),
            (
                "delete-object-style",
                current.is_some() && styles.is_some_and(|s| s.styles.len() > 1),
            ),
        ] {
            if let Some(action) = self
                .window
                .lookup_action(name)
                .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
            {
                action.set_enabled(on);
            }
        }
    }

    /// Asks for a new name of the style of the selected object, and
    /// renames it; a name that another style of its kind has is refused,
    /// and an alert says so.
    fn rename_object_style(self: &Rc<Self>) {
        use adw::prelude::*;
        let Some(editor) = self.editor() else { return };
        let Some(info) = editor.object_info() else {
            return;
        };
        let Some(style) = info
            .styles
            .current
            .and_then(|id| info.styles.styles.iter().find(|s| s.id == id))
        else {
            return;
        };
        let dialog = adw::AlertDialog::new(Some(&crate::i18n::tr("Rename Style")), None);
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let entry = adw::EntryRow::builder()
            .title(crate::i18n::tr("Name"))
            .activates_default(true)
            .build();
        entry.set_text(&crate::object_styles_pane::style_name(&style.name));
        list.append(&entry);
        dialog.set_extra_child(Some(&list));
        dialog.add_response("cancel", &crate::i18n::tr("_Cancel"));
        dialog.add_response("rename", &crate::i18n::tr("_Rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.connect_response(None, move |_, response| {
            let Some(this) = weak.upgrade() else { return };
            if response != "rename" {
                return;
            }
            if let Err(desk_model::ModelError::DuplicateStyleName(name)) =
                editor.rename_object_style(&entry.text())
            {
                let body = if name.is_empty() {
                    crate::i18n::tr("A style needs a name.")
                } else {
                    crate::i18n::tr("Another style is already named “{name}”.")
                        .replace("{name}", &name)
                };
                let error =
                    adw::AlertDialog::new(Some(&crate::i18n::tr("Rename Style")), Some(&body));
                error.add_response("close", &crate::i18n::tr("_Close"));
                error.present(Some(&this.window));
            }
        });
        dialog.present(Some(&self.window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_12_every_wrap_has_a_target() {
        for (name, _) in crate::format_bar::WRAPS {
            assert_eq!(wrap_of(name).map(wrap_name), Some(name));
        }
    }

    #[test]
    fn int_12_the_line_spacing_menu_names_multiples() {
        assert_eq!(spacing_name(LineSpacing::Multiple { percent: 150 }), "150");
        assert_eq!(
            spacing_name(LineSpacing::Exactly(desk_model::Length::from_points(18))),
            ""
        );
    }
}
