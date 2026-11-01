# Corpus

The reference corpus of Desk: what Desk must do the same as iWork '09.

- `matrix.tsv` is the parity matrix of Desk Writer: the 271 rows of the
  survey of Pages '09, each with the phase of the roadmap that completes
  it.
- Each folder under `writer/` is a case: `case.toml` describes a short
  document, and the reference virtual machine adds the `.pages` file that
  Pages builds from the same description and `reference.pdf`, the PDF that
  Pages exports.

The cases use the blank template and free fonts, such as Liberation,
DejaVu and Noto, that are installed on the virtual machine as well, so
that the comparison measures the layout and not a change of font.

## A case

```toml
title = "Espaço antes e depois do parágrafo"
rows = ["FMT-15"]
description = "Entre dois parágrafos vale o maior dos dois espaços."

[page]                # a4 by default, or letter; landscape = true turns it
size = "a4"
margin_top = 72       # lengths in points

[defaults]            # the format of the default paragraph style
font = "Liberation Serif"
size = 12

[[paragraphs]]
text = "Uma linha\noutra linha"   # \n is a line break
space_after = 24

[[paragraphs]]
filler = 120          # 120 words of fixed filler text
repeat = 3            # three copies of the paragraph
align = "justify"

[[paragraphs]]
runs = [{ text = "Texto em " }, { text = "negrito", bold = true }]
```

A paragraph takes `align`, `line_spacing` (`{ multiple = 150 }`,
`{ at_least = 14 }`, `{ exactly = 14 }` or `{ between = 4 }`),
`space_before`, `space_after`, `first_line_indent`, `left_indent`,
`right_indent`, `page_break_before`, `keep_with_next`,
`keep_lines_together` and `widow_control`. A paragraph and a run take
`font`, `size`, `bold`, `italic`, `underline` and `strikethrough`
(`single` or `double`), `color` (`#rrggbb`), `baseline` (`superscript` or
`subscript`) and `capitalization` (`all_caps`, `small_caps` or `title`).

A case passes when Desk puts the same words on the same pages as Pages,
each within `position_tolerance` points of its place (1.5 by default).
The share of pixels that differ is measured too, and fails the case only
when the case sets `pixel_tolerance`, in percent.

When the case has `case.pages`, Desk also imports that document, as
Pages saved it, and compares its layout with the same PDF. Both must
match: the first checks the layout, the second the reader of `.pages`
files.

## Documents of Pages

`fixtures/pages/` holds documents that Pages '09 saved on the reference
machine, each with the PDF it exported, for what a case in `case.toml`
cannot describe. There is one for each row of the matrix that a saved
document can hold; [`manifest.tsv`](fixtures/pages/manifest.tsv) says
which document shows each row, and why the others have none, such as
rows about the application alone or what the reference machine cannot
do. Desk reads only the format of Pages '09, so there are no documents
saved in the format of Pages '08.

The tests of `desk-filters` open every one of them and check that the
import names what it leaves out. `lossy-import.pages` has one of each
thing that Desk does not import yet, such as headers, images, footnotes,
comments and fields, and its log is checked entry by entry.

## Running it

```sh
cargo run -p desk-testkit --bin desk-parity
```

writes `target/parity/report.md`, `report.json`, and for each case the PDF
of Desk (and in `from-pages/` the one made from `case.pages`) and, when
there is a reference, the diff images: in red what only
Pages drew, in blue what only Desk drew. The CI keeps them as the
`parity-report` artifact.
