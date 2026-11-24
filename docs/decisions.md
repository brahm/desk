# Decision log

One row per decision, newest target first. A decision is never erased: when
another replaces it, its status becomes "Replaced by D-nnn". The owner of
the project takes the decisions; the date is the day he took them.

## Current

| Id | Date | Decision | Status |
| --- | --- | --- | --- |
| D-001 | 2026-11-16 | The target is feature parity with Pages, Numbers and Keynote 15.4. Where iWork '09 and the current iWork share a feature, the behaviour of the current one is adopted. | Current |
| D-002 | 2026-11-16 | Features that only iWork '09 has are implemented, adapted to the behaviour and presentation of the current iWork, including features without interface (live update of charts and tables inserted from other documents). | Current |
| D-003 | 2026-11-16 | What the current iWork lost inside a shared feature, or does only with AI, is kept, in the form of the current iWork. | Current |
| D-004 | 2026-11-16 | Features that need AI, an online service or a payment are out of scope and are not discarded: they stay in the matrices, marked. | Current |
| D-005 | 2026-11-16 | Features tied to Apple platforms are "not applicable to GNOME" unless GNOME has an equivalent. | Current (assumed, not contested) |
| D-006 | 2026-11-16 | The target version is 15.4; a new release of iWork is a review of the inventory. | Current (assumed, not contested) |
| D-007 | 2026-11-16 | Native Libadwaita widgets wherever one exists; otherwise a widget of Desk that follows the HIG, marked for review at each new Libadwaita. | Current |
| D-008 | 2026-11-16 | The window is proposal M+ of `docs/interface/proposals`: the mix with the HIG, with the six object buttons (Table, Chart, Text, Shape, Media, Comment) in the header bar beside Insert, so that no function shown in the toolbar of iWork is hidden. | Current |
| D-009 | 2026-11-16 | ODF stays the native format; every implemented feature that standard ODF cannot hold gets a documented extension. | Current |
| D-010 | 2026-11-16 | Desk reads and imports: Desk ODF, iWork '09, current iWork, current Microsoft Office, current LibreOffice and OpenOffice. | Current |
| D-011 | 2026-11-16 | The reader of current iWork files is permanent and is written under four rules: nothing is extracted from Apple's executables and no definition file taken from them is copied; it is written from public third-party documentation and from documents made by the project; no template, theme, font or image of Apple enters the repository; the names Pages, Numbers and Keynote are used only to say what Desk is compatible with. The project follows the practice of LibreOffice, whose import filter for these files (libetonyek, MPL 2.0) was written from the file format and has been distributed for years; that filter may be used as a reference under its licence. No legal review is required before version 1.0 (decided by the owner on 2026-11-16; backlog item B-004 dropped). | Current |
| D-013 | 2026-11-16 | Technology and distribution are unchanged: Rust, GTK 4, Libadwaita, Flatpak first, GNOME 51 as the minimum. | Current |
| D-014 | 2026-11-16 | Documentation for outside contributors and their agents, diagrams of the design, and source files commented to be understood on their own. | Current |
| D-015 | 2026-11-16 | A register of what each set of tasks left undone, with the reason, and a backlog worked at the end of each phase. | Current |
| D-016 | 2026-11-16 | Help uses the native GNOME shortcut. | Current |
| D-017 | 2026-11-16 | Shortcuts: GNOME's own first, then the shortcut of iWork 15.4. | Current (confirmed by the owner in D-021) |
| D-018 | 2026-11-16 | Everything in the repository is in English: documentation, code comments, matrices, inventories and mockups. The Brazilian Portuguese translation of the apps and of the site stays, as a translation. Older material still in Portuguese is listed in backlog item B-009. | Current |
| D-019 | 2026-11-16 | Export is limited to LibreOffice and OpenOffice (ODF), Microsoft Office, PDF and EPUB. The other exports of iWork (RTF, plain text, CSV and TSV, images, movie, animated GIF, HTML, the older Office formats) are out of scope and stay marked in the matrices. | Current |
| D-020 | 2026-11-16 | Checking has two references. Validations, interface tests and the documents made to compare parity are made on the Mac with the current iWork (15.4). Features that only iWork '09 has are tested, and their parity documents made and checked, on the Lion virtual machine that shows the screen, built for this purpose. | Current |
| D-022 | 2026-11-16 | Desk is made for users of the GNOME desktop, used to its workflow and its shortcuts. Where compatibility with GNOME and familiarity for those who come from iWork pull apart, GNOME wins. Parity with iWork is the target of the 1.0 versions; after them Desk grows new features of its own and an ever closer integration with the GNOME desktop and its applications. | Current |
| D-023 | 2026-11-16 | Under D-022, the document tabs take GNOME's shortcuts in the three apps: Ctrl+PageUp and Ctrl+PageDown (and Ctrl+Tab, Ctrl+Shift+Tab), Alt+1 to Alt+9, Ctrl+Shift+PageUp and Ctrl+Shift+PageDown to move a tab. The sheets of Tables, an iWork command that collides, move to Alt+PageUp and Alt+PageDown (rule 3 of D-017). Ctrl+Home and Ctrl+End stay with the text and the cells. | Current |
| D-024 | 2026-11-16 | Under D-022, Ctrl+T opens a new document in a tab, as in GNOME. Fonts, which had Ctrl+T as in iWork, has no shortcut of its own: it opens from the Font row of the Format pane and from Search Commands. | Current |
| D-025 | 2026-11-16 | The versions of a document that saves itself (DOC-11, D-001) are kept in the data folder of Desk (`~/.local/share/desk/versions`), outside the file, as macOS keeps those of iWork: the ODF file stays standard and the same size, and carries no past versions when it is sent. | Current |
| D-026 | 2026-11-17 | Notes in the code are information, not tasks. Every `TODO`, `FIXME`, `XXX` or `HACK` cites an open row of the backlog, as in `TODO(B-012)`, and is removed in the change that closes the row; work noticed outside the task becomes a backlog row and a line in the report, not part of the change. `ci/check-backlog.py` enforces it in `ci/check-all.sh` and in the CI (rules 5 and 6 of `docs/backlog/README.md`, rules 12 and 13 of `AGENTS.md`). | Current |
| D-027 | 2026-11-24 | Development pauses after pull request 200 for RV, a review of all the code written so far for bugs and optimizations, before going back to the planned phases. W2 is paused, not closed, and resumes where it stopped. The review runs by the plan the owner approved: six batches ordered by risk, a report of findings per batch, and a pull request per batch with a regression test for each fix. | Current |
| D-028 | 2026-11-24 | After batch 1 of RV, the format libraries (`desk-odf`, `desk-iwork`, `desk-formula`) deny the Clippy lints `indexing_slicing` and `unwrap_used`, so that a malformed file cannot bring back the panics the review removes. | Current |

