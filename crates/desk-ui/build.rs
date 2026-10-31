// SPDX-FileCopyrightText: 2026 Desk contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Builds the app icons in data/icons into a GResource bundle, so that the
//! apps show their own icons even when they run from the build tree.

fn main() {
    glib_build_tools::compile_resources(
        &["../../data/icons/hicolor"],
        "resources/icons.gresource.xml",
        "icons.gresource",
    );
}
