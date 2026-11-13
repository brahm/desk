# Fonts

Documents from iWork '09 name the fonts of the Mac: Helvetica, Times,
Gill Sans, Optima and many more. Apple's fonts may not be copied, so when
a document names a font that the system does not have, Desk lays it out
with a free font that stands for it and says so (DOC-05).
[`substitutes.tsv`](substitutes.tsv) lists them, in order of preference,
and `desk-text` reads it.

The rules:

1. **The font itself, when it is installed.** Desk never replaces a font
   that the system has.
2. **A metric-compatible free font.** It has the same advance widths, so
   lines and pages break where they did on the Mac.
3. **A free font of a similar style**, when no free font has the same
   metrics. The text still flows, but lines can break elsewhere.
4. Otherwise Liberation Sans, Serif or Mono, after the kind of font the
   name suggests.

A free font with the same advance widths still has its own vertical
metrics, ligatures and kerning, so Desk also lays a family out as Pages
does with the original, after [`originals.tsv`](originals.tsv), measured
on the reference machine (DOC-05):

- **The height of the lines** comes from the ascent, descent and leading
  of the original. Apple's own fonts (Helvetica, Times, Courier,
  Palatino) have no line gap, and Pages gives their lines a leading of a
  fifth of their height, over the text: Helvetica at 12 pt has lines
  14 pt tall, Palatino 16 pt. Fonts with a line gap, such as Helvetica
  Neue, have it under the text, and each face of Helvetica Neue has its
  own height: Light at 12 pt has lines 15 pt tall. Pages rounds the
  ascent, the descent and the leading each to a whole point, and a
  multiple of the line spacing to a whole point too.
- **The width of the lines** of a family whose free substitute has the
  widths of another font comes from a spacing between letters: TeX Gyre
  Heros and Nimbus Sans have the widths of Helvetica, and Helvetica Neue
  is about 1% wider in Regular and 3% narrower in Light, as measured on
  the text of a real document.
- **Ligatures** are those of Pages: fi and fl in Times and Palatino, and
  nothing in Courier or in Microsoft's fonts, whatever the substitute
  offers. The PDF keeps the letters a ligature joins as its text.
- **Kerning** is the substitute's. TeX Gyre Pagella kerns other pairs
  than the Palatino of the Mac, which moves a word up to 1.6 pt within a
  line, but lines break in the same places.

Some original fonts can be downloaded legally even though they may not be
redistributed. Microsoft's core fonts for the web (Arial, Times New
Roman, Courier New, Georgia, Verdana, Trebuchet MS and others) are such
fonts: their license allows the original installers to be shared
unchanged, but not the fonts to be bundled with another program. Desk
does not bundle them; it uses them when they are installed, for example
with the `ttf-mscorefonts-installer` package, and the Flatpak can offer
to download them from their original installers, keeping their license.

## The substitutes and their licenses

Every font that Desk bundles is free, and its license is installed with
it, as each license requires.

| Font | Stands for | License | Source |
|---|---|---|---|
| Desk Neue (`desk-neue/`), built from TeX Gyre Heros | Helvetica Neue | GUST Font License (LPPL 1.3c) | This repository, made by `desk-neue/make-desk-neue.py` |
| Desk Humanist, Flare, Geometric, Transitional, Oldstyle, Didone, Engraver, Typewriter and Marker (`desk-*/`) | Gill Sans, Optima, Futura, Baskerville, Hoefler Text, Didot, Cochin, American Typewriter and Marker Felt | SIL Open Font License 1.1 | This repository, made by `make-substitutes.py` from fonts of <https://github.com/google/fonts> |
| Liberation Sans, Serif, Mono | Arial and Helvetica, Times New Roman and Times, Courier New and Courier | SIL Open Font License 1.1 | <https://github.com/liberationfonts/liberation-fonts> (Debian: `fonts-liberation`) |
| TeX Gyre Heros, Termes, Cursor, Pagella, Bonum, Schola, Adventor, Chorus | Helvetica, Times, Courier, Palatino, Bookman, New Century Schoolbook, Avant Garde, Zapf Chancery | GUST Font License (LPPL 1.3c) | <https://www.gust.org.pl/projects/e-foundry/tex-gyre> (Debian: `fonts-texgyre`) |
| Nimbus Sans, Nimbus Roman, Nimbus Mono PS, P052, URW Bookman, C059, URW Gothic, Z003 | The same 35 PostScript fonts | AGPL 3.0 with the font exception | <https://github.com/ArtifexSoftware/urw-base35-fonts> (Debian: `fonts-urw-base35`) |
| Carlito | Calibri | SIL Open Font License 1.1 | <https://github.com/googlefonts/carlito> (Debian: `fonts-crosextra-carlito`) |
| Caladea | Cambria | SIL Open Font License 1.1 | <https://github.com/huertatipografica/Caladea> (Debian: `fonts-crosextra-caladea`) |
| Gelasio | Georgia | SIL Open Font License 1.1 | <https://github.com/SorkinType/Gelasio> |
| DejaVu Sans, Sans Mono | Menlo (built from DejaVu Sans Mono), and in style Verdana, Lucida Grande and Monaco | Bitstream Vera license and public domain | <https://dejavu-fonts.github.io> (Debian: `fonts-dejavu-core`) |

