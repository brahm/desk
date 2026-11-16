# Inventory for the current iWork target

`writer.tsv`, `tables.tsv` and `slides.tsv` list, feature by feature, what
Desk has to do under the target set on 2026-11-16
([docs/goals.md](../../docs/goals.md)). They come from the three reports
that compare iWork '09 with iWork 15.4, plus nine rows per app for the file
formats Desk reads and writes. `build.py` holds the rules that made them.
The reports were written in Portuguese; `translations.tsv` is the English
of each area, feature and note, which `build.py` applies.

| Column | Meaning |
| --- | --- |
| `id` | `W`, `T` or `S` and a number; the same ids as the reports of 2026-11-16 |
| `area`, `feature` | The area of the report and the feature |
| `in_iwork` | Whether the feature is equal, different, only in '09 or only in 15 |
| `target` | What Desk follows: 15.4; the '09 in the form of the 15; 15.4 plus what the '09 did; equal in both; or a decision of the project |
| `rule` | The decision of [docs/decisions.md](../../docs/decisions.md) that sets the target |
| `scope` | In scope; out of scope and kept (AI, online or paid; unsupported export); or not applicable |
| `status_in_desk` | Done, Partial, Engine ready (the engine exists but the app does not use it yet) or Missing, as read on 2026-11-16 at commit `a0e5ca5` |
| `review` | Why the status has to be checked again: the target changed, or the window changes with decision D-008 |
| `phase` | The phase of [docs/roadmap.md](../../docs/roadmap.md) that owns the row; `done` when Desk already does it; `-` when it is out of scope |
| `note` | How Desk does it today, or what is missing |

These files do not replace `corpus/matrix.tsv`, `tables-matrix.tsv` and
`slides-matrix.tsv` yet. Those are the matrices the tests cite, and they
still describe the iWork '09 target. The new implementation plan joins the
two: each row here gets its phase and the matrix rows that test it
(backlog item B-006).
