# Contributing to Desk

Thank you for looking. Read [docs/README.md](docs/README.md) first: it
points to the goals, the architecture and the roadmap.

## Build and test

Desk needs the libraries of GNOME 51 (GTK 4.24, Libadwaita 1.10) and Rust
1.98 or later; the [README](README.md#building) has the packages for
Fedora 45 and Ubuntu 26.10, and `ci/setup-env.sh` prepares an older
Ubuntu.

```sh
cargo run --package desk-writer      # or desk-tables, desk-slides
ci/check-all.sh                      # what the CI runs
```

While you work, run the tests of what you changed; run `ci/check-all.sh`
once before you send the change.

## What a change needs

1. **A row to point to.** A feature belongs to a row of
   [`corpus/inventory`](corpus/inventory/README.md) and, once specified, to
   rows of the parity matrix. Name them in the pull request and in the
   tests.
2. **Tests.** Engine code has unit tests; behaviour that iWork defines is
   compared with a reference case; the interface is tested through the
   accessibility tree (`tests/ui`).
3. **The layers kept.** Only `desk-ui` and the apps use GTK; the MPL
   libraries use no other Desk crate; only `desk-filters` turns a format
   into the model. See [docs/architecture.md](docs/architecture.md).
4. **Stored features documented.** A new `desk:` extension is described in
   [docs/odf-extensions.md](docs/odf-extensions.md) before it is merged.
5. **Nothing dropped silently.** What you meant to do and did not gets a
   row in [`docs/backlog/deferred.tsv`](docs/backlog/README.md) in the same
   pull request.
6. **Licenses.** Every file has an SPDX header or is covered by
   `REUSE.toml`. Nothing of Apple enters the repository: no template,
   font, image, or definition taken from its software (D-011).

## Comments

The standard is that a person understands how a file works by reading
that file.

- **Each file starts with a module comment** (`//!`): what the file is
  for, the main types or functions in it, and how they fit together. If
  the file implements part of a larger design, it links the page of `docs`
  that explains the design.
- **Each public item has a doc comment** (`///`) that says what it does
  and what its arguments and result mean, in the words of the domain (a
  paragraph, a sheet, a build), not of the implementation.
- **Each function whose body is not obvious says how it works**: the steps,
  the invariant it keeps, why this way and not the simpler one. A comment
  explains the reason; the code already says what.
- **Rows and decisions are cited by id** where they explain a behaviour:
  `(W104)`, `(TAB-21)`, `(D-003)`.
- **When understanding a function needs more than a comment can hold**,
  the comment points to the page and section of `docs`.
- Comments are full sentences in English. No commented-out code, and no
  notes about the history of a change: that belongs in the commit message.

The CI builds the documentation with warnings as errors, so a public item
without a doc comment fails.

## Commits and pull requests

A commit message says what changed and why, in the imperative, with a
subject of at most 72 characters. A pull request names the inventory rows
it covers, says how it was tested, and lists what it left undone.
