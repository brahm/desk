// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Table, Cell and Text tabs of the Inspector for a selected table
//! (TAB-02 to TAB-32): the headers, footers, name and options of the
//! table, the rows and columns, the data format, fill, borders,
//! conditional highlighting and comment of the selected cells, and the
//! formatting and alignment of their text.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use desk_model::table::{
    Border, BorderSet, CellFill, Color, Condition, DateStyle, Format, HAlign, LastCellTab,
    LineStyle, Negative, Num, Operand, Rule, RuleStyle, TimeStyle, VAlign,
};

use std::sync::Arc;

use crate::compact;
use crate::formatting::EditorOf;
use crate::i18n::{n_, tr};

/// The data formats of the menu, in the order of iWork (TAB-28).
pub(crate) const FORMATS: [&str; 9] = [
    n_("Automatic"),
    n_("Number"),
    n_("Currency"),
    n_("Percentage"),
    n_("Date and Time"),
    n_("Duration"),
    n_("Fraction"),
    n_("Scientific"),
    n_("Text"),
];

/// The borders of the menu, after none.
const BORDERS: [(BorderSet, &str); 7] = [
    (BorderSet::All, n_("All Borders")),
    (BorderSet::Outside, n_("Outside Borders")),
    (BorderSet::Inside, n_("Inside Borders")),
    (BorderSet::Top, n_("Top Border")),
    (BorderSet::Bottom, n_("Bottom Border")),
    (BorderSet::Left, n_("Left Border")),
    (BorderSet::Right, n_("Right Border")),
];

/// The conditions of the menu of conditional highlighting, after none.
const CONDITIONS: [&str; 6] = [
    n_("Greater Than"),
    n_("Less Than"),
    n_("Equal To"),
    n_("Text Contains"),
    n_("Date Is Before Today"),
    n_("Cell Is Empty"),
];

pub(crate) fn format_index(format: &Format) -> u32 {
    match format {
        Format::Automatic => 0,
        Format::Number { .. } => 1,
        Format::Currency { .. } => 2,
        Format::Percent { .. } => 3,
        Format::DateTime { .. } => 4,
        Format::Duration { .. } => 5,
        Format::Fraction { .. } => 6,
        Format::Scientific { .. } => 7,
        Format::Text => 8,
        _ => 0,
    }
}

fn format_decimals(format: &Format) -> Option<u8> {
    match format {
        Format::Number { decimals, .. } | Format::Percent { decimals, .. } => *decimals,
        Format::Currency { decimals, .. } => Some(*decimals),
        Format::Scientific { decimals } => Some(*decimals),
        _ => None,
    }
}

/// The format at an index of the menu, with decimals and separators.
pub(crate) fn format_at(index: u32, decimals: Option<u8>, thousands: bool, symbol: &str) -> Format {
    match index {
        1 => Format::Number {
            decimals,
            thousands,
            negative: Negative::Minus,
        },
        2 => Format::Currency {
            symbol: symbol.to_owned(),
            decimals: decimals.unwrap_or(2),
            thousands,
            negative: Negative::Minus,
            accounting: false,
        },
        3 => Format::Percent {
            decimals,
            thousands,
            negative: Negative::Minus,
        },
        4 => Format::DateTime {
            date: DateStyle::default(),
            time: TimeStyle::default(),
        },
        5 => Format::Duration {
            style: Default::default(),
        },
        6 => Format::Fraction {
            precision: desk_model::table::FractionPrecision::default(),
        },
        7 => Format::Scientific {
            decimals: decimals.unwrap_or(2),
        },
        8 => Format::Text,
        _ => Format::Automatic,
    }
}

pub(crate) fn to_gdk(c: Color) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
        f32::from(c.a) / 255.0,
    )
}

pub(crate) fn from_gdk(c: &gtk::gdk::RGBA) -> Color {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color {
        r: byte(c.red()),
        g: byte(c.green()),
        b: byte(c.blue()),
        a: byte(c.alpha()),
    }
}

