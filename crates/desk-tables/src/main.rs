// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Desk Tables, the spreadsheet app of Desk.
//!
//! Desk Tables aims at feature parity and document compatibility with
//! Numbers '09, and saves documents as ODF spreadsheets.

use desk_ui::AppInfo;
use desk_ui::gtk::glib;

const APP: AppInfo = AppInfo {
    id: "io.github.brahm.Desk.Tables",
    name: "Desk Tables",
    version: env!("CARGO_PKG_VERSION"),
    icon_name: "x-office-spreadsheet-symbolic",
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
