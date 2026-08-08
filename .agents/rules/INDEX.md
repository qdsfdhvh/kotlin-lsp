# Rules Index — STOP-and-Read Triggers

This file is the **lookup table** from task fact-pattern → rule file. Read it
on demand; do not load the whole directory into every session. If a row
matches your task, **STOP and read the linked rule file first** before coding.
Rules are hard constraints — violating one often compiles clean and breaks at
runtime or in CI (e.g. #228: `--help` advertised 12 commands the parser
rejected; #227: nested-tokio-runtime panic, exit 134).

Rule paths are relative to `.agents/rules/` unless prefixed.

| Task area | Rule |
|---|---|
| **Any CLI change** — adding/renaming/removing a subcommand, editing `--help`, `is_subcommand()`, `build_subcommand()`, or any command documentation (`docs/commands.md`, `skills/kotlin-lsp/SKILL.md`) | `cli-surface-consistency/RULE.md` |
| **Any git write** — commit, push, branch, PR, merge, worktree, tag | `git-workflow/RULE.md` (§ Hard bans are unrecoverable) |
| **Release** — version bump in `Cargo.toml`, CHANGELOG, tag `v*`, release PR, CI monitoring | `releasing/RULE.md` |
| **Multi-step work** — task_plan.md / findings.md / progress.md planning files | `local-planning.md` |
| Fixing `kotlin-lsp check` false positives | AGENTS.md rule 9 (test-first, `fp_*` regression test) |
| Parser/indexer/cache `SymbolEntry` changes | AGENTS.md rules 1–6 (core coding standards) |

**One exception that cannot wait for a lookup:** before any git write, read
`git-workflow/RULE.md` § Hard bans first — those are unrecoverable once done.

## Rule files

| File | What it governs |
|---|---|
| `cli-surface-consistency/RULE.md` | help ↔ parser ↔ docs contract; the five sync points; guardrail tests |
| `git-workflow/RULE.md` | PR-only flow, never push to main, run-before-push gate, squash merge |
| `releasing/RULE.md` | version numbering, release steps, tag safety, CI monitoring, install-from-release |
| `local-planning.md` | local planning files and when to publish them into tracked docs |

## Related

- Skills live in `.agents/skills/` (e.g. `rust-perf/` for profiling).
- Published agent skill for downstream Kotlin projects: `skills/kotlin-lsp/SKILL.md`.
- CLI reference: `docs/commands.md`.
