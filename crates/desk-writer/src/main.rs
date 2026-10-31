// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desk Writer, the word processing and page layout app of Desk.
//!
//! Desk Writer aims at feature parity and document compatibility with Pages
//! '09, and saves documents as ODF text documents.

use desk_ui::AppInfo;
use desk_ui::gtk::glib;

const APP: AppInfo = AppInfo {
    id: "br.com.brahm.Desk.Writer",
    name: "Desk Writer",
    version: env!("CARGO_PKG_VERSION"),
};

fn main() -> glib::ExitCode {
    desk_ui::run(APP)
}

#[cfg(test)]
mod tests {
    use desk_ui::gtk::gio;

    #[test]
    fn app_id_is_valid() {
        assert!(gio::Application::id_is_valid(super::APP.id));
    }

    #[test]
    fn app_has_its_icons() {
        assert!(desk_ui::has_icon(super::APP.id));
        assert!(desk_ui::has_icon(&format!("{}-symbolic", super::APP.id)));
    }
}