fn color_button() -> gtk::ColorDialogButton {
    gtk::ColorDialogButton::builder()
        .dialog(&gtk::ColorDialog::new())
        .valign(gtk::Align::Center)
        .build()
}

fn toggle(icon: &str, tip: &str) -> gtk::ToggleButton {
    let b = gtk::ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(tr(tip))
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    b.update_property(&[gtk::accessible::Property::Label(&tr(tip))]);
    b
}

/// What a control of the pane does to the selected cells.
type Run = Box<dyn Fn(&TableGroups, &crate::editing::TextEditor)>;
/// What a control of the pane does to the whole table.
type TableChange = fn(&TableGroups, &mut desk_model::Table);
/// What a toggle of the pane does to the text of the cells.
type TextChange = fn(&mut desk_model::table::TextStyle, bool);

/// The groups of the tabs of a selected table.
pub struct TableGroups {
    /// The table styles in view, with Copy, Paste and Create (PXV-14).
    styles: compact::Group,
    /// Where the pictures of the styles go.
    style_holder: gtk::Box,
    /// What the pictures show: the styles and the chosen one.
    shown_styles: std::cell::RefCell<
        Option<(
            Vec<desk_model::TableStyleEntry>,
            Option<desk_model::StyleId>,
        )>,
    >,
    paste_style: gtk::Button,
    editor: EditorOf,
    table: compact::Group,
    rows_cols: compact::Group,
    cell: compact::Group,
    highlight: compact::Group,
    text: compact::Group,
    name: compact::EntryRow,
    show_name: compact::SwitchRow,
    outline_name: compact::SwitchRow,
    header_rows: compact::SpinRow,
    header_cols: compact::SpinRow,
    footer_rows: compact::SpinRow,
    repeat_headers: compact::SwitchRow,
    alternating: compact::SwitchRow,
    font_size: compact::SpinRow,
    return_moves: compact::SwitchRow,
    autocomplete: compact::SwitchRow,
    last_tab: compact::ComboRow,
    rows: compact::SpinRow,
    cols: compact::SpinRow,
    format: compact::ComboRow,
    decimals: compact::SpinRow,
    thousands: compact::SwitchRow,
    fill_on: compact::SwitchRow,
    fill: gtk::ColorDialogButton,
    wrap: compact::SwitchRow,
    border_width: compact::SpinRow,
    border_color: gtk::ColorDialogButton,
    comment: compact::EntryRow,
    condition: compact::ComboRow,
    operand: compact::EntryRow,
    highlight_color: gtk::ColorDialogButton,
    bold: gtk::ToggleButton,
    italic: gtk::ToggleButton,
    underline: gtk::ToggleButton,
    strike: gtk::ToggleButton,
    text_color: gtk::ColorDialogButton,
    text_size: compact::SpinRow,
    h_align: compact::ComboRow,
    v_align: compact::ComboRow,
    updating: Cell<bool>,
}

