#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Render the mockups: python3 render.py OUTDIR [scene-prefix ...]

The scenes are written in English. DESK_MOCKUP_LANG=pt draws them in
Brazilian Portuguese, for the Portuguese page of the site (see translate.py).
"""
import importlib
import os
import sys

import kit


def main():
    outdir = sys.argv[1]
    only = sys.argv[2:]
    kit.init()
    if os.environ.get('DESK_MOCKUP_LANG', 'en') == 'pt':
        import translate
        translate.install('pt_BR.tsv')
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
