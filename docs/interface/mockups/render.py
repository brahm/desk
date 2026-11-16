#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Render the mockups: python3 render.py OUTDIR [scene-prefix ...]

The scenes were written with their text in Brazilian Portuguese and are
drawn in English (see english.py); DESK_MOCKUP_LANG=pt draws the original.
"""
import importlib
import os
import sys

import kit


def main():
    outdir = sys.argv[1]
    only = sys.argv[2:]
    kit.init()
    if os.environ.get('DESK_MOCKUP_LANG', 'en') != 'pt':
        import english
        english.install()
    scenes = []
    for mod in ('writer', 'tables', 'slides', 'extras', 'proposals'):
        try:
            m = importlib.import_module(mod)
        except ModuleNotFoundError as e:
            if e.name != mod:
                raise
            continue
        scenes += m.SCENES
    kit.run(scenes, outdir, only)


if __name__ == '__main__':
    main()
