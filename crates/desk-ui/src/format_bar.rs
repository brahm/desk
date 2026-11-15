// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The slim format bar of interface B (INT-12): what is used all the time
//! for the selection, and the More menu at its end for the rest and Show
//! in Inspector. Text has the paragraph style, the font, the size, bold,
//! italic and underline, the color, the alignment, the line spacing and
//! the list; an object has its line, shadow, reflection and opacity, the
//! tools of a picture, how it sits in the text, and the Arrange commands.
//!
//! The toggles and menus run actions of the window, which the keyboard
//! and the command search also run; the rest calls the editor through
//! `editor`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{BorderStyle, CharAttrs, Length, ObjectKind, Stroke, StyleId};
use gtk::{gio, glib};

use crate::editing::{Format, ObjectInfo};
use crate::formatting::{EditorOf, from_gdk, label_factory, labelled, to_gdk};
use crate::i18n::{n_, tr, trc};

/// The largest font size Pages takes, in points.
const MAX_FONT_SIZE: f64 = 1638.0;

/// The line spacings of the menu, in percent of single, and their names.
pub const LINE_SPACINGS: [(u32, &str); 4] = [
    (100, n_("Single")),
    (115, n_("1.15")),
    (150, n_("1.5 Lines")),
    (200, n_("Double")),
];

/// The lists of the menu: the target of `win.list-style` and its name.
pub const LISTS: [(&str, &str); 4] = [
    ("none", n_("None")),
    ("bullets", n_("Bullets")),
    ("numbers", n_("Numbers")),
    ("tiered", n_("Tiered Numbers")),
];

/// The wraps of the menu: the target of `win.object-wrap` and its name.
pub const WRAPS: [(&str, &str); 7] = [
    ("both", n_("Around")),
    ("largest", n_("Largest Side")),
    ("left", n_("Left Side")),
    ("right", n_("Right Side")),
    ("above-and-below", n_("Above and Below")),
    ("through", n_("In Front of Text")),
    ("behind", n_("Behind Text")),
];

/// The lines of the Stroke popover, after none.
const STROKES: [BorderStyle; 3] = [BorderStyle::Solid, BorderStyle::Dashed, BorderStyle::Dotted];

/// The format bar.
pub struct FormatBar {
    root: gtk::Box,
    /// The text and the object variants, one at a time.
    stack: gtk::Stack,
    styles: gtk::DropDown,
    style_ids: RefCell<Vec<StyleId>>,
    /// The character styles after none, for the Character Style menu
    /// under More (STY-02).
    character_ids: RefCell<Vec<Option<StyleId>>>,
    character_menu: gio::Menu,
    /// The font family, by name: a font the document names stays named
    /// here even when it is not installed, as in Pages.
    font: gtk::Button,
    size: gtk::SpinButton,
    color: gtk::ColorDialogButton,
    /// What a narrow window leaves out of the text variant, and finds
    /// under More: the alignment, the line spacing and the list.
    secondary: [gtk::Widget; 3],
    text_more: gtk::MenuButton,
    /// The hyperlink at the selection, in a popover from the Inspector.
    link: gtk::MenuButton,
    /// The tools of a picture, which only a picture shows.
    picture_tools: gtk::Box,
    stroke: gtk::DropDown,
    stroke_width: gtk::SpinButton,
    stroke_color: gtk::ColorDialogButton,
    opacity: gtk::Scale,
    /// Set while the bar shows the formatting at the selection, so that
    /// showing it does not change it.
    updating: Cell<bool>,
}

impl FormatBar {
    /// A format bar for the editor of a window.
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let character_menu = gio::Menu::new();
        let (text, text_parts) = text_variant(&character_menu);
        let (object, object_parts) = object_variant();
        let stack = gtk::Stack::builder()
            .hhomogeneous(false)
            .vhomogeneous(false)
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        stack.add_named(&text, Some("text"));
        stack.add_named(&object, Some("object"));
        let root = gtk::Box::builder()
            .margin_start(6)
            .margin_end(6)
            .margin_top(3)
            .margin_bottom(3)
            .css_classes(["toolbar"])
            .build();
        root.append(&stack);
        stack.set_hexpand(true);

