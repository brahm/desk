// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bullets and numbering in the Inspector (LST-02 to LST-05), as the List
//! pane of the Text inspector of Pages has them: the kind of list, the
//! level, the bullet or the number format, where the numbers start, and
//! the indents, size, position and color of the labels.
//!
//! The kind of list chooses a list style of Pages for the selection. The
//! other rows change the level of the selected items over their list
//! style, as Pages does, which shows the style with a red arrow in the
//! Styles drawer until it is redefined from the selection.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use desk_model::{Length, ListLabel, ListLevel, ListStyle, NumberFormat, Picture, StyleId};
use gtk::glib;

use crate::bullet_pictures;
use crate::editing::Format;
use crate::effects::ColorRow;
use crate::formatting::{EditorOf, labelled};
use crate::i18n::tr;
use crate::preferences::Units;
use crate::tab_stops::{self, string_list};

/// The kinds of list, in the order of their menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Not a list.
    None,
    /// Text bullets.
    Bullets,
    /// Image bullets (LST-03).
    Images,
    /// Numbers.
    Numbers,
    /// Tiered numbers, as 1.1.
    Tiered,
}

/// Makes a list style of Pages with an identifier and a name.
type MakeList = fn(StyleId, &str) -> ListStyle;

const KINDS: [Kind; 5] = [
    Kind::None,
    Kind::Bullets,
    Kind::Images,
    Kind::Numbers,
    Kind::Tiered,
];

impl Kind {
    fn name(self) -> String {
        match self {
            Self::None => tr("None"),
            Self::Bullets => tr("Text Bullets"),
            Self::Images => tr("Image Bullets"),
            Self::Numbers => tr("Numbers"),
            Self::Tiered => tr("Tiered Numbers"),
        }
    }

    /// The kind of a level of an item of a list style: numbers are tiered
    /// when any level is, as the first level of Legal, which has none above
    /// it.
    pub fn of(list: &ListStyle, definition: &ListLevel) -> Self {
        let tiered = |l: &ListLevel| matches!(l.label, ListLabel::Number { tiered: true, .. });
        match &definition.label {
            ListLabel::None => Self::None,
            ListLabel::Bullet(_) => Self::Bullets,
            ListLabel::Image(_) => Self::Images,
            ListLabel::Number { .. } if list.levels.iter().any(tiered) || tiered(definition) => {
                Self::Tiered
            }
            ListLabel::Number { .. } => Self::Numbers,
        }
    }

    /// The list style of Pages for the kind, by name, and how to make it
    /// when the document has none.
    fn style(self) -> Option<(&'static str, MakeList)> {
        match self {
            Self::None => None,
            Self::Bullets | Self::Images => Some(("Bullet", ListStyle::bullet)),
            Self::Numbers => Some(("Numbered List", ListStyle::numbered)),
            Self::Tiered => Some(("Legal", ListStyle::legal)),
        }
    }
}

/// The number formats of the menu: how the number is written, and the
/// text before and after it.
const NUMBERS: [(NumberFormat, &str, &str); 15] = {
    use NumberFormat::*;
    [
        (Decimal, "", "."),
        (UpperRoman, "", "."),
        (LowerRoman, "", "."),
        (UpperAlpha, "", "."),
        (LowerAlpha, "", "."),
        (Decimal, "", ")"),
        (UpperRoman, "", ")"),
        (LowerRoman, "", ")"),
        (UpperAlpha, "", ")"),
        (LowerAlpha, "", ")"),
        (Decimal, "(", ")"),
        (UpperRoman, "(", ")"),
        (LowerRoman, "(", ")"),
        (UpperAlpha, "(", ")"),
        (LowerAlpha, "(", ")"),
    ]
};

