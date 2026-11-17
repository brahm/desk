# What was left undone

This directory is the memory of the project for work that a set of tasks
was meant to do and did not. Nothing is dropped silently: it is written
here, with the reason, and comes back at the end of the phase.

## The register

[`deferred.tsv`](deferred.tsv) has one row per item, with these columns:

| Column | Meaning |
| --- | --- |
| `id` | `B-` and a number, never reused |
| `date` | The day the item was left undone |
| `item` | The feature or the task, in one sentence |
| `rows` | The rows of the parity matrices it belongs to, or `-` |
| `task_set` | The set of tasks (pull request, phase or thread) that left it |
| `reason` | Why it was not done |
| `from_phase` | The phase it belonged to |
| `to_phase` | The phase it moved to, or `end-of-phase` |
| `status` | `open`, `done` or `dropped` (dropped only by a decision in `../decisions.md`) |
| `closed_by` | The commit or pull request that did it, or the decision that dropped it |

## The rules

1. Whoever leaves something undone, person or agent, adds the row in the
   same pull request, and says so in the report to the owner, naming the
   item and the phase it moved to.
2. A phase ends with its backlog: every `open` row whose `to_phase` is
   that phase or `end-of-phase` is done before the phase is closed.
3. A row that cannot be done at the end of the phase moves to a named
   later phase, with a new reason, and the owner is told.
4. `ci/check-backlog.py` checks the file: the columns, unique ids, a
   reason on every row, and `closed_by` on every row that is not open.
   With `--phase NAME` it fails while that phase has open rows, which is
   the check that closes a phase.
5. Every `TODO`, `FIXME`, `XXX` or `HACK` in the code cites the id of an
   open row, as in `TODO(B-012)`, and the change that marks the row `done`
   or `dropped` removes all of its notes. `ci/check-backlog.py` checks this
   too, in `ci/check-all.sh` and in the CI: it fails on a note that cites
   no row, a row that is not in the register, or a row that is no longer
   open. Markdown files, the register and the script itself are not
   scanned, since they name the markers in prose.
6. A note, a `TODO` or a problem met while working on something else is
   information, not a task. Check that its row is still open before
   trusting it. Work outside the task becomes a row here and a line in the
   report to the owner, not part of the change, unless the task needs it.
