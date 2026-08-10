# kotlin-lsp — Agent Instructions

`kotlin-lsp` is a tree-sitter-backed CLI for Kotlin/Java/Swift symbol queries —
no daemon, no JVM. The CLI is the product: agents discover its capabilities
from `--help`, so the help text and the parser must never drift.

## Start here

At task start, scan [`.agents/rules/INDEX.md`](.agents/rules/INDEX.md) — the
task fact-pattern → rule file table. If a row matches, **STOP and read that
rule file before coding**. Open only the matching rows; never the whole
`.agents/rules/` directory up front. Most of those rules are silent-failure
rules — violating one compiles clean and breaks at runtime or in CI (#227,
#228).

**One exception that cannot wait for a lookup:** before any git write — commit,
push, branch, merge, PR, tag — read
[`git-workflow/RULE.md`](.agents/rules/git-workflow/RULE.md) § Hard bans first.
Those are unrecoverable once done.

Skills live in `.agents/skills/` (see [INDEX.md](.agents/rules/INDEX.md) for
routing). This project also publishes an agent skill for downstream Kotlin
projects at `skills/kotlin-lsp/SKILL.md` — keep its command names in sync with
the real CLI (see `cli-surface-consistency/RULE.md`).

## Non-negotiable core rules

Apply to every code change, no exceptions:

1. **Zero warnings** — fix clippy/fmt, never `#[allow]` without a comment
2. **No hardcoded node kind strings** — use `KIND_*` constants from `src/queries.rs`
3. **Prefer generics over `Box<dyn Trait>`** — static dispatch, zero cost
4. **No bare `unwrap()`** — use `expect("reason")`
5. **Tests in `*_tests.rs` files** — not inline `mod tests {}`
6. **`#[serde(default)]` on new `SymbolEntry` fields** — bump `CACHE_VERSION`
7. **No daemon mode** — Keep CLI simple. No background processes, no Unix
   sockets, no IPC. Each invocation is self-contained. Performance wins come
   from cache optimisations, not daemons.
8. **Install from GitHub, never local compile** — installing **or updating**
   `kotlin-lsp` on a machine always downloads the pre-built binary from GitHub
   Releases (`https://github.com/qdsfdhvh/kotlin-lsp/releases`). Never
   `cargo build --release && cp target/release/kotlin-lsp …` — that bypasses
   the release pipeline. Local `cargo build` is fine for tests/checks only;
   after every release, upgrade the local binary from the new Release asset
   (see `releasing/RULE.md` step 7) and verify `--version` matches the tag.
9. **Post-change documentation check** — any change affecting CLI commands,
   output format, architecture, or developer workflows must update the
   matching docs in the same change: `docs/commands.md`,
   `skills/kotlin-lsp/SKILL.md`, this file, and `README.md` as applicable.
   Default assumption: every CLI change needs a docs + skills update.

## Fixing `check` false positives (test-first)

When fixing `kotlin-lsp check` false positives:

1. First write a `#[test] fn fp_*` regression test that parses the valid
   Kotlin and asserts `data.syntax_errors.is_empty()`
2. Verify the test fails before the fix (reproduces the issue)
3. Then add suppression logic in `collect_syntax_errors()` (in `src/parser.rs`)
4. Verify the test passes after the fix
5. Run `cargo test --bin kotlin-lsp 'parser::tests::'` to confirm no regressions
6. Group related tests under `// ── false positive syntax error regression tests ───────────`

## CLI changes

See [`.agents/rules/cli-surface-consistency/RULE.md`](.agents/rules/cli-surface-consistency/RULE.md)
— the help ↔ parser ↔ docs contract, the five sync points, and the guardrail
tests. Highlights:

- Every subcommand belongs to a group (`search`/`edit`/`tool`/`call`/`type`/
  `module`/`android`/`format`); no orphaned top-level subcommands.
- `--help` must advertise ONLY invocable commands; the consistency tests
  (`args::tests::help_*`) fail the build on drift — never weaken them.
- Deprecation policy: keep old names in `is_subcommand()` +
  `build_subcommand()` for ≤1 release with `eprintln!("[WARN] …")`, remove the
  registration in the NEXT release.

## Quick reference

- CLI reference: **[docs/commands.md](docs/commands.md)** — full command list.
  Quick table: find/refs/hover/complete/context/check, `call hierarchy`,
  `call diff`, `call reach`, `type hierarchy`, `module …`, `android …`,
  `format …`, `search …`, `edit …`, `tool …`, `index`, `index-jars`,
  `cache stats`, `gradle-deps`, `docs <query>`, `capabilities --json`.
- Test coverage: `./coverage.sh [FILE|--all]` (llvm-cov).
- Rust skills: `cargo install cowork && cowork config install`.

## Git workflow / releasing

- **PR-only flow, squash-merge, CI green on all 3 platforms before merging,
  run-before-push gate** — [`git-workflow/RULE.md`](.agents/rules/git-workflow/RULE.md)
- **Releases**: bump Cargo.toml → CHANGELOG → release PR → tag `vX.Y.Z` →
  tag-triggered 5-platform build (darwin-x86_64 dropped 2026-08) → GitHub
  Release. Tag safety and steps:
  [`releasing/RULE.md`](.agents/rules/releasing/RULE.md)
- **CI monitoring via pi-loop**: watch PR CI with
  `MonitorCreate(command="gh pr checks --watch <PR>", onDone="Report CI results")`,
  or poll with a bounded loop. Auto-merge only after all three platforms pass.
- **PR merge → post-merge cleanup**: never end a session on a stale feature
  branch — after squash-merge, `git checkout main && git pull && git branch -d
  <topic>` in the same session. Full lifecycle:
  `.agents/skills/pr-lifecycle/SKILL.md`

## Local planning

For multi-step work, keep `task_plan.md` / `findings.md` / `progress.md`
locally (gitignored) — see [`.agents/rules/local-planning.md`](.agents/rules/local-planning.md).
Durable decisions that become project policy move into tracked docs.
