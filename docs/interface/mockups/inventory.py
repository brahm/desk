#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Join the rendered scenes with the catalogue: python3 inventory.py OUTDIR TSV JSON [VERSION]

Fails when a scene has a component that the catalogue does not describe, so a
mockup cannot gain a control that nobody documented. With VERSION (A, B, C, N,
M or M+) the inventory keeps only the scenes and the components of that
version; run.sh asks for M+, the window that Desk follows (D-008).
"""
import csv
import glob
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FIELDS = ('id', 'name', 'source', 'does', 'iwork09', 'iwork_current', 'state', 'shortcut')
APPS = ('writer', 'tables', 'slides')
MAIN = [f'{a}-{v}' for a in APPS for v in ('m-plus', 'a', 'b', 'c', 'n', 'm')]
VERSIONS = ('A', 'B', 'C', 'N', 'M', 'M+')
NOTE = """# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The components of the window of Desk (M+, decision D-008), one row for
# each piece of the mockups in docs/interface/mockups: what it is, the
# widget of GTK or Libadwaita that draws it, what it does, what it was in
# iWork '09 and is in iWork today, and its state in Desk today (exists:
# already there, even if it does nothing yet; changes: there in another
# form or place; new: to be written). Made by mockups/inventory.py from
# mockups/catalog.txt and the rendered scenes. Tab-separated.
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
            sys.exit(f'catalog.txt:{n}: {len(parts)} fields, expected {len(FIELDS)}')
        entry = dict(zip(FIELDS, parts))
        if entry['id'] in out:
            sys.exit(f'catalog.txt:{n}: repeated id {entry["id"]}')
        if entry['state'] not in ('exists', 'changes', 'new'):
            sys.exit(f'catalog.txt:{n}: unknown state {entry["state"]}')
        entry['section'] = section
        entry['order'] = len(out)
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
        sys.exit('no line in the catalogue: ' + ', '.join(sorted(missing)))
    undrawn = [cid for cid, u in use.items() if not u['shown'] and not u['hidden']]

    rows = []
    for cid, entry in cat.items():
        u = use[cid]
        if only and cid in undrawn:
            continue                    # a component of another version
        # the pieces drawn only in the common scenes belong to the three apps
        apps = u['apps'] or set(APPS)
        if any(s.startswith('common-') for s in u['shown'] + u['hidden']) and cid.split('.')[0] in (
                'window', 'dialog', 'commands', 'templates', 'notice', 'find', 'colors', 'fonts'):
            apps = set(APPS)
        rows.append({**entry, 'widget': ', '.join(u['gtypes']) or '-',
                     'versions': ' '.join(v for v in VERSIONS if v in u['versions']) or '-',
                     'apps': ', '.join(a for a in APPS if a in apps),
                     'scenes': ', '.join(u['shown']) or '-',
                     'only_in_hidden_tab': 'yes' if (u['hidden'] and not u['shown']) else
                                           ('not drawn' if not u['shown'] else 'no')})
    head = ['id', 'section', 'name', 'source', 'widget', 'does', 'iwork09', 'iwork_current', 'state',
            'shortcut', 'versions', 'apps', 'scenes', 'only_in_hidden_tab']
    if only:
        head.remove('versions')
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
        states[r['state']] = states.get(r['state'], 0) + 1
    print(f'{len(rows)} components, {len(scenes)} scenes; state: {states}')
    if undrawn and not only:
        print('in the catalogue, not drawn:', ', '.join(undrawn))


if __name__ == '__main__':
    main()
