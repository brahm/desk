#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Join the rendered scenes with the catalogue: python3 inventory.py OUTDIR TSV JSON [VERSION]

Fails when a scene has a component that the catalogue does not describe, so a
mockup cannot gain a control that nobody documented. With VERSION (A, B or C)
the inventory keeps only the scenes and the components of that version.
"""
import csv
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FIELDS = ('id', 'nome', 'origem', 'funcao', 'iwork09', 'iwork_atual', 'estado', 'atalho')
APPS = ('writer', 'tables', 'slides')
MAIN = [f'{a}-{v}' for a in APPS for v in 'abc']
NOTE = """# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The components of the interface of Desk, one row for each piece of the
# mockups in docs/interface/mockups: what it is, the widget of GTK or
# Libadwaita that draws it, what it does, what it was in iWork '09 and is
# in iWork today, and its state in Desk Writer (existe: already there;
# muda: there in another form; novo: to be written). Made by
# mockups/inventory.py from mockups/catalog.txt and the rendered scenes.
# Tab-separated.
"""


def catalogue(path):
    out, section = {}, ''
    for n, line in enumerate(open(path, encoding='utf-8'), 1):
        line = line.rstrip('\n')
        if line.startswith('## '):
            section = line[3:].strip()
            continue
        if not line.strip() or line.startswith('#'):
            continue
        parts = [p.strip() for p in line.split(' | ')]
        if len(parts) != len(FIELDS):
            sys.exit(f'catalog.txt:{n}: {len(parts)} campos, esperava {len(FIELDS)}')
        entry = dict(zip(FIELDS, parts))
        if entry['id'] in out:
            sys.exit(f'catalog.txt:{n}: id repetido {entry["id"]}')
        if entry['estado'] not in ('existe', 'muda', 'novo'):
            sys.exit(f'catalog.txt:{n}: estado desconhecido {entry["estado"]}')
        entry['secao'] = section
        entry['ordem'] = len(out)
        out[entry['id']] = entry
    return out


def main():
    outdir, tsv_path, json_path = sys.argv[1:4]
    only = sys.argv[4] if len(sys.argv) > 4 else ''
    cat = catalogue(os.path.join(HERE, 'catalog.txt'))
    scenes = []
    for path in sorted(glob.glob(os.path.join(outdir, '*.json'))):
        data = json.load(open(path, encoding='utf-8'))
        if 'items' in data and data.get('app') and only in ('', data['version']):
            scenes.append(data)
    order = {sid: n for n, sid in enumerate(MAIN)}
    scenes.sort(key=lambda s: (order.get(s['id'], 99), s['id']))

    use = {cid: {'gtypes': [], 'shown': [], 'hidden': [], 'versions': set(), 'apps': set()}
           for cid in cat}
    missing = set()
    for scene in scenes:
        for kind in ('items', 'hidden'):
            for item in scene.get(kind, []):
                cid = item['id']
                if cid not in cat:
                    missing.add(cid)
                    continue
                u = use[cid]
                if item['gtype'] not in u['gtypes']:
                    u['gtypes'].append(item['gtype'])
                target = u['shown'] if kind == 'items' else u['hidden']
                if scene['id'] not in target:
                    target.append(scene['id'])
                u['versions'].add(scene['version'])
                if scene['app'] in APPS:
                    u['apps'].add(scene['app'])
    if missing:
        sys.exit('sem linha no catálogo: ' + ', '.join(sorted(missing)))
    undrawn = [cid for cid, u in use.items() if not u['shown'] and not u['hidden']]

    rows = []
    for cid, entry in cat.items():
        u = use[cid]
        if only and cid in undrawn:
            continue                    # a component of another version
        # the pieces drawn only in the common scenes belong to the three apps
        apps = u['apps'] or set(APPS)
        if any(s.startswith('comum-') for s in u['shown'] + u['hidden']) and cid.split('.')[0] in (
                'janela', 'dialogo', 'comandos', 'modelos', 'aviso', 'busca', 'cores', 'fontes'):
            apps = set(APPS)
        rows.append({**entry, 'widget': ', '.join(u['gtypes']) or '-',
                     'versoes': ''.join(v for v in 'ABC' if v in u['versions']) or '-',
                     'apps': ', '.join(a for a in APPS if a in apps),
                     'cenas': ', '.join(u['shown']) or '-',
                     'so_em_aba_oculta': 'sim' if (u['hidden'] and not u['shown']) else
                                         ('fora das cenas' if not u['shown'] else 'não')})
    head = ['id', 'secao', 'nome', 'origem', 'widget', 'funcao', 'iwork09', 'iwork_atual', 'estado',
            'atalho', 'versoes', 'apps', 'cenas', 'so_em_aba_oculta']
    if only:
        head.remove('versoes')
    with open(tsv_path, 'w', encoding='utf-8', newline='') as f:
        f.write(NOTE)
        w = csv.writer(f, delimiter='\t', lineterminator='\n', quoting=csv.QUOTE_NONE, quotechar=None)
        w.writerow(head)
        for r in rows:
            w.writerow([r[h] for h in head])
    with open(json_path, 'w', encoding='utf-8') as f:
        json.dump({'components': [{h: r[h] for h in head} for r in rows],
                   'scenes': [{k: s[k] for k in ('id', 'title', 'app', 'version', 'dark', 'note',
                                                 'width', 'height', 'scale', 'items')}
                              for s in scenes]}, f, ensure_ascii=False, separators=(',', ':'))
    states = {}
    for r in rows:
        states[r['estado']] = states.get(r['estado'], 0) + 1
    print(f'{len(rows)} componentes, {len(scenes)} cenas; estado: {states}')
    if undrawn and not only:
        print('no catálogo, fora das cenas:', ', '.join(undrawn))


if __name__ == '__main__':
    main()
