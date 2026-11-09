# -*- coding: utf-8 -*-
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Measures the Helvetica Neue faces installed with macOS, using CoreText,
# and writes one tab-separated file per face. It reads only metrics: no
# outline or font file is copied. Desk uses the numbers to build
# metric-compatible substitutes from TeX Gyre Heros.
#
# Run it on the reference machine (macOS Mojave with iWork '09) with the
# system Python, which ships PyObjC:
#
#     /usr/bin/python measure.py OUTPUT_DIR
#
# It also prints the width of a check string set in Helvetica Neue Italic
# 9 pt, to compare with the layout of Pages.
#
# File format: lines starting with "#" are comments; every other line is a
# record whose first field names its type:
#
#     face     KEY  VALUE          names and vertical metrics (font units)
#     advance  U+XXXX  WIDTH       advance width in font units
#     kern     U+XXXX  U+XXXX  K   kerning of the pair, in font units
#
# Kerning is measured with CTLine at 1000 pt as the typographic width of
# the pair minus the widths of its two characters alone, with ligatures
# turned off, and rounded to font units. Only nonzero pairs are written.

from __future__ import print_function

import io
import os
import re
import sys

import CoreText as CT

try:
    unichr
except NameError:  # Python 3
    unichr = chr

FAMILY = u"Helvetica Neue"
KERN_SIZE = 1000.0

ADVANCE_CHARS = (
    list(range(0x20, 0x180))
    + list(range(0x2010, 0x2027))
    + [0x2030, 0x2039, 0x203A, 0x20AC, 0x2122, 0xFB01, 0xFB02]
)
# U+00A0 is already inside U+0020..U+017F.
KERN_CHARS = list(range(0x20, 0x7F))

HEADER = u"""\
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: CC0-1.0
#
# Metrics of {ps} ({family} {style}),
# measured with CoreText on the Desk reference machine (macOS {macos}, iWork '09
# 9.3) by measure.py in this folder. Only numbers are recorded here, never the font itself.
#
# Records: face KEY VALUE | advance U+XXXX WIDTH | kern U+XXXX U+XXXX KERN
# All lengths are in font units ({upem} per em). Kerning is the CTLine
# width of the pair at 1000 pt minus the widths of each character alone,
# with ligatures off; only nonzero pairs are listed.
"""


def faces():
    desc = CT.CTFontDescriptorCreateWithAttributes(
        {CT.kCTFontFamilyNameAttribute: FAMILY})
    found = CT.CTFontDescriptorCreateMatchingFontDescriptors(desc, None) or []
    out = []
    for d in found:
        ps = CT.CTFontDescriptorCopyAttribute(d, CT.kCTFontNameAttribute)
        style = CT.CTFontDescriptorCopyAttribute(d, CT.kCTFontStyleNameAttribute)
        fam = CT.CTFontDescriptorCopyAttribute(d, CT.kCTFontFamilyNameAttribute)
        if fam == FAMILY:
            out.append((ps, style))
    return out


def num(x):
    r = round(x)
    if abs(x - r) < 1e-4:
        return u"%d" % r
    return u"%.3f" % x


def line_width(font, text):
    attrs = {CT.kCTFontAttributeName: font, CT.kCTLigatureAttributeName: 0}
    s = CT.CFAttributedStringCreate(None, text, attrs)
    line = CT.CTLineCreateWithAttributedString(s)
    return CT.CTLineGetTypographicBounds(line, None, None, None)[0]


def glyph(font, ch):
    ok, glyphs = CT.CTFontGetGlyphsForCharacters(font, ch, None, len(ch))
    if not ok or glyphs[0] == 0:
        return None
    return glyphs[0]


