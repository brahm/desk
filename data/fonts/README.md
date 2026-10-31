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
| Liberation Sans, Serif, Mono | Arial and Helvetica, Times New Roman and Times, Courier New and Courier | SIL Open Font License 1.1 | <https://github.com/liberationfonts/liberation-fonts> (Debian: `fonts-liberation`) |
| TeX Gyre Heros, Termes, Cursor, Pagella, Bonum, Schola, Adventor, Chorus | Helvetica, Times, Courier, Palatino, Bookman, New Century Schoolbook, Avant Garde, Zapf Chancery | GUST Font License (LPPL 1.3c) | <https://www.gust.org.pl/projects/e-foundry/tex-gyre> (Debian: `fonts-texgyre`) |
| Nimbus Sans, Nimbus Roman, Nimbus Mono PS, P052, URW Bookman, C059, URW Gothic, Z003 | The same 35 PostScript fonts | AGPL 3.0 with the font exception | <https://github.com/ArtifexSoftware/urw-base35-fonts> (Debian: `fonts-urw-base35`) |
| Carlito | Calibri | SIL Open Font License 1.1 | <https://github.com/googlefonts/carlito> (Debian: `fonts-crosextra-carlito`) |
| Caladea | Cambria | SIL Open Font License 1.1 | <https://github.com/huertatipografica/Caladea> (Debian: `fonts-crosextra-caladea`) |
| Gelasio | Georgia | SIL Open Font License 1.1 | <https://github.com/SorkinType/Gelasio> |
| DejaVu Sans, Sans Mono | Menlo (built from DejaVu Sans Mono), and in style Verdana, Lucida Grande and Monaco | Bitstream Vera license and public domain | <https://dejavu-fonts.github.io> (Debian: `fonts-dejavu-core`) |

`ci/setup-env.sh` installs the packaged ones, which the tests use.
