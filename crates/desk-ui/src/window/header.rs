// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The header bar of window M+ (INT-11, INT-14, GNM-10): at the start,
//! the Sidebar button, the Insert menu and the buttons of the objects; at
//! the end, Open Documents, Find, the Inspector and the main menu, which
//! holds View.

use adw::prelude::*;
use gtk::gio;

use crate::AppInfo;
use crate::formatting;
use crate::i18n::tr;

/// The parts of the header bar that the window changes later.
pub(super) struct Header {
    pub header: adw::HeaderBar,
    pub title: adw::WindowTitle,
    /// What shows only with a document: the start and the end buttons.
    pub document_buttons: [gtk::Widget; 2],
    /// Insert, whose menu takes the objects in a narrow window.
    pub insert_button: gtk::MenuButton,
    /// The buttons of the objects, which a narrow window leaves out.
    pub objects: gtk::Box,
    /// Media, whose popover holds the Media Browser (INT-14).
    pub media_button: gtk::MenuButton,
    /// Table, whose popover shows the table styles (PXV-14).
    pub table_button: gtk::MenuButton,
    /// Chart, whose popover shows the chart styles (W080).
    pub chart_button: gtk::MenuButton,
    /// Find, in step with the find bar.
    pub find_toggle: gtk::ToggleButton,
    /// The main menu, whose View submenu holds Go to Page.
    pub menu_button: gtk::MenuButton,
}

impl Header {
    pub fn new(info: AppInfo, opens: bool, navigation: &gio::Menu) -> Self {
        let title = adw::WindowTitle::new(info.name, "");
        let slides = crate::is_slides(info);
        let (view, insert) = if slides {
            (
                super::slides::view_menu(navigation),
                super::slides::insert_menu(false),
            )
        } else {
            (view_menu(navigation), insert_menu(false))
        };
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
            .menu_model(&insert)
            .tooltip_text(tr("Insert"))
            .build();
        let objects = gtk::Box::builder().spacing(6).build();
        let media_button = gtk::MenuButton::builder()
            .icon_name("desk-media-symbolic")
            .tooltip_text(tr("Media"))
            .build();
        formatting::labelled(&media_button, &tr("Media"));
        let table_button = gtk::MenuButton::builder()
            .icon_name("desk-table-symbolic")
            .tooltip_text(tr("Table"))
            .build();
        formatting::labelled(&table_button, &tr("Table"));
        objects.append(&table_button);
        let chart_button = gtk::MenuButton::builder()
            .icon_name("desk-chart-symbolic")
            .tooltip_text(tr("Chart"))
            .build();
        formatting::labelled(&chart_button, &tr("Chart"));
        objects.append(&chart_button);
        objects.append(&object_button(
            "desk-textbox-symbolic",
            &tr("Text Box"),
            "win.insert-text-box",
        ));
        let shape_button = gtk::MenuButton::builder()
            .icon_name("desk-shape-symbolic")
            .menu_model(&shape_menu())
            .tooltip_text(tr("Shape"))
            .build();
        formatting::labelled(&shape_button, &tr("Shape"));
        objects.append(&shape_button);
        objects.append(&media_button);
        objects.append(&object_button(
            "desk-comment-symbolic",
            &tr("Comment"),
            "win.insert-comment",
        ));
        let start = gtk::Box::builder().spacing(6).visible(false).build();
        start.append(&sidebar_button);
        start.append(&insert_button);
        start.append(&objects);
        header.pack_start(&start);

        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&main_menu(info, opens, &view))
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
        let find_toggle = gtk::ToggleButton::builder()
            .icon_name("edit-find-symbolic")
            .tooltip_text(tr("Find"))
            .build();
        formatting::labelled(&find_toggle, &tr("Find"));
        // Open Documents shows the open documents as a grid
        // (AdwTabOverview), to switch between them.
        let overview_button = gtk::Button::builder()
            .icon_name("view-grid-symbolic")
            .action_name("win.tab-overview")
            .tooltip_text(tr("Open Documents"))
            .build();
        formatting::labelled(&overview_button, &tr("Open Documents"));
        let end = gtk::Box::builder().spacing(6).visible(false).build();
        end.append(&overview_button);
        if slides {
            // Play, before Find (KINT-05, KINT-12).
            end.append(&super::slides::play_button());
        }
        end.append(&find_toggle);
        end.append(&inspector_toggle);
        header.pack_end(&end);

