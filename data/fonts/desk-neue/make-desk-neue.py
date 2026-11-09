#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Makes Desk Neue, the fonts that stand for Helvetica Neue (DOC-05).

Each face is a copy of a face of TeX Gyre Heros 2.004 whose advance
widths, kerning and vertical metrics are those that Pages '09 lays
Helvetica Neue out with, as measured on the reference machine
(../helvetica-neue/*.tsv). Its outlines are those of TeX Gyre Heros: the
lighter faces are drawn as the regular one and the medium as the bold.
The fonts are renamed and their files too, as the GUST Font License asks
of derived works.

    python3 make-desk-neue.py [TEX_GYRE_DIR] [OUT_DIR]

TEX_GYRE_DIR defaults to /usr/share/texmf/fonts/opentype/public/tex-gyre
and OUT_DIR to the folder of this script. Needs fontTools.
"""
import sys
from pathlib import Path

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.ttLib import TTFont

# (measured face, TeX Gyre Heros face, style name, weight, italic)
FACES = [
    ("regular", "regular", "Regular", 400, False),
    ("italic", "italic", "Italic", 400, True),
    ("bold", "bold", "Bold", 700, False),
    ("bold-italic", "bolditalic", "Bold Italic", 700, True),
    ("light", "regular", "Light", 300, False),
    ("light-italic", "italic", "Light Italic", 300, True),
    ("ultralight", "regular", "UltraLight", 100, False),
    ("ultralight-italic", "italic", "UltraLight Italic", 100, True),
    ("medium", "bold", "Medium", 500, False),
    ("medium-italic", "bolditalic", "Medium Italic", 500, True),
]

FAMILY = "Desk Neue"


def measured(path):
    """The face values, advances and kerning pairs of a measured face."""
    face, advances, kerning = {}, {}, {}
    for line in path.read_text().splitlines():
        if line.startswith("#") or not line.strip():
            continue
        fields = line.split("\t")
        if fields[0] == "face":
            face[fields[1]] = fields[2]
        elif fields[0] == "advance":
            advances[int(fields[1][2:], 16)] = int(fields[2])
        elif fields[0] == "kern":
            kerning[(int(fields[1][2:], 16), int(fields[2][2:], 16))] = int(fields[3])
    return face, advances, kerning


def make(src, metrics, style, weight, italic, out_dir):
    face, advances, kerning = measured(metrics)
    font = TTFont(src)
    assert font["head"].unitsPerEm == int(face["unitsPerEm"]) == 1000
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]
    for code, width in advances.items():
        glyph = cmap.get(code)
        if glyph is not None:
            hmtx[glyph] = (width, hmtx[glyph][1])
    # The kerning of Helvetica Neue in place of that of TeX Gyre Heros; the
    # other features of GPOS are of scripts Pages '09 documents rarely use.
    del font["GPOS"]
    pairs = [
        f"    pos {cmap[a]} {cmap[b]} {k};"
        for (a, b), k in sorted(kerning.items())
        if a in cmap and b in cmap
    ]
    addOpenTypeFeaturesFromString(
        font,
        "languagesystem DFLT dflt;\nlanguagesystem latn dflt;\n"
        "feature kern {\n" + "\n".join(pairs) + "\n} kern;\n",
    )
    ascent = round(float(face["ascent"]))
    descent = round(float(face["descent"]))
    leading = round(float(face["leading"]))
    hhea, os2 = font["hhea"], font["OS/2"]
    hhea.ascent, hhea.descent, hhea.lineGap = ascent, -descent, leading
    os2.sTypoAscender, os2.sTypoDescender, os2.sTypoLineGap = ascent, -descent, leading
    os2.usWinAscent, os2.usWinDescent = ascent, descent
    os2.sxHeight = int(face["xHeight"])
    os2.sCapHeight = int(face["capHeight"])
    os2.usWeightClass = weight
    # Regular, italic, bold and bold italic make one family of four; the
    # other weights are faces of it by their weight, in typographic names.
    os2.fsSelection &= ~0b1100001
    if italic:
        os2.fsSelection |= 0b1
    if weight == 700:
        os2.fsSelection |= 0b100000
    if not italic and weight != 700:
        os2.fsSelection |= 0b1000000
    font["head"].macStyle = (1 if weight == 700 else 0) | (2 if italic else 0)
    font["post"].italicAngle = float(face.get("italicAngle", 0))
    ps = FAMILY.replace(" ", "") + "-" + style.replace(" ", "")
    basic = style in ("Regular", "Italic", "Bold", "Bold Italic")
    names = {
        1: FAMILY if basic else f"{FAMILY} {style.replace(' Italic', '')}",
        2: style if basic else ("Italic" if italic else "Regular"),
        3: f"Desk: {ps}",
        4: f"{FAMILY} {style}",
        6: ps,
        16: FAMILY,
        17: style,
    }
    table = font["name"]
    table.names = [r for r in table.names if r.nameID not in (1, 2, 3, 4, 6, 16, 17, 21, 22)]
    for name_id, text in names.items():
        table.setName(text, name_id, 3, 1, 0x409)
        table.setName(text, name_id, 1, 0, 0)
    copyright = (
        "Desk Neue: TeX Gyre Heros with the advance widths, kerning and vertical "
        "metrics of Helvetica Neue as Pages '09 lays it out, made by Desk "
        "contributors in 2026. "
    )
    for record in table.names:
        if record.nameID == 0:
            record.string = copyright + record.toUnicode()
    cff = font["CFF "].cff
    cff.fontNames = [ps]
    top = cff.topDictIndex[0]
    top.FullName = names[4]
    top.FamilyName = FAMILY
    top.Weight = style
    out = out_dir / f"{ps}.otf"
    font.save(out)
    return out


def main():
    src_dir = Path(
        sys.argv[1] if len(sys.argv) > 1 else "/usr/share/texmf/fonts/opentype/public/tex-gyre"
    )
    here = Path(__file__).resolve().parent
    out_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else here
    for metrics, heros, style, weight, italic in FACES:
        out = make(
            src_dir / f"texgyreheros-{heros}.otf",
            here.parent / "helvetica-neue" / f"{metrics}.tsv",
            style,
            weight,
            italic,
            out_dir,
        )
        print(out)


if __name__ == "__main__":
    main()
