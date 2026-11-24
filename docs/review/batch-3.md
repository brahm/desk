# Batch 3: the document model

The crate `desk-model`, which holds the document and its undo, with the
operations of `desk-table` that copy cells into new rows and columns.
Reviewed on 24 November 2026 from commit `0d2ab08`, with `/code-review`
at level high over the crate, `/security-review` (D-029), the risk lints
of D-028 on the modules that read bytes, and property tests of each
operation against its inverse.

## Findings

| # | Class | Where | What failed | What was done |
| --- | --- | --- | --- | --- |
| 1 | A | `desk-model/src/media_trim.rs` | *Security.* Reduce Media panicked on an AIFF with a second COMM or SSND too short to rewrite | The first of each, which was read, is kept and the others left out; test |
| 2 | A | `desk-model/src/media_trim.rs` | *Security.* An MP4 or QuickTime row of `stts` or `ctts` that counted billions of samples was expanded before it was checked: tens of GB | The counts are checked against the samples, and the samples against the size of the file, first; test |
| 3 | A | `desk-model/src/media_trim.rs` | *Security.* A `co64` offset or a time near the largest overflowed the checks of the samples, and the slice panicked | The sums are checked, and times scaled in 128 bits; test |
| 4 | A | `desk-model/src/story.rs`, `object.rs` | *Security.* Finding the stories of thousands of linked text boxes took the cube of their number, and an ODT with comments looked for them on opening | The heads of all the boxes are found together, in a loop of links too, by an index of the box before each; test against the earlier result and with 20,000 boxes |
| 5 | A | `desk-model/src/text.rs` | *Security.* A paragraph of one span for each letter, from a file, joined them in the square of its length | Runs of one format are joined once; test with 200,000 |
| 6 | A | `desk-model/src/text.rs`, `object.rs`, `document.rs` | *Security.* Each paragraph and each object a file adds was checked to be new by going through all the others: the square of their number | The body and the objects keep a set of their identifiers; tests, and the property tests of undo check it |
| 7 | B | `desk-model/src/transaction.rs` | Letters typed after an Undo joined the typing before what was undone, so one Undo took both back | A new step after an Undo; test |
| 8 | C | `desk-model/src/transaction.rs` | The undo history kept every action, each with a whole table for an edit of its cells | It keeps the last 1000 (`MAX_UNDO`); test. Keeping only the cells changed is B-054 |
| 9 | B | `desk-table/src/table.rs` | Rules copied into a row added in the middle pointed as many rows too far (B-053); columns did not move them at all | They are copied from before the insertion, relative parts at their distance and fixed ones with their cell; tests and property tests |
| 10 | A | `desk-table/src/table.rs` | The width of a table summed its columns in `u32`, which a file's widths overflowed: a panic in debug builds, a wrong width in ODT | Summed in `u64`, at most the largest; test |
| 11 | C | `desk-model/src/pixel_outline.rs` | Tracing a picture of many opaque specks made a list of every row for each speck: a dithered picture froze the window when text wrapped tightly | Each part keeps the rows it crosses; test with 90,000 specks |
| 12 | D | `desk-model/src/media_trim.rs`, `media.rs`, `picture.rs` | Bytes of media and pictures read with indexes that a file could put out of range | Taken with `get`; the lints of D-028 deny new ones there |

Class A: data lost, a panic, a hang or memory exhausted. B: wrong
behaviour. C: performance. D: clean-up. See [README.md](README.md).

One finding moved to the backlog: B-054, the copy of a whole table in
the undo history for each edit of its cells, to phase T1, whose editing
of tables brings operations of cells. B-053, from batch 2, is closed.

## Security (D-029)

`/security-review` read the crate for what a document from a file, or
pasted content, can make the model do. The model opens no file and runs
nothing; styles refuse parent cycles, list levels and roman numbers are
bounded, groups are bounded by the readers, and the identifiers of new
nodes never come from a file. What a document could do was spend time:
findings 1 to 6. It also noted, with less confidence, that notes in the
Symbols format have labels as long as their number divided by six; a
document needs thousands of notes for that to matter, and it is left
as it is.

## Lints (D-028)

The modules of `desk-model` that read bytes from files, `media_trim`,
`media` and `picture`, deny `clippy::indexing_slicing` and
`clippy::unwrap_used`. The rest of the crate works on a model that its
operations keep consistent, where an index out of range is a bug of the
model, not of a file.

## Property tests

`desk-model/tests/objects_undo.rs` runs sequences of adding, deleting,
moving, rotating, flipping, locking, linking, layering, duplicating,
grouping and ungrouping objects, and of changing the cells of tables
(rows and columns added and deleted, typing, footers, merges, sorting),
and checks that they undo back to the start and redo to the end, and
that each one's inverse restores the document. `desk-table/tests/inverse.rs`
checks that rows or columns added and deleted give a table back, with
its formulas and rules, and that rules copied into new rows keep their
distance. With the property tests of text that `desk-model/tests/undo.rs`
already had, each operation of the model is checked against its inverse.