`ci/setup-env.sh` installs the packaged ones, which the tests use. The
Flatpak bundles them from the Ubuntu 24.04 packages, whose checksums the
signed archive index gives, and their copyright files go to
`/app/share/licenses`. Gelasio, which no distribution packages, comes
from its own repository, at the commit that Google Fonts names for it,
with its license. It bundles the
Hunspell dictionaries and hyphenation patterns of American and British
English and Brazilian Portuguese the same way.

## Helvetica Neue metrics

`helvetica-neue/` holds the advance widths, vertical metrics and kerning
of each Helvetica Neue face, as Pages lays them out on the reference
machine (macOS 10.14.4 with iWork '09 9.3), one tab-separated file per
face. `measure.py` took them with CoreText; the files
record only numbers, never the fonts, and serve to build
metric-compatible substitutes from TeX Gyre Heros.

## Metrics of the fonts of the Pages templates

The same script, run as `measure.py "FAMILY" OUTPUT_DIR`, measured every
face installed on the reference machine of the other families that the
Pages '09 templates use, into one folder per family in the same format:
`gill-sans/`, `optima/`, `futura/`, `baskerville/`, `hoefler-text/`,
`didot/`, `cochin/`, `american-typewriter/` and `marker-felt/`. The fonts
below are built from them.

## The fonts that stand for those of the Pages templates

No free font has the widths of these families either, so
`make-substitutes.py` makes one for each from a free font of a similar
design, under the SIL Open Font License:

| Font | Stands for | Made from |
|---|---|---|
| Desk Humanist (`desk-humanist/`) | Gill Sans | Cabin |
| Desk Flare (`desk-flare/`) | Optima | Lato |
| Desk Geometric (`desk-geometric/`) | Futura | Jost |
| Desk Transitional (`desk-transitional/`) | Baskerville | Baskervville |
| Desk Oldstyle (`desk-oldstyle/`) | Hoefler Text | EB Garamond |
| Desk Didone (`desk-didone/`) | Didot | Bodoni Moda |
| Desk Engraver (`desk-engraver/`) | Cochin | Cormorant Garamond |
| Desk Typewriter (`desk-typewriter/`) | American Typewriter | Zilla Slab |
| Desk Marker (`desk-marker/`) | Marker Felt | Kalam |

The sources come from the `ofl` folder of
<https://github.com/google/fonts>. Each face is given the advance widths,
the kerning and the vertical metrics measured above, and each glyph is
scaled so that its capitals are as tall as those of the original and it
is as wide as its measured advance, or centered in that advance when it
would have to stretch by more than 15%. The figures are those of the
source that are like the original's: old style for Hoefler Text, lining
for the others. fi and fl are joined as in the original, drawn as the two
letters side by side when the source has no glyph for them. Only the
characters that were measured are kept, so text in another script falls
to another font.

The outlines are not those of the originals: a page looks different, but
its lines and pages break where they did on the Mac. A weight that the
source lacks is drawn with the nearest one: the light faces of Gill Sans
as the regular one, and its ultra bold as the bold. The condensed faces
that Desk cannot tell from another face of the same weight (Futura
Condensed Medium and the condensed faces of American Typewriter) and the
ornaments of Hoefler Text are left out.

The fonts are renamed, as the license asks of modified versions, and none
uses a reserved font name. Each folder has the license of its source,
`OFL.txt`, with its copyright notice, and the Flatpak installs it.
`desk-text` brings the faces inside the program, and `substitutes.tsv`
puts each first for its family. To make them again:

```sh
python3 data/fonts/make-substitutes.py SOURCES_DIR
```

where `SOURCES_DIR` has the folders `cabin`, `lato`, `jost`,
`baskervville`, `ebgaramond`, `bodonimoda`, `cormorantgaramond`,
`zillaslab` and `kalam` of the `ofl` folder of Google Fonts.

## Desk Neue

No free font has the widths of Helvetica Neue, so Desk makes one:
`desk-neue/make-desk-neue.py` copies each face of TeX Gyre Heros, gives
it the advance widths, kerning and vertical metrics measured above, and
renames it Desk Neue, as the GUST Font License asks of derived works.
Its outlines stay those of TeX Gyre Heros: the light faces are drawn as
the regular one and the medium as the bold, but each lays out as wide as
in Pages, so lines and pages break where they did on the Mac. The
condensed and thin faces are left out, as Desk does not tell them from
the bold and the ultralight. `desk-text` brings the ten faces inside the
program and loads them before the system fonts are consulted, and
`substitutes.tsv` puts Desk Neue first for Helvetica Neue.