        let this = Rc::new(Self {
            root,
            stack,
            styles: text_parts.styles,
            style_ids: RefCell::new(Vec::new()),
            character_ids: RefCell::new(Vec::new()),
            character_menu,
            font: text_parts.font,
            size: text_parts.size,
            color: text_parts.color,
            secondary: text_parts.secondary,
            text_more: text_parts.more,
            link: text_parts.link,
            picture_tools: object_parts.picture_tools,
            stroke: object_parts.stroke,
            stroke_width: object_parts.stroke_width,
            stroke_color: object_parts.stroke_color,
            opacity: object_parts.opacity,
            updating: Cell::new(false),
        });
        this.connect(editor);
        this
    }

    /// Leaves out the secondary controls of text, on a narrow window: they
    /// go under More.
    pub fn set_compact(&self, compact: bool) {
        for widget in &self.secondary {
            widget.set_visible(!compact);
        }
        self.text_more
            .set_menu_model(Some(&text_more_menu(&self.character_menu, compact)));
    }

    /// The font button, where the Fonts popover opens.
    pub fn font_button(&self) -> &gtk::Button {
        &self.font
    }

    /// Opens the color chooser of the text color.
    pub fn choose_color(&self) {
        self.color.activate();
    }

    /// Gives the Link button the popover of the hyperlink.
    pub fn set_link_popover(&self, popover: &gtk::Popover) {
        self.link.set_popover(Some(popover));
    }

    /// The widget to put under the header bar.
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    /// The More menus of both variants, with the names they go by, for
    /// the command search.
    pub fn more_menus(&self) -> [(String, gio::MenuModel); 2] {
        [
            (
                tr("More"),
                text_more_menu(&self.character_menu, true).upcast(),
            ),
            (tr("More"), object_more_menu().upcast()),
        ]
    }

    fn connect(self: &Rc<Self>, editor: EditorOf) {
        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.styles.connect_selected_notify(move |dropdown| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let id = this
                .style_ids
                .borrow()
                .get(dropdown.selected() as usize)
                .copied();
            if let (Some(id), Some(editor)) = (id, of()) {
                editor.set_paragraph_style(id);
                editor.focus();
            }
        });

        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.size.connect_value_changed(move |spin| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = of() {
                editor.set_character_attrs(
                    "Font Size",
                    &CharAttrs {
                        font_size: Some(Length::from_pt(spin.value())),
                        ..CharAttrs::default()
                    },
                );
            }
        });

        let weak = Rc::downgrade(self);
        let of = editor.clone();
        self.color.connect_rgba_notify(move |button| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            if let Some(editor) = of() {
                editor.set_character_attrs(
                    "Text Color",
                    &CharAttrs {
                        color: Some(from_gdk(&button.rgba())),
                        ..CharAttrs::default()
                    },
                );
                editor.focus();
            }
        });

        // The line of the selected objects, from the Stroke popover.
        let restroke = {
            let weak = Rc::downgrade(self);
            let of = editor.clone();
            move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let stroke = this.stroke_value();
                this.stroke_width.set_sensitive(stroke.is_some());
                this.stroke_color.set_sensitive(stroke.is_some());
                if let Some(editor) = of() {
                    editor.style_objects("Stroke", move |style, _| {
                        style.stroke = stroke;
                        if stroke.is_some() {
                            style.frame = None;
                        }
                    });
                }
            }
        };
        let again = restroke.clone();
        self.stroke.connect_selected_notify(move |_| again());
        let again = restroke.clone();
        self.stroke_width.connect_value_changed(move |_| again());
        self.stroke_color.connect_rgba_notify(move |_| restroke());

        let weak = Rc::downgrade(self);
        self.opacity.connect_value_changed(move |scale| {
            let Some(this) = weak.upgrade() else { return };
            if this.updating.get() {
                return;
            }
            let opacity = scale.value().round().clamp(0.0, 100.0) as u8;
            if let Some(editor) = editor() {
                editor.style_objects("Opacity", move |style, _| style.opacity = opacity);
            }
        });
    }

    /// The line that the Stroke popover asks for.
    fn stroke_value(&self) -> Option<Stroke> {
        let style = *STROKES.get((self.stroke.selected() as usize).checked_sub(1)?)?;
        Some(Stroke {
            style,
            ..Stroke::new(
                from_gdk(&self.stroke_color.rgba()),
                Length::from_pt(self.stroke_width.value()),
            )
        })
    }

    /// Shows the paragraph styles of a document. Returns whether they
    /// changed.
    pub fn set_styles(&self, styles: &[(StyleId, String)]) -> bool {
        let ids: Vec<StyleId> = styles.iter().map(|(id, _)| *id).collect();
        let model = self
            .styles
            .model()
            .and_then(|m| m.downcast::<gtk::StringList>().ok());
        let same_names = model.is_some_and(|m| {
            m.n_items() as usize == styles.len()
                && styles
                    .iter()
                    .enumerate()
                    .all(|(i, (_, n))| m.string(i as u32).as_deref() == Some(n.as_str()))
        });
        if same_names && *self.style_ids.borrow() == ids {
            return false;
        }
        self.updating.set(true);
        let names: Vec<&str> = styles.iter().map(|(_, n)| n.as_str()).collect();
        self.styles.set_model(Some(&gtk::StringList::new(&names)));
        *self.style_ids.borrow_mut() = ids;
        self.updating.set(false);
        true
    }

    /// Shows the character styles of a document, after none, in the
    /// Character Style menu. Returns whether they changed.
    pub fn set_character_styles(&self, styles: &[(StyleId, String)]) -> bool {
        let ids: Vec<Option<StyleId>> = std::iter::once(None)
            .chain(styles.iter().map(|(id, _)| Some(*id)))
            .collect();
        let mut names = vec![tr("None")];
        names.extend(styles.iter().map(|(_, n)| n.clone()));
        let same_names = self.character_menu.n_items() as usize == names.len()
            && names.iter().enumerate().all(|(i, name)| {
                self.character_menu
                    .item_attribute_value(i as i32, "label", Some(glib::VariantTy::STRING))
                    .and_then(|v| v.get::<String>())
                    .as_deref()
                    == Some(name.as_str())
            });
        if same_names && *self.character_ids.borrow() == ids {
            return false;
        }
        self.character_menu.remove_all();
        for (index, name) in names.iter().enumerate() {
            let item = gio::MenuItem::new(Some(name), None);
            item.set_action_and_target_value(
                Some("win.character-style"),
                Some(&(index as i32).to_variant()),
            );
            self.character_menu.append_item(&item);
        }
        *self.character_ids.borrow_mut() = ids;
        true
    }

    /// The place of a character style in the Character Style menu, as the
    /// state of `win.character-style`; -1 when it is not there.
    pub fn character_index(&self, style: Option<StyleId>) -> i32 {
        self.character_ids
            .borrow()
            .iter()
            .position(|id| *id == style)
            .map_or(-1, |i| i as i32)
    }

    /// The character style at a place of the Character Style menu: none
    /// for no style, and nothing when there is no such place.
    pub fn character_style(&self, index: i32) -> Option<Option<StyleId>> {
        let index = usize::try_from(index).ok()?;
        self.character_ids.borrow().get(index).copied()
    }

    /// Shows the formatting at the selection.
    pub fn show(&self, format: &Format) {
        self.updating.set(true);
        let index = format
            .style
            .and_then(|s| self.style_ids.borrow().iter().position(|id| *id == s));
        self.styles
            .set_selected(index.map_or(gtk::INVALID_LIST_POSITION, |i| i as u32));
        let character = &format.character;
        self.font.set_label(&character.font_family);
        let size = character.font_size.to_pt();
        if (self.size.value() - size).abs() > 0.01 {
            self.size.set_value(size);
        }
        let color = to_gdk(character.color);
        if self.color.rgba() != color {
            self.color.set_rgba(&color);
        }
        self.updating.set(false);
    }

    /// Shows the variant of the selection: that of the objects when some
    /// are selected, with their line and opacity, and that of text
    /// otherwise.
    pub fn show_objects(&self, info: Option<&ObjectInfo>) {
        let Some(info) = info else {
            self.stack.set_visible_child_name("text");
            return;
        };
        self.updating.set(true);
        let style = &info.object.style;
        let selected = style
            .stroke
            .and_then(|s| STROKES.iter().position(|k| *k == s.style))
            .map_or(0, |i| i as u32 + 1);
        self.stroke.set_selected(selected);
        if let Some(stroke) = style.stroke {
            self.stroke_width.set_value(stroke.width.to_pt());
            let color = to_gdk(stroke.color);
            if self.stroke_color.rgba() != color {
                self.stroke_color.set_rgba(&color);
            }
        }
        self.stroke_width.set_sensitive(style.stroke.is_some());
        self.stroke_color.set_sensitive(style.stroke.is_some());
        self.opacity.set_value(f64::from(style.opacity));
        self.picture_tools
            .set_visible(info.count == 1 && matches!(info.object.kind, ObjectKind::Image(_)));
        self.updating.set(false);
        self.stack.set_visible_child_name("object");
    }
}

