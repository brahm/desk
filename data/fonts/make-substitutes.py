#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Makes the fonts that stand for the fonts of the Pages templates (DOC-05).

Each face is made from a face of a free font of a similar design, under
the SIL Open Font License, and is given the advance widths, the kerning
and the vertical metrics that Pages '09 lays the original out with, as
measured on the reference machine (the folders beside this script, one
per family). Each glyph is scaled so that its capitals are as tall as
those of the original and it is as wide as its measured advance, or
centered in it when that would stretch it much. Only the
characters that were measured are kept, so text in another script falls
to another font. The fonts are renamed, as the license asks of modified
versions, and stay under the SIL Open Font License.

    python3 make-substitutes.py SOURCES_DIR [OUT_DIR]

SOURCES_DIR holds one folder per source family, as in the `ofl` folder of
https://github.com/google/fonts (cabin, lato, jost, baskervville,
ebgaramond, bodonimoda, cormorantgaramond, zillaslab, kalam), with its
font files and its OFL.txt. OUT_DIR defaults to the folder of this
script: each family goes to its own folder there, with the license of its
source. Needs fontTools.
"""
import shutil
import sys
from pathlib import Path

from fontTools import subset
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.misc.roundTools import otRound
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

# Each family: the folder of its measured faces, the name of the font that
# stands for it, the folder of its source, the name of the source, and its
# faces: (measured face, source file, place on the axes of a variable
# source, style name, weight, italic). The weights are those that Desk
# reads from the PostScript names of the originals. The condensed faces
# that Desk cannot tell from another face of the same weight are left out.
FAMILIES = [
    (
        "gill-sans",
        "Desk Humanist",
        "cabin",
        "Cabin",
        [
            ("light", "Cabin[wdth,wght].ttf", {"wght": 400, "wdth": 100}, "Light", 300, False),
            (
                "light-italic",
                "Cabin-Italic[wdth,wght].ttf",
                {"wght": 400, "wdth": 100},
                "Light Italic",
                300,
                True,
            ),
            ("regular", "Cabin[wdth,wght].ttf", {"wght": 400, "wdth": 100}, "Regular", 400, False),
            (
                "italic",
                "Cabin-Italic[wdth,wght].ttf",
                {"wght": 400, "wdth": 100},
                "Italic",
                400,
                True,
            ),
            (
                "semibold",
                "Cabin[wdth,wght].ttf",
                {"wght": 600, "wdth": 100},
                "SemiBold",
                600,
                False,
            ),
            (
                "semibold-italic",
                "Cabin-Italic[wdth,wght].ttf",
                {"wght": 600, "wdth": 100},
                "SemiBold Italic",
                600,
                True,
            ),
            ("bold", "Cabin[wdth,wght].ttf", {"wght": 700, "wdth": 100}, "Bold", 700, False),
            (
                "bold-italic",
                "Cabin-Italic[wdth,wght].ttf",
                {"wght": 700, "wdth": 100},
                "Bold Italic",
                700,
                True,
            ),
            (
                "ultrabold",
                "Cabin[wdth,wght].ttf",
                {"wght": 700, "wdth": 100},
                "UltraBold",
                800,
                False,
            ),
        ],
    ),
    (
        "optima",
        "Desk Flare",
        "lato",
        "Lato",
        [
            ("regular", "Lato-Regular.ttf", None, "Regular", 400, False),
            ("italic", "Lato-Italic.ttf", None, "Italic", 400, True),
            ("bold", "Lato-Bold.ttf", None, "Bold", 700, False),
            ("bold-italic", "Lato-BoldItalic.ttf", None, "Bold Italic", 700, True),
            ("extrablack", "Lato-Black.ttf", None, "ExtraBlack", 900, False),
        ],
    ),
    (
        "futura",
        "Desk Geometric",
        "jost",
        "Jost",
        [
            ("medium", "Jost[wght].ttf", {"wght": 500}, "Medium", 500, False),
            ("medium-italic", "Jost-Italic[wght].ttf", {"wght": 500}, "Medium Italic", 500, True),
            ("bold", "Jost[wght].ttf", {"wght": 700}, "Bold", 700, False),
            # The only face of weight 800, so Desk finds it by its weight.
            (
                "condensed-extrabold",
                "Jost[wght].ttf",
                {"wght": 800},
                "Condensed ExtraBold",
                800,
                False,
            ),
        ],
    ),
    (
        "baskerville",
        "Desk Transitional",
        "baskervville",
        "Baskervville",
        [
            ("regular", "Baskervville[wght].ttf", {"wght": 400}, "Regular", 400, False),
            ("italic", "Baskervville-Italic[wght].ttf", {"wght": 400}, "Italic", 400, True),
            ("semibold", "Baskervville[wght].ttf", {"wght": 600}, "SemiBold", 600, False),
            (
                "semibold-italic",
                "Baskervville-Italic[wght].ttf",
                {"wght": 600},
                "SemiBold Italic",
                600,
                True,
            ),
            ("bold", "Baskervville[wght].ttf", {"wght": 700}, "Bold", 700, False),
            ("bold-italic", "Baskervville-Italic[wght].ttf", {"wght": 700}, "Bold Italic", 700, True),
        ],
    ),
    (
        "hoefler-text",
        "Desk Oldstyle",
        "ebgaramond",
        "EB Garamond",
        [
            ("regular", "EBGaramond[wght].ttf", {"wght": 400}, "Regular", 400, False),
            ("italic", "EBGaramond-Italic[wght].ttf", {"wght": 400}, "Italic", 400, True),
            ("black", "EBGaramond[wght].ttf", {"wght": 800}, "Black", 900, False),
            ("black-italic", "EBGaramond-Italic[wght].ttf", {"wght": 800}, "Black Italic", 900, True),
        ],
    ),
    (
        "didot",
        "Desk Didone",
        "bodonimoda",
        "Bodoni Moda",
        [
            ("regular", "BodoniModa[opsz,wght].ttf", {"wght": 400, "opsz": 28}, "Regular", 400, False),
            (
                "italic",
                "BodoniModa-Italic[opsz,wght].ttf",
                {"wght": 400, "opsz": 28},
                "Italic",
                400,
                True,
            ),
            ("bold", "BodoniModa[opsz,wght].ttf", {"wght": 700, "opsz": 28}, "Bold", 700, False),
        ],
    ),
    (
        "cochin",
        "Desk Engraver",
        "cormorantgaramond",
        "Cormorant Garamond",
        [
            ("regular", "CormorantGaramond[wght].ttf", {"wght": 500}, "Regular", 400, False),
            ("italic", "CormorantGaramond-Italic[wght].ttf", {"wght": 500}, "Italic", 400, True),
            ("bold", "CormorantGaramond[wght].ttf", {"wght": 700}, "Bold", 700, False),
            (
                "bold-italic",
                "CormorantGaramond-Italic[wght].ttf",
                {"wght": 700},
                "Bold Italic",
                700,
                True,
            ),
        ],
    ),
    (
        "american-typewriter",
        "Desk Typewriter",
        "zillaslab",
        "Zilla Slab",
        [
            ("light", "ZillaSlab-Light.ttf", None, "Light", 300, False),
            ("regular", "ZillaSlab-Regular.ttf", None, "Regular", 400, False),
            ("semibold", "ZillaSlab-SemiBold.ttf", None, "SemiBold", 600, False),
            ("bold", "ZillaSlab-Bold.ttf", None, "Bold", 700, False),
        ],
    ),
    (
        "marker-felt",
        "Desk Marker",
        "kalam",
        "Kalam",
        [
            # Desk reads MarkerFelt-Thin as weight 100 and MarkerFelt-Wide
            # as the regular weight.
            ("thin", "Kalam-Regular.ttf", None, "Thin", 100, False),
            ("wide", "Kalam-Bold.ttf", None, "Wide", 400, False),
        ],
    ),
]

# Tables that hold hinting, variations or data in the units of the source,
# which do not hold for the scaled glyphs.
DROPPED = [
    "GSUB", "GPOS", "GDEF", "kern", "hdmx", "VDMX", "LTSH", "fpgm", "prep", "cvt ", "gasp",
    "DSIG", "STAT", "avar", "fvar", "gvar", "HVAR", "MVAR", "meta", "vhea", "vmtx", "BASE",
    "FFTM", "PfEd", "TSI0", "TSI1", "TSI2", "TSI3", "TSI5", "prop",
]  # fmt: skip


# The families whose figures are old style in the original.
OLD_STYLE_FIGURES = {"hoefler-text"}

# How much wider than its own shape a glyph may be drawn. A glyph whose
# advance in the original is wider still keeps this width and is centered,
# so that its strokes do not grow thick.
STRETCH = 1.15

# Characters that a source may lack and the one that is drawn the same.
SAME = {0x2010: 0x2D, 0x2011: 0x2D, 0xAD: 0x2D, 0x2015: 0x2014, 0x2024: 0x2E}

# The ligatures that Pages joins, drawn as their letters side by side
# when a source has no glyph for them.
LIGATURES = {0xFB01: "fi", 0xFB02: "fl"}


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


def substitutions(font, tag):
    """What a feature of the source replaces, glyph by glyph."""
    found = {}
    if "GSUB" not in font:
        return found
    table = font["GSUB"].table
    for record in table.FeatureList.FeatureRecord:
        if record.FeatureTag == tag:
            for index in record.Feature.LookupListIndex:
                for sub in table.LookupList.Lookup[index].SubTable:
                    if sub.LookupType == 7:
                        sub = sub.ExtSubTable
                    if sub.LookupType == 1:
                        found.update(sub.mapping)
    return found


def choose_figures(font, advances, oldstyle):
    """Maps the digits to the figures of the source that are like those of
    the original: old style or lining, and all of one width when those of
    the original are."""
    steps = [substitutions(font, "onum" if oldstyle else "lnum")]
    if len({advances.get(code) for code in range(0x30, 0x3A)}) == 1:
        steps.append(substitutions(font, "tnum"))
    for table in font["cmap"].tables:
        if table.isUnicode():
            for code in range(0x30, 0x3A):
                glyph = table.cmap.get(code)
                for step in steps:
                    glyph = step.get(glyph, glyph)
                if glyph is not None:
                    table.cmap[code] = glyph


def cap_height(font):
    """The height of the capitals of a font: that of its H."""
    glyph = font["glyf"][font.getBestCmap()[ord("H")]]
    return glyph.yMax


def make(src, location, metrics, family, source_name, oldstyle, style, weight, italic, out_dir):
    face, advances, kerning = measured(metrics)
    font = TTFont(src)
    if "fvar" in font:
        font = instancer.instantiateVariableFont(font, location)
    old_copyright = font["name"].getDebugName(0) or ""
    choose_figures(font, advances, oldstyle)

    # Only what was measured stays, with no layout tables of the source.
    options = subset.Options()
    options.layout_features = []
    options.hinting = False
    options.notdef_outline = True
    options.name_IDs = [0]
    options.glyph_names = True
    options.drop_tables += DROPPED
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=sorted(advances))
    subsetter.subset(font)
    for tag in DROPPED:
        if tag in font:
            del font[tag]

    cmap = dict(font.getBestCmap())
    units = int(face["unitsPerEm"])
    # Capitals as tall as those of the original, in its units.
    tall = int(face["capHeight"]) / cap_height(font)
    glyphs = font.getGlyphSet()
    glyf, hmtx = font["glyf"], font["hmtx"]

    # Each glyph of the new font is drawn from glyphs of the source, set
    # side by side: itself, or for a character that the source lacks, the
    # one that looks the same, or the two letters of a ligature.
    parts = {name: [name] for name in font.getGlyphOrder()}
    widths = {}
    for code, glyph in sorted(cmap.items()):
        if code in advances:
            widths.setdefault(glyph, advances[code])
    added = {}
    for code, same in SAME.items():
        if code in advances and code not in cmap and same in cmap:
            added[code] = [cmap[same]]
    for code, letters in LIGATURES.items():
        if code in advances and code not in cmap and all(ord(c) in cmap for c in letters):
            added[code] = [cmap[ord(c)] for c in letters]
    for code, sources in added.items():
        name = f"uni{code:04X}"
        parts[name], widths[name], cmap[code] = sources, advances[code], name
        for table in font["cmap"].tables:
            if table.isUnicode():
                table.cmap[code] = name

    drawn = {}
    for name, sources in parts.items():
        old = sum(hmtx[source][0] for source in sources)
        new = widths.get(name)
        if new is None or old == 0:
            # .notdef and the like keep their shape.
            wide, new = tall, otRound(old * tall)
        else:
            wide = new / old
        shift = 0
        if wide > tall * STRETCH:
            shift = (new - old * tall * STRETCH) / 2
            wide = tall * STRETCH
        pen = TTGlyphPen(None)
        at = 0
        for source in sources:
            recording = DecomposingRecordingPen(glyphs)
            glyphs[source].draw(recording)
            recording.replay(TransformPen(pen, (wide, 0, 0, tall, at * wide + shift, 0)))
            at += hmtx[source][0]
        drawn[name] = (pen.glyph(), new)
    for name, (glyph, advance) in drawn.items():
        glyf[name] = glyph
        glyph.recalcBounds(glyf)
        hmtx[name] = (advance, glyph.xMin if glyph.numberOfContours else 0)
    font.setGlyphOrder(list(parts))

    head, hhea, os2, post = font["head"], font["hhea"], font["OS/2"], font["post"]
    scale = units / head.unitsPerEm
    head.unitsPerEm = units
    for field in (
        "ySubscriptXSize", "ySubscriptYSize", "ySubscriptXOffset", "ySubscriptYOffset",
        "ySuperscriptXSize", "ySuperscriptYSize", "ySuperscriptXOffset", "ySuperscriptYOffset",
        "yStrikeoutSize", "yStrikeoutPosition",
    ):  # fmt: skip
        setattr(os2, field, otRound(getattr(os2, field) * scale))
    ascent = round(float(face["ascent"]))
    descent = round(float(face["descent"]))
    leading = round(float(face["leading"]))
    hhea.ascent, hhea.descent, hhea.lineGap = ascent, -descent, leading
    os2.sTypoAscender, os2.sTypoDescender, os2.sTypoLineGap = ascent, -descent, leading
    os2.usWinAscent, os2.usWinDescent = ascent, descent
    os2.sxHeight = int(face["xHeight"])
    os2.sCapHeight = int(face["capHeight"])
    os2.usWeightClass = weight
    os2.fsSelection &= ~0b1100001
    if italic:
        os2.fsSelection |= 0b1
    if weight == 700:
        os2.fsSelection |= 0b100000
    if not italic and weight != 700:
        os2.fsSelection |= 0b1000000
    head.macStyle = (1 if weight == 700 else 0) | (2 if italic else 0)
    post.italicAngle = float(face.get("italicAngle", 0))
    post.underlinePosition = round(float(face["underlinePosition"]))
    post.underlineThickness = round(float(face["underlineThickness"]))

    # The kerning of the original, and fi and fl joined where it has them.
    rules = "languagesystem DFLT dflt;\nlanguagesystem latn dflt;\n"
    letter = {c: cmap.get(ord(c)) for c in "fil"}
    joined = [
        f"    sub {letter['f']} {letter[second]} by {cmap[code]};"
        for second, code in (("i", 0xFB01), ("l", 0xFB02))
        if code in cmap and letter["f"] and letter[second]
    ]
    if joined:
        rules += "feature liga {\n" + "\n".join(joined) + "\n} liga;\n"
    pairs = [
        f"    pos {cmap[a]} {cmap[b]} {k};"
        for (a, b), k in sorted(kerning.items())
        if a in cmap and b in cmap
    ]
    if pairs:
        rules += "feature kern {\n" + "\n".join(pairs) + "\n} kern;\n"
    addOpenTypeFeaturesFromString(font, rules)

    ps = family.replace(" ", "") + "-" + style.replace(" ", "")
    basic = style in ("Regular", "Italic", "Bold", "Bold Italic")
    names = {
        0: (
            f"{family}: {source_name} with the advance widths, kerning and vertical "
            f"metrics of {face['family']} as Pages '09 lays it out, made by Desk "
            f"contributors in 2026. {old_copyright}"
        ),
        1: family if basic else f"{family} {style.replace(' Italic', '')}",
        2: style if basic else ("Italic" if italic else "Regular"),
        3: f"Desk: {ps}",
        4: f"{family} {style}",
        5: "Version 1.000",
        6: ps,
        13: (
            "This Font Software is licensed under the SIL Open Font License, "
            "Version 1.1. This license is available with a FAQ at: "
            "https://openfontlicense.org"
        ),
        14: "https://openfontlicense.org",
        16: family,
        17: style,
    }
    table = font["name"]
    table.names = []
    for name_id, text in names.items():
        table.setName(text, name_id, 3, 1, 0x409)
        table.setName(text, name_id, 1, 0, 0)
    out = out_dir / f"{ps}.ttf"
    font.save(out)
    return out


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    sources = Path(sys.argv[1])
    here = Path(__file__).resolve().parent
    out_root = Path(sys.argv[2]) if len(sys.argv) > 2 else here
    for folder, family, source, source_name, faces in FAMILIES:
        out_dir = out_root / family.lower().replace(" ", "-")
        out_dir.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(sources / source / "OFL.txt", out_dir / "OFL.txt")
        for metrics, file, location, style, weight, italic in faces:
            out = make(
                sources / source / file,
                location,
                here / folder / f"{metrics}.tsv",
                family,
                source_name,
                folder in OLD_STYLE_FIGURES,
                style,
                weight,
                italic,
                out_dir,
            )
            print(out)


if __name__ == "__main__":
    main()
