// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The header bar of Desk Tables (NINT-07): the same as Desk Writer's in
//! interface B. At the start, the Sidebar button with the View menu, the
//! Insert menu and the buttons of the objects; in the middle, the title;
//! at the end, Find, the Inspector and the main menu. Sheet, which the
//! toolbar of Numbers had, is the Add button of the sidebar; Function is
//! in the formula bar; Reorganize is the Data level of the Inspector.

use adw::prelude::*;
use gtk::gio;

use crate::AppInfo;
use crate::formatting;
use crate::i18n::tr;

/// The parts of the header bar that the window changes later.
pub(super) struct Header {
    pub header: adw::HeaderBar,
    pub title: adw::WindowTitle,
    /// Insert, whose menu takes the objects in a narrow window.
    pub insert_button: gtk::MenuButton,
    /// The buttons of the objects, which a narrow window leaves out.
    pub objects: gtk::Box,
}

impl Header {
    pub fn new(info: AppInfo) -> Self {
        let title = adw::WindowTitle::new(info.name, "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();

        // The Sidebar button shows the navigation sidebar (F9). In window
        // M+ it has no menu: View is a submenu of the main menu.
        let sidebar_button = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .action_name("win.sidebar")
            .tooltip_text(tr("Sidebar"))
            .build();
        formatting::labelled(&sidebar_button, &tr("Sidebar"));
        let insert_label = adw::ButtonContent::builder()
            .icon_name("desk-insert-symbolic")
            .label(tr("_Insert"))
            .use_underline(true)
            .build();
        let insert_button = gtk::MenuButton::builder()
            .child(&insert_label)
            .menu_model(&insert_menu(false))
            .tooltip_text(tr("Insert"))
            .build();
        let objects = gtk::Box::builder().spacing(6).build();
        for (icon, label, action) in [
            ("desk-table-symbolic", tr("Table"), "win.insert-table"),
            ("desk-chart-symbolic", tr("Chart"), "win.insert-chart"),
            (
                "desk-textbox-symbolic",
                tr("Text Box"),
                "win.insert-text-box",
            ),
            ("desk-shape-symbolic", tr("Shape"), "win.insert-shape"),
            ("desk-media-symbolic", tr("Media"), "win.insert-picture"),
            ("desk-comment-symbolic", tr("Comment"), "win.insert-comment"),
        ] {
            let button = gtk::Button::builder()
                .icon_name(icon)
                .action_name(action)
                .tooltip_text(&label)
                .build();
            formatting::labelled(&button, &label);
            objects.append(&button);
        }
        let start = gtk::Box::builder().spacing(6).build();
        start.append(&sidebar_button);
        start.append(&insert_button);
        start.append(&objects);
        header.pack_start(&start);

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&main_menu(info))
            .primary(true)
            .tooltip_text(tr("Main Menu"))
            .build();
        formatting::labelled(&menu_button, &tr("Main Menu"));
        header.pack_end(&menu_button);
        let inspector_toggle = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-right-symbolic")
            .action_name("win.inspector")
            .tooltip_text(tr("Inspector"))
            .build();
        formatting::labelled(&inspector_toggle, &tr("Inspector"));
        let find_button = gtk::Button::builder()
            .icon_name("edit-find-symbolic")
            .action_name("win.find")
            .tooltip_text(tr("Find"))
            .build();
        formatting::labelled(&find_button, &tr("Find"));
        // Open Documents shows the open documents as a grid
        // (AdwTabOverview), to switch between them.
        let overview_button = gtk::Button::builder()
            .icon_name("view-grid-symbolic")
            .action_name("win.tab-overview")
            .tooltip_text(tr("Open Documents"))
            .build();
        formatting::labelled(&overview_button, &tr("Open Documents"));
        let end = gtk::Box::builder().spacing(6).build();
        end.append(&overview_button);
        end.append(&find_button);
        end.append(&inspector_toggle);
        header.pack_end(&end);

        Self {
            header,
            title,
            insert_button,
            objects,
        }
    }
}

