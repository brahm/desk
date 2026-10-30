# Desk

Desk is a free office suite for the Linux desktop, made of three apps
that share one document model:

- **Desk Writer**, for word processing and page layout
- **Desk Tables**, for spreadsheets
- **Desk Slides**, for presentations

The apps are written in Rust with GTK 4 and Libadwaita and follow the
GNOME Human Interface Guidelines. They aim for feature parity and
document compatibility with Apple iWork '09 (Pages, Numbers and Keynote
9.3). Documents are saved as OpenDocument (ODF) files, with documented
extensions for what standard ODF cannot hold.

The project is at the planning stage, and there is no code yet.

## License

Desk uses two licenses:

- The applications are licensed under the GNU General Public License,
  version 3 or (at your option) any later version (GPL-3.0-or-later).
  See [LICENSE](LICENSE).
- The file format libraries (`desk-odf`, `desk-iwork` and
  `desk-formula`) are licensed under the Mozilla Public License 2.0
  (MPL-2.0), so that other programs can reuse them to read and write
  the same formats.

The full license texts are in [LICENSES](LICENSES). The repository
follows the [REUSE](https://reuse.software/) specification: source files
carry SPDX headers, and [REUSE.toml](REUSE.toml) covers files that
cannot.

Desk is not affiliated with or endorsed by Apple Inc. iWork, Pages,
Numbers and Keynote are trademarks of Apple Inc.