/// The controls of the text variant that the bar keeps.
struct TextParts {
    styles: gtk::DropDown,
    font: gtk::Button,
    size: gtk::SpinButton,
    color: gtk::ColorDialogButton,
    secondary: [gtk::Widget; 3],
    more: gtk::MenuButton,
    link: gtk::MenuButton,
}

/// The text variant: the paragraph style, the font and its size, bold,
/// italic and underline, the color, the alignment, the line spacing, the
/// list, and More.
fn text_variant(character_menu: &gio::Menu) -> (gtk::Box, TextParts) {
    let bar = gtk::Box::builder().spacing(6).build();
    let styles = gtk::DropDown::builder()
        .model(&gtk::StringList::new(&[]))
        .tooltip_text(tr("Paragraph Style"))
        .enable_search(true)
        .build();
    // The button shows the name cut short, so that a long name does not
    // widen the bar; the list shows names whole.
    styles.set_factory(Some(&label_factory(Some(14))));
    styles.set_list_factory(Some(&label_factory(None)));
    labelled(&styles, &tr("Paragraph Style"));
    bar.append(&styles);

    let font = gtk::Button::builder()
        .action_name("win.fonts")
        .tooltip_text(tr("Font"))
        .width_request(140)
        .build();
    bar.append(&font);
    let size = gtk::SpinButton::with_range(1.0, MAX_FONT_SIZE, 1.0);
    size.set_digits(0);
    size.set_tooltip_text(Some(&tr("Font Size")));
    labelled(&size, &tr("Font Size"));
    bar.append(&size);

    let toggles = gtk::Box::builder().css_classes(["linked"]).build();
    for (icon, action, tooltip) in [
        ("format-text-bold-symbolic", "win.bold", n_("Bold")),
        ("format-text-italic-symbolic", "win.italic", n_("Italic")),
        (
            "format-text-underline-symbolic",
            "win.underline",
            n_("Underline"),
        ),
    ] {
        let button = gtk::ToggleButton::builder()
            .icon_name(icon)
            .action_name(action)
            .tooltip_text(tr(tooltip))
            .build();
        labelled(&button, &tr(tooltip));
        toggles.append(&button);
    }
    bar.append(&toggles);

    let color = gtk::ColorDialogButton::new(Some(
        gtk::ColorDialog::builder()
            .title(tr("Text Color"))
            .modal(true)
            .with_alpha(true)
            .build(),
    ));
    color.set_tooltip_text(Some(&tr("Text Color")));
    labelled(&color, &tr("Text Color"));
    let colors = gtk::Box::builder().css_classes(["linked"]).build();
    colors.append(&color);
    colors.append(&crate::eyedropper::button(&color));
    bar.append(&colors);

    let alignments = gtk::Box::builder().css_classes(["linked"]).build();
    for (icon, target, tooltip) in ALIGNMENTS {
        let button = gtk::ToggleButton::builder()
            .icon_name(icon)
            .tooltip_text(tr(tooltip))
            .build();
        labelled(&button, &tr(tooltip));
        button.set_action_name(Some("win.align"));
        button.set_action_target_value(Some(&target.to_variant()));
        alignments.append(&button);
    }
    bar.append(&alignments);
    let spacing = icon_menu_button(
        "desk-line-spacing-symbolic",
        &tr("Line Spacing"),
        &line_spacing_menu(),
    );
    bar.append(&spacing);
    let lists = icon_menu_button("view-list-bullet-symbolic", &tr("List"), &list_menu());
    bar.append(&lists);
    // The Link inspector of Pages is a popover in interface B (INT-13).
    let link = gtk::MenuButton::builder()
        .icon_name("desk-link-symbolic")
        .tooltip_text(tr("Link"))
        .build();
    labelled(&link, &tr("Link"));
    bar.append(&link);

    bar.append(&gtk::Box::builder().hexpand(true).build());
    let more = icon_menu_button(
        "view-more-horizontal-symbolic",
        &tr("More"),
        &text_more_menu(character_menu, false),
    );
    more.set_tooltip_text(Some(&tr("More Formatting")));
    bar.append(&more);
    let parts = TextParts {
        styles,
        font,
        size,
        color,
        secondary: [alignments.upcast(), spacing.upcast(), lists.upcast()],
        more,
        link,
    };
    (bar, parts)
}