/// The main menu (NGNM-10): what belongs to the file and to the app.
/// Opening and saving come with the spreadsheet files.
pub(super) fn main_menu(info: AppInfo) -> gio::Menu {
    let menu = gio::Menu::new();
    let file = gio::Menu::new();
    file.append(Some(&tr("_New Document")), Some("app.new-document"));
    file.append(Some(&tr("_Duplicate")), Some("win.duplicate"));
    menu.append_section(None, &file);
    // View, which was the menu of the Sidebar button in interface B.
    let shown = gio::Menu::new();
    shown.append_submenu(Some(&tr("_View")), &view_menu());
    menu.append_section(None, &shown);
    let commands = gio::Menu::new();
    commands.append(Some(&tr("Search _Commands…")), Some("win.command-search"));
    menu.append_section(None, &commands);
    let help = gio::Menu::new();
    help.append(Some(&tr("_Keyboard Shortcuts")), Some("win.shortcuts"));
    help.append(
        Some(&tr("_About {app}").replace("{app}", info.name)),
        Some("app.about"),
    );
    menu.append_section(None, &help);
    menu
}

/// The commands that are in no menu, which the command search still finds
/// by name.
pub(super) fn more_commands() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("_New Window")), Some("app.new-window"));
    menu.append(Some(&tr("_Undo")), Some("win.undo"));
    menu.append(Some(&tr("_Redo")), Some("win.redo"));
    menu.append(Some(&tr("_Deselect All")), Some("win.deselect-all"));
    menu.append(Some(&tr("_Previous Sheet")), Some("win.previous-sheet"));
    menu.append(Some(&tr("_Next Sheet")), Some("win.next-sheet"));
    menu.append(Some(&tr("C_lose")), Some("win.close"));
    menu.append(Some(&tr("Close _Window")), Some("window.close"));
    menu.append(Some(&tr("Close _All Windows")), Some("app.close-all"));
    menu
}

/// The View menu, a submenu of the main menu (NGNM-10): the
/// pages of the sidebar, the bars the window shows, and the zoom.
pub(super) fn view_menu() -> gio::Menu {
    let panes = gio::Menu::new();
    panes.append(Some(&tr("S_heets")), Some("win.sheets"));
    panes.append(Some(&tr("_Table Styles")), Some("win.table-styles"));
    panes.append(Some(&tr("_Comments")), Some("win.comments"));
    panes.append(Some(&tr("Search _Results")), Some("win.search"));
    panes.append(Some(&tr("Formula _List")), Some("win.formula-list"));
    let bars = gio::Menu::new();
    bars.append(Some(&tr("_Format Bar")), Some("win.format-bar"));
    bars.append(Some(&tr("For_mula Bar")), Some("win.formula-bar"));
    let window = gio::Menu::new();
    window.append(Some(&tr("_Fullscreen")), Some("win.fullscreen"));
    window.append_submenu(Some(&tr("_Zoom")), &zoom_menu());
    let menu = gio::Menu::new();
    menu.append_section(Some(&tr("Sidebar")), &panes);
    menu.append_section(None, &bars);
    menu.append_section(None, &window);
    menu
}

/// The zoom levels, at the bottom and in the View menu (NINT-04).
pub(super) fn zoom_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    for level in crate::geometry::ZOOM_LEVELS {
        let item = gio::MenuItem::new(Some(&crate::geometry::zoom_label(level)), None);
        item.set_action_and_target_value(Some("win.zoom"), Some(&level.to_string().to_variant()));
        menu.append_item(&item);
    }
    menu
}

/// The Insert menu (NGNM-10): with `objects`, the objects too, as a window
/// narrower than 720 px shows them; then the sheet, and what goes in a
/// table.
pub(super) fn insert_menu(objects: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if objects {
        let items = gio::Menu::new();
        items.append(Some(&tr("_Table")), Some("win.insert-table"));
        items.append(Some(&tr("C_hart")), Some("win.insert-chart"));
        items.append(Some(&tr("Te_xt Box")), Some("win.insert-text-box"));
        items.append(Some(&tr("S_hape")), Some("win.insert-shape"));
        items.append(
            Some(&tr("_Image, Movie or Sound…")),
            Some("win.insert-picture"),
        );
        items.append(Some(&tr("Co_mment")), Some("win.insert-comment"));
        menu.append_section(None, &items);
    }
    let sheet = gio::Menu::new();
    sheet.append(Some(&tr("_Sheet")), Some("win.add-sheet"));
    menu.append_section(None, &sheet);
    let table = gio::Menu::new();
    table.append(Some(&tr("Row _Above")), Some("win.add-row-above"));
    table.append(Some(&tr("Row _Below")), Some("win.add-row-below"));
    table.append(Some(&tr("Column B_efore")), Some("win.add-column-before"));
    table.append(Some(&tr("Column A_fter")), Some("win.add-column-after"));
    menu.append_section(None, &table);
    let other = gio::Menu::new();
    other.append_submenu(Some(&tr("_Function")), &super::bars::function_menu());
    menu.append_section(None, &other);
    menu
}

