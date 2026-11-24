# Code review (phase RV)

On 24 November 2026 the owner paused development after pull request 200
to review all the code written so far for bugs and optimizations, before
going back to the planned phases (D-027). The same day the owner added
security: each batch also looks for security flaws, and every pull
request is reviewed for them before it is merged (D-029). This directory holds the plan of
that review and one report of findings per batch.

## Where the code stood

Read at commit `092db61`: about 148,000 lines of Rust in 19 crates, with
the tests. No `unsafe` block: the format libraries forbid it and the rest
of the workspace denies it. Clippy's `indexing_slicing` finds 360 indexes
and 10 slices that can panic in `desk-odf`, `desk-iwork`, `desk-formula`,
`desk-model`, `desk-table` and `desk-filters`, which read files that Desk
did not write. Five fuzz targets cover the ODT reader, the ODT package,
the ODF streams, the Pages '09 reader and HTML paste; formulas and tables
have none.

## Batches

The batches go by risk: what reads files from outside and what computes
comes before what draws. Each ends in a pull request of its own.

| Batch | Crates | Looked for | Days |
| --- | --- | --- | --- |
| 1. File readers and formats | `desk-iwork`, `desk-odf`, `desk-filters` | Panics on malformed files, files made to exhaust memory or the stack (zip, deep XML), data lost silently on the way in and out of ODT and Pages '09. Security: paths in a package that leave it, entities and external references in XML, links and embedded files a document carries | 1.5 |
| 2. Formulas and tables | `desk-formula`, `desk-table` | Wrong results, numeric casts, circular references, slow recalculation. Security: formulas from a file made to exhaust memory or time (huge texts, ranges, deep nesting) | 1 |
| 3. Document model | `desk-model` | Each operation and its inverse (undo and redo), invariants, `unwrap` on paths a file can reach. Security: what a document can make the model hold or repeat without bound | 1.5 |
| 4. Text, layout and drawing | `desk-text`, `desk-layout`, `desk-render`, `desk-chart` | Relayout and drawing of large documents, caches, PDF. Security: pictures, fonts and PDFs from a document handed to the system libraries | 1 |
| 5. Interface | `desk-ui`, the apps | Leaks by reference cycles, signals connected twice, work that blocks the main thread, files of 4,000 lines. Security: links a document opens, files and folders written (versions, temporary files, exports), the clipboard, the Flatpak permissions | 2 |
| 6. Across the crates | all, `ci`, `fuzz` | Duplicated code, unused dependencies, gaps in tests and fuzzing. Security: advisories against the dependencies, the CI and its secrets | 0.5 |

## Classes of findings

- **A, severe:** data lost or corrupted, a panic, a hang, a security
  flaw that a file, a link or pasted content can exploit. Fixed in the
  batch, with a regression test.
- **B, bug:** wrong behaviour without loss of data. Fixed in the batch,
  with a test.
- **C, performance:** fixed when a measurement shows the gain; otherwise
  a row of the backlog.
- **Security** is not a class of its own: a security finding takes the
  class of its harm (A when a file or a link can exploit it, B when it
  only weakens a defence) and is marked *security* in the report.
- **D, clean-up:** duplication, dead code, names, an `unwrap` that should
  be an error. Fixed when cheap and local; otherwise a row of the backlog.

A batch closes only when every A and B finding is fixed, or is a row of
[`backlog/deferred.tsv`](../backlog/deferred.tsv) with its reason and
phase. A fix that changes visible behaviour, or conflicts with a
decision, goes to the owner first.

## Tools

| Tool | Use | Batches |
| --- | --- | --- |
| `/code-review` at level high | Reading each crate of the batch for bugs; then reviewing the pull request of fixes | All |
| `/security-review` | Reading each crate of the batch for security flaws; then reviewing the pull request of fixes, as every pull request of the project before it is merged (D-029) | All |
| Clippy pedantic and the risk lints (`indexing_slicing`, `unwrap_used`, casts) | A list of places to read, not all turned on | 1 to 4 |
| Fuzzing, an hour per target, with two new targets: the formula parser and the ODT round trip | Panics and slowness on odd input | 1, 2 |
| `proptest` | An operation then its inverse gives back the same document; formulas against known values | 2, 3 |
| `cargo llvm-cov` | Coverage per crate, to find code without tests | 6 |
| Timings of opening, saving, relayout and recalculation with the largest documents of the corpus | A baseline before and after each optimization | 1, 2, 4 |
| heaptrack and the GTK inspector, on the owner's Linux machine | Objects that are not freed when a document closes | 5 |
| `cargo machete` and `cargo deny` | Unused dependencies and advisories | 6 |
| `ci/check-all.sh` and the UI tests | Before every pull request | All |

After batch 1, the format libraries deny `indexing_slicing` and
`unwrap_used` in the CI (D-028).

## Reports

Each batch writes `batch-N.md` here: the findings with their class, file,
failure scenario and what was done. The pull request of the batch links
it.
