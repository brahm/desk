#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Checks docs/backlog/deferred.tsv, the register of what was left undone,
and the notes in the code that point to it.

Usage: ci/check-backlog.py [--phase NAME]

Without arguments it checks the form of the file and every TODO, FIXME,
XXX or HACK in the files that git tracks: each must cite an open row of
the register, as in `TODO(B-012)`, so that a note never outlives the work
it names and an agent never takes a stale note for a task. With --phase it
also fails while a row that moved to that phase (or to `end-of-phase`) is
still open: run it before closing a phase. The rules are in
docs/backlog/README.md.
"""
import csv
import fnmatch
import pathlib
import re
import subprocess
import sys

COLUMNS = ['id', 'date', 'item', 'rows', 'task_set', 'reason', 'from_phase', 'to_phase', 'status',
           'closed_by']
STATUSES = {'open', 'done', 'dropped'}
ROOT = pathlib.Path(__file__).resolve().parent.parent
PATH = ROOT / 'docs' / 'backlog' / 'deferred.tsv'

# A note in the code, with the row it cites when it cites one.
MARKER = re.compile(r'\b(TODO|FIXME|XXX|HACK)\b(?:\((B-\d{3,})\))?')
# Prose may name the markers without being one: the documentation, the
# register itself and this script, which spells them out.
NOT_SCANNED = ['*.md', 'docs/backlog/deferred.tsv', 'ci/check-backlog.py']


def check(rows, phase=None):
    """Returns the list of problems found in the rows of the register."""
    problems, seen = [], set()
    for n, row in enumerate(rows, start=2):       # line 1 is the header
        where = f'line {n} ({row.get("id") or "no id"})'
        if not re.fullmatch(r'B-\d{3,}', row['id'] or ''):
            problems.append(f'{where}: id must be B- and at least three digits')
        if row['id'] in seen:
            problems.append(f'{where}: id used twice')
        seen.add(row['id'])
        if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', row['date'] or ''):
            problems.append(f'{where}: date must be YYYY-MM-DD')
        for col in ('item', 'task_set', 'reason', 'from_phase', 'to_phase'):
            if not (row[col] or '').strip(' -'):
                problems.append(f'{where}: {col} is empty')
        if row['status'] not in STATUSES:
            problems.append(f'{where}: status must be one of {sorted(STATUSES)}')
        elif row['status'] != 'open' and not (row['closed_by'] or '').strip(' -'):
            problems.append(f'{where}: a {row["status"]} row needs closed_by')
        if phase and row['status'] == 'open' and row['to_phase'] in (phase, 'end-of-phase'):
            problems.append(f'{where}: still open, and phase "{phase}" cannot close: {row["item"]}')
    return problems


def tracked_files():
    """Returns the paths, relative to the root, of the files git tracks."""
    out = subprocess.run(['git', 'ls-files', '-z'], cwd=ROOT, capture_output=True, check=True)
    return [name for name in out.stdout.decode().split('\0') if name]


def check_markers(status, files):
    """Returns the notes that cite no row, an unknown row or a closed one.

    `status` maps each id of the register to its status; `files` maps each
    path to its text. A note must cite an open row (rule 5 of
    docs/backlog/README.md), so closing a row fails here until its notes
    are removed in the same change.
    """
    problems = []
    for name, text in files.items():
        for n, line in enumerate(text.splitlines(), start=1):
            for m in MARKER.finditer(line):
                marker, row = m.groups()
                if not row:
                    problems.append(f'{name}:{n}: {marker} cites no backlog row, as in {marker}(B-012)')
                elif row not in status:
                    problems.append(f'{name}:{n}: {marker}({row}) is not in the register')
                elif status[row] != 'open':
                    problems.append(f'{name}:{n}: {marker}({row}) is {status[row]}; remove the note')
    return problems


def read_sources():
    """Reads every tracked text file that may hold notes, by path."""
    files = {}
    for name in tracked_files():
        if any(fnmatch.fnmatch(name, p) or fnmatch.fnmatch(pathlib.PurePath(name).name, p)
               for p in NOT_SCANNED):
            continue
        try:
            text = (ROOT / name).read_text(encoding='utf-8')
        except (OSError, UnicodeDecodeError):
            continue                                # binary, or gone from the tree
        if '\0' not in text:
            files[name] = text
    return files


def main(argv):
    phase = None
    if argv[:1] == ['--phase'] and len(argv) == 2:
        phase = argv[1]
    elif argv:
        sys.exit(__doc__)
    with open(PATH, newline='', encoding='utf-8') as f:
        reader = csv.DictReader(f, delimiter='\t', quoting=csv.QUOTE_NONE)
        if reader.fieldnames != COLUMNS:
            sys.exit(f'{PATH}: the columns must be {COLUMNS}')
        rows = list(reader)
    problems = check(rows, phase)
    problems += check_markers({r['id']: r['status'] for r in rows}, read_sources())
    for p in problems:
        print(p, file=sys.stderr)
    if problems:
        sys.exit(1)
    print(f'{len(rows)} rows, {sum(r["status"] == "open" for r in rows)} open; '
          'every note in the code cites an open row')


if __name__ == '__main__':
    main(sys.argv[1:])
