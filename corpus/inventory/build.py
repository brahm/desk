#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Builds the inventory of the target set on 2026-11-16 (docs/goals.md).

Usage: corpus/inventory/build.py SOURCE_DIR

SOURCE_DIR holds inventario-writer.tsv, inventario-tables.tsv and
inventario-slides.tsv, the inventory of 2026-11-16 made under the rule that
came before ("the better version of each feature"). This script applies
the rules of docs/decisions.md to each row and writes writer.tsv,
tables.tsv and slides.tsv beside itself, in Portuguese like the parity
matrices. It was run once; it is kept so that the mapping can be read and
repeated.

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
COLUMNS = ['id', 'area', 'recurso', 'no_iwork', 'alvo', 'regra', 'escopo', 'situacao_no_desk',
           'revisar', 'fase', 'nota']

# Exports that decision D-019 leaves out, by the id of the old inventory.
EXPORT_OUT = {'S169': 'filme', 'S170': 'GIF animado', 'S171': 'imagens', 'S172': 'HTML',
              'S174': 'formato da versão anterior'}
# Rows that describe export as a whole: D-019 narrows them.
EXPORT_NARROWED = {'W031', 'T033'}

# The rows of decision D-010 and D-019, the same for the three apps:
# (suffix, feature, target, rule, status by app).
FORMATS = [
    ('Abrir ODF do Desk, com as extensões', 'Decisão do projeto', 'D-009',
     {'W': 'Pronto', 'T': 'Ausente', 'S': 'Ausente'}),
    ('Abrir ODF do LibreOffice e do OpenOffice atuais', 'Decisão do projeto', 'D-010',
     {'W': 'Parcial', 'T': 'Ausente', 'S': 'Ausente'}),
    ("Abrir arquivos do iWork '09", 'Decisão do projeto', 'D-010', {'W': 'Pronto', 'T': 'Ausente', 'S': 'Ausente'}),
    ('Abrir arquivos do iWork atual', 'Decisão do projeto', 'D-010, D-011',
     {'W': 'Ausente', 'T': 'Ausente', 'S': 'Ausente'}),
    ('Abrir arquivos do Microsoft Office atual', 'Decisão do projeto', 'D-010',
     {'W': 'Ausente', 'T': 'Ausente', 'S': 'Ausente'}),
    ('Salvar ODF com as extensões do Desk', 'Decisão do projeto', 'D-009',
     {'W': 'Pronto', 'T': 'Ausente', 'S': 'Ausente'}),
    ('Exportar para o Microsoft Office', 'Decisão do projeto', 'D-019', {'W': 'Ausente', 'T': 'Ausente', 'S': 'Ausente'}),
    ('Exportar PDF', 'Decisão do projeto', 'D-019', {'W': 'Parcial', 'T': 'Ausente', 'S': 'Ausente'}),
    ('Exportar EPUB', 'Decisão do projeto', 'D-019', {'W': 'Ausente', 'T': 'Não se aplica', 'S': 'Não se aplica'}),
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
    if scope != 'Dentro':
        return '-'
    if status == 'Pronto' and not review:
        return 'feito'
    return PHASES[row_id[0]][area.split(' ')[0]]


def convert(row):
    """One row of the old inventory as a row of the new one."""
    seen, old_target, status = row['situacao_no_relatorio'], row['alvo'], row['situacao_no_desk']
    review = []
    if seen.startswith('Igual'):
        target, rule = 'Igual nas duas', '-'
    elif seen == 'Só 15':
        target, rule = '15.4', 'D-001'
    elif seen == "Só '09":
        target, rule = "'09, na forma do 15", 'D-002'
    elif old_target in ("'09", 'Melhor de cada'):
        target, rule = "15.4, mais o que o '09 fazia", 'D-001, D-003'
        if status in ('Pronto', 'Parcial', 'Base pronta'):
            review.append('o alvo mudou')
    else:
        target, rule = '15.4', 'D-001'
    scope = {'Descartado': 'Fora: IA, online ou pago (D-004)',
             'Não se aplica': 'Não se aplica ao GNOME (D-005)'}.get(status, 'Dentro')
    note = row['nota']
    if row['id'] in EXPORT_OUT:
        scope, status = 'Fora: exportação não suportada (D-019)', 'Descartado'
    elif row['id'] in EXPORT_NARROWED:
        note = 'D-019: só ODF, Microsoft Office, PDF e EPUB. ' + note
        review.append('D-019')
    if row['area'].startswith('3.1') and scope == 'Dentro':
        review.append('a janela muda (D-008)')
    if scope != 'Dentro':
        status = 'Fora do escopo' if scope.startswith('Fora') else 'Não se aplica'
    return {'id': row['id'], 'area': row['area'], 'recurso': row['recurso'], 'no_iwork': seen,
            'alvo': target, 'regra': rule, 'escopo': scope, 'situacao_no_desk': status,
            'revisar': '; '.join(review) or '-',
            'fase': phase(row['id'], row['area'], scope, status, review), 'nota': note}


def main(source):
    for app, letter in (('writer', 'W'), ('tables', 'T'), ('slides', 'S')):
        with open(pathlib.Path(source) / f'inventario-{app}.tsv', newline='', encoding='utf-8') as f:
            rows = [convert(r) for r in csv.DictReader(f, delimiter='\t', quoting=csv.QUOTE_NONE)]
        last = max(int(r['id'][1:]) for r in rows)
        for n, (feature, target, rule, status) in enumerate(FORMATS, start=1):
            st = status[letter]
            scope = 'Não se aplica ao GNOME (D-005)' if st == 'Não se aplica' else 'Dentro'
            if st == 'Não se aplica':
                scope = 'Não se aplica a este aplicativo'
            rows.append({'id': f'{letter}{last + n:03d}', 'area': 'Compatibilidade de arquivos (D-010, D-019)',
                         'recurso': feature, 'no_iwork': '-', 'alvo': target, 'regra': rule, 'escopo': scope,
                         'situacao_no_desk': st, 'revisar': '-',
                         'fase': '-' if scope != 'Dentro' else 'feito' if st == 'Pronto' else FORMAT_PHASE[letter],
                         'nota': '-'})
        with open(HERE / f'{app}.tsv', 'w', newline='', encoding='utf-8') as f:
            f.write('# SPDX-FileCopyrightText: 2026 Desk contributors\n'
                    '# SPDX-License-Identifier: GPL-3.0-or-later\n')
            w = csv.DictWriter(f, COLUMNS, delimiter='\t', quoting=csv.QUOTE_NONE, lineterminator='\n',
                               escapechar='\\')
            w.writeheader()
            w.writerows(rows)
        print(app, len(rows))


if __name__ == '__main__':
    main(sys.argv[1])