        Self {
            header,
            title,
            document_buttons: [start.upcast(), end.upcast()],
            insert_button,
            objects,
            media_button,
            table_button,
            chart_button,
            find_toggle,
            menu_button,
        }
    }
}

/// A button that inserts an object.
fn object_button(icon: &str, label: &str, action: &str) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .action_name(action)
        .tooltip_text(label)
        .build();
    formatting::labelled(&button, label);
    button
}

/// The main menu: what belongs to the file and to the app (GNM-10), and
/// `view`, the View menu, as a submenu. What the menu bar of iWork had and
/// is not here, the command search finds. Save is not in it: a document
/// saves itself, and its versions are browsed from here (D-001, DOC-11);
/// Ctrl+S keeps a version.
pub(super) fn main_menu(info: AppInfo, opens: bool, view: &gio::Menu) -> gio::Menu {
    let menu = gio::Menu::new();
    let file = gio::Menu::new();
    if opens {
        file.append(Some(&tr("_New Document")), Some("app.new-document"));
        file.append(Some(&tr("_Open…")), Some("app.open"));
        file.append_submenu(Some(&tr("Open _Recent")), &recent_menu());
    } else {
        file.append(Some(&tr("_New Window")), Some("app.new-window"));
    }
    menu.append_section(None, &file);
    if opens {
        let save = gio::Menu::new();
        save.append(Some(&tr("Save _As…")), Some("win.save-as"));
        save.append(Some(&tr("_Browse Versions…")), Some("win.versions"));
        save.append(
            Some(&tr("_Language and Region…")),
            Some("win.language-region"),
        );
        menu.append_section(None, &save);
    }
    let shown = gio::Menu::new();
    shown.append_submenu(Some(&tr("_View")), view);
    menu.append_section(None, &shown);
    let commands = gio::Menu::new();
    commands.append(Some(&tr("Search _Commands…")), Some("win.command-search"));
    menu.append_section(None, &commands);
    let help = gio::Menu::new();
    help.append(Some(&tr("_Preferences")), Some("app.preferences"));
    help.append(Some(&tr("_Keyboard Shortcuts")), Some("win.shortcuts"));
    help.append(
        Some(&tr("_About {app}").replace("{app}", info.name)),
        Some("app.about"),
    );
    menu.append_section(None, &help);
    menu
}

/// The commands that left the main menu, which the command search still
/// finds by name: they have shortcuts, or belonged to the File menu of
/// iWork.
pub(super) fn more_commands(opens: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if opens {
        menu.append(Some(&tr("_New Window")), Some("app.new-window"));
        menu.append(Some(&tr("_Save")), Some("win.save"));
        menu.append(Some(&tr("Save a _Copy…")), Some("win.save-copy"));
        menu.append(Some(&tr("_Duplicate")), Some("win.duplicate"));
        menu.append(Some(&tr("Re_name…")), Some("win.rename"));
        menu.append(Some(&tr("Revert to _Last Opened")), Some("win.revert"));
        menu.append(Some(&tr("Document _Info")), Some("win.properties"));
        menu.append(Some(&tr("_Import Styles…")), Some("win.import-styles"));
    }
    menu.append(Some(&tr("C_lose")), Some("win.close"));
    menu.append(Some(&tr("Close _Window")), Some("window.close"));
    menu.append(Some(&tr("Close _All Windows")), Some("app.close-all"));
    menu
}

