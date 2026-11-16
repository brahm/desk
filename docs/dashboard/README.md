# Progress dashboard

One page that says where Desk stands: features done per app and per
phase, the backlog, the recent pull requests and the forecast of parity.
`ci/dashboard.py` generates it from files of this repository, so nothing
on it is typed by hand. The page is in Portuguese, the language of the
project owner (D-018).

It is published in two places:

| Where | Address | How it is updated |
| --- | --- | --- |
| Website | <https://brahm.github.io/desk/painel/> | By itself: the [Website workflow](../../.github/workflows/pages.yml) regenerates it when the inventory, the roadmap, the backlog or this directory changes on `main` |
| Artifact of the project owner | <https://claude.ai/artifact/KtMvLZGQdaALziw5ZHJw3u> | By the agent that ends a set of tasks, with the steps below |

## Update it when a set of tasks ends

A set of tasks is a pull request, a phase or a thread of work. Before its
last commit:

1. Bring the sources up to date (next section).
2. Run `python3 ci/dashboard.py --record --artifact /tmp/desk-painel.html`.
   `--record` writes today's row in [`history.tsv`](history.tsv), the
   changes of status in `changes.tsv` and the new `snapshot.tsv`; commit
   the three with the rest of the change.
3. Publish `/tmp/desk-painel.html` to the Artifact above, at the same
   address: with the Artifact tool, read that address first, then publish
   the file with `url` set to it. Never publish without `url`, which would
   create a second page.

The website needs no step: it follows `main`.

## The sources, and who changes them

| File | What the page reads | When to change it |
| --- | --- | --- |
| [`corpus/inventory/*.tsv`](../../corpus/inventory/README.md) | Scope, status, phase and note of every feature | When a feature changes status |
| [`../roadmap.md`](../roadmap.md) | The table of phases: id, name, what it delivers, estimate in days | When the plan changes |
| [`phases.tsv`](phases.tsv) | The status of each phase: `not-started`, `in-progress` or `done`, with the days it started and ended | When a phase starts or ends |
| [`forecast.tsv`](forecast.tsv) | The estimate of the plan: its day and the least, likely and most days | When the roadmap corrects its estimate |
| [`../backlog/deferred.tsv`](../backlog/README.md) | What was left undone, and the phase it moved to | By the rules of the backlog |
| [`history.tsv`](history.tsv) | One row per day the dashboard was recorded | Only through `--record` |
| [`snapshot.tsv`](snapshot.tsv), [`changes.tsv`](changes.tsv) | The status of every feature, phase and backlog item at the last record, and what changed between records | Only through `--record` |

The pull requests come from GitHub (`gh api search/issues`). Where `gh`
cannot answer, the page lists the merges of the local git history and says
so.

## What was just done, and what comes next

"Acabou de ser feito" lists what changed status since the last record: the
script compares the files as they are with `snapshot.tsv`, and `--record`
writes the differences in `changes.tsv` with the day. So a set of tasks
shows on the page only if it updated the inventory, `phases.tsv` and the
backlog before `--record` ran. The section also lists the last pull
requests merged.

"A seguir" lists the phases in progress and the next ones by the roadmap
(the first not started of R and B1; then the first not started of each app
track; then X1), each with every feature still to do and the backlog items
that come back in it.

## How a feature counts as done

A row of the inventory is **done** when `scope` is `In`,
`status_in_desk` is `Done` and `review` is `-`. A row that is
`Done` with a reason in `review` shows as "Pronto, a rever" and does
not count, as in the roadmap.

When you finish a feature, set `status_in_desk` to `Done` and
`review` to `-`, and **leave its phase in `phase`**. The phase then shows
its own progress. `done` in `phase` is only for the rows that were done
before the plan of 2026-11-16.

## How the forecast is worked out

The work is measured in the estimated days of the phases in the roadmap.
A phase that is `done` counts whole; a phase `in-progress` counts by the
share of its inventory rows that are done; the others count nothing. The
forecast is the day the page is generated plus the share still to do of
the least, likely and most days of `forecast.tsv`. With no progress it
moves one day later each day. It is an estimate, not a commitment.

The script stops with a message when the sources disagree: a phase of the
roadmap without a row in `phases.tsv`, a phase in the inventory that the
roadmap does not have, or an unknown status. It warns when the row count
of a phase in the roadmap differs from the inventory.
