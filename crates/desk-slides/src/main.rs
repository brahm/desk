// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desk Slides, the presentation app of Desk.
//!
//! Desk Slides aims at feature parity and document compatibility with
//! Keynote '09, and saves documents as ODF presentations.

use desk_ui::AppInfo;
use desk_ui::gtk::glib;

const APP: AppInfo = AppInfo {
    id: "io.github.brahm.Desk.Slides",
    name: "Desk Slides",
    version: env!("CARGO_PKG_VERSION"),
    icon_name: "x-office-presentation-symbolic",
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
}
