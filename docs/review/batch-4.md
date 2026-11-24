# Batch 4: text, layout and drawing

The crates `desk-text`, `desk-layout`, `desk-render` and `desk-chart`.
Reviewed on 24 November 2026 from commit `c00c915`, with `/code-review`
at level high over the four crates and `/security-review` (D-029) of what
a document from a file can make them do: pictures, fonts and PDFs handed
to Cairo and Poppler, sizes and counts that a file sets.

## Findings

| # | Class | Where | What failed | What was done |
| --- | --- | --- | --- | --- |
| 1 | A | `desk-chart/src/draw.rs` | A bar, line or area chart with series and no categories panicked on drawing: all its columns deleted in the Data Editor, or none in a file | Each series has a value, empty, for the one category that the axes draw; test |
| 2 | B | `desk-chart/src/model.rs` | Pasted numbers with both marks, as `1.234,56`, were read a thousand times too small | The last mark is the decimal one; test |
| 3 | B | `desk-text/src/layout.rs` | The ffl ligature of Hoefler Text stayed U+FB04 in PDFs, where a search for "baffle" did not find it | Read as its letters, as the other ligatures; test |
| 4 | B | `desk-layout/src/notes.rs` | Notes numbered by page: a mark that starts a page took the number of the page before | The page of the line that holds the mark comes first; test |
| 5 | B | `desk-text/src/layout.rs` | After fonts were added, what was measured of a missing family with the font that stood for it was kept | Measured again; test |
| 6 | B | `desk-layout/src/pages.rs` | With footnotes, the leading of the last line of a page could stick out of the body, which Pages 15.4 does not let it, and did not without them | The same rule with footnotes; test |
| 7 | B | `desk-layout/src/pages.rs` | A chain of paragraphs kept with the next, laid out again, kept the notes it had met where it was first | They are met again; a test compares with a column break. The case was found by reading and not reproduced |
| 8 | B | `desk-text/src/tabs.rs` | Tab stops kept short of the edge of a column could come out of order, and a tab passed the nearer | Sorted after; test |
| 9 | A | `desk-text/src/layout.rs`, `space.rs` | *Security.* In a column narrower than 18 points, as a narrow column of a table, each line looked 10,000 points down for room; past 2^24 points a point added changed nothing, and the layout never ended | A column the same all the way down overflows at once; the search is counted in steps; test |
| 10 | A | `desk-chart/src/draw.rs` | *Security.* A logarithmic axis to 1.7e308 counted its decades without end, and a value of 1e-323 made an axis of NaN, which `clamp` refused with a panic | The ranges of the axes are always finite, and decades at most 700; test |
| 11 | A | `desk-render/src/backend.rs` | *Security.* A PNG of a few megabytes may declare 30,000 pixels a side, which Cairo allocated and filled; the decoded pictures kept were bounded by their number, not their size | PNGs over 8192 pixels a side are not decoded, as JPEG; the pictures kept hold at most 512 MB; test |
| 12 | A | `desk-render/src/backend.rs` | *Security.* The shadow of text at 8000 points was a mask of a gigabyte; with a blur of 5000 points Cairo refused the mask, and the rest of the page, its PDF and its thumbnail failed | Large text gets fewer pixels for its shadow, a blur too large is not drawn, and a refused mask skips the shadow; test |
| 13 | A | `desk-text/src/layout.rs` | *Security.* The leader of a tab in a tiny font was hundreds of thousands of dots, and a page of them gigabytes | At most 10,000; test |
| 14 | A | `desk-layout/src/objects.rs` | *Security.* A wrap margin of millions of points made millions of bands of contour for each paragraph placed, and far down the page they stopped moving, without end | The bands are counted, at most 20,000, and wider past that; test |
| 15 | B | `desk-render/src/backend.rs` | *Security.* A dashed line 0 wide, dashes all of 0 or negative, or a picture 0 wide put Cairo in an error for the rest of the page, and PDF export failed | Such lines and pictures are not drawn, and such dashes draw a solid line; test |
| 16 | A | `desk-text/src/layout.rs` | *Security.* Each place a line could end summed the line again from its start: a paragraph in a tiny font, thousands of words a line, took minutes | The advances are summed once; test with 200,000 words |

Class A: data lost, a panic, a hang or memory exhausted. B: wrong
behaviour. C: performance. D: clean-up. See [README.md](README.md).

Three findings of performance moved to the backlog:
- B-055: balanced columns copy the pages for each step of their search, to phase W6.
- B-056: merged cells are looked up through all the merges, to phase T1.
- B-057: a PDF placed as a picture is read again at each paint, and a line with tabs is laid out again for each place it may end, to phase W6.

## Security (D-029)

The security review read the four crates for what a document can make them
do. Findings 9 to 16 came from it.

These paths were checked and found safe:
- JPEG is decoded with a limit of 8192 pixels a side, and PDF pictures
  are drawn at most 8192 a side.
- The shadows of objects are bounded.
- PDF metadata goes through Cairo, which escapes it. The crates write no
  link, URI or SVG.
- Instant Alpha is bounded in its seeds.
- The spans of tables are checked by the table.
- Groups are at most 16 deep.
- Lengths in the model are whole thousandths of a point, never NaN.
- Fonts come only from the system and from Desk.

Body font sizes from a file have no lower bound. The fixes above bound
what a tiny size could do, and the font size itself is left as the file
gives it.
