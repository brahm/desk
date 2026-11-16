#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Checks that every desk: extension of the ODF code is defined in the docs.

Usage: ci/check-extensions.py

Rule 5 of docs/odf-extensions.md asks that each extension be documented
before it is merged. This script reads every `desk:name` that the sources
of crates/desk-odf write or name, and every name that a heading of
docs/odf-extensions.md defines (`#### `desk:a`, `desk:b``), and fails when
one side has a name the other lacks: an extension without a definition,
or a definition of an extension that no longer exists (backlog B-008).
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = ROOT / 'crates' / 'desk-odf' / 'src'
CATALOGUE = ROOT / 'docs' / 'odf-extensions.md'
# A desk: name: lowercase words joined by hyphens. The namespace
# declaration (`xmlns:desk`) and the prefix alone are not names.
NAME = re.compile(r'desk:([a-z][a-z0-9]*(?:-[a-z0-9]+)*)')


def names_in_sources():
    """The extension names that the Rust sources of desk-odf mention."""
    names = set()
    for path in SOURCES.glob('*.rs'):
        names.update(NAME.findall(path.read_text(encoding='utf-8')))
    return names


def names_defined():
    """The extension names that the headings of the catalogue define."""
    names = set()
    for line in CATALOGUE.read_text(encoding='utf-8').splitlines():
        if line.startswith('#### '):
            names.update(NAME.findall(line))
    return names


def main():
    used, defined = names_in_sources(), names_defined()
    problems = [f'desk:{n} is used in crates/desk-odf/src but not defined in '
                f'docs/odf-extensions.md' for n in sorted(used - defined)]
    problems += [f'desk:{n} is defined in docs/odf-extensions.md but not used in '
                 f'crates/desk-odf/src' for n in sorted(defined - used)]
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        return 1
    print(f'All {len(used)} desk: extensions are defined.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