/// The alignments: an icon, the target of `win.align` and a name.
const ALIGNMENTS: [(&str, &str, &str); 4] = [
    ("format-justify-left-symbolic", "left", n_("Align Left")),
    ("format-justify-center-symbolic", "center", n_("Center")),
    ("format-justify-right-symbolic", "right", n_("Align Right")),
    ("format-justify-fill-symbolic", "justify", n_("Justify")),
];

/// A button with an icon and a menu, named for the accessibility tree.
fn icon_menu_button(icon: &str, name: &str, menu: &gio::Menu) -> gtk::MenuButton {
    let button = gtk::MenuButton::builder()
        .icon_name(icon)
        .menu_model(menu)
        .tooltip_text(name)
        .build();
    labelled(&button, name);
    button
}

/// A button with an icon that runs an action, named for the accessibility
/// tree.
fn icon_button(icon: &str, name: &str, action: &str, toggle: bool) -> gtk::Widget {
    let button: gtk::Button = if toggle {
        gtk::ToggleButton::builder()
            .icon_name(icon)
            .build()
            .upcast()
    } else {
        gtk::Button::builder().icon_name(icon).build()
    };
    button.set_action_name(Some(action));
    button.set_tooltip_text(Some(name));
    labelled(&button, name);
    button.upcast()
}