impl TableGroups {
    pub fn new(editor: EditorOf) -> Rc<Self> {
        let spin = |title: &str, min: f64, max: f64| {
            let row = compact::SpinRow::with_range(min, max, 1.0);
            row.set_title(&tr(title));
            row
        };
        let switch = |title: &str| compact::SwitchRow::builder().title(tr(title)).build();
        let combo = |title: &str, items: &[&str]| {
            let names: Vec<String> = items.iter().map(|s| tr(s)).collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            compact::ComboRow::builder()
                .title(tr(title))
                .model(&gtk::StringList::new(&names))
                .build()
        };

        let name = compact::EntryRow::builder().title(tr("Name")).build();
        // Title of the table options of Pages 15: the name centered
        // above the table, with a line around it when asked (PXV-15).
        let show_name = switch(n_("Title"));
        let outline_name = switch(n_("Outline Table Title"));
        let header_rows = spin(n_("Header Rows"), 0.0, 5.0);
        let header_cols = spin(n_("Header Columns"), 0.0, 5.0);
        let footer_rows = spin(n_("Footer Rows"), 0.0, 5.0);
        let repeat_headers = switch(n_("Repeat Header Rows on Each Page"));
        let alternating = switch(n_("Alternating Row Color"));
        let font_size = spin(n_("Table Font Size"), 6.0, 96.0);
        font_size.set_subtitle(&tr("Points"));
        let return_moves = switch(n_("Return Moves to Next Cell"));
        let autocomplete = switch(n_("Suggest Values of the Column"));
        let last_tab = combo(
            n_("Tab in the Last Cell"),
            &[
                n_("Adds a Row"),
                n_("Adds a Column"),
                n_("Goes to the First Cell"),
            ],
        );
        let table = compact::Group::builder().title(tr("Table")).build();
        table.add(&name);
        table.add(&show_name);
        table.add(&outline_name);
        table.add(&header_rows);
        table.add(&header_cols);
        table.add(&footer_rows);
        table.add(&repeat_headers);
        table.add(&alternating);
        table.add(&font_size);
        table.add(&return_moves);
        table.add(&autocomplete);
        table.add(&last_tab);

        let rows = spin(n_("Rows"), 1.0, 1000.0);
        let cols = spin(n_("Columns"), 1.0, 1000.0);
        let rows_cols = compact::Group::builder()
            .title(tr("Rows and Columns"))
            .build();
        rows_cols.add(&rows);
        rows_cols.add(&cols);
        let actions = gtk::Box::builder()
            .spacing(6)
            .halign(gtk::Align::End)
            .build();
        for (label, action) in [
            (n_("Merge"), "win.table-merge-cells"),
            (n_("Sort"), "win.table-sort-ascending"),
            (n_("Fill"), "win.table-fill-down"),
        ] {
            let b = gtk::Button::builder()
                .label(tr(label))
                .action_name(action)
                .build();
            actions.append(&b);
        }
        rows_cols.add_widget(&actions);

        let format = combo(n_("Data Format"), &FORMATS);
        let decimals = spin(n_("Decimals"), 0.0, 10.0);
        let thousands = switch(n_("Thousands Separator"));
        let fill_on = switch(n_("Fill"));
        let fill = color_button();
        fill_on.add_suffix(&fill);
        let wrap = switch(n_("Wrap Text in Cell"));
        let border_names: Vec<&str> = std::iter::once(n_("No Borders"))
            .chain(BORDERS.iter().map(|(_, n)| *n))
            .collect();
        let borders = combo(n_("Borders"), &border_names);
        let border_width = spin(n_("Border Width"), 0.0, 20.0);
        border_width.set_subtitle(&tr("Points"));
        border_width.set_digits(1);
        let border_color = color_button();
        border_width.add_suffix(&border_color);
        let comment = compact::EntryRow::builder().title(tr("Comment")).build();
        let cell = compact::Group::builder().title(tr("Cell")).build();
        cell.add(&format);
        cell.add(&decimals);
        cell.add(&thousands);
        cell.add(&fill_on);
        cell.add(&wrap);
        cell.add(&borders);
        cell.add(&border_width);
        cell.add(&comment);

        let condition_names: Vec<&str> = std::iter::once(n_("None"))
            .chain(CONDITIONS.iter().copied())
            .collect();
        let condition = combo(n_("Highlight When"), &condition_names);
        let operand = compact::EntryRow::builder().title(tr("Value")).build();
        let highlight_color = color_button();
        highlight_color.set_rgba(&gtk::gdk::RGBA::new(1.0, 0.8, 0.8, 1.0));
        condition.add_suffix(&highlight_color);
        let highlight = compact::Group::builder()
            .title(tr("Conditional Highlighting"))
            .build();
        highlight.add(&condition);
        highlight.add(&operand);

        let bold = toggle("format-text-bold-symbolic", n_("Bold"));
        let italic = toggle("format-text-italic-symbolic", n_("Italic"));
        let underline = toggle("format-text-underline-symbolic", n_("Underline"));
        let strike = toggle("format-text-strikethrough-symbolic", n_("Strikethrough"));
        let text_color = color_button();
        let styles = compact::ActionRow::builder().title(tr("Font")).build();
        for b in [&bold, &italic, &underline, &strike] {
            styles.add_suffix(b);
        }
        styles.add_suffix(&text_color);
        let text_size = spin(n_("Size"), 0.0, 400.0);
        text_size.set_subtitle(&tr("Points; 0 for that of the table"));
        let h_align = combo(
            n_("Alignment"),
            &[
                n_("Automatic"),
                n_("Left"),
                n_("Center"),
                n_("Right"),
                n_("Justified"),
            ],
        );
        let v_align = combo(
            n_("Vertical Alignment"),
            &[n_("Top"), n_("Middle"), n_("Bottom")],
        );
        let text = compact::Group::builder().title(tr("Cell Text")).build();
        text.add(&styles);
        text.add(&text_size);
        text.add(&h_align);
        text.add(&v_align);

        let style_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let style_buttons = gtk::Box::builder()
            .spacing(6)
            .homogeneous(true)
            .margin_top(6)
            .build();
        let style_button = |label: &str| {
            let button = gtk::Button::with_label(&tr(label));
            style_buttons.append(&button);
            button
        };
        let copy_style = style_button(n_("Copy Style"));
        let paste_style = style_button(n_("Paste Style"));
        let create_style = style_button(n_("Create Style"));
        copy_style.set_tooltip_text(Some(&tr("Copy Table Style")));
        paste_style.set_tooltip_text(Some(&tr("Paste Table Style")));
        create_style.set_tooltip_text(Some(&tr("Create Table Style")));
        let styles_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        styles_box.append(&style_holder);
        styles_box.append(&style_buttons);
        let styles = compact::Group::builder().title(tr("Table Styles")).build();
        styles.add_widget(&styles_box);
        for (button, run) in [
            (
                &copy_style,
                crate::editing::TextEditor::copy_table_style as fn(&crate::editing::TextEditor),
            ),
            (&paste_style, crate::editing::TextEditor::paste_table_style),
            (
                &create_style,
                crate::editing::TextEditor::create_table_style,
            ),
        ] {
            let editor = editor.clone();
            button.connect_clicked(move |_| {
                if let Some(editor) = editor() {
                    run(&editor);
                }
            });
        }

        let this = Rc::new(Self {
            styles,
            style_holder,
            shown_styles: std::cell::RefCell::new(None),
            paste_style,
            editor: editor.clone(),
            table,
            rows_cols,
            cell,
            highlight,
            text,
            name,
            show_name,
            outline_name,
            header_rows,
            header_cols,
            footer_rows,
            repeat_headers,
            alternating,
            font_size,
            return_moves,
            autocomplete,
            last_tab,
            rows,
            cols,
            format,
            decimals,
            thousands,
            fill_on,
            fill,
            wrap,
            border_width,
            border_color,
            comment,
            condition,
            operand,
            highlight_color,
            bold,
            italic,
            underline,
            strike,
            text_color,
            text_size,
            h_align,
            v_align,
            updating: Cell::new(false),
        });
        this.connect(&editor, &borders);
        this
    }

