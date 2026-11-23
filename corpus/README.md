# Corpus

The reference corpus of Desk: what Desk must do the same as iWork. The
target is the current iWork (15.4, decision D-001); the matrices and the
cases here were written from iWork '09, whose features Desk keeps
(D-002), and the parity documents of features only '09 has are made on
the Lion virtual machine (D-020). The matrices of the current iWork are
the inventory in [`inventory/`](inventory).

- `matrix.tsv` is the parity matrix of Desk Writer, 330 rows, each with
  the phase that completes it: the 260 rows of the survey
  of Pages '09, the 33 cases of integration among the three apps of the
  surveys of Pages, Numbers and Keynote (`IOP`), 14 rows for what the
  GNOME HIG and the desktop ask of the apps (`GNM`), and 4 commands that
  the inventory of Pages 4.3 on the reference virtual machine added to
  the survey (`VMI`), with the numbered phases of the old plan, and 19
  features of Pages 15 seen in Pages 15.4 on the owner's Mac from
  2026-11-18 (`PXV`, D-020), whose phase is the id of the roadmap, such
  as `W1`. The 8 rows of review (`REV`) were written again from Pages
  15.4 on 2026-11-22, keeping what Pages '09 did (D-003), and take the
  phase `W2`. Its last column, `gnome`, says how Desk adapts a
  row to GNOME where the decisions of 2026-11-13, 2026-11-14 and
  2026-11-16 changed its form (one Inspector, a header bar without
  customization, popovers for Fonts and Colors, shortcuts that leave
  GNOME's own alone, automatic saving with versions, and window M+ of
  [`docs/interface/proposals`](../docs/interface/proposals/README.md),
  decision D-008); the row passes when Desk does it that way.
- `slides-matrix.tsv` and `tables-matrix.tsv` (181 rows each) are the
  matrices of Desk Slides and Desk Tables, with a `shared` column that
  names the rows of the other surveys that describe the same feature, the
  same `gnome` column, their own GNOME rows (`KGNM`, `NGNM`) and their
  own rows of the inventory (`KVMI`, `NVMI`).
- `vm-review.tsv` lists the rows whose interface was compared by eye with
  Pages 4.3 on the reference Mac OS X Lion; the parity report shows them
  as "reviewed on the Lion VM" when only tests check them otherwise.
- `mac-review.tsv` lists the rows of the inventory whose interface was
  compared by eye with Pages, Numbers and Keynote 15.4 on the owner's Mac,
  with a verdict: the same, another form set by a decision, or a gap with
  its backlog item.
- Each folder under `writer/` is a case: `case.toml` describes a short
  document, and the reference virtual machine adds the `.pages` file that
  Pages '09 builds from the same description. `reference.pdf` is the PDF
  that Pages 15.4 exports from that file, on the owner's Mac with
  macOS 26.7, since 17 November 2026: the target is the current Pages
  (D-001), which opens the file of '09 as it is. Before, the PDF came
  from Pages '09.

The cases use the blank template and free fonts, such as Liberation,
DejaVu and Noto, that are installed on the virtual machine and on the
Mac as well, so that the comparison measures the layout and not a change
of font. The case of missing fonts (DOC-05) uses the fonts of the Mac,
which Desk replaces.

## A case

```toml
title = "Space before and after the paragraph"
rows = ["FMT-15"]
description = "Between two paragraphs the larger of the two spaces counts."

[page]                # a4 by default, or letter; landscape = true turns it
size = "a4"
margin_top = 72       # lengths in points

[defaults]            # the format of the default paragraph style
font = "Liberation Serif"
size = 12

[[paragraphs]]
text = "One line\nanother line"   # \n is a line break
space_after = 24

[[paragraphs]]
filler = 120          # 120 words of fixed filler text
repeat = 3            # three copies of the paragraph
align = "justify"

[[paragraphs]]
runs = [{ text = "Text in " }, { text = "bold", bold = true }]
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

Since 17 November 2026 their PDFs are those that Pages 15.4 exports from
the same documents on the owner's Mac (D-020), which opens them as they
are. Two kinds keep the PDF of Pages '09, and the manifest says so on
their rows: features that only Pages '09 has, such as the outline, and
documents that Pages 15.4 lays out otherwise where Desk does not follow
it yet, or that need fonts not yet on the Mac (justify-spacing and
justify-space-limit, B-023). `mod-12-template`
is a template package, which this export left out, and keeps its PDF of
Pages '09 too.

Pages 15.4 draws each ligature in a span of its own that names its
letters, which Poppler reads in the wrong place; the tests read such a
PDF after Cairo writes it again (`pdftocairo`), where the ligature stands
in its word.

`long-document.pages` is a long document of 41 pages, with sections,
headers and footers, a table of contents, footnotes, pictures, columns
and page breaks, whose text is filler, so that the tests of
`desk-testkit` check a long document against Pages in public.

`justify-spacing.pages`, `justify-space-limit.pages` and
`justify-cases.pages` hold justified lines narrow enough for Pages to
spread their letters: in Liberation, in the test fonts of
[`fixtures/fonts/`](fixtures/fonts/README.md), whose spaces have other
widths, and with sizes mixed in a line, tracking, tabs, hyphenation and a
text box. The tests of `desk-testkit` add those
fonts to the ones Desk lays out with, so they need not be installed,
and compare the documents with the PDF of Pages word by word (FMT-12).

The tests of `desk-filters` open every one of them and check that the
import names what it leaves out. `lossy-import.pages` has one of each
thing that Desk did not import when it was made, such as headers, images,
footnotes, comments and fields, and its log is checked entry by entry;
Desk reads all of them now.

## Running it

```sh
cargo run -p desk-testkit --bin desk-parity
```

writes `target/parity/report.md`, `report.json`, and for each case the PDF
of Desk (and in `from-pages/` the one made from `case.pages`) and, when
there is a reference, the diff images: in red what only
Pages drew, in blue what only Desk drew. The CI keeps them as the
`parity-report` artifact.
