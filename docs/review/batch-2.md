# Batch 2: formulas and tables

The crates that compute: `desk-formula` (the parser and evaluator of
formulas) and `desk-table` (the table model, its operations, formats and
evaluation). Reviewed on 24 November 2026 from commit `145c979`, with
`/code-review` at level high over the two crates, `/security-review`
(D-029), the risk lints of D-028 and a new fuzz target for formulas.

## Findings

| # | Class | Where | What failed | What was done |
| --- | --- | --- | --- | --- |
| 1 | A | `desk-formula/src/parse.rs` | A formula of many parentheses, signs or operators overflowed the stack of the parser, which aborts the app | Nesting is bounded (`MAX_NESTING`, 512 levels, parentheses and calls counting twice); deeper formulas are a syntax error; tests at the limit and past it |
| 2 | A | `desk-table/src/eval.rs` | A chain of formulas that each refer to the next, as a running balance from the bottom up, overflowed the stack | What a formula depends on is computed first with a stack of its own, and every cell of a cycle is circular; test with a chain of 20,000 |
| 3 | A | `desk-formula/src/parse.rs`, `value.rs` | A formula written back after a row was inserted lost its small constants: `=A1*1E-16` became `=A2*0` | Constants are written in full (`formula_number`); test |
| 4 | A | `desk-formula/src/eval.rs`, `functions.rs`, `desk-table/src/eval.rs` | *Security.* A file could hold formulas that each read a large range, or texts that double from cell to cell (`=A1&A1`) or grow by `SUBSTITUTE`, so that opening it held the app for minutes or filled its memory | A formula makes at most 32,767 bytes of text (`MAX_TEXT`); one evaluation of a table reads at most 2^27 cells of ranges, header cells and bytes of text, and the formulas past it give `#VALUE!`; the worst table measured stops in under two seconds; tests |
| 5 | A | `desk-table/src/copy.rs` | *Security.* Pasted text grew the table one row and one column at a time, moving the whole table each time, so a long text took minutes; one line of many tabs and many lines, a short text, asked for billions of cells | The table grows once, to a million cells at most; test |
| 6 | B | `desk-table/src/format.rs` | *Security.* A custom number format of many zeros, from a file, padded the number one zero at a time, in quadratic time for every cell shown | The zeros are put at once; test with 200,000 |
| 7 | C | `desk-formula/src/functions.rs` | `SEARCH` with `*` or `?` built every prefix of the text from every start, in cubic time: a text of a few thousand characters took minutes | The pattern followed by `*` is matched once from each start; test |
| 8 | B | `desk-formula/src/odf.rs` | `LOG10(100)` went to ODF as `[.LOG10](100)`, a cell reference no reader takes | A name before a parenthesis stays a function; test |
| 9 | B | `desk-formula/src/odf.rs` | A header named in quotes with a bracket, such as `'Price [R$]'`, was read back from ODF as a reference to another sheet, `#REF!` | Quotes are followed when reading; test |
| 10 | B | `desk-formula/src/functions.rs` | `ROUND`, `TRUNC` and `SUM` rounded to a fixed number of decimals, losing the digits of very small or very large numbers | They round relative to the number (14 significant digits); test |
| 11 | B | `desk-table/src/table.rs`, `desk-ui/src/table_pane.rs` | Rows added after the footer did not join it: the new rows became the footer and the totals moved into the body | The footer grows; the Rows field adds body rows; test |
| 12 | B | `desk-table/src/ops.rs` | Splitting a cell shrank the merges that crossed the split row, and those that ended on it did not grow | Only the cells beside it that end on the split row grow; test |
| 13 | B | `desk-table/src/ops.rs` | A fill of `Jan, Feb` went on in Portuguese, the first series that held `Jan` | A fill goes by the series that holds every name it starts from; test |
| 14 | B | `desk-table/src/table.rs` | Autocomplete skipped the cell typed in by its place in the list of the column's texts, which leaves out empty cells and repeats, so it could skip another cell or suggest the cell's own text | It goes by row; test |
| 15 | B | `desk-table/src/conditional.rs`, `ops.rs`, `drag.rs` | A conditional rule that compares with another cell kept pointing at the same place when rows and columns were added or deleted, and when cells were filled, copied, dragged or sorted | The cell of a rule moves as a formula's reference does; deleted with its row or column, the rule no longer applies; test |
| 16 | B | `desk-table/src/ops.rs` | Sort and Convert to Text computed formulas on day 0, so `TODAY` and `NOW` sorted as 1899 | They take the day it is; test |
| 17 | B | `desk-table/src/ops.rs` | Merge joined numbers with text in en_US, so `2,5` became `2.5` in a Brazilian document | Numbers are written as they are typed in the locale of the document; test |
| 18 | B | `desk-table/src/format.rs` | A date typed without a year, `15/03`, was 15 March 2000 | It is of the current year, as in Numbers; test |
| 19 | B | `desk-formula/src/value.rs` | `YEAR(1E300)` passed the largest `i64` to `date_from_days`, whose first sum overflowed: a panic in debug builds, a wrong date in release | Days are clamped to well past the years an `i32` holds; test |
| 20 | D | `desk-table/src/table.rs` | `Table::new` sized its borders by products in `u32`, which only just fit at the largest size | Sizes in `usize` |

Class A: data lost, a panic, a hang or memory exhausted. B: wrong
behaviour. C: performance. D: clean-up. See [README.md](README.md).

No finding of the batch was moved to the backlog.

## Security (D-029)

`/security-review` read the two crates for what a formula or a table from
a file, or text pasted from elsewhere, can make Desk do. Formulas compute
values only: no function reads files, the network, the environment or
other documents, and references to other sheets become `#REF!`. What a
formula can do is spend time and memory, which findings 1, 2, 4, 5 and 6
now bound. The rest it checked: a range is bounded to a million cells and
a table to 65,535 rows and columns; `REPT` was bounded already;
`FIXED` and the number formats bound their decimals.

## Lints (D-028)

`desk-formula` denies `clippy::indexing_slicing` and `clippy::unwrap_used`
in its library code. Its arguments are taken with `at`, which gives
`#VALUE!` for one missing, and the tokenizer, the wildcard matcher,
`MEDIAN`, `SUMPRODUCT`, `CHOOSE` and `RIGHT` use checked access.

## Fuzzing

A new target, `formula_parse`, reads any text as a formula, writes it as
text and as OpenFormula and reads both back the same, and evaluates it in
a table. Its results are in the pull request.

`proptest`, which the plan named for formulas against known values, was
not needed here: the round trip of the fuzz target checks the writers,
and the tests check each function against known values. It is used in
batch 3, for each operation of the model and its inverse.
