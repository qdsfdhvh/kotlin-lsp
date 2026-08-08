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

## Development Workflow

See `.agents/rules/local-planning.md` for the local planning workflow.

Before changing any CLI command, help text, or command docs, read
`.agents/rules/cli-surface-consistency/RULE.md` — it governs the
help↔parser↔docs contract (rule 13).

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

11. **CLI command grouping** — Every subcommand MUST belong to a parent group. No orphaned top-level subcommands.
    - New features NEVER get their own top-level subcommand. Merge into an existing group, or create a group for ≥2 related commands.
    - **Grouped today:** `call hierarchy`, `type hierarchy`, `module`, `android`, `format`.
    - **Consolidation targets** (ungrouped → group when refactoring):
      - `gradle-deps`, `sealed` → candidate `gradle` / `inspect` group
      - `imports-of`, `annotated` → candidate `query` or `find` filters
      - `find-test`, `expect-actual` → candidate `find` sub-mode
      - `docs`, `summarize`, `summary-cache` → candidate `info` / `symbol` group
      - `batch`, `batch-imports`, `new-file`, `inject`, `insert` → candidate `edit` group
      - `index`, `index-jars`, `sources`, `extract-sources`, `cache` → candidate `index` group
      - `tokens`, `tree`, `inspect`, `symbol-graph`, `snapshot` → candidate `debug` group
      - `benchmark`, `doctor` → candidate `diag` group
      - `skills`, `workspace`, `query`, `rename`, `organize-imports` — each a single-use, keep grouped in next pass
    - **Deprecation policy:** keep old names in `is_subcommand()` + `build_subcommand()` for ≤1 release with `eprintln!("[WARN] ...")`. Remove the registration in the NEXT release.
    - **Alias cleanup:** remove `code_action` (underscore variant), remove all deprecated commands that are ≥2 releases old.

12. **Install from GitHub, never local compile** — When installing or updating `kotlin-lsp` on a machine, always download the pre-built binary from GitHub Releases (`https://github.com/qdsfdhvh/kotlin-lsp/releases`). Never use `cargo build --release && cp target/release/kotlin-lsp ~/.local/bin/` — this bypasses the release pipeline (CI signing, checksum verification, cross-platform testing). For local development, `cargo build` is fine for running tests and checks, but the installed binary must come from the release tag.