/// How a number format of the menu looks, as "1. 2. 3.".
fn number_name((format, prefix, suffix): (NumberFormat, &str, &str)) -> String {
    (1..=3)
        .map(|n| format!("{prefix}{}{suffix}", format.write(n)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The item of the menu for a label: the same format, prefix and suffix,
/// or else the same format.
pub fn number_index(label: &ListLabel) -> Option<usize> {
    let ListLabel::Number {
        format,
        prefix,
        suffix,
        ..
    } = label
    else {
        return None;
    };
    NUMBERS
        .iter()
        .position(|&(f, p, s)| f == *format && p == &**prefix && s == &**suffix)
        .or_else(|| NUMBERS.iter().position(|&(f, _, _)| f == *format))
}

/// The largest size of a label, in percent.
const MAX_SIZE: f64 = 400.0;

/// The bullets and numbering of the Inspector.
pub struct ListsGroup {
    group: adw::PreferencesGroup,
    kind: adw::ComboRow,
    level: adw::SpinRow,
    bullet: adw::EntryRow,
    /// The picture of an image bullet, and the button that chooses one.
    image: adw::ActionRow,
    image_menu: gtk::MenuButton,
    image_preview: gtk::Image,
    number: adw::ComboRow,
    continues: adw::SwitchRow,
    start: adw::SpinRow,
    label_indent: adw::SpinRow,
    text_indent: adw::SpinRow,
    size: adw::SpinRow,
    scale: adw::SwitchRow,
    raise: adw::SpinRow,
    color: ColorRow,
    /// The list style and the level shown, which the rows change, and
    /// the definition of the level that the item has.
    shown: RefCell<Option<(ListStyle, u8, ListLevel)>>,
    units: Cell<Units>,
    editor: EditorOf,
    updating: Cell<bool>,
}

impl ListsGroup {
    /// The bullets and numbering of the Inspector of a window.
    pub fn new(editor: EditorOf, units: Units) -> Rc<Self> {
        let group = adw::PreferencesGroup::builder()
            .title(gtk::glib::markup_escape_text(&tr("Bullets & Numbering")))
            .build();
        let kind = adw::ComboRow::builder()
            .title(tr("List"))
            .model(&string_list(KINDS.map(Kind::name)))
            .build();
        let level = adw::SpinRow::with_range(1.0, desk_model::LIST_LEVELS as f64, 1.0);
        level.set_title(&tr("Indent Level"));
        let bullet = adw::EntryRow::builder()
            .title(tr("Bullet"))
            .show_apply_button(true)
            .build();
        let image_preview = gtk::Image::builder().pixel_size(24).build();
        let image_menu = gtk::MenuButton::builder()
            .child(&image_preview)
            .valign(gtk::Align::Center)
            .tooltip_text(tr("Choose the Picture of the Bullets"))
            .build();
        let image = adw::ActionRow::builder().title(tr("Image")).build();
        image.add_suffix(&image_menu);
        image.set_activatable_widget(Some(&image_menu));
        let number = adw::ComboRow::builder()
            .title(tr("Number Format"))
            .model(&string_list(NUMBERS.map(number_name)))
            .build();
        let continues = adw::SwitchRow::builder()
            .title(tr("Continue from Previous"))
            .build();
        let start = adw::SpinRow::with_range(1.0, 9999.0, 1.0);
        start.set_title(&tr("Start At"));
        let label_indent = adw::SpinRow::with_range(0.0, 1.0, 1.0);
        label_indent.set_title(&tr("Bullet Indent"));
        let text_indent = adw::SpinRow::with_range(0.0, 1.0, 1.0);
        text_indent.set_title(&tr("Text Indent"));
        let size = adw::SpinRow::with_range(1.0, MAX_SIZE, 1.0);
        size.set_title(&tr("Size"));
        size.set_subtitle(&tr("Percent"));
        let scale = adw::SwitchRow::builder()
            .title(tr("Scale with Text"))
            .build();
        let raise = adw::SpinRow::with_range(-100.0, 100.0, 0.5);
        raise.set_title(&tr("Align"));
        raise.set_subtitle(&tr("Points above the baseline"));
        raise.set_digits(1);
        let color = ColorRow::new(&tr("Label Color"), desk_model::Rgba::BLACK);
        for row in [
            kind.upcast_ref::<gtk::Widget>(),
            level.upcast_ref(),
            bullet.upcast_ref(),
            image.upcast_ref(),
            number.upcast_ref(),
            continues.upcast_ref(),
            start.upcast_ref(),
            label_indent.upcast_ref(),
            text_indent.upcast_ref(),
            size.upcast_ref(),
            scale.upcast_ref(),
            raise.upcast_ref(),
            color.row.upcast_ref(),
        ] {
            group.add(row);
        }

        let this = Rc::new(Self {
            group,
            kind,
            level,
            bullet,
            image,
            image_menu,
            image_preview,
            number,
            continues,
            start,
            label_indent,
            text_indent,
            size,
            scale,
            raise,
            color,
            shown: RefCell::new(None),
            units: Cell::new(units),
            editor,
            updating: Cell::new(false),
        });
        this.set_units(units);
        this.show(&Format::default());
        this.connect();
        this.build_image_menu();
        this
    }

    fn connect(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.kind.connect_selected_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if let Some(&kind) = KINDS.get(row.selected() as usize)
                && !this.updating.get()
            {
                this.set_kind(kind);
            }
        });
        let weak = Rc::downgrade(self);
        self.level.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            let shown = this.shown.borrow().as_ref().map(|(_, level, _)| *level);
            if let Some(level) = shown
                && !this.updating.get()
                && let Some(editor) = (this.editor)()
            {
                let by = row.value() as i32 - 1 - i32::from(level);
                if by != 0 {
                    editor.change_list_level(by, true);
                }
            }
        });
        let weak = Rc::downgrade(self);
        self.bullet.connect_apply(move |row| {
            let Some(this) = weak.upgrade() else { return };
            let text = row.text();
            if !text.is_empty() {
                this.edit_level("Bullet", |level| {
                    level.label = ListLabel::bullet(&text);
                });
            }
        });
        let weak = Rc::downgrade(self);
        self.number.connect_selected_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            let Some(&(format, prefix, suffix)) = NUMBERS.get(row.selected() as usize) else {
                return;
            };
            this.edit_level("Number Format", |level| {
                let tiered = matches!(level.label, ListLabel::Number { tiered: true, .. });
                level.label = ListLabel::Number {
                    format,
                    prefix: prefix.into(),
                    suffix: suffix.into(),
                    tiered,
                };
            });
        });
        let weak = Rc::downgrade(self);
        self.continues.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            this.start.set_sensitive(!row.is_active());
            let start = (!row.is_active()).then(|| this.start.value() as u32);
            this.set_start(start);
        });
        let weak = Rc::downgrade(self);
        self.start.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            if !this.continues.is_active() {
                this.set_start(Some(row.value() as u32));
            }
        });
        for (row, indent) in [(&self.label_indent, true), (&self.text_indent, false)] {
            let weak = Rc::downgrade(self);
            row.connect_value_notify(move |row| {
                let Some(this) = weak.upgrade() else { return };
                let length = tab_stops::from_units(row.value(), this.units.get());
                this.edit_level("Indent", |level| {
                    if indent {
                        level.label_indent = length;
                    } else {
                        level.text_indent = length;
                    }
                });
            });
        }
        let weak = Rc::downgrade(self);
        self.size.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            let size = row.value().round() as u16;
            this.edit_level("Label Size", |level| level.size = size);
        });
        let weak = Rc::downgrade(self);
        self.scale.connect_active_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            let scale = row.is_active();
            this.edit_level("Label Size", |level| level.scale_with_text = scale);
        });
        let weak = Rc::downgrade(self);
        self.raise.connect_value_notify(move |row| {
            let Some(this) = weak.upgrade() else { return };
            let raise = Length::from_pt(row.value());
            this.edit_level("Label Position", |level| level.raise = raise);
        });
        let weak = Rc::downgrade(self);
        self.color.connect_changed(move || {
            let Some(this) = weak.upgrade() else { return };
            this.color
                .button
                .set_sensitive(this.color.switch.is_active());
            let color = this.color.value();
            this.edit_level("Label Color", |level| level.color = color);
        });
    }

    /// Makes the selection a list of a kind, with the list style of Pages
    /// for it, or takes it out of its list.
    fn set_kind(&self, kind: Kind) {
        let Some(editor) = (self.editor)() else {
            return;
        };
        let shown = self.shown.borrow().clone();
        if let Some((list, _, definition)) = &shown
            && Kind::of(list, definition) == kind
        {
            return;
        }
        let in_list = shown.is_some();
        match kind.style() {
            None => editor.set_list_style(None),
            // Image bullets go over the list of the items, or over bullets.
            Some(_) if kind == Kind::Images && in_list => {
                if let Some(picture) = bullet_pictures::default_picture() {
                    self.set_picture(picture);
                }
            }
            Some((name, make)) => {
                let picture = (kind == Kind::Images)
                    .then(bullet_pictures::default_picture)
                    .flatten();
                let result = editor.style_edit("List Style", |edit, from, to| {
                    let style = edit.list_style_or_add(name, make)?;
                    edit.set_list_style(from, to, Some(style))?;
                    if let Some(picture) = &picture {
                        edit.change_list_items(from, to, |level| {
                            level.label = ListLabel::Image(picture.clone());
                        })?;
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    glib::g_warning!("desk", "cannot make a list: {error}");
                }
            }
        }
    }

    /// Gives the selected items an image bullet of a picture.
    fn set_picture(&self, picture: Picture) {
        self.edit_level("Image Bullet", move |level| {
            level.label = ListLabel::Image(picture.clone());
        });
    }

    /// The menu of the pictures of image bullets: those that come with
    /// Desk, and a picture from a file.
    fn build_image_menu(self: &Rc<Self>) {
        let popover = gtk::Popover::new();
        let grid = gtk::FlowBox::builder()
            .max_children_per_line(4)
            .selection_mode(gtk::SelectionMode::None)
            .build();
        for (name, picture) in bullet_pictures::pictures() {
            let image = gtk::Image::builder().pixel_size(24).build();
            if let Some(texture) = bullet_pictures::texture(&picture) {
                image.set_paintable(Some(&texture));
            }
            let button = gtk::Button::builder()
                .child(&image)
                .tooltip_text(&name)
                .css_classes(["flat"])
                .build();
            labelled(&button, &name);
            let weak = Rc::downgrade(self);
            let popover_ = popover.downgrade();
            button.connect_clicked(move |_| {
                if let Some(popover) = popover_.upgrade() {
                    popover.popdown();
                }
                if let Some(this) = weak.upgrade() {
                    this.set_picture(picture.clone());
                }
            });
            grid.insert(&button, -1);
        }
        let choose = gtk::Button::builder()
            .label(tr("Choose Image…"))
            .css_classes(["flat"])
            .build();
        let weak = Rc::downgrade(self);
        let popover_ = popover.downgrade();
        choose.connect_clicked(move |button| {
            if let Some(popover) = popover_.upgrade() {
                popover.popdown();
            }
            let filter = gtk::FileFilter::new();
            filter.set_name(Some(&tr("Images")));
            filter.add_pixbuf_formats();
            let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            let dialog = gtk::FileDialog::builder()
                .title(tr("Choose an Image for the Bullets"))
                .modal(true)
                .filters(&filters)
                .default_filter(&filter)
                .build();
            let parent = button.root().and_downcast::<gtk::Window>();
            let weak = weak.clone();
            dialog.open(
                parent.as_ref(),
                gtk::gio::Cancellable::NONE,
                move |result| {
                    let (Some(this), Ok(file)) = (weak.upgrade(), result) else {
                        return;
                    };
                    match bullet_pictures::picture_from_file(&file) {
                        Some(picture) => this.set_picture(picture),
                        None => glib::g_warning!("desk", "cannot read the picture"),
                    }
                },
            );
        });
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(&grid);
        content.append(&choose);
        popover.set_child(Some(&content));
        self.image_menu.set_popover(Some(&popover));
    }

    /// Changes the levels of the selected items over their list style.
    fn edit_level(&self, name: &str, change: impl Fn(&mut ListLevel)) {
        if self.updating.get() {
            return;
        }
        let Some((_, _, definition)) = self.shown.borrow().clone() else {
            return;
        };
        let Some(editor) = (self.editor)() else {
            return;
        };
        let mut changed = definition.clone();
        change(&mut changed);
        if changed == definition {
            return;
        }
        let result = editor.style_edit(name, |edit, from, to| {
            edit.change_list_items(from, to, &change)
        });
        if let Err(error) = result {
            glib::g_warning!("desk", "cannot change the list: {error}");
        }
    }

    /// Makes the first selected item start its list again at a number, or
    /// count on from the item before it.
    fn set_start(&self, start: Option<u32>) {
        if self.updating.get() || self.shown.borrow().is_none() {
            return;
        }
        let Some(editor) = (self.editor)() else {
            return;
        };
        let result = editor.style_edit("List Start", |edit, from, _| {
            edit.set_list_start(from.paragraph, start)
        });
        if let Err(error) = result {
            glib::g_warning!("desk", "cannot number the list: {error}");
        }
    }

    /// The group to put in the Inspector.
    pub fn widget(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    /// Measures the indents in other units.
    pub fn set_units(&self, units: Units) {
        self.updating.set(true);
        self.units.set(units);
        tab_stops::set_units(&self.label_indent, units, 0.0, 1584.0);
        tab_stops::set_units(&self.text_indent, units, 0.0, 1584.0);
        self.updating.set(false);
        let shown = self.shown.borrow().clone();
        if let Some((_, level, definition)) = shown {
            self.show_level(&definition, level);
        }
    }

    /// Shows the list at the selection.
    pub fn show(&self, format: &Format) {
        let paragraph = &format.paragraph;
        let list = format
            .list
            .clone()
            .filter(|list| list.level(paragraph.list_level).label.is_some());
        self.updating.set(true);
        let kind = list.as_ref().map_or(Kind::None, |l| {
            let definition = paragraph
                .list_override
                .as_deref()
                .unwrap_or(l.level(paragraph.list_level));
            Kind::of(l, definition)
        });
        let index = KINDS.iter().position(|k| *k == kind).unwrap_or(0) as u32;
        if self.kind.selected() != index {
            self.kind.set_selected(index);
        }
        let continues = paragraph.list_start.is_none();
        if self.continues.is_active() != continues {
            self.continues.set_active(continues);
        }
        self.start.set_sensitive(!continues);
        self.updating.set(false);
        match list {
            Some(list) => {
                let level = paragraph.list_level;
                let definition = paragraph
                    .list_override
                    .as_deref()
                    .unwrap_or(list.level(level))
                    .clone();
                self.show_level(&definition, level);
                *self.shown.borrow_mut() = Some((list, level, definition));
                if let Some(start) = paragraph.list_start {
                    self.updating.set(true);
                    self.start.set_value(f64::from(start));
                    self.updating.set(false);
                }
            }
            None => {
                *self.shown.borrow_mut() = None;
                for row in self.rows() {
                    row.set_visible(false);
                }
            }
        }
    }

    /// The rows under the kind of list.
    fn rows(&self) -> [&gtk::Widget; 12] {
        [
            self.level.upcast_ref(),
            self.bullet.upcast_ref(),
            self.image.upcast_ref(),
            self.number.upcast_ref(),
            self.continues.upcast_ref(),
            self.start.upcast_ref(),
            self.label_indent.upcast_ref(),
            self.text_indent.upcast_ref(),
            self.size.upcast_ref(),
            self.scale.upcast_ref(),
            self.raise.upcast_ref(),
            self.color.row.upcast_ref(),
        ]
    }

    fn show_level(&self, definition: &ListLevel, level: u8) {
        self.updating.set(true);
        for row in self.rows() {
            row.set_visible(true);
        }
        let set = |row: &adw::SpinRow, value: f64| {
            if (row.value() - value).abs() > 0.001 {
                row.set_value(value);
            }
        };
        set(&self.level, f64::from(level) + 1.0);
        let numbered = matches!(definition.label, ListLabel::Number { .. });
        let image = matches!(definition.label, ListLabel::Image(_));
        self.bullet.set_visible(!numbered && !image);
        self.image.set_visible(image);
        // A picture has no color of its own to set.
        self.color.row.set_visible(!image);
        self.number.set_visible(numbered);
        self.continues.set_visible(numbered);
        self.start.set_visible(numbered);
        match &definition.label {
            ListLabel::Bullet(text) => {
                if self.bullet.text() != **text {
                    self.bullet.set_text(text);
                }
            }
            ListLabel::Image(picture) => {
                self.image_preview
                    .set_paintable(bullet_pictures::texture(picture).as_ref());
            }
            label => {
                if let Some(index) = number_index(label)
                    && self.number.selected() != index as u32
                {
                    self.number.set_selected(index as u32);
                }
                if self.continues.is_active() {
                    set(&self.start, f64::from(definition.start));
                }
            }
        }
        let units = self.units.get();
        set(
            &self.label_indent,
            tab_stops::in_units(definition.label_indent, units),
        );
        set(
            &self.text_indent,
            tab_stops::in_units(definition.text_indent, units),
        );
        set(&self.size, f64::from(definition.size));
        if self.scale.is_active() != definition.scale_with_text {
            self.scale.set_active(definition.scale_with_text);
        }
        set(&self.raise, definition.raise.to_pt());
        self.color.show(definition.color);
        self.updating.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats_look_like_their_labels() {
        assert_eq!(number_name(NUMBERS[0]), "1. 2. 3.");
        assert_eq!(number_name(NUMBERS[11]), "(I) (II) (III)");
        let label = ListLabel::number(NumberFormat::LowerAlpha, ")");
        assert_eq!(number_index(&label), Some(9));
        // A label that the menu does not have shows its number format.
        let label = ListLabel::Number {
            format: NumberFormat::LowerRoman,
            prefix: "[".into(),
            suffix: "]".into(),
            tiered: false,
        };
        assert_eq!(number_index(&label), Some(2));
        assert_eq!(number_index(&ListLabel::bullet("•")), None);
    }

    #[test]
    fn kinds_of_the_styles_of_pages() {
        let id = StyleId::new(1).expect("not zero");
        let kind = |list: &ListStyle, level: u8| Kind::of(list, list.level(level));
        assert_eq!(kind(&ListStyle::bullet(id, "B"), 0), Kind::Bullets);
        assert_eq!(kind(&ListStyle::numbered(id, "N"), 3), Kind::Numbers);
        assert_eq!(kind(&ListStyle::legal(id, "L"), 0), Kind::Tiered);
        let none = ListStyle::from_levels(id, "None", |_| ListLevel::default());
        assert_eq!(kind(&none, 0), Kind::None);
    }
}
