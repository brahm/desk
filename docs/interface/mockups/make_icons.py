#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Symbolic icons that Adwaita 51 does not ship and the mockups need.

Each one is a 16x16 monochrome drawing in the symbolic style of the GNOME HIG
(2 px strokes, no perspective). They stand for icons Desk has to draw or take
from the Icon Dev Kit; the names are the ones the component inventory uses.
"""
import math
import os
import re

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'icons/hicolor/scalable/actions')
F = '#222222'

def svg(body):
    return ('<?xml version="1.0" encoding="UTF-8"?>\n'
            '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">'
            f'<g fill="{F}">{body}</g></svg>\n')


def _rrect(x, y, w, h, rx):
    rx = max(0, min(rx, w / 2, h / 2))
    if rx == 0:
        return f'M{x:g} {y:g}h{w:g}v{h:g}h{-w:g}z'
    return (f'M{x + rx:g} {y:g}h{w - 2 * rx:g}a{rx:g} {rx:g} 0 0 1 {rx:g} {rx:g}v{h - 2 * rx:g}'
            f'a{rx:g} {rx:g} 0 0 1 {-rx:g} {rx:g}h{-(w - 2 * rx):g}a{rx:g} {rx:g} 0 0 1 {-rx:g} {-rx:g}'
            f'v{-(h - 2 * rx):g}a{rx:g} {rx:g} 0 0 1 {rx:g} {-rx:g}z')


def r(x, y, w, h, rx=0):
    """A filled rectangle."""
    return f'<path d="{_rrect(x, y, w, h, rx)}"/>'


def ro(x, y, w, h, rx=1, sw=2):
    """The outline of a rectangle, `sw` thick, as a ring."""
    return (f'<path fill-rule="evenodd" d="{_rrect(x, y, w, h, rx + sw / 2)}'
            f'{_rrect(x + sw, y + sw, w - 2 * sw, h - 2 * sw, max(0, rx - sw / 2))}"/>')


def _circle(cx, cy, rad):
    return f'M{cx - rad:g} {cy:g}a{rad:g} {rad:g} 0 1 0 {2 * rad:g} 0a{rad:g} {rad:g} 0 1 0 {-2 * rad:g} 0z'


def c(cx, cy, rad, fill=True, sw=2):
    if fill:
        return f'<path d="{_circle(cx, cy, rad)}"/>'
    return f'<path fill-rule="evenodd" d="{_circle(cx, cy, rad + sw / 2)}{_circle(cx, cy, rad - sw / 2)}"/>'


def _flatten(d):
    """Polylines of a path made of M, L, H, V, Z and their relative forms."""
    toks = re.findall(r'[MmLlHhVvZz]|-?\d*\.?\d+', d)
    polys, cur, i, cmd, x, y = [], [], 0, None, 0.0, 0.0
    while i < len(toks):
        t = toks[i]
        if t.isalpha():
            cmd = t
            i += 1
            if cmd in 'Zz':
                if cur:
                    cur.append(cur[0])
                continue
        if cmd in 'Mm':
            nx, ny = float(toks[i]), float(toks[i + 1])
            i += 2
            x, y = (x + nx, y + ny) if cmd == 'm' else (nx, ny)
            if cur:
                polys.append(cur)
            cur = [(x, y)]
            cmd = 'l' if cmd == 'm' else 'L'
        elif cmd in 'Ll':
            nx, ny = float(toks[i]), float(toks[i + 1])
            i += 2
            x, y = (x + nx, y + ny) if cmd == 'l' else (nx, ny)
            cur.append((x, y))
        elif cmd in 'Hh':
            nx = float(toks[i])
            i += 1
            x = x + nx if cmd == 'h' else nx
            cur.append((x, y))
        elif cmd in 'Vv':
            ny = float(toks[i])
            i += 1
            y = y + ny if cmd == 'v' else ny
            cur.append((x, y))
        else:
            raise ValueError(d)
    if cur:
        polys.append(cur)
    return polys


def p(d, sw=2, fill='none', cap='round', join='round'):
    """A stroked path of straight segments, drawn as the area it covers."""
    out = []
    h = sw / 2
    for poly in _flatten(d):
        if fill != 'none':
            out.append('<path d="M' + 'L'.join(f'{x:g} {y:g}' for x, y in poly) + 'z"/>')
        for (x1, y1), (x2, y2) in zip(poly, poly[1:]):
            dx, dy = x2 - x1, y2 - y1
            n = math.hypot(dx, dy)
            if n == 0:
                continue
            ox, oy = -dy / n * h, dx / n * h
            out.append(f'<path d="M{x1 + ox:g} {y1 + oy:g}L{x2 + ox:g} {y2 + oy:g}'
                       f'L{x2 - ox:g} {y2 - oy:g}L{x1 - ox:g} {y1 - oy:g}z"/>')
        for x, y in poly:
            out.append(f'<path d="{_circle(x, y, h)}"/>')
    return ''.join(out)


ICONS = {
    # insert
    'desk-table': ro(1, 2, 14, 12, 2) + r(2, 6, 12, 2) + r(6, 3, 2, 10),
    'desk-chart': r(2, 8, 3, 6, 0.5) + r(6.5, 2, 3, 12, 0.5) + r(11, 5, 3, 9, 0.5),
    'desk-shape': r(1, 1, 8, 8, 1.5) + c(10, 10, 4, fill=False),
    'desk-textbox': ro(1, 2, 14, 12, 2) + r(5, 5, 6, 1.6) + r(7.2, 5, 1.6, 6),
    'desk-comment': ro(1, 2, 14, 10, 2) + p('M4.5 11v3.2L8 11z', sw=1.6, fill=F),
    'desk-media': ro(1, 2, 14, 12, 2) + c(5.2, 6.2, 1.3) + p('M3 12l3.5-3.5 2.5 2.5 2-2 2 2', sw=1.6),
    # inspector and tools
    'desk-format': p('M9.5 2.5l4 4-5 5-4-4z', sw=2) + p('M5 9.5L2.6 12l1.4 1.4L6.5 11', sw=2),
    'desk-animate': p('M8 2l5 6-5 6-5-6z', sw=2) + r(0.5, 4, 2, 1.5, 0.7) + r(0, 11, 2.5, 1.5, 0.7),
    'desk-function': p('M12 3H4.5L9 8l-4.5 5H12', sw=2),
    'desk-filter': p('M2 3h12l-4.5 5.5V13l-3 1V8.5z', sw=2),
    'desk-sort': p('M4.5 2.5v11M2 5l2.5-2.5L7 5', sw=2) + p('M11.5 13.5v-11M9 11l2.5 2.5L14 11', sw=2),
    'desk-text-color': p('M4 10.5L8 2l4 8.5M5.6 7.5h4.8', sw=1.8) + r(2, 13, 12, 2, 1),
    'desk-highlight': p('M10 2l4 4-6 6H4V8z', sw=1.8) + r(2, 13.4, 12, 1.6, 0.8),
    'desk-line-spacing': r(7, 3, 8, 2, 1) + r(7, 7, 8, 2, 1) + r(7, 11, 8, 2, 1) + p('M3 3.5v9M1.2 5.3L3 3.5l1.8 1.8M1.2 10.7L3 12.5l1.8-1.8', sw=1.5),
    'desk-columns': r(1, 2, 6, 2, 1) + r(1, 5.5, 6, 2, 1) + r(1, 9, 6, 2, 1) + r(1, 12.5, 6, 2, 1)
                    + r(9, 2, 6, 2, 1) + r(9, 5.5, 6, 2, 1) + r(9, 9, 6, 2, 1) + r(9, 12.5, 6, 2, 1),
    'desk-pilcrow': r(7.2, 2, 1.8, 12, 0.6) + r(10.6, 2, 1.8, 12, 0.6) + r(7, 2, 6.5, 1.8, 0.6)
                    + '<path d="M8 2.9v5.6H6.3a2.8 2.8 0 0 1 0-5.6z"/>',
    'desk-character-style': p('M5 12.5L8.2 3h.6l3.2 9.5M6.3 9.5h4.4', sw=1.8) + r(2, 14, 12, 1.4, 0.7),
    'desk-fill': r(2, 2, 12, 12, 2.5),
    'desk-stroke': ro(2, 2, 12, 12, 2.5),
    'desk-shadow': r(1, 1, 10, 10, 2) + p('M13.5 5v8.5H5', sw=2),
    'desk-opacity': r(2, 2, 6, 6) + r(8, 8, 6, 6) + ro(1, 1, 14, 14, 2.5, 1.4),
    'desk-wrap': ro(5, 5, 6, 6, 1, 1.6) + r(1, 1, 14, 1.6, 0.8) + r(1, 13.4, 14, 1.6, 0.8) + r(1, 5, 2.5, 1.6, 0.8) + r(1, 9, 2.5, 1.6, 0.8) + r(12.5, 5, 2.5, 1.6, 0.8) + r(12.5, 9, 2.5, 1.6, 0.8),
    'desk-arrange': r(1, 1, 9, 9, 1.5) + ro(6, 6, 9, 9, 1.5),
    'desk-master': ro(1, 4, 12, 9, 1.5) + p('M4 2h11v8', sw=1.6),
    'desk-notes': ro(1, 2, 14, 12, 2) + r(4, 5.5, 8, 1.5, 0.7) + r(4, 9, 5, 1.5, 0.7),
    'desk-ruler': ro(1, 4, 14, 8, 1.5) + r(4, 5, 1.4, 3) + r(7.3, 5, 1.4, 4.5) + r(10.6, 5, 1.4, 3),
    'desk-outline': r(1, 2, 2, 2, 1) + r(5, 2, 10, 2, 1) + r(4, 7, 2, 2, 1) + r(8, 7, 7, 2, 1) + r(4, 12, 2, 2, 1) + r(8, 12, 7, 2, 1),
    'desk-thumbnails': ro(1, 1, 6, 6, 1.5, 1.6) + ro(9, 1, 6, 6, 1.5, 1.6) + ro(1, 9, 6, 6, 1.5, 1.6) + ro(9, 9, 6, 6, 1.5, 1.6),
    'desk-light-table': r(1, 2, 4, 3, 0.8) + r(6, 2, 4, 3, 0.8) + r(11, 2, 4, 3, 0.8) + r(1, 6.5, 4, 3, 0.8) + r(6, 6.5, 4, 3, 0.8) + r(11, 6.5, 4, 3, 0.8) + r(1, 11, 4, 3, 0.8) + r(6, 11, 4, 3, 0.8),
    'desk-sheet': ro(1, 1, 14, 14, 2) + r(2, 5, 12, 1.6) + r(5, 2, 1.6, 12),
    'desk-borders': ro(1, 1, 14, 14, 1, 2) + r(7.2, 3.5, 1.6, 2) + r(7.2, 7, 1.6, 2) + r(7.2, 10.5, 1.6, 2) + r(3.5, 7.2, 2, 1.6) + r(10.5, 7.2, 2, 1.6),
    'desk-merge-cells': ro(1, 3, 14, 10, 1.5, 1.6) + p('M4 8h8M6 6L4 8l2 2M10 6l2 2-2 2', sw=1.4),
    'desk-number-format': p('M2.5 5.5h11M2.5 10.5h11M6.5 2.5l-1.5 11M11 2.5l-1.5 11', sw=1.7),
    'desk-play-from-start': p('M5 3.5v9l8-4.5z', sw=1.6, fill=F) + r(2, 3, 1.8, 10, 0.9),
    'desk-presenter': ro(1, 2, 14, 9, 1.5) + r(7.2, 11, 1.6, 2.5) + r(4.5, 13, 7, 1.6, 0.8),
    'desk-build-order': r(1, 2, 2.4, 2.4, 1.2) + r(5, 2.2, 10, 2, 1) + r(1, 6.8, 2.4, 2.4, 1.2) + r(5, 7, 10, 2, 1) + r(1, 11.6, 2.4, 2.4, 1.2) + r(5, 11.8, 10, 2, 1),
    'desk-styles': p('M3 13L6.5 3h1L11 13M4.4 9.5h5.2', sw=1.7) + p('M12 6.5v6.5M13.6 11.4L12 13l-1.6-1.6', sw=1.4),
    'desk-page-break': r(1, 1, 14, 4, 1) + r(1, 11, 14, 4, 1) + r(1, 7.3, 2, 1.4) + r(5, 7.3, 2, 1.4) + r(9, 7.3, 2, 1.4) + r(13, 7.3, 2, 1.4),
    'desk-footnote': r(2, 4, 9, 1.6, 0.8) + r(2, 7.5, 9, 1.6, 0.8) + r(2, 12, 6, 1.6, 0.8) + p('M12.5 2.2l1.3-.7V6', sw=1.3),
    'desk-crop': p('M4 1v11h11M1 4h11v11', sw=1.8),
    'desk-wand': p('M3 13l7-7', sw=2) + p('M12 1.5v3M10.5 3h3M14 7v2M13 8h2M6 1.5v2M5 2.5h2', sw=1.2),
    'desk-group': ro(1, 1, 14, 14, 2, 1.2) + r(3.5, 3.5, 5, 5, 1) + c(10, 10, 2.6),
    'desk-zoom-fit': ro(1, 3, 14, 10, 2) + p('M5 6.5L3.5 8 5 9.5M11 6.5L12.5 8 11 9.5', sw=1.4),
    'desk-instant-calc': p('M3 4h10M3 8h10M3 12h6', sw=1.8) + p('M11.5 11v3M10 12.5h3', sw=1.4),
    'desk-category': r(1, 2, 14, 3, 1) + r(4, 7, 11, 2.4, 1) + r(4, 11.4, 11, 2.4, 1),
    'desk-transition': ro(1, 3, 9, 10, 1.5, 1.6) + p('M12 5l3 3-3 3', sw=1.6) + r(9.5, 7.2, 4.5, 1.6),
    'desk-link': p('M6.3 9.7l3.4-3.4', sw=1.6) + p('M7 5L4 8v4h4l3-3', sw=1.6) + p('M9 11l3-3V4H8L5 7', sw=1.6),
    'desk-bookmark': p('M4 2h8v12l-4-3-4 3z', sw=1.8),
    'desk-align-auto': r(1, 2, 8, 2, 1) + r(7, 7, 8, 2, 1) + r(1, 12, 8, 2, 1),
    'desk-valign-top': r(1, 1, 14, 1.6, 0.8) + r(4, 4.6, 8, 2, 1) + r(4, 8.6, 8, 2, 1),
    'desk-valign-middle': r(1, 7.2, 14, 1.6, 0.8) + r(4, 2.6, 8, 2, 1) + r(4, 11.4, 8, 2, 1),
    'desk-valign-bottom': r(1, 13.4, 14, 1.6, 0.8) + r(4, 5.4, 8, 2, 1) + r(4, 9.4, 8, 2, 1),
    'desk-front': r(1, 1, 9, 9, 1.5) + ro(6, 6, 9, 9, 1.5, 1.4),
    'desk-back': ro(1, 1, 9, 9, 1.5, 1.4) + r(7, 7, 8, 8, 1.5),
    'desk-ungroup': ro(1, 1, 6.5, 6.5, 1.5, 1.6) + ro(8.5, 8.5, 6.5, 6.5, 1.5, 1.6),
    'desk-image-adjust': r(1, 3, 14, 1.4, 0.7) + c(5, 3.7, 2) + r(1, 7.3, 14, 1.4, 0.7) + c(11, 8, 2)
                         + r(1, 11.6, 14, 1.4, 0.7) + c(7, 12.3, 2),
    'desk-reflection': r(3, 1, 10, 6, 1.5) + r(3, 9, 10, 1.6, 0.8) + r(4.5, 12, 7, 1.4, 0.7),
    'desk-slide-layout': ro(1, 2, 14, 12, 2) + r(3.5, 4.5, 9, 2, 1) + r(3.5, 8, 4, 3.5, 1) + r(8.5, 8, 4, 3.5, 1),
    'desk-skip-slide': ro(1, 3, 14, 10, 2) + p('M4 11l8-6', sw=1.6),
    'desk-guides': p('M5 1v14M1 11h14', sw=1.4) + ro(7.5, 3, 6, 5.5, 1, 1.4),
    'desk-insert': ro(1, 1, 10, 10, 2, 1.6) + p('M12.5 9.5v6M9.5 12.5h6', sw=1.8),
    'desk-template': ro(1, 1, 14, 14, 2) + r(3.5, 3.5, 9, 3, 0.8) + r(3.5, 8, 4, 4.5, 0.8) + r(9, 8, 3.5, 1.6, 0.8) + r(9, 10.9, 3.5, 1.6, 0.8),
}

def main():
    os.makedirs(OUT, exist_ok=True)
    for name, body in ICONS.items():
        with open(os.path.join(OUT, f'{name}-symbolic.svg'), 'w') as f:
            f.write(svg(body))
    print(len(ICONS), 'icons in', OUT)

if __name__ == '__main__':
    main()