13. **CLI surface has a single source of truth** — `--help` is a contract: agents discover capabilities from it, so it must advertise ONLY invocable commands, and every command must be advertised. When adding/renaming/removing ANY CLI command, update ALL of these in the SAME change:
    - `is_subcommand()` — parser gate (`src/cli/args.rs`)
    - `build_subcommand()` — handler (`src/cli/args.rs`)
    - `print_help()` / `help_text()` — what `--help` advertises (`src/cli/args.rs`)
    - `docs/commands.md` — command reference
    - `skills/kotlin-lsp/SKILL.md` — agent-facing docs
    Never remove a name from `is_subcommand()` while it is still in `print_help()` or `build_subcommand()` (that split caused #228: help advertised 12 commands the parser rejected). Never let the `search` catch-all swallow a documented member word.
    The consistency tests in `src/cli/args_tests.rs` (`help_advertises_only_invocable_commands`, `help_group_members_parse`) fail the build on help↔parser drift — never `#[ignore]` or weaken them. See `.agents/rules/cli-surface-consistency/RULE.md` for the full checklist.

## CLI Reference

See **[docs/commands.md](docs/commands.md)** for the full command reference.
Quick reference:

| Need | Command |
|------|---------|
| Find definition | `kotlin-lsp find <NAME>` |
| Find references | `kotlin-lsp refs <NAME>` |
| Get signature | `kotlin-lsp hover <FILE> <LINE> <COL>` |
| Completions | `kotlin-lsp complete <FILE> <LINE> [COL]` |
| One-stop context | `kotlin-lsp context <FILE> <LINE> <COL>` |
| Syntax errors | `kotlin-lsp check <FILE>...` |
| Code actions | `kotlin-lsp tool code-action <FILE> <LINE> <COL>` |
| Call hierarchy | `kotlin-lsp call hierarchy <FILE> <LINE> <COL>` |
| Type hierarchy | `kotlin-lsp type hierarchy <NAME>` |
| Organize imports | `kotlin-lsp edit organize <FILE>...` |
| Batch imports | `kotlin-lsp edit imports <FILE>` |
| Index JAR sources | `kotlin-lsp index-jars [ROOT]` |
| Index workspace | `kotlin-lsp index [--root <DIR>]` |
| Cache stats | `kotlin-lsp cache stats` |
| Benchmark | `kotlin-lsp tool bench` |
| Filter by kind | `--kind class,fun,interface` |
| Callers tree | `kotlin-lsp call hierarchy <FILE> <LINE> <COL> --incoming` |
| Callees tree | `kotlin-lsp call hierarchy <FILE> <LINE> <COL> --outgoing` |
| Impact analysis | `kotlin-lsp impact <FILE> <LINE> <COL>` |
| Semantic search | `kotlin-lsp search <QUERY>` |
| KDoc search | `kotlin-lsp docs <QUERY>` |
| Symbol overview | `kotlin-lsp search summarize <NAME>` |
| Find tests | `kotlin-lsp search find-test <FILE> <LINE> <COL>` |
| KMP expect/actual | `kotlin-lsp search expect-actual <NAME>` |
| Module deps | `kotlin-lsp module list / deps / files / packages` |
| Android resources | `kotlin-lsp android activities / composables` |
| Format check | `kotlin-lsp format check <FILE>...` |
| Format apply | `kotlin-lsp format apply <FILE>...` |
| Capabilities | `kotlin-lsp capabilities --json` |
| File inspect | `kotlin-lsp tool inspect <FILE>` |
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

11. **Post-change documentation check** — After any code change that affects CLI commands, output format, architecture, or developer workflows:
    - **docs/commands.md** — update if CLI commands changed (new, renamed, or removed)
    - **skills/kotlin-lsp/SKILL.md** — update if command names/signatures changed (agents use this)
    - **AGENTS.md** — update if development rules or workflows changed
    - **README.md** — update if user-facing behavior or install instructions changed

    Default assumption: every CLI change needs a docs + skills update. Flag it explicitly if not.

## CI Monitoring via pi-loop

This project uses the `pi-loop` extension for CI/CD automation.

### Watch PR CI

After pushing a PR, start a monitor to watch CI completion:

```
MonitorCreate(command="gh pr checks --watch <PR_NUMBER>", onDone="Report CI results")
```

Or poll every 2 minutes until green:

```
LoopCreate(trigger="2m", prompt="Check 'gh pr checks <N>' — if all pass, report; if fail, fix and push", maxFires=20)
```

### Auto-merge on green

```
LoopCreate(
  trigger="2m",
  prompt="Run 'gh pr checks <N>' — if all green, run 'gh pr merge <N> --squash --delete-branch' then delete this loop",
  maxFires=15
)
```

### Test loop (CI simulation)

Before pushing, run a local test loop to catch regressions early:

```
LoopCreate(trigger="1m", prompt="Run 'cargo test 2>&1 | tail -5' — if any failure, stop and report", maxFires=5)
```

### Key loops for this project

| Use case | Trigger | Pattern |
|----------|---------|---------|
| Watch PR CI | `MonitorCreate` + `onDone` | One-shot background |
| Poll PR CI | `2m` cron | LoopCreate with maxFires=20 |
| Auto-merge on green | `2m` cron | Check `gh pr checks` → `gh pr merge` |
| Local pre-push test | `1m` cron | `cargo test && cargo clippy` |
| Dogfood regressions | `5m` cron | Run dogfood.conf projects |12. **Development flow with LSP** — The pre-commit hook runs fmt → test → clippy.  
    Before committing, run `cargo check` for fast type-checking (2-5s vs 30s test).  
    rust-analyzer is recommended for real-time diagnostics during development:  
    `rustup component add rust-analyzer`  
    (Already installed at `~/.cargo/bin/rust-analyzer`).  
    The pre-commit hook uses toolchain binaries directly to avoid rustup PATH issues.
