# Goals of Desk

*Set on 2026-11-16. This page replaces the goal "feature parity with
iWork '09" that the project had until then. Each rule points to its entry
in [decisions.md](decisions.md).*

Desk is a free office suite for GNOME: Desk Writer, Desk Tables and Desk
Slides. Its target is **feature parity with the current iWork**: Pages,
Numbers and Keynote 15.4, the versions checked on a Mac on 2026-11-16.

## The rules of the target

1. **A feature that both iWork '09 and the current iWork have** behaves as
   in the current iWork (D-001).
2. **A feature that only iWork '09 has** is kept, with its behaviour, its
   presentation and its use adapted to how the current iWork does things.
   This includes features without interface, such as charts and tables
   inserted from another document that update when the source changes
   (D-002).
3. **What the current iWork lost inside a shared feature, or does only
   with AI,** is kept as in rule 2: wrap direction, split cells, the 3D
   chart scene with light, tracking of format changes, Instant Alpha, the
   Proofreader (D-003).
4. **Features that need AI, an online service or a payment are out of
   scope but not discarded.** They stay in the parity matrices with their
   mark and reason, and the architecture must not rule them out (D-004).
5. **Features tied to Apple platforms** (Touch ID, Handoff, AirDrop,
   Shortcuts, Apple Books) are "not applicable to GNOME"; where GNOME has
   an equivalent, the equivalent is a row of the matrix (D-005).
6. **A new release of iWork** is a review of the inventory, not a change
   of target in the middle of a phase (D-006).

## Interface

The window follows the look of the current iWork with native Libadwaita
widgets wherever one exists. Where none exists, Desk builds one that
follows the GNOME HIG and marks it for review at each new Libadwaita
(D-007). The window is proposal M+ of
[interface/propostas](interface/propostas/README.md) (D-008).

## Documents and compatibility

- The native format is OpenDocument (ODF), with documented extensions for
  every implemented feature that standard ODF cannot hold (D-009).
- Desk reads and imports five families of files (D-010):

| Family | Formats | Today |
| --- | --- | --- |
| Desk | ODT, ODS, ODP with the Desk extensions | ODT read and written; ODS and ODP not yet |
| iWork '09 | `.pages`, `.numbers`, `.key` of iWork 9 | Pages only |
| Current iWork | `.pages`, `.numbers`, `.key` of iWork 15 | None |
| Microsoft Office, current | `.docx`, `.xlsx`, `.pptx` | None |
| LibreOffice and OpenOffice, current | ODF as they write it | ODT only |

- Desk exports to four targets only (D-019): ODF as LibreOffice and
  OpenOffice read it, Microsoft Office (`.docx`, `.xlsx`, `.pptx`), PDF and
  EPUB. It does not write iWork files.
- The other exports of iWork are out of scope by that decision and stay
  marked in the matrices: RTF, plain text, CSV and TSV, images, movie,
  animated GIF, HTML, and the older Office formats (`.doc`, `.xls`,
  `.ppt`).
- The reader of current iWork files follows the clean-room rules of D-011.

## Verification

Layout and behaviour are checked against iWork 15.4 on a Mac. The cases
made with Pages '09 on the Lion virtual machine are redone there, and they
remain only as tests of the reader of iWork '09 files (D-012).

## Technology and distribution

Unchanged (D-013): Rust, GTK 4.24, Libadwaita 1.10, the GNOME 51 runtime,
Flatpak as the main distribution, a minimum screen of 1280 × 720. Building
outside Flatpak needs GNOME 51 libraries (Fedora 45, Ubuntu 26.10, Arch
Linux).

## Documentation and code

The project is documented for outside contributors, human or agent:
architecture, definitions, roadmap and diagrams. Every source file is
commented so that a person understands how it works by reading it, with a
pointer to these documents where more context is needed (D-014).

## What is not done

Whatever a set of tasks leaves undone is recorded with its reason in
[backlog](backlog/README.md), and the backlog of a phase is worked at the
end of that phase (D-015).