/// The documents opened last, newest first, for Open Recent. The menu is
/// made again each time the main menu opens.
pub(super) fn recent_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    fill_recent(&menu);
    menu
}

/// The number of documents that Open Recent lists.
const RECENT: usize = 10;

/// Puts the recent documents of Desk in `menu`.
pub(super) fn fill_recent(menu: &gio::Menu) {
    menu.remove_all();
    let mut items: Vec<_> = gtk::RecentManager::default()
        .items()
        .into_iter()
        .filter(|item| {
            item.exists()
                && item
                    .mime_type()
                    .as_str()
                    .starts_with("application/vnd.oasis.opendocument.text")
                    | item.uri().ends_with(".pages")
        })
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.modified().to_unix()));
    for item in items.into_iter().take(RECENT) {
        let entry = gio::MenuItem::new(Some(&item.display_name()), None);
        entry.set_action_and_target_value(
            Some("app.open-recent"),
            Some(&item.uri().as_str().to_variant()),
        );
        menu.append_item(&entry);
    }
}

/// The views of the markup of tracked changes (REV-04).
pub(crate) fn markup_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    for markup in desk_model::Markup::ALL {
        let label = match markup {
            desk_model::Markup::Markup => tr("_Markup"),
            desk_model::Markup::WithoutDeletions => tr("Markup _Without Deletions"),
            desk_model::Markup::Final => tr("_Final"),
        };
        menu.append(
            Some(&label),
            Some(&format!("win.markup('{}')", markup.key())),
        );
    }
    menu
}

/// The View menu, a submenu of the main menu (GNM-10): the pages of the
/// sidebar, what the window shows, the zoom, and `navigation`, Go to Page
/// and the places to go to (INT-19).
pub(super) fn view_menu(navigation: &gio::Menu) -> gio::Menu {
    let panes = gio::Menu::new();
    panes.append(Some(&tr("Page _Thumbnails")), Some("win.thumbnails"));
    panes.append(Some(&tr("Table of C_ontents")), Some("win.contents"));
    panes.append(Some(&tr("_Styles")), Some("win.styles-drawer"));
    panes.append(Some(&tr("_Comments")), Some("win.comments"));
    panes.append(Some(&tr("Search _Results")), Some("win.search"));
    panes.append(Some(&tr("Spe_lling and Grammar")), Some("win.spelling"));
    let document = gio::Menu::new();
    document.append(Some(&tr("_Ruler")), Some("win.ruler"));
    document.append(Some(&tr("_Layout")), Some("win.show-layout"));
    document.append(Some(&tr("_Invisibles")), Some("win.invisibles"));
    // Show Warnings, as the View menu of Pages 15.4 has it (W009).
    document.append(Some(&tr("Document _Warnings")), Some("win.warnings"));
    // Author Color, as View > Comments & Changes of Pages 15.4 (W150).
    let colors = gio::Menu::new();
    for color in desk_model::AuthorColor::ALL {
        colors.append(
            Some(&author_color_name(color)),
            Some(&format!("win.author-color('{}')", color.key())),
        );
    }
    document.append_submenu(Some(&tr("_Author Color")), &colors);
    // How tracked changes show, as the review toolbar of Pages 15.4
    // (REV-04).
    document.append_submenu(Some(&tr("Change _Markup")), &markup_menu());
    let window = gio::Menu::new();
    window.append(Some(&tr("As _Outline")), Some("win.outline"));
    // Screen View, and the size of its text (PXV-01).
    window.append(Some(&tr("Scr_een View")), Some("win.screen-view"));
    let text_size = gio::Menu::new();
    text_size.append(Some(&tr("_Auto")), Some("win.text-size('auto')"));
    text_size.append(Some(&tr("_Bigger")), Some("win.text-size('bigger')"));
    text_size.append(Some(&tr("_Smaller")), Some("win.text-size('smaller')"));
    window.append_submenu(Some(&tr("Te_xt Size")), &text_size);
    window.append(Some(&tr("_Fullscreen")), Some("win.fullscreen"));
    window.append_submenu(
        Some(&tr("_Zoom")),
        &crate::document_controls::zoom_menu(false),
    );
    let menu = gio::Menu::new();
    menu.append_section(Some(&tr("Sidebar")), &panes);
    menu.append_section(None, &document);
    menu.append_section(None, &window);
    menu.append_section(None, navigation);
    menu
}

