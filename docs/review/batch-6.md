# Batch 6: across the crates

All the crates, `ci`, `fuzz`, the workflows and the Flatpak manifest.
Reviewed on 24 November 2026 from commit `f79b4da`. Unused dependencies
were found with Cargo's own `unused_dependencies` lint, which now does
what `cargo machete` was planned for. Advisories, licenses, bans and
sources were checked with `cargo deny`, on both the workspace and the
fuzz lock files. `/security-review` (D-029) covered the workflows, their
secrets and how the Flatpak is built and signed. The fuzz targets were
compared with the readers they leave out. The three rows the earlier
batches moved here (B-060, B-062, B-063) are done.

## Findings

| # | Class | Where | What failed | What was done |
| --- | --- | --- | --- | --- |
| 1 | A | `.github/workflows/*.yml` | *Security.* The actions ran by tag or branch. `flatpak-builder@master` builds the bundle, and the repository job then signs it with the GPG key of the project, so a change pushed upstream would be signed and published | Every action is pinned by commit, with its tag beside it; Dependabot proposes the updates |
| 2 | A | `build-aux/flatpak/` | *Security (B-060).* The Flatpak build had network access and downloaded its crates itself | Crates are vendored offline from `cargo-sources.json`, which `ci/flatpak-cargo-sources.py` writes from the checksums of `Cargo.lock`. A CI job fails when the two disagree |
| 3 | A | `build-aux/flatpak/` | *Security (B-060).* Poppler was built with its own JPEG 2000 decoder, which Poppler no longer maintains, for the PDFs that documents place as pictures | Poppler uses OpenJPEG 2.5.4, built in the manifest from a pinned tarball |
| 4 | B | `.github/workflows/ci.yml` | *B-062.* The CI ran `cargo test` without a display, so every test of windows, editors and the clipboard was skipped there | The tests run under `xvfb-run` |
| 5 | C | `desk-lang/src/lib.rs`, `dictionaries.rs` | *B-063.* Each language tag that no dictionary fits listed the dictionary folders again, on the thread of the window | The folders are listed once, and each tag is looked up in that list; test |
| 6 | D | the `Cargo.toml` files | Nine dependencies were never used. `desk-anim` depended on `desk-model`, `desk-filters` on `desk-anim`, `desk-slides` on `desk-filters` and `desk-anim`, `desk-tables` on `desk-contacts` and `desk-filters`, and `desk-writer` on `desk-contacts`. The workspace also listed `desk-testkit`, `desk-anim` and `desk-contacts` for no one | Removed. `desk-anim` and `desk-contacts` stay members of the workspace, with their tests, for the phases that will use them (S3 and W3) |
| 7 | D | `desk-ui/src/window.rs`, `tables/window.rs`, `tabs.rs` | The windows of Writer and Slides and of Tables each kept their own controllers on the window and moved them by the same code when a tab moved | One `WindowBindings` holds the controllers and the handlers, and moves them; test |
| 8 | D | `fuzz/` | The picture converters (TIFF and GIF to PNG), where batch 1 found overflows, had no fuzz target | A `picture` target, with seeds of each kind. It ran 226,705 inputs in three minutes without a failure, and the CI runs it with the others |

Class A: data lost, a panic, a hang or memory exhausted, or a security
flaw. B: wrong behaviour. C: performance. D: clean-up. See
[README.md](README.md).

The coverage per crate that `cargo llvm-cov` would give was not measured.
Its instrumented build of the whole workspace did not fit the disk of the
review session. It moved to the backlog as B-064, for phase W6.

## Security (D-029)

Findings 1, 2 and 3 came from the security review.

These were checked and found safe:
- `cargo deny` finds no advisory, no yanked crate, no license outside the
  policy and no source other than crates.io, in either lock file. Four
  crates come in two versions through other crates (`miniz_oxide`,
  `rand_core`, `syn`, `system-deps`); that is a warning only.
- The workflows can only read the repository, except in the jobs that
  publish. Those jobs run only on `main` or on tags, never for a pull
  request.
- The GPG key is read only in the job that signs the repository. Code
  from a pull request never runs with it.
- The containers of the jobs are pulled by tag (`ubuntu:26.10` and the
  `gnome-51` image of Flathub). Pinning them by digest would also stop
  their security updates, and they run nothing with a secret, so they
  stay as they are.
- The website workflow reads the pull requests with the token of the
  run, and writes nothing.