/// The menu of the Add button of the sidebar: a new sheet, or a copy of
/// the one shown (NTAB-02).
pub(super) fn add_sheet_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some(&tr("_Blank Sheet")), Some("win.add-sheet"));
    menu.append(Some(&tr("_Duplicate Sheet")), Some("win.duplicate-sheet"));
    menu
}

/// The menu of a sheet in the sidebar (NINT-16, NTAB-02, NTAB-03).
pub(super) fn sheet_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let edit = gio::Menu::new();
    edit.append(Some(&tr("Re_name…")), Some("win.rename-sheet"));
    edit.append(Some(&tr("_Duplicate")), Some("win.duplicate-sheet"));
    menu.append_section(None, &edit);
    let order = gio::Menu::new();
    order.append(Some(&tr("Move _Up")), Some("win.move-sheet('up')"));
    order.append(Some(&tr("Move Do_wn")), Some("win.move-sheet('down')"));
    menu.append_section(None, &order);
    let delete = gio::Menu::new();
    delete.append(Some(&tr("_Delete")), Some("win.delete-sheet"));
    menu.append_section(None, &delete);
    menu
}

/// The menu of a table or a chart in the sidebar (NINT-16, NTAB-03).
pub(super) fn item_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let order = gio::Menu::new();
    order.append(Some(&tr("Move _Up")), Some("win.move-item('up')"));
    order.append(Some(&tr("Move Do_wn")), Some("win.move-item('down')"));
    menu.append_section(None, &order);
    let sheet = gio::Menu::new();
    sheet.append(
        Some(&tr("Move to the _Previous Sheet")),
        Some("win.move-item('previous-sheet')"),
    );
    sheet.append(
        Some(&tr("Move to the _Next Sheet")),
        Some("win.move-item('next-sheet')"),
    );
    menu.append_section(None, &sheet);
    let delete = gio::Menu::new();
    delete.append(Some(&tr("_Delete")), Some("win.delete-item"));
    menu.append_section(None, &delete);
    menu
}

/// The context menu of a table on the canvas (NINT-16): undo, the rows
/// and columns, and the table itself.
pub(super) fn table_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let history = gio::Menu::new();
    history.append(Some(&tr("_Undo")), Some("win.undo"));
    history.append(Some(&tr("_Redo")), Some("win.redo"));
    menu.append_section(None, &history);
    let rows = gio::Menu::new();
    rows.append(Some(&tr("Add Row _Above")), Some("win.add-row-above"));
    rows.append(Some(&tr("Add Row _Below")), Some("win.add-row-below"));
    rows.append(
        Some(&tr("Add Column B_efore")),
        Some("win.add-column-before"),
    );
    rows.append(Some(&tr("Add Column A_fter")), Some("win.add-column-after"));
    menu.append_section(None, &rows);
    let table = gio::Menu::new();
    table.append(Some(&tr("Show Table _Name")), Some("win.show-table-name"));
    table.append(
        Some(&tr("Show in _Inspector")),
        Some("win.show-in-inspector"),
    );
    table.append(Some(&tr("_Delete Table")), Some("win.delete-item"));
    menu.append_section(None, &table);
    menu
}

/// Whether a monitor of this size, in logical pixels, is smaller than the
/// 1280 × 720 that Desk asks for (NGNM-14).
pub(super) fn is_small_screen(width: i32, height: i32) -> bool {
    width < 1280 || height < 720
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ngnm_14_a_screen_below_720p_is_small() {
        assert!(!is_small_screen(1280, 720));
        assert!(is_small_screen(1024, 600));
        assert!(is_small_screen(800, 1280));
    }
}