/// The name of an author colour, as the menu shows it (W150).
pub(crate) fn author_color_name(color: desk_model::AuthorColor) -> String {
    use desk_model::AuthorColor::*;
    match color {
        Yellow => tr("Yellow"),
        Green => tr("Green"),
        Blue => tr("Blue"),
        Pink => tr("Pink"),
        Purple => tr("Purple"),
        Teal => tr("Teal"),
        Orange => tr("Orange"),
        Red => tr("Red"),
        Brown => tr("Brown"),
        GrayBlue => tr("Gray Blue"),
    }
}

/// The Insert menu (GNM-10): what the Insert menu of Pages had besides the
/// objects, which have buttons of their own: breaks, fields, notes,
/// tables of contents, links and bookmarks (EST-04, EST-08, EST-09,
/// EST-13, EST-15, EST-16, EST-22, EST-25, EST-26, EST-27) and comments
/// (W149); with `objects`, the objects too, as a window narrower than
/// 720 px shows them.
pub(super) fn insert_menu(objects: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    if objects {
        let items = gio::Menu::new();
        items.append(Some(&tr("_Table")), Some("win.insert-table"));
        items.append_submenu(Some(&tr("C_hart")), &super::tables::chart_menu());
        items.append(Some(&tr("Te_xt Box")), Some("win.insert-text-box"));
        items.append_submenu(Some(&tr("S_hape")), &shape_menu());
        items.append(
            Some(&tr("_Image, Movie or Sound…")),
            Some("win.insert-picture"),
        );
        menu.append_section(None, &items);
    }
    let breaks = gio::Menu::new();
    breaks.append(Some(&tr("_Page Break")), Some("win.insert-page-break"));
    breaks.append(Some(&tr("_Column Break")), Some("win.insert-column-break"));
    breaks.append(Some(&tr("_Layout Break")), Some("win.insert-layout-break"));
    breaks.append(
        Some(&tr("_Section Break")),
        Some("win.insert-section-break"),
    );
    breaks.append(Some(&tr("_Captured Pages…")), Some("win.captured-pages"));
    let fields = gio::Menu::new();
    fields.append(
        Some(&tr("_Auto Page Numbers…")),
        Some("win.insert-auto-page-numbers"),
    );
    fields.append(Some(&tr("Page _Number")), Some("win.insert-page-number"));
    fields.append(Some(&tr("Page Co_unt")), Some("win.insert-page-count"));
    fields.append(Some(&tr("_Date")), Some("win.insert-date"));
    fields.append(Some(&tr("_File Name")), Some("win.insert-file-name"));
    let inserts = gio::Menu::new();
    inserts.append_submenu(Some(&tr("_Break")), &breaks);
    inserts.append_submenu(Some(&tr("_Field")), &fields);
    menu.append_section(None, &inserts);
    let notes = gio::Menu::new();
    notes.append(Some(&tr("Foot_note")), Some("win.insert-note"));
    notes.append(Some(&tr("Ne_xt Note")), Some("win.next-note"));
    notes.append(Some(&tr("Pre_vious Note")), Some("win.previous-note"));
    menu.append_section(None, &notes);
    let comments = gio::Menu::new();
    comments.append(Some(&tr("Co_mment")), Some("win.insert-comment"));
    comments.append(Some(&tr("_Highlight")), Some("win.insert-highlight"));
    comments.append(Some(&tr("Next Comment")), Some("win.next-comment"));
    comments.append(Some(&tr("Previous Comment")), Some("win.previous-comment"));
    menu.append_section(None, &comments);
    let structure = gio::Menu::new();
    // What the table lists, as in Pages 15.4 (W100).
    let tocs = gio::Menu::new();
    for (scope, name) in [
        ("document", tr("_Document")),
        ("section", tr("_Section")),
        ("next", tr("To _Next Occurrence")),
    ] {
        tocs.append(Some(&name), Some(&format!("win.insert-toc-in::{scope}")));
    }
    structure.append_submenu(Some(&tr("_Table of Contents")), &tocs);
    structure.append(
        Some(&tr("Update Table of C_ontents")),
        Some("win.update-toc"),
    );
    structure.append(Some(&tr("_Bookmark")), Some("win.insert-bookmark"));
    // Edit > Manage Bookmark Suggestions of Pages 15.4, in the Bookmarks
    // tab of the Document pane (W101).
    structure.append(
        Some(&tr("Manage Bookmark _Suggestions")),
        Some("win.inspector-tab::bookmarks"),
    );
    structure.append(Some(&tr("_Hyperlink")), Some("win.insert-link"));
    menu.append_section(None, &structure);
    let other = gio::Menu::new();
    other.append(
        Some(&tr("Special _Characters…")),
        Some("win.special-characters"),
    );
    other.append(
        Some(&tr("Add _Linked Text Box")),
        Some("win.add-linked-text-box"),
    );
    other.append(Some(&tr("C_hoose…")), Some("win.insert-picture"));
    menu.append_section(None, &other);
    menu
}

