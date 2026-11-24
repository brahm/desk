# Batch 1: file readers and formats

The crates that read files Desk did not write: `desk-iwork` (Pages '09),
`desk-odf` (ODF) and `desk-filters` (ODT, Pages and HTML into the model).
Reviewed on 24 November 2026 from commit `b643b75`, with `/code-review` at
level high over the three crates, the risk lints of D-028 and an hour of
fuzzing for each target.

## Findings

| # | Class | Where | What failed | What was done |
| --- | --- | --- | --- | --- |
| 1 | A | `desk-iwork/src/pages.rs`, `pages/drawables.rs` | A span or a group that refers back to itself (`sfa:IDREF`) made the reader recurse until the stack overflowed, which aborts the app; spans that refer twice to the next at each level did 2^n work | A guard (`sf::Guard`) bounds the depth and the references one document follows, shared by every reader of the document; tests with a loop, with 40 levels of branching references and with two readers |
| 2 | A | `desk-filters/src/pages.rs` | Character styles whose parents loop were left out of the document while its runs named them, and saving as ODT failed | One style of the cycle is put without a parent, and the styles below it keep theirs; tests that read and save them |
| 3 | A | `desk-filters/src/odt/tables.rs` | The `desk:table` JSON of a damaged file could name a table whose cells, lines or merges do not fit its size; the first cell read panicked. Chart data was trusted the same way | `Table::is_consistent` and `ChartData::is_consistent`; the reader falls back to the ODF cells; tests in `desk-table`, `desk-chart` and `desk-filters` |
| 4 | A | `desk-filters/src/picture.rs` | A GIF of a few bytes that declared a 65535 × 65535 screen was allocated at once, and its size overflowed `u32`; the sums of a picture scaled by a large factor overflowed too | GIF and TIFF share a limit of 64 million pixels; sizes and sums in `usize` and `u64`; tests |
| 5 | A | `desk-odf/src/table.rs`, `chart.rs` | The limits of rows and columns held per table, so a few hundred bytes cloned millions of cells across tables; a chart expanded its ranges whole | A budget for the whole document (`desk-odf/src/budget.rs`): a million table cells, counting the cells a row is filled with; 64 MB of content copied by repetition, each copy weighed; a million chart cells and eight million series values across the charts; chart ranges cut to their data and counted apart from the data table, so a large valid chart keeps its series; tests |
| 6 | A | `desk-odf/src/xml.rs` | Control characters from pasted HTML were written as they were, and LibreOffice could not open the file. A damaged file that carried them was read with them, so it read differently once saved again (found by fuzzing `odt_streams` and `odt_import`) | The characters XML 1.0 does not allow are left out on writing and on reading; tests |
| 7 | B | `desk-iwork/src/lib.rs`, `package.rs` | A Pages flat file larger than 256 MB (one with a movie) was refused as an index too large | The zip is read from the file, and opened once for the index and its files rather than once per file; test with a 257 MB file |
| 8 | B | `desk-filters/src/pages.rs` | TIFF and GIF pictures of objects, fills and posters came in scaled down to 256 pixels, the size of an image bullet | They keep up to 4096 pixels, as from ODT (`picture::OBJECT_MAX_SIDE`); test |
| 9 | B | `desk-odf/src/chart.rs` | Rows that ODF writes once for several (`table:number-rows-repeated`) were read once, so the series that followed moved up | Read as many times as they repeat; test |
| 10 | D | `desk-iwork/src/sf.rs` | Of two objects with the same `sfa:ID`, the last was kept, against the intent of the code | The first is kept; test |
| 11 | B | `desk-odf/src/read.rs` | A paragraph in a `text:list-header` leaves its list on saving again | Backlog B-052, to W5 (LibreOffice ODT in full): the model has no list item without a label |
| 13 | A | `desk-iwork/src/package.rs` | *Security.* A link inside a Pages package folder, named as a picture, was followed out of the folder: a package received from someone else could bring any file of the user, such as a key, into the document, to leave with it when saved and sent. Found by `/security-review` (D-029) | Paths are resolved and must stay inside the folder; test with a link out and a link in |
| 12 | D | `desk-filters/src/pages/objects.rs` | A currency cell of Pages '09 with no currency code shows R$ whatever the region | Backlog B-051, to T5, with the other cell formats of Pages '09 (B-050) |

Class A: data lost, a panic, a hang or memory exhausted. B: wrong
behaviour. C: performance. D: clean-up. See [README.md](README.md).

A second reading of the pull request found that the fixes themselves
cost too much or lost data: cycles of styles were cut one per pass, each
pass looking up every style again; a cell the budget refused let the
next move into its place; every cell paid for its bytes, though only the
copies that repetition makes should; empty cells and rows repeated before
others were cut and what followed moved; and the floating objects could
spend the references of the body. Each now has its test: one pass cuts
all cycles, a refused cell ends its row, what is written once costs no
bytes, empty cells and rows keep their place unless they end the row or
the table, and the text and the objects count their references apart,
with the log saying when the limit was reached.

The review of the pull request asked for the guard and the budgets to hold
for the document rather than for each body, table or chart, and for the
cycle fallback to keep the parents of the styles below a cycle; it is
done in the rows above. It also asked that the pictures of a Pages
document be decoded once (only TIFF and GIF, which are scaled, are read
again at the size of objects) and that the zip of a flat file be read
through a buffer.

## Security (D-029)

After the owner added security to the review, the branch was read with
`/security-review`. It found nothing the batch added; it pointed at
finding 13, older than the batch, which is fixed here. The rest it
checked: the zip of a flat file is only read, never extracted, and its
paths cannot leave the package; the XML reader expands no entity but the
five predefined ones and character references, and loads no DTD or
external reference; the writer still escapes everything its contexts
need.

## Lints (D-028)

`desk-odf` and `desk-iwork` deny `clippy::indexing_slicing` and
`clippy::unwrap_used` in their library code; their 19 places that indexed
or unwrapped were changed to checked access. `desk-formula` has 184 such
places and is the subject of batch 2: its lints are turned on there.

## Fuzzing

Each of the five targets (`odt_streams`, `odt_package`, `odt_import`,
`pages_import`, `html_paste`) ran for an hour on the fixes of this batch.
`odt_streams` and `odt_import` stopped within a minute on a control
character in a font name (finding 6); they ran their hour again after the
fix. The results are in the pull request. The ODT round trip already has its
target, `odt_import`; the formula parser gets one in batch 2.

## Noticed on the way, for later batches

- `desk-table`: `Table::new` multiplies rows by columns in `u32`, which
  overflows at the largest size it allows (65535 × 65535). Batch 2.
- `desk-filters` depends on `desk-anim` without using it, and the
  workspace declares `desk-testkit` as a dependency no crate takes.
  Batch 6.
