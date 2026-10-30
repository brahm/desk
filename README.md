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

The project is in early development. The repository has the layout of
the planned code, and each app only opens an empty window so far.

## Building

Desk needs Rust 1.92 or later, and the development files of GTK 4.14
and Libadwaita 1.5 or later. On Debian and Ubuntu:

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev
```

On Fedora:

```sh
sudo dnf install gtk4-devel libadwaita-devel
```

Then run an app with Cargo:

```sh
cargo run --package desk-writer
cargo run --package desk-tables
cargo run --package desk-slides
```

## Repository layout

The code is one Cargo workspace, with one crate per directory in
[crates](crates):

| Crate | Purpose |
|---|---|
| `desk-writer`, `desk-tables`, `desk-slides` | The three apps |
| `desk-ui` | Interface shared by the apps, and the only library that uses GTK |
| `desk-model` | Document model shared by the apps |
| `desk-text` | Text shaping and paragraph layout |
| `desk-layout` | Pagination and composition of pages, slides and sheets |
| `desk-render` | Display list, effects and the Cairo back end |
| `desk-table` | Table model shared by the apps |
| `desk-formula` | Formula parser, dependency graph and functions |
| `desk-chart` | Chart model and drawing |
| `desk-anim` | Slide transitions and builds |
| `desk-contacts` | Contacts for merge fields |
| `desk-odf` | OpenDocument package reader and writer, with the Desk extensions |
| `desk-iwork` | Reader for iWork '09 and '08 documents |
| `desk-filters` | Import and export filters |
| `desk-convert` | Command-line converter that runs without GTK |
| `desk-testkit` | Reference corpus, visual comparison and parity reports for the tests |

The CI checks formatting, Clippy warnings, tests and documentation, the
minimum Rust version, the licenses and advisories of dependencies (with
cargo-deny) and REUSE compliance. It also checks two dependency rules:
the MPL-2.0 libraries use no other Desk crate, and only `desk-ui` and the
apps use GTK.

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
