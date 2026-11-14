# Verone documentation

This folder is the single source of truth for the project. The owner and all agents
work from it.

## Read in this order

| Document | What it answers | Status |
|---|---|---|
| [vision-and-scope.md](vision-and-scope.md) | What Verone is, who it is for, what it will never do. | Approved baseline |
| [decisions.md](decisions.md) | What has been decided, by whom, and why. | Live log; all current entries accepted |
| [architecture.md](architecture.md) | How Verone is put together. | Draft for owner review |
| [roadmap.md](roadmap.md) | The phases, their outcomes and exit tests. | Draft for owner review |
| [assumptions.md](assumptions.md) | What the design depends on, what has been tested, known risks. | Live register |
| [future-ideas.md](future-ideas.md) | Ideas not yet scheduled. Not to be built until approved. | Live backlog |

## Still to be written

| Document | Written when |
|---|---|
| `phases/phase-0.md` — step-by-step proof tests | Next, once the drafts above are approved |
| `phases/phase-N.md` — task lists per phase | Before each phase starts |
| `user-guide/` | During Phases 2 and 3 |
| `developer-guide/` | During Phase 4 |
| `CONTRIBUTING.md`, `SECURITY.md` (repository root) | During Phase 1 |

## Rules for agents

1. Read `vision-and-scope.md` and `decisions.md` before any task.
2. Work only from the current phase's task file. Do not start work from a later phase.
3. A task is finished when its acceptance test passes, and not before. Record what was run and what happened.
4. Never depend on an assumption marked "untested" in `assumptions.md`. Test it first and record the result.
5. If a task conflicts with a decision or the scope, stop and ask the owner. Do not work around it.
6. Small improvements inside a task are welcome. New features, new dependencies and changes of approach go to the owner first.
7. Nothing that spoofs a device identity or bypasses integrity checks is ever added (D-005).
8. Write for the next reader: plain language, exact commands, no unexplained shorthand.
