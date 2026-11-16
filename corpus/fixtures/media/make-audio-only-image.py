#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Draw the picture that Pages '09 shows for a sound placed in a document.

Pages '09 copies its own speaker icon, AudioOnlyMovie@2x.tiff, into every
document that holds a sound. Rule 3 of D-011 keeps Apple images out of the
repository, so the owner chose on 2026-11-16 to replace that file in the
fixtures with this one: a speaker drawn by the project, with the same name,
size (106 × 106 pixels, 144 dpi) and mode (RGBA) as the original, so the
package stays complete. Desk itself does not read the file.

Usage: make-audio-only-image.py OUT.tiff
"""
import sys

from PIL import Image, ImageDraw

SIZE = 106
SCALE = 4          # draw large, then reduce, for smooth edges
GREY = (94, 92, 100, 255)


def draw():
    big = SIZE * SCALE
    im = Image.new('RGBA', (big, big), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    s = SCALE
    # The body of the speaker: a box and the cone that opens to the right.
    d.rectangle([18 * s, 40 * s, 34 * s, 66 * s], fill=GREY)
    d.polygon([(34 * s, 40 * s), (54 * s, 22 * s), (54 * s, 84 * s), (34 * s, 66 * s)], fill=GREY)
    # Two waves of sound, arcs centred on the mouth of the cone.
    for r in (16, 30):
        box = [(54 - r) * s, (53 - r) * s, (54 + r) * s, (53 + r) * s]
        d.arc(box, -45, 45, fill=GREY, width=6 * s)
    return im.resize((SIZE, SIZE), Image.LANCZOS)


if __name__ == '__main__':
    draw().save(sys.argv[1], format='TIFF', dpi=(144, 144), compression='tiff_lzw')