## Replaced

| Id | Date | Decision | Status |
| --- | --- | --- | --- |
| R-001 | 2026-10-30 | Feature parity and document compatibility with iWork '09 (9.3). | Replaced by D-001 |
| R-002 | 2026-11-13 | Path L: manual saving and a recovery draft every two minutes, without versions. | Replaced by D-001 (automatic saving and versions, as in iWork 15) |
| R-003 | 2026-11-13 | One window per document. | Replaced by D-001 (documents in tabs) |
| R-004 | 2026-11-14 | Interface B, with the sheets of Tables in the sidebar, a format bar and a bottom bar. | Replaced by D-008 |
| R-005 | 2026-11-16 | In each feature, the better version according to the comparison reports. | Replaced by D-001 to D-003 |
| R-006 | 2026-10-30 | Layout compared with PDFs of Pages '09 on the Lion virtual machine. | Replaced by D-012, then D-020 |
| D-012 | 2026-11-16 | The reference for checking layout and behaviour is iWork 15.4 on a Mac, for everything. | Replaced by D-020 (features only iWork '09 has are checked on the Lion virtual machine) |
| D-021 | 2026-11-16 | The owner confirmed D-017: Desk follows GNOME's shortcut wherever the action is like Desk's and it does not conflict. Document tabs: Ctrl+PageUp and Ctrl+PageDown in Writer and Slides, Alt+1 to Alt+9 in the three apps. In Tables, Ctrl+PageUp and Ctrl+PageDown keep switching sheets and the tabs use Ctrl+Tab and Ctrl+Shift+Tab. Ctrl+Home and Ctrl+End stay with the text and the cells, not the tabs. | Replaced by D-023 (under D-022, GNOME wins over the sheets of Tables) |
