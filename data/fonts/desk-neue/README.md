<!--
SPDX-FileCopyrightText: 2026 Desk contributors
SPDX-License-Identifier: CC0-1.0
-->

# Desk Neue

Desk Neue is a modified version of TeX Gyre Heros 2.004 (2009) by
B. Jackowski and J.M. Nowacki, on behalf of the TeX Users Groups,
distributed under the GUST Font License, an instance of the LaTeX Project
Public License 1.3c (`LICENSES/LPPL-1.3c.txt`). It is not TeX Gyre Heros:
its fonts and files are renamed, as the license asks of derived works.

What changed, by `make-desk-neue.py` in this folder, from the
`texgyreheros-*.otf` files of the Debian package `fonts-texgyre`:

- The advance widths of the characters measured in
  `../helvetica-neue/` are those of Helvetica Neue as Pages '09 lays it
  out, and its kerning replaces the GPOS table.
- The ascent, descent, line gap, x-height and cap height are those of
  Helvetica Neue.
- The names, weights and styles are those of the faces of Desk Neue:
  Regular, Italic, Bold and Bold Italic, and Light, UltraLight and Medium
  with their italics, drawn with the outlines of the regular or the bold.

The outlines are those of TeX Gyre Heros, unchanged. The original is at
<https://www.gust.org.pl/projects/e-foundry/tex-gyre>.