/// The Line Spacing menu: single to double, and the Inspector for more.
fn line_spacing_menu() -> gio::Menu {
    let spacings = gio::Menu::new();
    for (percent, name) in LINE_SPACINGS {
        let item = gio::MenuItem::new(Some(&tr(name)), None);
        item.set_action_and_target_value(
            Some("win.line-spacing"),
            Some(&percent.to_string().to_variant()),
        );
        spacings.append_item(&item);
    }
    let menu = gio::Menu::new();
    menu.append_section(None, &spacings);
    let more = gio::Menu::new();
    more.append(
        Some(&tr("Show in _Inspector")),
        Some("win.inspector-tab::style"),
    );
    menu.append_section(None, &more);
    menu
}

/// The List menu: none, bullets or numbers.
fn list_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    for (target, name) in LISTS {
        let item = gio::MenuItem::new(Some(&tr(name)), None);
        item.set_action_and_target_value(Some("win.list-style"), Some(&target.to_variant()));
        menu.append_item(&item);
    }
    menu
}

/// The More menu of text: what the bar leaves out, the Format menu of
/// Pages, and Show in Inspector. A narrow window adds the alignment, the
/// line spacing and the list.
fn text_more_menu(character_menu: &gio::Menu, compact: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    let character = gio::Menu::new();
    character.append_submenu(Some(&tr("C_haracter Style")), character_menu);
    character.append(Some(&tr("_Strikethrough")), Some("win.strikethrough"));
    let columns = gio::Menu::new();
    for (count, label) in [
        (1, n_("1 Column")),
        (2, n_("2 Columns")),
        (3, n_("3 Columns")),
        (4, n_("4 Columns")),
    ] {
        let item = gio::MenuItem::new(Some(&tr(label)), None);
        item.set_action_and_target_value(Some("win.columns"), Some(&count.to_variant()));
        columns.append_item(&item);
    }
    character.append_submenu(Some(&tr("Col_umns")), &columns);
    menu.append_section(None, &character);
    if compact {
        let narrow = gio::Menu::new();
        let alignment = gio::Menu::new();
        for (_, target, name) in ALIGNMENTS {
            let item = gio::MenuItem::new(Some(&tr(name)), None);
            item.set_action_and_target_value(Some("win.align"), Some(&target.to_variant()));
            alignment.append_item(&item);
        }
        narrow.append_submenu(Some(&tr("_Alignment")), &alignment);
        narrow.append_submenu(Some(&tr("_Line Spacing")), &line_spacing_menu());
        narrow.append_submenu(Some(&tr("L_ist")), &list_menu());
        menu.append_section(None, &narrow);
    }
    let format = crate::window::format_menu();
    for i in 0..format.n_items() {
        if let Some(section) = format.item_link(i, "section") {
            menu.append_section(None, &section);
        }
    }
    let inspector = gio::Menu::new();
    inspector.append(Some(&tr("Show in _Inspector")), Some("win.show-inspector"));
    menu.append_section(None, &inspector);
    menu
}

