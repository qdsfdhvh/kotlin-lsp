# kotlin-lsp — Agent Instructions

## Rust Skills

This project uses [actionbook/rust-skills](https://github.com/actionbook/rust-skills) via CoWork. Install with:

```sh
cargo install cowork
cowork config install
```

See `.cowork/Skills.toml` for config.

This project also publishes its own agent skill at `skills/kotlin-lsp/SKILL.md`
for use in downstream Kotlin projects. See [`skills/README.md`](skills/README.md)
for how to install, use, and maintain skills.

## Local Planning Files

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
4. Keep these files local by default. They are intentionally gitignored (`TASK_PLAN.md`, `task_plan.md`, `findings.md`, `progress.md`) and should not be committed unless the user explicitly asks to publish planning artifacts.
5. If a plan decision should become public project policy, move the durable part into tracked docs such as `AGENTS.md`, `README.md`, or `docs/`.

## Quick Start

### Test coverage

Use `coverage.sh` for instant targeted or full-project reports:

```sh
./coverage.sh                          # Phase 23+26 core files
./coverage.sh src/indexer/resolution.rs  # single file
./coverage.sh --all                      # everything
```

Under the hood it uses `-C instrument-coverage` + `llvm-cov`.
Manual audit pattern when adding new code:
```sh
cargo build --release
cargo test
cargo clippy -- -D warnings
```

## Non-Negotiable Rules

1. **Zero warnings** — fix clippy/fmt, never `#[allow]` without a comment
2. **No hardcoded node kind strings** — use `KIND_*` constants from `src/queries.rs`
3. **Prefer generics over `Box<dyn Trait>`** — static dispatch, zero cost
4. **No bare `unwrap()`** — use `expect("reason")`
5. **Tests in `*_tests.rs` files** — not inline `mod tests {}`
6. **`#[serde(default)]` on new `SymbolEntry` fields** — bump `CACHE_VERSION`
7. **Always PR, never push to main** — Repo settings enforce this (Require a PR before merging, squash-merge only).
8. **Run locally before push** — `cargo fmt --all -- --check && cargo test && cargo clippy -- -D warnings`

   **With fmt proxy issue:** `find src tests -name '*.rs' | xargs rustfmt --edition 2021 --check`

9. **False-positive syntax error fix — test-first** — When fixing `check` false positives:
   - First write a `#[test] fn fp_*` regression test that parses the valid Kotlin and asserts `data.syntax_errors.is_empty()`
10. **No daemon mode** — Keep CLI simple. No background processes, no Unix sockets, no IPC. Each invocation is self-contained. Performance wins come from cache optimisations, not daemons.
   - Verify the test fails before the fix (reproduces the issue)
   - Then add suppression logic in `collect_syntax_errors()` (in `src/parser.rs`)
   - Verify the test passes after the fix
   - Run `cargo test --bin kotlin-lsp 'parser::tests::'` to confirm no regressions
   - Group related tests under `// ── false positive syntax error regression tests ───────────`

## CLI Reference

| Need | Command |
|------|---------|
| Find definition | `kotlin-lsp find <NAME>` |
| Find references | `kotlin-lsp refs <NAME>` |
| Get signature | `kotlin-lsp hover <FILE> <LINE> <COL>` |
| Completions | `kotlin-lsp complete <FILE> <LINE> [COL]` |
| One-stop context | `kotlin-lsp context <FILE> <LINE> <COL>` |
| Syntax errors | `kotlin-lsp check <FILE>...` |
| Code actions | `kotlin-lsp code-action <FILE> <LINE> <COL>` |
| Call hierarchy | `kotlin-lsp call-hierarchy <FILE> <LINE> <COL>` |
| Type hierarchy | `kotlin-lsp type-hierarchy <NAME>` |
| Organize imports | `kotlin-lsp organize-imports <FILE>...` |
| Batch imports | `kotlin-lsp batch-imports <FILE>` |
| Index JAR sources | `kotlin-lsp index-jars [ROOT]` |
| Index workspace | `kotlin-lsp index [--root <DIR>]` |
| Cache stats | `kotlin-lsp cache stats` |
| Benchmark | `kotlin-lsp benchmark` |
| Filter by kind | `--kind class,fun,interface` |
| Call hierarchy | `kotlin-lsp call hierarchy <FILE> <LINE> <COL>` |
| Call hierarchy (outgoing) | `kotlin-lsp call hierarchy <FILE> <LINE> <COL> --outgoing` |
| Impact analysis | `kotlin-lsp impact <FILE> <LINE> <COL>` |
| Symbol overview | `kotlin-lsp summarize <NAME>` |
| Find tests | `kotlin-lsp find-test <FILE> <LINE> <COL>` |
| KMP expect/actual | `kotlin-lsp expect-actual <NAME>` |
| Module deps | `kotlin-lsp module list / module deps / module files / module packages` |
| Android resources | `kotlin-lsp android activities / android composables` |
| Format check | `kotlin-lsp format check <FILE>...` |
| Format apply | `kotlin-lsp format apply <FILE>...` |
| File inspect | `kotlin-lsp inspect <FILE>` |
| JSON output | `--json` |

## Merge Rules

**Never merge until CI is green on all 3 platforms.** Wait for `gh pr checks` to show all pass before merging.

## Project Skills

The project ships agent skills in `.agents/skills/`:
- **`rust-perf/`** — Performance optimization (profiling, binary size, cold start)
  tailored for kotlin-lsp's CLI-first profile.

## How to Release

When asked to "release" or "publish":

1. Bump version in `Cargo.toml` (line 6)
2. Add section to top of `CHANGELOG.md`
3. Commit, create PR, merge on green CI
4. `git tag vX.Y.Z && git push origin vX.Y.Z`

## Tag Safety (2026-07-12)

**NEVER delete or force-push Git tags without explicit user confirmation.**
Tags represent published releases. Deleting a tag breaks GitHub Releases,
CI artifacts, and downstream consumers that reference the tag.

- `git tag -d` → ask first
- `git push --delete origin <tag>` → ask first
- `git tag -f` → ask first
- `git push origin <tag> -f` → ask first

Before touching tags, verify:
1. Is this a new tag or an existing release?
2. Ask: "vX.Y.Z already exists as a published release. Are you sure you want to overwrite it?"
