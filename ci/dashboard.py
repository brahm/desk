#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Desk contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Builds the progress dashboard of Desk from the files of the repository.

Usage: ci/dashboard.py [--out FILE] [--artifact FILE] [--record]
                       [--date YYYY-MM-DD] [--no-prs]

  --out FILE       write the page as a whole HTML document (the website)
  --artifact FILE  write it as a fragment without <html>, <head> and
                   <body>, the form a claude.ai Artifact is published in
  --record         add or replace today's row of docs/dashboard/history.tsv
  --date           the day the page is generated for (default: today in
                   America/Sao_Paulo); for tests
  --no-prs         do not ask GitHub for the pull requests; read the
                   merges of the local git history instead

Every number on the page is read from a file of the repository:

  corpus/inventory/*.tsv        the features, their status and phase
  docs/roadmap.md               the phases and their estimates in days
  docs/dashboard/phases.tsv     which phases started and ended
  docs/dashboard/forecast.tsv   the estimate of the plan, in days
  docs/dashboard/history.tsv    one row per day the dashboard was recorded
  docs/backlog/deferred.tsv     what was left undone

and the recent pull requests come from GitHub (`gh api search/issues`), or
from the merges of the git history when `gh` cannot answer. The page is in
Portuguese, the language of the project owner (D-018). How the numbers are
worked out, and how to update the page, is in docs/dashboard/README.md.
"""
import argparse
import collections
import csv
import datetime
import html
import json
import pathlib
import re
import subprocess
import sys
import zoneinfo

ROOT = pathlib.Path(__file__).resolve().parent.parent
HERE = ROOT / 'docs' / 'dashboard'
REPO = 'brahm/desk'
SITE = 'https://brahm.github.io/desk/'

APPS = [('writer', 'Writer'), ('tables', 'Tables'), ('slides', 'Slides')]

# The states of a feature in scope, in the order the bars stack them, with
# the label the page shows. `done` is the only one that counts as finished.
STATES = [
    ('done', 'Pronto'),
    ('review', 'Pronto, a rever'),
    ('partial', 'Parcial'),
    ('base', 'Base pronta'),
    ('missing', 'Ausente'),
]
STATE_LABEL = dict(STATES) | {'out': 'Fora do escopo', 'na': 'Não se aplica'}
STATE_HELP = {
    'done': 'funciona como o alvo pede e não precisa de nova conferência',
    'review': 'funciona hoje, mas o alvo ou a janela mudou e precisa ser conferido de novo',
    'partial': 'parte do recurso existe',
    'base': 'o motor existe, mas o aplicativo ainda não o usa',
    'missing': 'ainda não começou',
}
PHASE_STATUS = {'not-started': 'Não iniciada', 'in-progress': 'Em andamento', 'done': 'Concluída'}
MONTHS = ['janeiro', 'fevereiro', 'março', 'abril', 'maio', 'junho', 'julho', 'agosto',
          'setembro', 'outubro', 'novembro', 'dezembro']
HISTORY_COLUMNS = ['date', 'in_scope', 'done', 'review', 'partial', 'base', 'missing',
                   'backlog_open', 'forecast']


def read_tsv(path):
    """The rows of a TSV file as dicts, without the `#` lines of its header."""
    with open(path, newline='', encoding='utf-8') as f:
        lines = [line for line in f if not line.startswith('#')]
    return list(csv.DictReader(lines, delimiter='\t', quoting=csv.QUOTE_NONE))


def feature_state(row):
    """The state of one row of the inventory, as a key of STATE_LABEL.

    A feature is `done` only when it is `Pronto` and nothing asks for a
    second look (`revisar` is `-`); `Pronto` with a reason to review is
    `review`, which the roadmap does not count as done.
    """
    if row['escopo'] != 'Dentro':
        return 'out' if row['escopo'].startswith('Fora') else 'na'
    status = row['situacao_no_desk']
    if status == 'Pronto':
        return 'done' if row['revisar'].strip() in ('', '-') else 'review'
    try:
        return {'Parcial': 'partial', 'Base pronta': 'base', 'Ausente': 'missing'}[status]
    except KeyError:
        sys.exit(f'{row["id"]}: unknown situacao_no_desk "{status}" for a row in scope')


def load_features():
    """Every row of the three inventory files, with its app and state."""
    rows = []
    for app, _ in APPS:
        for row in read_tsv(ROOT / 'corpus' / 'inventory' / f'{app}.tsv'):
            rows.append(row | {'app': app, 'state': feature_state(row)})
    return rows


def load_phases(features):
    """The phases of the roadmap table, joined with their status and rows.

    The roadmap gives the id, the name, what the phase delivers and the
    estimate; docs/dashboard/phases.tsv gives the status; the inventory
    gives the rows. A row that is done keeps its phase in the `fase`
    column, so the phase shows its own progress.
    """
    status = {r['phase']: r for r in read_tsv(HERE / 'phases.tsv')}
    row_re = re.compile(r'^\| \*\*(\w+)\*\* (.+?) \| (.+) \| (\d+|-) \| (.+?) \|$')
    phases = []
    for line in (ROOT / 'docs' / 'roadmap.md').read_text(encoding='utf-8').splitlines():
        m = row_re.match(line)
        if not m:
            continue
        pid, name, delivers, planned_rows, estimate = m.groups()
        st = status.get(pid)
        if st is None:
            sys.exit(f'docs/dashboard/phases.tsv has no row for phase {pid} of the roadmap')
        if st['status'] not in PHASE_STATUS:
            sys.exit(f'phase {pid}: status must be one of {sorted(PHASE_STATUS)}')
        counts = collections.Counter(f['state'] for f in features if f['fase'] == pid)
        total = sum(counts[k] for k, _ in STATES)
        if planned_rows != '-' and int(planned_rows) != total:
            print(f'warning: the roadmap gives {planned_rows} rows to {pid}, the inventory has {total}',
                  file=sys.stderr)
        phases.append({
            'id': pid, 'name': name, 'delivers': delivers,
            'days': int(estimate) if estimate.isdigit() else None,
            'status': st['status'], 'started': st['started'], 'ended': st['ended'],
            'counts': counts, 'total': total,
        })
    if not phases:
        sys.exit('docs/roadmap.md: the table of phases was not found')
    unknown = set(status) - {p['id'] for p in phases}
    if unknown:
        sys.exit(f'docs/dashboard/phases.tsv names phases the roadmap does not have: {sorted(unknown)}')
    return phases


def load_prs(use_github):
    """The pull requests changed most recently, newest first.

    Returns (rows, source). GitHub's search is the one call that works both
    in the cloud sessions and in the workflows; without it the merges of
    the local history give the pull requests already merged.
    """
    if use_github:
        try:
            out = subprocess.run(
                ['gh', 'api', '-X', 'GET', 'search/issues', '-f', f'q=repo:{REPO} is:pr',
                 '-f', 'sort=updated', '-f', 'order=desc', '-f', 'per_page=12'],
                capture_output=True, text=True, timeout=60, check=True).stdout
            rows = []
            for item in json.loads(out)['items']:
                merged = (item.get('pull_request') or {}).get('merged_at')
                state = ('merged' if merged else 'draft' if item.get('draft') and item['state'] == 'open'
                         else item['state'])
                rows.append({'number': item['number'], 'title': item['title'], 'state': state,
                             'date': (merged or item['updated_at'])[:10], 'url': item['html_url']})
            if rows:
                return rows, 'GitHub'
            print('warning: GitHub listed no pull request; reading the merges of git', file=sys.stderr)
        except (OSError, subprocess.SubprocessError, ValueError, KeyError) as e:
            print(f'warning: GitHub did not answer ({e}); reading the merges of git', file=sys.stderr)
    out = subprocess.run(['git', '-C', str(ROOT), 'log', '--merges', '-n', '200', '--format=%cs%x09%s%x09%b%x00'],
                         capture_output=True, text=True, check=True).stdout
    rows = []
    for entry in out.split('\0'):
        date, _, rest = entry.strip().partition('\t')
        subject, _, body = rest.partition('\t')
        m = re.match(r'Merge pull request #(\d+) ', subject)
        if m:
            title = body.strip().splitlines()[0] if body.strip() else subject
            rows.append({'number': int(m.group(1)), 'title': title, 'state': 'merged', 'date': date,
                         'url': f'https://github.com/{REPO}/pull/{m.group(1)}'})
    return rows[:12], 'git'


def forecast(phases, today):
    """The forecast of parity, worked out from the plan and the progress.

    The plan (docs/dashboard/forecast.tsv, copied from the roadmap) says
    how many days the whole work takes: least, likely and most. The share
    of the work still to do is measured in the estimated days of the
    phases: a phase that ended counts whole, a phase in progress counts by
    the share of its inventory rows that are done, and the others count
    nothing. Today's forecast is today plus that share of the plan's days.
    """
    plan = {r['key']: r['value'] for r in read_tsv(HERE / 'forecast.tsv')}
    base = datetime.date.fromisoformat(plan['baseline_date'])
    days = {k: int(plan[f'days_{k}']) for k in ('low', 'likely', 'high')}
    total = sum(p['days'] for p in phases if p['days'])
    earned = 0.0
    for p in phases:
        if not p['days']:
            continue
        if p['status'] == 'done':
            earned += p['days']
        elif p['status'] == 'in-progress' and p['total']:
            earned += p['days'] * p['counts']['done'] / p['total']
    left = 1 - earned / total
    add = datetime.timedelta
    return {
        'base': base, 'days': days, 'total_days': total, 'earned_days': earned, 'left': left,
        'plan': {k: base + add(days=v) for k, v in days.items()},
        'now': {k: today + add(days=round(v * left)) for k, v in days.items()},
    }


def record_history(row):
    """Adds the row of today to history.tsv, replacing one of the same day."""
    path = HERE / 'history.tsv'
    head = [line for line in path.read_text(encoding='utf-8').splitlines(keepends=True)
            if line.startswith('#')]
    rows = [r for r in read_tsv(path) if r['date'] != row['date']] + [row]
    rows.sort(key=lambda r: r['date'])
    with open(path, 'w', newline='', encoding='utf-8') as f:
        f.writelines(head)
        w = csv.DictWriter(f, HISTORY_COLUMNS, delimiter='\t', quoting=csv.QUOTE_NONE, lineterminator='\n')
        w.writeheader()
        w.writerows(rows)


# ---------------------------------------------------------------- the page

def esc(text):
    return html.escape(str(text), quote=True)


def num(value, digits=1):
    """A number in the Brazilian form, with a decimal comma."""
    return f'{value:.{digits}f}'.replace('.', ',')


def day(d):
    return d.strftime('%d/%m/%Y')


def long_day(d):
    return f'{d.day} de {MONTHS[d.month - 1]} de {d.year}'


def bar(counts, label):
    """A stacked bar of the five states; each segment is as wide as its count."""
    total = sum(counts[k] for k, _ in STATES)
    if not total:
        return '<span class="dim">sem linhas no inventário</span>'
    parts = ', '.join(f'{counts[k]} {STATE_LABEL[k].lower()}' for k, _ in STATES if counts[k])
    segs = ''.join(f'<span class="seg s-{k}" style="flex-grow:{counts[k]}" title="{esc(STATE_LABEL[k])}: {counts[k]}"></span>'
                   for k, _ in STATES if counts[k])
    return f'<div class="bar" role="img" aria-label="{esc(label)}: {esc(parts)}">{segs}</div>'


def legend(counts=None):
    items = ''.join(
        f'<li><span class="key s-{k}"></span>{esc(name)}'
        + (f' <b>{counts[k]}</b>' if counts is not None else '') + '</li>'
        for k, name in STATES)
    return f'<ul class="legend">{items}</ul>'


def timeline(fc, today):
    """Two bands on one time axis: the range of the plan and today's range."""
    start = fc['base']
    end = max(fc['plan']['high'], fc['now']['high']) + datetime.timedelta(days=7)
    span = (end - start).days

    def pos(d):
        return f'{100 * (d - start).days / span:.2f}%'

    def width(a, b):
        return f'{100 * (b - a).days / span:.2f}%'

    def row(name, r, cls):
        return (f'<div class="tl-row"><div class="tl-name">{name}</div><div class="tl-track">'
                f'<span class="tl-band {cls}" style="left:{pos(r["low"])};width:{width(r["low"], r["high"])}"></span>'
                f'<span class="tl-mark {cls}" style="left:{pos(r["likely"])}"></span></div>'
                f'<div class="tl-dates"><b>{day(r["likely"])}</b> <span class="dim">({day(r["low"])} a {day(r["high"])})</span></div></div>')

    ticks, d = '', start.replace(day=1)
    while d <= end:
        if d >= start:
            ticks += f'<span class="tl-tick" style="left:{pos(d)}">{MONTHS[d.month - 1][:3]}</span>'
        d = (d.replace(day=28) + datetime.timedelta(days=4)).replace(day=1)
    today_mark = (f'<span class="tl-today" style="left:{pos(today)}"><i>hoje</i></span>'
                  if start <= today <= end else '')
    return ('<div class="timeline">'
            + row('Plano de ' + day(fc['base']), fc['plan'], 'plan')
            + row('Previsão de hoje', fc['now'], 'now')
            + f'<div class="tl-row"><div class="tl-name"></div><div class="tl-track tl-axis">{ticks}{today_mark}</div><div class="tl-dates"></div></div>'
            + '</div>')


def sparkline(history):
    """The share of features done over the recorded days, as a small line."""
    pts = [(datetime.date.fromisoformat(r['date']), int(r['done']), int(r['in_scope'])) for r in history]
    if len(pts) < 2:
        return ''
    w, h, pad = 600, 120, 6
    span = max((pts[-1][0] - pts[0][0]).days, 1)
    top = max(p[1] for p in pts)
    xy = [(pad + (w - 2 * pad) * (d - pts[0][0]).days / span, h - pad - (h - 2 * pad) * (v / top if top else 0))
          for d, v, _ in pts]
    line = ' '.join(f'{x:.1f},{y:.1f}' for x, y in xy)
    dots = ''.join(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="3" fill="var(--accent)"><title>{day(p[0])}: {p[1]} prontos</title></circle>'
                   for (x, y), p in zip(xy, pts))
    return (f'<svg class="spark" viewBox="0 0 {w} {h}" preserveAspectRatio="none" role="img" '
            f'aria-label="Recursos prontos de {day(pts[0][0])} a {day(pts[-1][0])}: de {pts[0][1]} para {pts[-1][1]}">'
            f'<polyline points="{line}" fill="none" stroke="var(--accent)" stroke-width="2" vector-effect="non-scaling-stroke"/>{dots}</svg>'
            f'<div class="spark-ends"><span>{day(pts[0][0])}: {pts[0][1]}</span><span>{day(pts[-1][0])}: {pts[-1][1]}</span></div>')


CSS = """
/* The look of the Desk website: Libadwaita colors, white cards on a light
   window. One column of sections; wide tables scroll inside their card. */