def measure(ps, style, macos):
    probe = CT.CTFontCreateWithName(ps, 12.0, None)
    upem = CT.CTFontGetUnitsPerEm(probe)
    font = CT.CTFontCreateWithName(ps, float(upem), None)  # 1 pt = 1 unit
    kfont = CT.CTFontCreateWithName(ps, KERN_SIZE, None)
    scale = upem / KERN_SIZE

    rows = []
    rows.append((u"face", u"family", FAMILY))
    rows.append((u"face", u"style", style))
    rows.append((u"face", u"postscript", ps))
    rows.append((u"face", u"full", CT.CTFontCopyFullName(font)))
    rows.append((u"face", u"unitsPerEm", u"%d" % upem))
    rows.append((u"face", u"ascent", num(CT.CTFontGetAscent(font))))
    rows.append((u"face", u"descent", num(CT.CTFontGetDescent(font))))
    rows.append((u"face", u"leading", num(CT.CTFontGetLeading(font))))
    rows.append((u"face", u"xHeight", num(CT.CTFontGetXHeight(font))))
    rows.append((u"face", u"capHeight", num(CT.CTFontGetCapHeight(font))))
    rows.append((u"face", u"italicAngle", num(CT.CTFontGetSlantAngle(font))))
    rows.append((u"face", u"underlinePosition",
                 num(CT.CTFontGetUnderlinePosition(font))))
    rows.append((u"face", u"underlineThickness",
                 num(CT.CTFontGetUnderlineThickness(font))))

    count = 0
    for cp in ADVANCE_CHARS:
        g = glyph(font, unichr(cp))
        if g is None:
            continue
        total, _ = CT.CTFontGetAdvancesForGlyphs(
            font, CT.kCTFontOrientationHorizontal, [g], None, 1)
        rows.append((u"advance", u"U+%04X" % cp, num(total)))
        count += 1

    single = {}
    for cp in KERN_CHARS:
        single[cp] = line_width(kfont, unichr(cp))
    kerns = 0
    for a in KERN_CHARS:
        for b in KERN_CHARS:
            if a == 0x20 and b == 0x20:
                continue
            w = line_width(kfont, unichr(a) + unichr(b))
            k = int(round((w - single[a] - single[b]) * scale))
            if k:
                rows.append((u"kern", u"U+%04X" % a, u"U+%04X" % b, u"%d" % k))
                kerns += 1

    header = HEADER.format(ps=ps, family=FAMILY, style=style, macos=macos,
                           upem=upem)
    body = u"".join(u"\t".join(r) + u"\n" for r in rows)
    return header + body, count, kerns


def slug(style):
    return re.sub(r"[^a-z0-9]+", u"-", style.lower()).strip(u"-")


def check():
    text = u"Figure 1. The test picture, with a drop"
    font = CT.CTFontCreateWithName(u"HelveticaNeue-Italic", 9.0, None)
    s = CT.CFAttributedStringCreate(None, text, {CT.kCTFontAttributeName: font})
    line = CT.CTLineCreateWithAttributedString(s)
    width = CT.CTLineGetTypographicBounds(line, None, None, None)[0]
    i = text.index(u"drop")
    x0 = CT.CTLineGetOffsetForStringIndex(line, i, None)[0]
    x1 = CT.CTLineGetOffsetForStringIndex(line, i + 4, None)[0]
    print(u"check: %r in HelveticaNeue-Italic 9 pt: width %.3f pt; "
          u"\"drop\" from %.3f to %.3f (at x 139: %.2f to %.2f)"
          % (text, width, x0, x1, 139 + x0, 139 + x1))


def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else u"."
    if not os.path.isdir(outdir):
        os.makedirs(outdir)
    import platform
    macos = platform.mac_ver()[0]
    for ps, style in faces():
        text, count, kerns = measure(ps, style, macos)
        path = os.path.join(outdir, slug(style) + u".tsv")
        with io.open(path, u"w", encoding=u"utf-8", newline=u"\n") as f:
            f.write(text)
        print(u"%s\t%s\t%d advances\t%d kerning pairs" % (path, ps, count, kerns))
    check()


if __name__ == u"__main__":
    main()
