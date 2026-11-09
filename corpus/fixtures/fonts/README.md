# Test fonts

Fonts that a document in `../pages` uses and that are not on the
reference machine. Install them before opening that document in Pages
or in Desk; the PDF next to the document already embeds them. The tests
of `desk-testkit` load them from here without installing them.

| File | Family | What changes | Used by |
|---|---|---|---|
| `DeskSansSpace60.ttf` | DeskSans Space60 | Liberation Sans Regular with a space 0.6 em wide (was 0.278 em) | `justify-space-limit.pages` |
| `DeskSansSpace10.ttf` | DeskSans Space10 | Liberation Sans Regular with a space 0.1 em wide | `justify-space-limit.pages` |
| `DeskMonoSpace28.ttf` | DeskMono Space28 | Liberation Mono Regular with a space 0.278 em wide (was 0.6 em), so no longer monospaced | `justify-space-limit.pages` |

Only the advance width of the space differs from Liberation 2.1.5; the
glyphs, kerning and metrics are the same. They separate the width of the
space from the size of the font in justified lines: Pages widens each
space by at most 0.5 em, whatever the width of the space.

`make-fonts.py` makes them again from the Liberation fonts (needs
fontTools). Its output differs from these files only in the dates in the
`head` table.

The fonts are modified versions of the Liberation fonts, under the SIL
Open Font License 1.1 (`LICENSES/OFL-1.1.txt`). They have new names
because "Liberation" is a Reserved Font Name.