:root {
  --window: #fafafb; --card: #ffffff; --fg: #2a2a30; --strong: #1e1e24; --dim: #63636b;
  --border: rgba(0, 0, 6, 0.12); --accent: #1c71d8; --track: rgba(0, 0, 6, 0.08);
  --s-done: #26a269; --s-review: #8ac926; --s-partial: #e5a50a; --s-base: #3584e4; --s-missing: #b8b8c0;
  --plan: #9a9996; --ok: #1a7f4e; --warn: #9a6700; --bad: #c01c28;
  --shadow: 0 0 0 1px rgba(0, 0, 6, 0.03), 0 1px 3px 1px rgba(0, 0, 6, 0.07);
  --sans: "Adwaita Sans", Inter, Cantarell, system-ui, -apple-system, "Segoe UI", sans-serif;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --window: #222226; --card: #2e2e32; --fg: #e8e8ec; --strong: #ffffff; --dim: #a8a8b0;
    --border: rgba(255, 255, 255, 0.14); --accent: #78aeed; --track: rgba(255, 255, 255, 0.1);
    --s-done: #57e389; --s-review: #b5e36b; --s-partial: #f8c84a; --s-base: #62a0ea; --s-missing: #6c6c74;
    --plan: #9a9996; --ok: #8ff0a4; --warn: #f8e45c; --bad: #ff7b63;
    --shadow: 0 0 0 1px rgba(0, 0, 6, 0.3); color-scheme: dark;
  }
}
:root[data-theme="dark"] {
  --window: #222226; --card: #2e2e32; --fg: #e8e8ec; --strong: #ffffff; --dim: #a8a8b0;
  --border: rgba(255, 255, 255, 0.14); --accent: #78aeed; --track: rgba(255, 255, 255, 0.1);
  --s-done: #57e389; --s-review: #b5e36b; --s-partial: #f8c84a; --s-base: #62a0ea; --s-missing: #6c6c74;
  --plan: #9a9996; --ok: #8ff0a4; --warn: #f8e45c; --bad: #ff7b63;
  --shadow: 0 0 0 1px rgba(0, 0, 6, 0.3); color-scheme: dark;
}
body { background: var(--window); color: var(--fg); font: 15px/1.5 var(--sans); margin: 0;
  padding-block: 28px 48px; padding-left: max(16px, env(safe-area-inset-left, 0px));
  padding-right: max(16px, env(safe-area-inset-right, 0px)); }