/// The menu of the Shape button: the shapes, the pen and the connection
/// line (OBJ-08).
pub(super) fn shape_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    let shapes = gio::Menu::new();
    for (label, value) in super::objects::shape_names()
        .into_iter()
        .chain([(tr("_Line"), "line")])
    {
        shapes.append(Some(&label), Some(&format!("win.insert-shape('{value}')")));
    }
    menu.append_section(None, &shapes);
    let drawn = gio::Menu::new();
    drawn.append(Some(&tr("Pe_n")), Some("win.draw-with-pen"));
    drawn.append(Some(&tr("Connection L_ine")), Some("win.insert-connection"));
    menu.append_section(None, &drawn);
    menu
}

/// The popover of the Media button: the Media Browser, and Choose File
/// for what is elsewhere (INT-14). The browser goes in when the popover
/// first opens.
pub(super) fn media_popover() -> (gtk::Popover, gtk::Box) {
    let holder = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .width_request(360)
        .height_request(420)
        .build();
    let choose = gtk::Button::builder()
        .label(tr("Choose _File…"))
        .use_underline(true)
        .action_name("win.insert-picture")
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    content.append(&holder);
    content.append(&choose);
    let popover = gtk::Popover::builder().child(&content).build();
    // Choosing a file closes the popover, as the file dialog takes over.
    let weak = popover.downgrade();
    choose.connect_clicked(move |_| {
        if let Some(popover) = weak.upgrade() {
            popover.popdown();
        }
    });
    (popover, holder)
}

/// Whether a monitor of this size, in logical pixels, is smaller than the
/// 1280 × 720 that Desk asks for (GNM-14).
pub(super) fn is_small_screen(width: i32, height: i32) -> bool {
    width < MIN_SCREEN.0 || height < MIN_SCREEN.1
}

/// The smallest screen Desk supports, in logical pixels.
const MIN_SCREEN: (i32, i32) = (1280, 720);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnm_14_a_screen_below_720p_is_small() {
        assert!(!is_small_screen(1280, 720));
        assert!(!is_small_screen(2560, 1440));
        assert!(is_small_screen(1024, 600));
        assert!(is_small_screen(1279, 800));
        // A tablet standing up.
        assert!(is_small_screen(800, 1280));
        // 1920 × 1080 at 200 %.
        assert!(is_small_screen(960, 540));
    }
}
