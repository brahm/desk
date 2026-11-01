// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desk Slides, the presentation app of Desk.
//!
//! Desk Slides aims at feature parity and document compatibility with
//! Keynote '09, and saves documents as ODF presentations.

use desk_ui::AppInfo;
use desk_ui::gtk::glib;

const APP: AppInfo = AppInfo {
    id: "br.com.brahm.Desk.Slides",
    name: "Desk Slides",
    version: env!("CARGO_PKG_VERSION"),
};

fn main() -> glib::ExitCode {
    desk_ui::run(APP, None)
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