.dk { max-width: 1040px; margin: 0 auto; display: flex; flex-direction: column; gap: 28px; }
.dk h1 { font-size: 28px; line-height: 1.2; margin: 0; color: var(--strong); font-weight: 800; }
.dk h2 { font-size: 18px; margin: 0 0 4px; color: var(--strong); }
.dk p { margin: 0; }
.dk a { color: var(--accent); }
.dk a:focus-visible, .dk select:focus-visible, .dk input:focus-visible, .dk summary:focus-visible {
  outline: 2px solid var(--accent); outline-offset: 2px; }
.dim { color: var(--dim); }
.lead { font-size: 17px; max-width: 70ch; margin-top: 8px !important; }
.stamp { color: var(--dim); font-size: 13px; margin-top: 6px !important; }
.intro { color: var(--dim); max-width: 75ch; margin-bottom: 12px !important; }
.card { background: var(--card); border-radius: 12px; box-shadow: var(--shadow); padding: 16px 18px; min-width: 0; }
.kpis { display: grid; grid-template-columns: repeat(auto-fit, minmax(210px, 1fr)); gap: 12px; }
.kpi .label { color: var(--dim); font-size: 13px; }
.kpi .value { font-size: 28px; font-weight: 700; color: var(--strong); line-height: 1.25; font-variant-numeric: tabular-nums; }
.kpi .value small { font-size: 15px; font-weight: 400; color: var(--dim); }
.kpi .note { font-size: 13px; color: var(--dim); }
.bar { display: flex; height: 14px; border-radius: 7px; overflow: hidden; background: var(--track); gap: 1px; min-width: 120px; }
.seg { min-width: 2px; }
.s-done { background: var(--s-done); } .s-review { background: var(--s-review); }
.s-partial { background: var(--s-partial); } .s-base { background: var(--s-base); } .s-missing { background: var(--s-missing); }
.legend { list-style: none; display: flex; flex-wrap: wrap; gap: 4px 16px; padding: 0; margin: 8px 0 0; font-size: 13px; color: var(--dim); }
.legend b { color: var(--strong); font-variant-numeric: tabular-nums; }
.key { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 6px; }
.apps { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; }
.app h3 { margin: 0; font-size: 16px; color: var(--strong); }
.app .pct { font-size: 24px; font-weight: 700; color: var(--strong); font-variant-numeric: tabular-nums; }
.app .bar { margin-top: 8px; }
.scroll { overflow-x: auto; }
.dk table { border-collapse: collapse; width: 100%; font-size: 14px; }
.dk th { text-align: left; font-weight: 600; color: var(--dim); font-size: 12px; letter-spacing: 0.03em; text-transform: uppercase;
  padding: 6px 10px; border-bottom: 1px solid var(--border); white-space: nowrap; }
