# Instructions for coding agents

You are working on Desk, an office suite for GNOME in Rust, GTK 4 and
Libadwaita. Read these before changing anything; they are short on
purpose and point to the full pages.

1. **Target.** Feature parity with Pages, Numbers and Keynote 15.4, by the
   rules of [docs/goals.md](docs/goals.md). Features that need AI, an
   online service or a payment are out of scope: do not implement them,
   and do not delete their rows.
2. **Before writing code**, find the feature in
   [`corpus/inventory`](corpus/inventory/README.md) and its phase in
   [docs/roadmap.md](docs/roadmap.md). Do not start work of a later phase
   unless the task says so.
3. **Layers** ([docs/architecture.md](docs/architecture.md)): GTK only in
   `desk-ui` and the apps; the MPL crates (`desk-odf`, `desk-iwork`,
   `desk-formula`) depend on no other Desk crate; formats meet the model
   only in `desk-filters`; every change to a document is an operation of
   `desk-model` with an inverse.
4. **Interface.** Use a native Libadwaita widget when one exists. When
   none does, build one that follows the GNOME HIG and record it in the
   component table of `docs/interface/proposals/README.md`, to be reviewed
   at each new Libadwaita.
5. **Storage.** Standard ODF first; a `desk:` extension only when ODF has
   no place for the feature, documented in
   [docs/odf-extensions.md](docs/odf-extensions.md) in the same change.
6. **Comments** follow [CONTRIBUTING.md](CONTRIBUTING.md#comments): a
   module comment per file, a doc comment per public item, the how and the
   why of anything not obvious, ids of rows and decisions where they
   explain a behaviour.
7. **Tests.** Run the tests of what you changed while you work and
   `ci/check-all.sh` once before you finish. Never skip, disable or loosen
   a test to make it pass.
8. **Left undone.** If you do not finish something you were asked to do,
   add a row to `docs/backlog/deferred.tsv` with the reason and the phase
   it moves to, and say so plainly in your report. Run
   `ci/check-backlog.py`.
9. **Apple material.** Never extract anything from Apple software, and
   never add its templates, fonts, images or format definitions to the
   repository (D-011).
10. **Decisions** belong to the project owner. When a task conflicts with
    [docs/decisions.md](docs/decisions.md), stop and ask; do not pick a
    side in code.
