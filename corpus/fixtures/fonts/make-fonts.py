#!/usr/bin/env python3
"""Makes the test fonts of justify-space-limit.pages (corpus/fixtures/pages).

Each one is a copy of a Liberation 2.1.5 Regular face in which only the
advance width of the space (U+0020) changes. They are renamed, as the
SIL OFL asks of modified versions ("Liberation" is a Reserved Font Name).

    python3 make-fonts.py [LIBERATION_DIR] [OUT_DIR]

LIBERATION_DIR defaults to /usr/share/fonts/truetype/liberation and
OUT_DIR to the folder of this script. Needs fontTools.
"""
import sys
from pathlib import Path

from fontTools.ttLib import TTFont

# (source face, family name, PostScript name, space width in em)
FONTS = [
    ("LiberationSans-Regular.ttf", "DeskSans Space60", "DeskSansSpace60", 0.60),
    ("LiberationSans-Regular.ttf", "DeskSans Space10", "DeskSansSpace10", 0.10),
    ("LiberationMono-Regular.ttf", "DeskMono Space28", "DeskMonoSpace28", 0.278),
]


def main():
    src_dir = Path(sys.argv[1] if len(sys.argv) > 1 else "/usr/share/fonts/truetype/liberation")
    out_dir = Path(sys.argv[2] if len(sys.argv) > 2 else Path(__file__).resolve().parent)
    for src, family, ps, em in FONTS:
        f = TTFont(src_dir / src)
        upm = f["head"].unitsPerEm
        space = f.getBestCmap()[32]
        old = f["hmtx"][space]
        f["hmtx"][space] = (round(em * upm), old[1])
        # The Mono copy is no longer monospaced.
        if "Mono" in src:
            f["post"].isFixedPitch = 0
        for r in f["name"].names:
            if r.nameID in (1, 4):
                r.string = family
            elif r.nameID == 6:
                r.string = ps
            elif r.nameID == 3:
                r.string = ps + " desk test"
            elif r.nameID == 2:
                r.string = "Regular"
        f["name"].names = [r for r in f["name"].names if r.nameID not in (16, 17)]
        # Keep head.modified from the source, so the output is the same every time.
        f.recalcTimestamp = False
        f.save(out_dir / f"{ps}.ttf")
        print(f"{ps}.ttf: space {old[0] / upm:.3f} em -> {em} em")


if __name__ == "__main__":
    main()
