#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Render the mockups: python3 render.py OUTDIR [scene-prefix ...]"""
import importlib
import sys

import kit


def main():
    outdir = sys.argv[1]
    only = sys.argv[2:]
    kit.init()
    scenes = []
    for mod in ('writer', 'tables', 'slides', 'extras', 'novo'):
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