.dk td { padding: 8px 10px; border-bottom: 1px solid var(--border); vertical-align: top; }
.dk tr:last-child td { border-bottom: 0; }
.dk td.n, .dk th.n { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
.dk td.id { font-weight: 700; color: var(--strong); white-space: nowrap; }
.dk td.wide { min-width: 220px; }
.dk td .sub { color: var(--dim); font-size: 13px; }
.track-head td { background: var(--track); font-weight: 700; color: var(--strong); padding: 5px 10px; font-size: 13px; }
.pill { display: inline-block; border-radius: 99px; padding: 1px 10px; font-size: 12px; font-weight: 600; white-space: nowrap;
  border: 1px solid var(--border); color: var(--dim); }
.pill.in-progress, .pill.open { color: var(--accent); border-color: currentColor; }
.pill.done, .pill.merged { color: var(--ok); border-color: currentColor; }
.pill.closed { color: var(--bad); border-color: currentColor; }
.timeline { display: flex; flex-direction: column; gap: 10px; margin-top: 12px; }
.tl-row { display: grid; grid-template-columns: 150px minmax(0, 1fr) 250px; gap: 12px; align-items: center; font-size: 14px; }
.tl-track { position: relative; height: 14px; background: var(--track); border-radius: 7px; }
.tl-band { position: absolute; top: 0; bottom: 0; border-radius: 7px; opacity: 0.45; }
.tl-mark { position: absolute; top: -3px; bottom: -3px; width: 4px; margin-left: -2px; border-radius: 2px; }
.tl-band.plan, .tl-mark.plan { background: var(--plan); } .tl-band.now, .tl-mark.now { background: var(--accent); }
.tl-mark.plan, .tl-mark.now { opacity: 1; }
.tl-axis { background: none; height: 30px; }
.tl-tick { position: absolute; top: 0; font-size: 12px; color: var(--dim); border-left: 1px solid var(--border); padding-left: 4px; }
.tl-today { position: absolute; top: -52px; bottom: 12px; border-left: 2px dashed var(--strong); }
.tl-today i { position: absolute; bottom: -16px; left: 4px; font-style: normal; font-size: 12px; color: var(--strong); font-weight: 600; }
.tl-dates { font-variant-numeric: tabular-nums; }
@media (max-width: 720px) {
  .tl-row { grid-template-columns: 1fr; gap: 2px; }
  .tl-today { top: -8px; }
}
.dk details summary { cursor: pointer; color: var(--accent); font-size: 14px; margin-top: 10px; }
.dk details p, .dk details ul { margin-top: 8px; color: var(--dim); font-size: 14px; max-width: 80ch; }
.filters { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 10px; align-items: center; }
.filters select, .filters input { font: inherit; padding: 6px 10px; border-radius: 8px; border: 1px solid var(--border);
  background: var(--card); color: var(--fg); min-width: 0; }
.filters input { flex: 1 1 180px; }
.filters .count { color: var(--dim); font-size: 13px; margin-left: auto; }
.spark { width: 100%; height: 120px; display: block; margin-top: 8px; }
.spark-ends { display: flex; justify-content: space-between; font-size: 13px; color: var(--dim); }
.foot { color: var(--dim); font-size: 13px; }
"""

JS = """
(function () {
  var data = JSON.parse(document.getElementById('dk-data').textContent);
  var body = document.getElementById('dk-rows'), count = document.getElementById('dk-count');
  var f = { app: document.getElementById('dk-app'), fase: document.getElementById('dk-fase'),
            state: document.getElementById('dk-state'), q: document.getElementById('dk-q') };
  function cell(tr, text, cls) { var td = document.createElement('td'); if (cls) td.className = cls; td.textContent = text; tr.appendChild(td); return td; }
  function draw() {
    var q = f.q.value.trim().toLowerCase(), n = 0, frag = document.createDocumentFragment();
    data.rows.forEach(function (r) {
      if (f.app.value && r.app !== f.app.value) return;
      if (f.fase.value && r.fase !== f.fase.value) return;
      if (f.state.value && r.state !== f.state.value) return;
      if (q && (r.id + ' ' + r.recurso + ' ' + r.area + ' ' + r.nota).toLowerCase().indexOf(q) < 0) return;
      n += 1;
      var tr = document.createElement('tr');
      cell(tr, r.id, 'id');
      var td = cell(tr, r.recurso, 'wide');
      if (r.nota && r.nota !== '-') { var s = document.createElement('div'); s.className = 'sub'; s.textContent = r.nota; td.appendChild(s); }
      cell(tr, data.apps[r.app]);
      cell(tr, r.area);
      cell(tr, r.fase);
      var p = document.createElement('span'); p.className = 'pill'; p.textContent = data.states[r.state];
      if (r.state !== 'out' && r.state !== 'na') { var k = document.createElement('span'); k.className = 'key s-' + r.state; p.insertBefore(k, p.firstChild); }
      cell(tr, '').appendChild(p);
      frag.appendChild(tr);
    });
    body.replaceChildren(frag);
    count.textContent = n + ' de ' + data.rows.length + ' recursos';
  }
  Object.keys(f).forEach(function (k) { f[k].addEventListener(k === 'q' ? 'input' : 'change', draw); });
  draw();
})();
"""


def render(features, phases, backlog, prs, prs_source, history, fc, today, commit):
    """The content of the page, shared by the website and the Artifact."""
    scope = [f for f in features if f['state'] in dict(STATES)]
    totals = collections.Counter(f['state'] for f in scope)
    n_scope, n_done = len(scope), totals['done']
    out_rows = [f for f in features if f['state'] in ('out', 'na')]
    open_items = [b for b in backlog if b['status'] == 'open']
    current = [p for p in phases if p['status'] == 'in-progress']
    closed = [p for p in phases if p['status'] == 'done']
    slip = (fc['now']['likely'] - fc['plan']['likely']).days

    o = []
    w = o.append
    w('<div class="dk">')

    # Header: what the numbers say today.
    w('<header><h1>Painel do Desk</h1>')
    w(f'<p class="lead"><b>{n_done} dos {n_scope} recursos</b> do alvo (iWork 15.4) estão prontos, '
      f'{num(100 * n_done / n_scope)}% do escopo. '
      + (f'Fase em andamento: {esc(", ".join(p["id"] + " " + p["name"] for p in current))}. ' if current
         else 'Nenhuma fase em andamento. ')
      + f'A previsão de paridade é <b>{long_day(fc["now"]["likely"])}</b>, '
      + ('igual à do plano.' if slip == 0 else f'{abs(slip)} dias {"depois" if slip > 0 else "antes"} do plano.')
      + '</p>')
    w(f'<p class="stamp">Gerado em {long_day(today)} a partir do commit <code>{esc(commit)}</code> do '
      f'<a href="https://github.com/{REPO}">repositório</a>.</p></header>')

    # Headline figures.
    prev = history[-2] if len(history) > 1 else None
    delta = ''
    if prev:
        d = n_done - int(prev['done'])
        delta = f'{"+" if d >= 0 else "−"}{abs(d)} desde {day(datetime.date.fromisoformat(prev["date"]))}'
    w('<section class="kpis" aria-label="Resumo">')
    w(f'<div class="card kpi"><div class="label">Recursos prontos</div><div class="value">{n_done} <small>de {n_scope}</small></div>'
      f'<div class="note">{esc(delta) if delta else num(100 * n_done / n_scope) + "% do escopo"}</div></div>')
    w(f'<div class="card kpi"><div class="label">Fases concluídas</div><div class="value">{len(closed)} <small>de {len(phases)}</small></div>'
      f'<div class="note">{esc(", ".join(p["id"] for p in current)) + " em andamento" if current else "nenhuma em andamento"}</div></div>')
    w(f'<div class="card kpi"><div class="label">Pendências abertas</div><div class="value">{len(open_items)}</div>'
      f'<div class="note">{len(backlog) - len(open_items)} já encerradas</div></div>')
    w(f'<div class="card kpi"><div class="label">Previsão de paridade</div><div class="value">{day(fc["now"]["likely"])}</div>'
      f'<div class="note">entre {day(fc["now"]["low"])} e {day(fc["now"]["high"])}</div></div>')
    w('</section>')

    # Forecast.
    w('<section class="card" id="previsao"><h2>Previsão</h2>')
    w(f'<p class="intro">O plano de {day(fc["base"])} estima {fc["days"]["likely"]} dias de trabalho com os aplicativos em sequência '
      f'(entre {fc["days"]["low"]} e {fc["days"]["high"]}). Falta {num(100 * fc["left"])}% desse trabalho.</p>')
    w(timeline(fc, today))
    w('<details><summary>Como a previsão é calculada</summary>'
      f'<p>O trabalho é medido nos dias estimados de cada fase no roadmap ({fc["total_days"]} dias somados). '
      'Uma fase concluída conta inteira. Uma fase em andamento conta pela parte dos seus recursos do inventário que já está pronta. '
      f'Até hoje isso soma {num(fc["earned_days"])} dias. A previsão de hoje é a data de hoje mais a parte que falta dos '
      f'{fc["days"]["low"]}, {fc["days"]["likely"]} e {fc["days"]["high"]} dias do plano. '
      'Por isso ela anda um dia para a frente a cada dia sem avanço. É uma estimativa, não um compromisso, '
      'e o roadmap a corrige ao fim de cada fase.</p></details>')
    w('</section>')

    # Per app.
    w('<section id="aplicativos"><h2>Por aplicativo</h2>'
      '<p class="intro">Recursos dentro do escopo em cada aplicativo, pela situação lida no inventário.</p><div class="apps">')
    for app, name in APPS:
        c = collections.Counter(f['state'] for f in scope if f['app'] == app)
        n = sum(c.values())
        w(f'<div class="card app"><h3>Desk {name}</h3><div class="pct">{num(100 * c["done"] / n)}% <small class="dim" style="font-size:14px;font-weight:400">'
          f'{c["done"]} de {n} prontos</small></div>{bar(c, "Desk " + name)}{legend(c)}</div>')
    w('</div><details><summary>O que cada situação quer dizer</summary><ul>'
      + ''.join(f'<li><b>{esc(STATE_LABEL[k])}</b>: {esc(STATE_HELP[k])}.</li>' for k, _ in STATES)
      + '</ul></details></section>')

    # Per phase.
    by_phase = collections.Counter(b['to_phase'] for b in open_items)
    tracks = [('Preparação', lambda p: p['id'] in ('R', 'B1')), ('Writer', lambda p: p['id'].startswith('W')),
              ('Tables', lambda p: p['id'].startswith('T')), ('Slides', lambda p: p['id'].startswith('S')),
              ('Fechamento', lambda p: p['id'].startswith('X'))]
    w('<section class="card" id="fases"><h2>Por fase</h2>'
      '<p class="intro">As fases do roadmap, com os recursos do inventário que cada uma entrega. '
      'As três trilhas de aplicativo podem andar em paralelo depois da B1.</p>')
    w('<div class="scroll"><table><thead><tr><th>Fase</th><th>Situação</th><th>Recursos</th><th class="n">Prontos</th>'
      '<th class="n">Dias estimados</th><th class="n">Pendências</th></tr></thead><tbody>')
    listed = set()
    for track, belongs in tracks:
        group = [p for p in phases if belongs(p)]
        listed.update(p['id'] for p in group)
        if not group:
            continue
        w(f'<tr class="track-head"><td colspan="6">{track}</td></tr>')
        for p in group:
            when = ''
            if p['status'] == 'in-progress' and p['started'] != '-':
                when = f' <span class="sub">desde {day(datetime.date.fromisoformat(p["started"]))}</span>'
            if p['status'] == 'done' and p['ended'] != '-':
                when = f' <span class="sub">em {day(datetime.date.fromisoformat(p["ended"]))}</span>'
            w(f'<tr><td class="wide"><b>{esc(p["id"])}</b> {esc(p["name"])}<div class="sub">{esc(p["delivers"])}</div></td>'
              f'<td><span class="pill {p["status"]}">{PHASE_STATUS[p["status"]]}</span>{when}</td>'
              f'<td style="min-width:150px">{bar(p["counts"], "Fase " + p["id"])}</td>'
              f'<td class="n">{str(p["counts"]["done"]) + " de " + str(p["total"]) if p["total"] else "–"}</td>'
              f'<td class="n">{p["days"] if p["days"] else "–"}</td>'
              f'<td class="n">{by_phase.get(p["id"], 0) or "–"}</td></tr>')
    w('</tbody></table></div>' + legend())
    before = sum(1 for f in scope if f['fase'] == 'feito')
    w(f'<p class="foot" style="margin-top:8px">{before} recursos já estavam prontos antes deste plano e não pertencem a nenhuma fase.</p>')
    w('</section>')

    # Backlog.
    w('<section class="card" id="pendencias"><h2>Pendências</h2>'
      '<p class="intro">O que um conjunto de tarefas deixou sem fazer, com o motivo e a fase em que volta. '
      'Uma fase só fecha quando as pendências dela estão resolvidas.</p>')
    w('<div class="scroll"><table><thead><tr><th>Id</th><th>Pendência</th><th>Volta na fase</th><th>Deixada por</th><th>Situação</th></tr></thead><tbody>')
    labels = {'open': 'Aberta', 'done': 'Feita', 'dropped': 'Descartada'}
    for b in sorted(backlog, key=lambda b: (b['status'] != 'open', b['id'])):
        w(f'<tr><td class="id">{esc(b["id"])}</td><td class="wide">{esc(b["item"])}<div class="sub">{esc(b["reason"])}</div></td>'
          f'<td>{esc(b["to_phase"])}</td><td>{esc(b["task_set"])}<div class="sub">{day(datetime.date.fromisoformat(b["date"]))}</div></td>'
          f'<td><span class="pill {esc(b["status"])}">{labels.get(b["status"], esc(b["status"]))}</span></td></tr>')
    w('</tbody></table></div></section>')

    # Pull requests.
    pr_labels = {'merged': 'Integrado', 'open': 'Aberto', 'draft': 'Rascunho', 'closed': 'Fechado'}
    w('<section class="card" id="prs"><h2>Pull requests recentes</h2>'
      + ('<p class="intro">Os doze alterados por último no GitHub.</p>' if prs_source == 'GitHub' else
         '<p class="intro">Os integrados por último, lidos do histórico do git. Os que ainda estão abertos não aparecem aqui.</p>'))
    w('<div class="scroll"><table><thead><tr><th>Nº</th><th>Título</th><th>Situação</th><th class="n">Data</th></tr></thead><tbody>')
    for pr in prs:
        w(f'<tr><td class="id"><a href="{esc(pr["url"])}">#{pr["number"]}</a></td><td class="wide">{esc(pr["title"])}</td>'
          f'<td><span class="pill {pr["state"]}">{pr_labels.get(pr["state"], esc(pr["state"]))}</span></td>'
          f'<td class="n">{day(datetime.date.fromisoformat(pr["date"]))}</td></tr>')
    w('</tbody></table></div></section>')

    # History.
    w('<section class="card" id="historico"><h2>Histórico</h2>')
    if len(history) > 1:
        w('<p class="intro">Recursos prontos em cada dia em que o painel foi registrado.</p>' + sparkline(history))
        w('<div class="scroll"><table><thead><tr><th>Data</th><th class="n">Prontos</th><th class="n">A rever</th><th class="n">Parciais</th>'
          '<th class="n">Base pronta</th><th class="n">Ausentes</th><th class="n">Pendências abertas</th><th class="n">Previsão</th></tr></thead><tbody>')
        for r in reversed(history[-12:]):
            w(f'<tr><td>{day(datetime.date.fromisoformat(r["date"]))}</td><td class="n">{esc(r["done"])}</td><td class="n">{esc(r["review"])}</td>'
              f'<td class="n">{esc(r["partial"])}</td><td class="n">{esc(r["base"])}</td><td class="n">{esc(r["missing"])}</td>'
              f'<td class="n">{esc(r["backlog_open"])}</td><td class="n">{day(datetime.date.fromisoformat(r["forecast"]))}</td></tr>')
        w('</tbody></table></div>')
    elif history:
        w(f'<p class="intro">O histórico começa em {day(datetime.date.fromisoformat(history[0]["date"]))}, com {esc(history[0]["done"])} recursos prontos. '
          'A curva aparece a partir do segundo registro.</p>')
    else:
        w('<p class="intro">Ainda não há registros.</p>')
    w('</section>')

    # Out of scope.
    reasons = collections.Counter(f['escopo'] for f in out_rows)
    w(f'<section class="card" id="fora"><h2>Fora das fases</h2><p class="intro">{len(out_rows)} linhas do inventário não entram em nenhuma fase. '
      'Elas continuam registradas com o motivo.</p><div class="scroll"><table><thead><tr><th>Motivo</th><th class="n">Recursos</th></tr></thead><tbody>'
      + ''.join(f'<tr><td>{esc(k)}</td><td class="n">{v}</td></tr>' for k, v in reasons.most_common())
      + '</tbody></table></div></section>')

    # Every feature, with filters.
    phase_ids = [p['id'] for p in phases if p['total']]
    w('<section class="card" id="recursos"><h2>Todos os recursos</h2>'
      '<p class="intro">As linhas do inventário, uma por recurso. A fase "feito" marca o que já estava pronto antes do plano.</p>'
      '<div class="filters">'
      '<select id="dk-app" aria-label="Aplicativo"><option value="">Todos os aplicativos</option>'
      + ''.join(f'<option value="{a}">Desk {n}</option>' for a, n in APPS) + '</select>'
      '<select id="dk-fase" aria-label="Fase"><option value="">Todas as fases</option>'
      + ''.join(f'<option value="{esc(p)}">{esc(p)}</option>' for p in phase_ids)
      + '<option value="feito">feito</option><option value="-">fora das fases</option></select>'
      '<select id="dk-state" aria-label="Situação"><option value="">Todas as situações</option>'
      + ''.join(f'<option value="{k}">{esc(v)}</option>' for k, v in STATE_LABEL.items()) + '</select>'
      '<input id="dk-q" type="search" placeholder="Buscar recurso" aria-label="Buscar recurso">'
      '<span class="count" id="dk-count" aria-live="polite"></span></div>'
      '<div class="scroll" style="max-height:560px;overflow-y:auto"><table><thead><tr><th>Id</th><th>Recurso</th><th>Aplicativo</th>'
      '<th>Área</th><th>Fase</th><th>Situação</th></tr></thead><tbody id="dk-rows"></tbody></table></div>'
      '<noscript><p class="foot">A lista precisa de JavaScript. Os mesmos dados estão em corpus/inventory no repositório.</p></noscript>'
      '</section>')

    w(f'<p class="foot">Fontes: <code>corpus/inventory</code>, <code>docs/roadmap.md</code>, <code>docs/backlog/deferred.tsv</code> e '
      f'<code>docs/dashboard</code> no <a href="https://github.com/{REPO}">repositório do Desk</a>. '
      f'Página gerada por <code>ci/dashboard.py</code>. <a href="{SITE}pt-BR/">Site do Desk</a>.</p>')
    w('</div>')

    data = {'apps': {a: n for a, n in APPS}, 'states': STATE_LABEL,
            'rows': [{k: f[k] for k in ('id', 'app', 'area', 'recurso', 'fase', 'state', 'nota')} for f in features]}
    blob = json.dumps(data, ensure_ascii=False, separators=(',', ':')).replace('</', '<\\/')
    o.append(f'<script type="application/json" id="dk-data">{blob}</script><script>{JS}</script>')
    return '\n'.join(o)


TITLE = 'Painel do Desk'
DESCRIPTION = 'Andamento do Desk: recursos prontos por aplicativo e por fase, pendências e previsão de paridade.'


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--out')
    ap.add_argument('--artifact')
    ap.add_argument('--record', action='store_true')
    ap.add_argument('--date')
    ap.add_argument('--no-prs', action='store_true')
    args = ap.parse_args()

    today = (datetime.date.fromisoformat(args.date) if args.date
             else datetime.datetime.now(zoneinfo.ZoneInfo('America/Sao_Paulo')).date())
    features = load_features()
    phases = load_phases(features)
    stray = sorted({f['fase'] for f in features} - {p['id'] for p in phases} - {'feito', '-'})
    if stray:
        sys.exit(f'the inventory names phases the roadmap does not have: {stray}')
    backlog = read_tsv(ROOT / 'docs' / 'backlog' / 'deferred.tsv')
    prs, prs_source = load_prs(not args.no_prs)
    fc = forecast(phases, today)
    commit = subprocess.run(['git', '-C', str(ROOT), 'rev-parse', '--short', 'HEAD'],
                            capture_output=True, text=True).stdout.strip() or 'desconhecido'

    totals = collections.Counter(f['state'] for f in features)
    row = {'date': today.isoformat(), 'in_scope': sum(totals[k] for k, _ in STATES),
           **{k: totals[k] for k, _ in STATES},
           'backlog_open': sum(1 for b in backlog if b['status'] == 'open'),
           'forecast': fc['now']['likely'].isoformat()}
    row = {k: str(v) for k, v in row.items()}
    if args.record:
        record_history(row)
    # The page always shows today as the last point, recorded or not.
    history = [r for r in read_tsv(HERE / 'history.tsv') if r['date'] < row['date']] + [row]

    content = render(features, phases, backlog, prs, prs_source, history, fc, today, commit)
    head = f'<title>{TITLE}</title>\n<style>{CSS}</style>'
    if args.out:
        path = pathlib.Path(args.out)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            '<!doctype html>\n<html lang="pt-BR">\n<head>\n<meta charset="utf-8">\n'
            '<meta name="viewport" content="width=device-width, initial-scale=1">\n'
            f'<meta name="description" content="{esc(DESCRIPTION)}">\n'
            '<meta name="color-scheme" content="light dark">\n'
            '<link rel="icon" href="../img/desk.svg" type="image/svg+xml">\n'
            f'{head}\n</head>\n<body>\n{content}\n</body>\n</html>\n', encoding='utf-8')
    if args.artifact:
        path = pathlib.Path(args.artifact)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text('<meta name="claude:viewport-fit-sides" content="cover">\n' + head + '\n' + content + '\n',
                        encoding='utf-8')
    print(f'{row["done"]} of {row["in_scope"]} features done; {row["backlog_open"]} backlog items open; '
          f'forecast {row["forecast"]}; pull requests from {prs_source}')


if __name__ == '__main__':
    main()
