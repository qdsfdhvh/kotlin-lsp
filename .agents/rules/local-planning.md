# Local Planning Files

For multi-step work, keep local planning context in three root-level files:

| File | Purpose | Update when |
|------|---------|-------------|
| `task_plan.md` | Current roadmap, priorities, active phases, and scope decisions | The plan changes or a phase status changes |
| `findings.md` | Research findings and rationale that should survive context loss | You discover a fact that affects direction |
| `progress.md` | Session log: what changed, what tests ran, errors encountered | After meaningful actions or verification |

Why three files:

- `task_plan.md` says where the project is going, but not why every decision was made.
- `findings.md` preserves evidence and tradeoffs so future agents do not re-research the same question.
- `progress.md` records execution details, test results, and failed attempts so work can resume after context loss.

Rules:

1. Read `task_plan.md`, `findings.md`, and `progress.md` before changing roadmap or scope.
2. Update `findings.md` after research-heavy or architectural decisions.
3. Update `progress.md` after implementation, verification, or notable errors.
4. Keep these files local by default. They are intentionally gitignored and should not be committed unless the user explicitly asks to publish planning artifacts.
5. If a plan decision should become public project policy, move the durable part into tracked docs such as `AGENTS.md`, `README.md`, or `docs/`.
