#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Checks docs/backlog/deferred.tsv, the register of what was left undone.

Usage: ci/check-backlog.py [--phase NAME]

Without arguments it checks the form of the file. With --phase it also
fails while a row that moved to that phase (or to `end-of-phase`) is still
open: run it before closing a phase. The rules are in
docs/backlog/README.md.
"""
import csv
import pathlib
import re
import sys

COLUMNS = ['id', 'date', 'item', 'rows', 'task_set', 'reason', 'from_phase', 'to_phase', 'status',
           'closed_by']
STATUSES = {'open', 'done', 'dropped'}
PATH = pathlib.Path(__file__).resolve().parent.parent / 'docs' / 'backlog' / 'deferred.tsv'


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
    for p in problems:
        print(p, file=sys.stderr)
    if problems:
        sys.exit(1)
    print(f'{len(rows)} rows, {sum(r["status"] == "open" for r in rows)} open')


if __name__ == '__main__':
    main(sys.argv[1:])