    /// The groups: Table Styles, Table and Rows and Columns for the Table
    /// tab, Cell and Conditional Highlighting for the Cell tab, and Cell
    /// Text for the Text tab.
    pub fn groups(&self) -> [&compact::Group; 6] {
        [
            &self.styles,
            &self.table,
            &self.rows_cols,
            &self.cell,
            &self.highlight,
            &self.text,
        ]
    }

    fn connect(self: &Rc<Self>, editor: &EditorOf, borders: &compact::ComboRow) {
        // Runs `f` with the editor, unless the rows are being filled.
        let run = |this: &Rc<Self>, f: Run| {
            let weak = Rc::downgrade(this);
            let editor = editor.clone();
            Rc::new(move || {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    f(&this, &editor);
                }
            })
        };
        let on_table =
            |this: &Rc<Self>, name: &'static str, f: fn(&Self, &mut desk_model::Table)| {
                run(
                    this,
                    Box::new(move |this, editor| {
                        editor.change_table(name, |table, _| f(this, table));
                    }),
                )
            };
        {
            let f = on_table(self, "Table Name", |this, table| {
                table.name = this.name.text().to_string();
            });
            self.name.connect_apply(move |_| f());
        }
        let switches: [(&compact::SwitchRow, &'static str, TableChange); 6] = [
            (&self.show_name, "Table Title", |this, t| {
                t.options.show_name = this.show_name.is_active();
                t.options.name_centered = true;
            }),
            (&self.outline_name, "Outline Table Title", |this, t| {
                t.options.name_outline = this.outline_name.is_active();
            }),
            (&self.repeat_headers, "Header Rows", |this, t| {
                t.options.repeat_headers = this.repeat_headers.is_active();
            }),
            (&self.alternating, "Alternating Rows", |this, t| {
                t.style.alternating = this
                    .alternating
                    .is_active()
                    .then_some(Color::rgb(0xf2, 0xf2, 0xf2));
            }),
            (&self.return_moves, "Table Options", |this, t| {
                t.options.return_moves = this.return_moves.is_active();
            }),
            (&self.autocomplete, "Table Options", |this, t| {
                t.options.autocomplete = this.autocomplete.is_active();
            }),
        ];
        for (row, name, f) in switches {
            let f = on_table(self, name, f);
            row.connect_active_notify(move |_| f());
        }
        let spins: [(&compact::SpinRow, &'static str, TableChange); 6] = [
            (&self.header_rows, "Header Rows", |this, t| {
                let _ = t.set_header_rows(this.header_rows.value() as u32);
            }),
            (&self.header_cols, "Header Columns", |this, t| {
                let _ = t.set_header_cols(this.header_cols.value() as u32);
            }),
            (&self.footer_rows, "Footer Rows", |this, t| {
                let _ = t.set_footer_rows(this.footer_rows.value() as u32);
            }),
            (&self.font_size, "Table Font Size", |this, t| {
                t.style.font_size = (this.font_size.value() * 1000.0) as u32;
            }),
            (&self.rows, "Rows", |this, t| {
                let n = this.rows.value() as u32;
                if n > t.rows() {
                    let _ = t.insert_rows(t.rows(), n - t.rows());
                } else if n < t.rows() {
                    let _ = t.delete_rows(n, t.rows() - n);
                }
            }),
            (&self.cols, "Columns", |this, t| {
                let n = this.cols.value() as u32;
                if n > t.cols() {
                    let _ = t.insert_cols(t.cols(), n - t.cols());
                } else if n < t.cols() {
                    let _ = t.delete_cols(n, t.cols() - n);
                }
            }),
        ];
        for (row, name, f) in spins {
            let f = on_table(self, name, f);
            row.connect_value_notify(move |_| f());
        }
        {
            let f = on_table(self, "Table Options", |this, t| {
                t.options.last_cell_tab = match this.last_tab.selected() {
                    1 => LastCellTab::AddColumn,
                    2 => LastCellTab::Wrap,
                    _ => LastCellTab::AddRow,
                };
            });
            self.last_tab.connect_selected_notify(move |_| f());
        }
        // The data format, its decimals and thousands.
        let format = run(
            self,
            Box::new(|this, editor| {
                let decimals = (this.format.selected() != 0 && this.decimals.value() >= 0.0)
                    .then(|| this.decimals.value() as u8);
                let symbol = editor.locale().currency.clone();
                editor.set_cell_format(format_at(
                    this.format.selected(),
                    decimals,
                    this.thousands.is_active(),
                    &symbol,
                ));
            }),
        );
        {
            let f = format.clone();
            self.format.connect_selected_notify(move |_| f());
        }
        {
            let f = format.clone();
            self.decimals.connect_value_notify(move |_| f());
        }
        self.thousands.connect_active_notify(move |_| format());
        // The fill of the cells.
        let fill = run(
            self,
            Box::new(|this, editor| {
                let fill = this
                    .fill_on
                    .is_active()
                    .then(|| CellFill::Color(from_gdk(&this.fill.rgba())));
                editor.set_cell_style("Fill", |s| s.fill = fill.clone());
            }),
        );
        {
            let f = fill.clone();
            self.fill_on.connect_active_notify(move |_| f());
        }
        self.fill.connect_rgba_notify(move |_| fill());
        {
            let f = run(
                self,
                Box::new(|this, editor| {
                    let wrap = this.wrap.is_active();
                    editor.set_cell_style("Wrap Text", |s| s.wrap = wrap);
                }),
            );
            self.wrap.connect_active_notify(move |_| f());
        }
        // The borders: which, as wide and of the color shown.
        {
            let weak = Rc::downgrade(self);
            let editor = editor.clone();
            borders.connect_selected_notify(move |combo| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                let Some(editor) = editor() else { return };
                let index = combo.selected();
                if index == 0 {
                    editor.set_cell_borders(BorderSet::All, None);
                } else if let Some((which, _)) = BORDERS.get(index as usize - 1) {
                    let border = Border {
                        width: (this.border_width.value().max(0.25) * 1000.0) as u32,
                        color: from_gdk(&this.border_color.rgba()),
                        style: LineStyle::Solid,
                    };
                    editor.set_cell_borders(*which, Some(border));
                }
                // The menu is a command: it shows none chosen after.
                this.updating.set(true);
                combo.set_selected(gtk::INVALID_LIST_POSITION);
                this.updating.set(false);
            });
        }
        self.border_width.set_value(1.0);
        self.border_color
            .set_rgba(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));
        {
            let f = run(
                self,
                Box::new(|this, editor| {
                    let text = this.comment.text().to_string();
                    let comment = (!text.trim().is_empty()).then_some(text);
                    editor.change_cells("Comment", |c| c.comment = comment.clone());
                }),
            );
            self.comment.connect_apply(move |_| f());
        }
        // Conditional highlighting: one rule of the cells.
        let rule = run(
            self,
            Box::new(|this, editor| {
                let text = this.operand.text().to_string();
                let number = desk_model::table::parse_number(&text, &editor.locale())
                    .map(|(n, _)| Operand::Number(Num(n)))
                    .unwrap_or_else(|| Operand::Text(text.clone()));
                let condition = match this.condition.selected() {
                    1 => Some(Condition::Greater(number)),
                    2 => Some(Condition::Less(number)),
                    3 => Some(Condition::Equal(number)),
                    4 => Some(Condition::Contains(text)),
                    5 => Some(Condition::BeforeToday),
                    6 => Some(Condition::Empty),
                    _ => None,
                };
                let rules: Arc<[Rule]> = match condition {
                    Some(condition) => Arc::from([Rule {
                        condition,
                        style: RuleStyle {
                            fill: Some(from_gdk(&this.highlight_color.rgba())),
                            ..RuleStyle::default()
                        },
                    }]),
                    None => Arc::from([]),
                };
                editor.change_cells("Conditional Highlighting", |c| c.rules = rules.clone());
            }),
        );
        {
            let f = rule.clone();
            self.condition.connect_selected_notify(move |_| f());
        }
        {
            let f = rule.clone();
            self.operand.connect_apply(move |_| f());
        }
        self.highlight_color.connect_rgba_notify(move |_| rule());
        // The text of the cells.
        let toggles: [(&gtk::ToggleButton, &'static str, TextChange); 4] = [
            (&self.bold, "Bold", |t, on| t.bold = on),
            (&self.italic, "Italic", |t, on| t.italic = on),
            (&self.underline, "Underline", |t, on| t.underline = on),
            (&self.strike, "Strikethrough", |t, on| t.strike = on),
        ];
        for (button, name, set) in toggles {
            let weak = Rc::downgrade(self);
            let editor = editor.clone();
            button.connect_toggled(move |b| {
                let Some(this) = weak.upgrade() else { return };
                if this.updating.get() {
                    return;
                }
                if let Some(editor) = editor() {
                    let on = b.is_active();
                    editor.set_cell_text(name, |t| set(t, on));
                }
            });
        }
        {
            let f = run(
                self,
                Box::new(|this, editor| {
                    let color = from_gdk(&this.text_color.rgba());
                    editor.set_cell_text("Text Color", |t| t.color = Some(color));
                }),
            );
            self.text_color.connect_rgba_notify(move |_| f());
        }
        {
            let f = run(
                self,
                Box::new(|this, editor| {
                    let size = this.text_size.value();
                    let size = (size > 0.0).then_some((size * 1000.0) as u32);
                    editor.set_cell_text("Font Size", |t| t.size = size);
                }),
            );
            self.text_size.connect_value_notify(move |_| f());
        }
        {
            let f = run(
                self,
                Box::new(|this, editor| {
                    let h = match this.h_align.selected() {
                        1 => HAlign::Left,
                        2 => HAlign::Center,
                        3 => HAlign::Right,
                        4 => HAlign::Justify,
                        _ => HAlign::Auto,
                    };
                    editor.set_cell_style("Alignment", |s| s.h_align = h);
                }),
            );
            self.h_align.connect_selected_notify(move |_| f());
        }
        {
            let f = run(
                self,
                Box::new(|this, editor| {
                    let v = match this.v_align.selected() {
                        1 => VAlign::Middle,
                        2 => VAlign::Bottom,
                        _ => VAlign::Top,
                    };
                    editor.set_cell_style("Alignment", |s| s.v_align = v);
                }),
            );
            self.v_align.connect_selected_notify(move |_| f());
        }
    }

    /// The pictures of the table styles, with the one the table looks
    /// like chosen; drawn again only when they change.
    fn show_styles(&self, editor: &crate::editing::TextEditor) {
        self.paste_style
            .set_sensitive(editor.can_paste_table_style());
        let shown = (editor.table_styles(), editor.current_table_style());
        if self.shown_styles.borrow().as_ref() == Some(&shown) {
            return;
        }
        while let Some(child) = self.style_holder.first_child() {
            self.style_holder.remove(&child);
        }
        let editor_of = self.editor.clone();
        let grid = crate::table_styles::grid(&shown.0, shown.1, move |style| {
            if let Some(editor) = editor_of() {
                editor.apply_table_style(style);
            }
        });
        self.style_holder.append(&grid);
        *self.shown_styles.borrow_mut() = Some(shown);
    }

    /// Shows the selected table and cells.
    pub fn show(&self, editor: &crate::editing::TextEditor) {
        let Some((table, area)) = editor.table_selection() else {
            return;
        };
        self.updating.set(true);
        // A locked table is not edited: only Unlock, in Arrange, works
        // (PXV-16).
        let editable = !editor.selection_locked();
        for group in self.groups() {
            group.set_sensitive(editable);
        }
        self.show_styles(editor);
        let set_spin = |row: &compact::SpinRow, v: f64| {
            if (row.value() - v).abs() > 0.001 {
                row.set_value(v);
            }
        };
        let set_switch = |row: &compact::SwitchRow, on: bool| {
            if row.is_active() != on {
                row.set_active(on);
            }
        };
        if self.name.text() != table.name {
            self.name.set_text(&table.name);
        }
        set_switch(&self.show_name, table.options.show_name);
        set_switch(&self.outline_name, table.options.name_outline);
        self.outline_name.set_sensitive(table.options.show_name);
        set_spin(&self.header_rows, f64::from(table.header_rows()));
        set_spin(&self.header_cols, f64::from(table.header_cols()));
        set_spin(&self.footer_rows, f64::from(table.footer_rows()));
        set_switch(&self.repeat_headers, table.options.repeat_headers);
        set_switch(&self.alternating, table.style.alternating.is_some());
        set_spin(&self.font_size, f64::from(table.style.font_size) / 1000.0);
        set_switch(&self.return_moves, table.options.return_moves);
        set_switch(&self.autocomplete, table.options.autocomplete);
        self.last_tab
            .set_selected(match table.options.last_cell_tab {
                LastCellTab::AddRow => 0,
                LastCellTab::AddColumn => 1,
                LastCellTab::Wrap => 2,
            });
        set_spin(&self.rows, f64::from(table.rows()));
        set_spin(&self.cols, f64::from(table.cols()));
        let cell = table.cell(area.row, area.col);
        self.format.set_selected(format_index(&cell.format));
        set_spin(
            &self.decimals,
            f64::from(format_decimals(&cell.format).unwrap_or(0)),
        );
        self.decimals
            .set_sensitive(matches!(format_index(&cell.format), 1 | 2 | 3 | 7));
        let thousands = matches!(
            cell.format,
            Format::Number {
                thousands: true,
                ..
            } | Format::Currency {
                thousands: true,
                ..
            } | Format::Percent {
                thousands: true,
                ..
            }
        );
        set_switch(&self.thousands, thousands);
        let style = &cell.style;
        set_switch(&self.fill_on, style.fill.is_some());
        if let Some(CellFill::Color(c)) = &style.fill {
            self.fill.set_rgba(&to_gdk(*c));
        }
        set_switch(&self.wrap, style.wrap);
        let comment = cell.comment.clone().unwrap_or_default();
        if self.comment.text() != comment {
            self.comment.set_text(&comment);
        }
        let (index, operand) = match cell.rules.first().map(|r| &r.condition) {
            Some(Condition::Greater(o)) => (1, Some(o)),
            Some(Condition::Less(o)) => (2, Some(o)),
            Some(Condition::Equal(o)) => (3, Some(o)),
            Some(Condition::Contains(_)) => (4, None),
            Some(Condition::BeforeToday) => (5, None),
            Some(Condition::Empty) => (6, None),
            _ => (0, None),
        };
        self.condition.set_selected(index);
        let operand = match (operand, cell.rules.first().map(|r| &r.condition)) {
            (Some(Operand::Number(n)), _) => n.0.to_string(),
            (Some(Operand::Text(t)), _) => t.clone(),
            (_, Some(Condition::Contains(t))) => t.clone(),
            _ => String::new(),
        };
        if self.operand.text() != operand {
            self.operand.set_text(&operand);
        }
        if let Some(fill) = cell.rules.first().and_then(|r| r.style.fill) {
            self.highlight_color.set_rgba(&to_gdk(fill));
        }
        let text = &style.text;
        self.bold.set_active(text.bold);
        self.italic.set_active(text.italic);
        self.underline.set_active(text.underline);
        self.strike.set_active(text.strike);
        self.text_color
            .set_rgba(&to_gdk(text.color.unwrap_or(Color::BLACK)));
        set_spin(
            &self.text_size,
            text.size.map_or(0.0, |s| f64::from(s) / 1000.0),
        );
        self.h_align.set_selected(match style.h_align {
            HAlign::Auto => 0,
            HAlign::Left => 1,
            HAlign::Center => 2,
            HAlign::Right => 3,
            HAlign::Justify => 4,
        });
        self.v_align.set_selected(match style.v_align {
            VAlign::Top => 0,
            VAlign::Middle => 1,
            VAlign::Bottom => 2,
        });
        // The cells' tabs say nothing while the whole table is selected.
        let cells = editor.selected_cells().is_some();
        self.cell.set_sensitive(cells);
        self.highlight.set_sensitive(cells);
        self.updating.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_28_every_format_of_the_menu_comes_back_to_its_place() {
        for i in 0..FORMATS.len() as u32 {
            assert_eq!(format_index(&format_at(i, Some(2), true, "R$")), i);
        }
    }
}
