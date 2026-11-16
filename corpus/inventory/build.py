#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Builds the inventory of the target set on 2026-11-16 (docs/goals.md).

Usage: corpus/inventory/build.py SOURCE_DIR

SOURCE_DIR holds inventario-writer.tsv, inventario-tables.tsv and
inventario-slides.tsv, the inventory of 2026-11-16 made under the rule that
came before ("the better version of each feature"). This script applies
the rules of docs/decisions.md to each row and writes writer.tsv,
tables.tsv and slides.tsv beside itself. The old inventory is in Portuguese;
translations.tsv gives the English of each area, feature and note, and this
script fails on a text that is not there. It is kept so that the mapping
can be read and repeated.

What each rule does to a row:
  D-001  a feature of both versions takes the behaviour of iWork 15.4
  D-002  a feature only iWork '09 has is kept, in the form of the 15
  D-003  where the old inventory chose the '09 (the 15 lost something or
         uses AI), the target is the 15 plus what the '09 did
  D-004  rows that need AI, a service or a payment are out of scope but kept
  D-010  the five families of files Desk reads (rows added at the end)
  D-019  export only to ODF, Microsoft Office, PDF and EPUB
"""
import csv
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
COLUMNS = ['id', 'area', 'feature', 'in_iwork', 'target', 'rule', 'scope', 'status_in_desk',
           'review', 'phase', 'note']

# The fixed values of the old inventory, in English.
SEEN = {'Igual': 'Equal', 'Igual (15 escolhe a cor)': 'Equal (15 chooses the color)',
        'Igual (nome novo)': 'Equal (new name)', 'Diferente': 'Different',
        'Diferente (lugar)': 'Different (place)', "Só '09": "Only '09", 'Só 15': 'Only 15'}
STATUS = {'Pronto': 'Done', 'Parcial': 'Partial', 'Base pronta': 'Engine ready', 'Ausente': 'Missing',
          'Descartado': 'Discarded', 'Não se aplica': 'Not applicable'}


def load_translations():
    """translations.tsv as {field: {portuguese: english}}."""
    table = {'area': {}, 'feature': {}, 'note': {'-': '-'}}
    with open(HERE / 'translations.tsv', encoding='utf-8') as f:
        for line in f:
            if line.startswith('#') or line.startswith('field\t'):
                continue
            field, portuguese, english = line.rstrip('\n').split('\t')
            table[field][portuguese] = english
    return table


EN = load_translations()

# Exports that decision D-019 leaves out, by the id of the old inventory.
EXPORT_OUT = {'S169': 'movie', 'S170': 'animated GIF', 'S171': 'images', 'S172': 'HTML',
              'S174': 'format of the previous version'}
# Rows that describe export as a whole: D-019 narrows them.
EXPORT_NARROWED = {'W031', 'T033'}

# The rows of decision D-010 and D-019, the same for the three apps:
# (suffix, feature, target, rule, status by app).
FORMATS = [
    ('Open Desk ODF, with the extensions', 'Project decision', 'D-009',
     {'W': 'Done', 'T': 'Missing', 'S': 'Missing'}),
    ('Open ODF from current LibreOffice and OpenOffice', 'Project decision', 'D-010',
     {'W': 'Partial', 'T': 'Missing', 'S': 'Missing'}),
    ("Open iWork '09 files", 'Project decision', 'D-010', {'W': 'Done', 'T': 'Missing', 'S': 'Missing'}),
    ('Open current iWork files', 'Project decision', 'D-010, D-011',
     {'W': 'Missing', 'T': 'Missing', 'S': 'Missing'}),
    ('Open current Microsoft Office files', 'Project decision', 'D-010',
     {'W': 'Missing', 'T': 'Missing', 'S': 'Missing'}),
    ('Save ODF with the Desk extensions', 'Project decision', 'D-009',
     {'W': 'Done', 'T': 'Missing', 'S': 'Missing'}),
    ('Export to Microsoft Office', 'Project decision', 'D-019', {'W': 'Missing', 'T': 'Missing', 'S': 'Missing'}),
    ('Export PDF', 'Project decision', 'D-019', {'W': 'Partial', 'T': 'Missing', 'S': 'Missing'}),
    ('Export EPUB', 'Project decision', 'D-019', {'W': 'Missing', 'T': 'Not applicable', 'S': 'Not applicable'}),
]


# The phase of docs/roadmap.md that owns each area of a report, by the number
# that starts the name of the area. It is a first assignment by area; the
# phase refines it row by row when it starts.
PHASES = {
    'W': {'3.1': 'B1', '3.2': 'W5', '3.3': 'W1', '3.4': 'W1', '3.5': 'W1', '3.6': 'W1', '3.7': 'W3',
          '3.8': 'W4', '3.9': 'W1', '3.10': 'W4', '3.11': 'W2', '3.12': 'W6'},
    'T': {'3.1': 'B1', '3.2': 'T5', '3.3': 'T1', '3.4': 'T1', '3.5': 'T2', '3.6': 'T3', '3.7': 'T2',
          '3.8': 'T4', '3.9': 'T4', '3.10': 'T4', '3.11': 'T3', '3.12': 'T5'},
    'S': {'3.1': 'B1', '3.2': 'S1', '3.3': 'S1', '3.4': 'S1', '3.5': 'S2', '3.6': 'S2', '3.7': 'S2',
          '3.8': 'S2', '3.9': 'S2', '3.10': 'S5', '3.11': 'S5', '3.12': 'S3', '3.13': 'S4',
          '3.14': 'S2', '3.15': 'S5', '3.16': 'S5'},
}
FORMAT_PHASE = {'W': 'W5', 'T': 'T5', 'S': 'S5'}


def phase(row_id, area, scope, status, review):
    """The phase of a row: none when it is out of scope, done when Desk
    already does it and nothing asks for a second look."""
    if scope != 'In':
        return '-'
    if status == 'Done' and not review:
        return 'done'
    return PHASES[row_id[0]][area.split(' ')[0]]


def convert(row):
    """One row of the old inventory as a row of the new one."""
    seen, old_target = SEEN[row['situacao_no_relatorio']], row['alvo']
    status = STATUS[row['situacao_no_desk']]
    review = []
    if seen.startswith('Equal'):
        target, rule = 'Equal in both', '-'
    elif seen == 'Only 15':
        target, rule = '15.4', 'D-001'
    elif seen == "Only '09":
        target, rule = "'09, in the form of the 15", 'D-002'
    elif old_target in ("'09", 'Melhor de cada'):
        target, rule = "15.4, plus what the '09 did", 'D-001, D-003'
        if status in ('Done', 'Partial', 'Engine ready'):
            review.append('the target changed')
    else:
        target, rule = '15.4', 'D-001'
    scope = {'Discarded': 'Out: AI, online or paid (D-004)',
             'Not applicable': 'Not applicable to GNOME (D-005)'}.get(status, 'In')
    note = EN['note'][row['nota']]
    if row['id'] in EXPORT_OUT:
        scope, status = 'Out: unsupported export (D-019)', 'Discarded'
    elif row['id'] in EXPORT_NARROWED:
        note = 'D-019: only ODF, Microsoft Office, PDF and EPUB. ' + note
        review.append('D-019')
    if row['area'].startswith('3.1') and scope == 'In':
        review.append('the window changes (D-008)')
    if scope != 'In':
        status = 'Out of scope' if scope.startswith('Out') else 'Not applicable'
    area = EN['area'][row['area']]
    return {'id': row['id'], 'area': area, 'feature': EN['feature'][row['recurso']], 'in_iwork': seen,
            'target': target, 'rule': rule, 'scope': scope, 'status_in_desk': status,
            'review': '; '.join(review) or '-',
            'phase': phase(row['id'], area, scope, status, review), 'note': note}


def main(source):
    for app, letter in (('writer', 'W'), ('tables', 'T'), ('slides', 'S')):
        with open(pathlib.Path(source) / f'inventario-{app}.tsv', newline='', encoding='utf-8') as f:
            rows = [convert(r) for r in csv.DictReader(f, delimiter='\t', quoting=csv.QUOTE_NONE)]
        last = max(int(r['id'][1:]) for r in rows)
        for n, (feature, target, rule, status) in enumerate(FORMATS, start=1):
            st = status[letter]
            scope = 'Not applicable to this app' if st == 'Not applicable' else 'In'
            rows.append({'id': f'{letter}{last + n:03d}', 'area': 'File compatibility (D-010, D-019)',
                         'feature': feature, 'in_iwork': '-', 'target': target, 'rule': rule, 'scope': scope,
                         'status_in_desk': st, 'review': '-',
                         'phase': '-' if scope != 'In' else 'done' if st == 'Done' else FORMAT_PHASE[letter],
                         'note': '-'})
        with open(HERE / f'{app}.tsv', 'w', newline='', encoding='utf-8') as f:
            # Written in two pieces so that REUSE does not read them as the
            # licence of this script.
            f.write('# SPDX-' 'FileCopyrightText: 2026 Desk contributors\n'
                    '# SPDX-' 'License-Identifier: GPL-3.0-or-later\n')
            w = csv.DictWriter(f, COLUMNS, delimiter='\t', quoting=csv.QUOTE_NONE, lineterminator='\n',
                               escapechar='\\')
            w.writeheader()
            w.writerows(rows)
        print(app, len(rows))


if __name__ == '__main__':
    main(sys.argv[1])