/// The controls of the object variant that the bar changes.
struct ObjectParts {
    picture_tools: gtk::Box,
    stroke: gtk::DropDown,
    stroke_width: gtk::SpinButton,
    stroke_color: gtk::ColorDialogButton,
    opacity: gtk::Scale,
}

/// The object variant: the line, shadow, reflection and opacity; Instant
/// Alpha, Mask and Adjust Image for a picture; inline or floating and the
/// wrap; front, back and Arrange; and More.
fn object_variant() -> (gtk::Box, ObjectParts) {
    let bar = gtk::Box::builder().spacing(6).build();

    // The line: none, solid, dashed or dotted, its width and its color.
    let stroke =
        gtk::DropDown::from_strings(&[&tr("No Line"), &tr("Line"), &tr("Dashed"), &tr("Dotted")]);
    labelled(&stroke, &tr("Line Style"));
    let stroke_width = gtk::SpinButton::with_range(0.25, 100.0, 0.25);
    stroke_width.set_digits(2);
    labelled(&stroke_width, &tr("Line Width"));
    let stroke_color = gtk::ColorDialogButton::new(Some(
        gtk::ColorDialog::builder()
            .title(tr("Line Color"))
            .modal(true)
            .with_alpha(true)
            .build(),
    ));
    labelled(&stroke_color, &tr("Line Color"));
    let stroke_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    stroke_box.append(&stroke);
    let width_row = gtk::Box::builder().spacing(6).build();
    width_row.append(&stroke_width);
    width_row.append(&stroke_color);
    stroke_box.append(&width_row);
    let stroke_button = gtk::MenuButton::builder()
        .icon_name("desk-stroke-symbolic")
        .popover(&gtk::Popover::builder().child(&stroke_box).build())
        .tooltip_text(tr("Line"))
        .build();
    labelled(&stroke_button, &tr("Line"));
    bar.append(&stroke_button);

    let effects = gtk::Box::builder().css_classes(["linked"]).build();
    effects.append(&icon_button(
        "desk-shadow-symbolic",
        &tr("Shadow"),
        "win.object-shadow",
        true,
    ));
    effects.append(&icon_button(
        "desk-reflection-symbolic",
        &tr("Reflection"),
        "win.object-reflection",
        true,
    ));
    bar.append(&effects);

    let opacity = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    opacity.set_width_request(180);
    opacity.set_draw_value(true);
    opacity.set_value_pos(gtk::PositionType::Right);
    opacity.set_digits(0);
    labelled(&opacity, &tr("Opacity"));
    let opacity_button = gtk::MenuButton::builder()
        .icon_name("desk-opacity-symbolic")
        .popover(&gtk::Popover::builder().child(&opacity).build())
        .tooltip_text(tr("Opacity"))
        .build();
    labelled(&opacity_button, &tr("Opacity"));
    bar.append(&opacity_button);

    let picture_tools = gtk::Box::builder().spacing(6).build();
    let tools = gtk::Box::builder().css_classes(["linked"]).build();
    tools.append(&icon_button(
        "desk-wand-symbolic",
        &tr("Instant Alpha"),
        "win.instant-alpha",
        false,
    ));
    tools.append(&icon_button(
        "desk-crop-symbolic",
        &tr("Mask"),
        "win.mask-with-shape('rectangle')",
        false,
    ));
    tools.append(&icon_button(
        "desk-image-adjust-symbolic",
        &tr("Adjust Image"),
        "win.inspector-tab::image",
        false,
    ));
    picture_tools.append(&tools);
    bar.append(&picture_tools);

    // Inline moves with the text; floating stays where it is put.
    let placement = gtk::Box::builder().css_classes(["linked"]).build();
    for (target, name) in [("inline", n_("Inline")), ("floating", n_("Floating"))] {
        let button = gtk::ToggleButton::builder().label(tr(name)).build();
        button.set_action_name(Some("win.object-placement"));
        button.set_action_target_value(Some(&target.to_variant()));
        placement.append(&button);
    }
    bar.append(&placement);
    let wraps = gio::Menu::new();
    for (target, name) in WRAPS {
        let item = gio::MenuItem::new(Some(&tr(name)), None);
        item.set_action_and_target_value(Some("win.object-wrap"), Some(&target.to_variant()));
        wraps.append_item(&item);
    }
    bar.append(&icon_menu_button("desk-wrap-symbolic", &tr("Wrap"), &wraps));

    let order = gtk::Box::builder().css_classes(["linked"]).build();
    order.append(&icon_button(
        "desk-front-symbolic",
        &tr("Bring to Front"),
        "win.bring-to-front",
        false,
    ));
    order.append(&icon_button(
        "desk-back-symbolic",
        &tr("Send to Back"),
        "win.send-to-back",
        false,
    ));
    bar.append(&order);
    bar.append(&icon_menu_button(
        "desk-arrange-symbolic",
        &tr("Arrange"),
        &crate::window::arrange_menu(),
    ));

    bar.append(&gtk::Box::builder().hexpand(true).build());
    let more = icon_menu_button(
        "view-more-horizontal-symbolic",
        &tr("More"),
        &object_more_menu(),
    );
    more.set_tooltip_text(Some(&tr("More Formatting")));
    bar.append(&more);
    let parts = ObjectParts {
        picture_tools,
        stroke,
        stroke_width,
        stroke_color,
        opacity,
    };
    (bar, parts)
}

/// The More menu of objects: the masks and the shapes, the masters, and
/// Show in Inspector.
fn object_more_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let picture = gio::Menu::new();
    picture.append(Some(&tr("_Instant Alpha")), Some("win.instant-alpha"));
    picture.append(Some(&tr("Un_mask")), Some("win.unmask"));
    menu.append_section(None, &picture);
    let copy = gio::Menu::new();
    copy.append(
        Some(&trc("Arrange", "Dupl_icate")),
        Some("win.duplicate-objects"),
    );
    copy.append(Some(&tr("_Lock")), Some("win.lock"));
    copy.append(Some(&tr("U_nlock")), Some("win.unlock"));
    menu.append_section(None, &copy);
    let defaults = gio::Menu::new();
    defaults.append(
        Some(&tr("Reapply _Defaults to Selection")),
        Some("win.reapply-defaults"),
    );
    defaults.append(
        Some(&tr("Show Co_nnection Lines")),
        Some("win.connection-lines"),
    );
    menu.append_section(None, &defaults);
    let inspector = gio::Menu::new();
    inspector.append(Some(&tr("Show in _Inspector")), Some("win.show-inspector"));
    menu.append_section(None, &inspector);
    menu
}
