# CLI Surface Consistency

> The `--help` text is a **contract**: agents and scripts read it to discover
> capabilities. If it advertises a command the parser rejects — or omits a
> command that exists — the tool is written off as less capable than it is
> (#228: `--help` advertised 12 top-level subcommands that all returned
> `unknown subcommand`). Read this rule before touching anything in
> `src/cli/args.rs` or any CLI docs.

## Hard rules

- ❌ NEVER change only one of the command-listing sources. The CLI surface is
  defined in FIVE places that must stay in sync in the SAME change:
  1. `is_subcommand()` (`src/cli/args.rs`) — parser gate: what the first
     positional may be.
  2. `build_subcommand()` (`src/cli/args.rs`) — handlers: what each command
     actually does.
  3. `print_help()` / `help_text()` (`src/cli/args.rs`) — what `--help`
     advertises.
  4. `docs/commands.md` — the human command reference (groups, top-level,
     removed-aliases tables).
  5. `skills/kotlin-lsp/SKILL.md` — agent-facing docs; flat names here are
     broken instructions for every downstream agent.
  `AGENTS.md`'s quick-reference table counts too if it mentions the command.

- ❌ NEVER hand-edit `capabilities --json`. The manifest is GENERATED from the
  help table (`args::capabilities_manifest()`, derived from
  `help_command_lines()`), so it is consistent with `--help` and the parser by
  construction (issue #231 — the old hand-written manifest omitted 25 working
  commands and listed 3 the parser rejected). Keep the help-table format
  machine-parseable: each SUBCOMMANDS line is
  `<cmd> [member] <placeholders>␣␣<description>` with TWO spaces before the
  description; the `help_command_parts_are_structural` test enforces this.

- ❌ NEVER add a new top-level subcommand. New commands go under an existing
  group (`search` / `edit` / `tool` / `call` / `type` / `module` / `android` /
  `format`) — see AGENTS.md rule 11. A group with no home means the command is
  undiscoverable or a top-level orphan.

- ❌ NEVER let the `search` group's catch-all swallow a real subcommand word.
  `build_subcommand`'s `search` arm falls through to semantic search for
  unknown members by design (so `search <query>` works), which silently turns
  `search summarize` / `search annotated` / … into a *different command*. Every
  word documented as a `search` member must have an explicit arm.

- ❌ NEVER delete a command from `is_subcommand()` while leaving it in
  `print_help()` or in `build_subcommand()` (dead handler + live advertisement
  = the #228 bug). Deletion removes ALL of: gate, handler, help line, docs rows,
  skill mentions.

- ✅ ALWAYS run the consistency tests after any CLI change:
  `cargo test --bin kotlin-lsp args::tests::help_` must pass.
  - `help_advertises_only_invocable_commands` — every advertised top-level
    word must be in `is_subcommand()`.
  - `help_group_members_parse` — every advertised group member must parse, and
    every `search` member must resolve to its intended variant (not fall into
    the semantic-search catch-all).
  These tests are the guardrail; NEVER `#[ignore]` or weaken them to make a
  CLI change pass.

- ✅ ALWAYS verify by hand after changing the CLI surface: run `--help`, pick a
  sample of advertised commands (especially the changed ones), and invoke each.
  A command that "parses" in tests can still fail at runtime (e.g. #227's
  nested-tokio-runtime panic — exit 134).

## Checklist for any CLI change

- [ ] `is_subcommand()` gate updated (added / removed / renamed)
- [ ] `build_subcommand()` handler updated (same change)
- [ ] `print_help()` updated — every line invocable, no duplicates, no stale names
- [ ] `capabilities --json` regenerated (automatic — verify it lists the change)
- [ ] `docs/commands.md` group / top-level / removed-aliases tables updated
- [ ] `skills/kotlin-lsp/SKILL.md` command names updated (flat → grouped)
- [ ] `cargo test --bin kotlin-lsp args::tests::help_` green
- [ ] Manual smoke: `--help` output diffed, changed commands invoked

## Consolidation targets (future work)

Ungrouped or flat commands that still exist internally — group when
refactoring (do not leave orphans at top level):

- `gradle-deps`, `sealed` → candidate `gradle` / `inspect` group
- `imports-of`, `annotated` → candidate `query` or `find` filters
- `find-test`, `expect-actual` → candidate `find` sub-mode
- `docs`, `summarize`, `summary-cache` → candidate `info` / `symbol` group
- `index`, `index-jars`, `sources`, `extract-sources`, `cache` → candidate `index` group
- `benchmark`, `doctor` → candidate `diag` group

## Rationale

#228 happened because #211 removed 43 names from `is_subcommand()` only — the
gate — while `print_help()` and `build_subcommand()` kept advertising and
handling them, and no test asserted help↔parser agreement. Help became a
discovery mechanism that generated commands guaranteed to fail. The two
consistency tests exist so that failure mode cannot come back silently.

## Related

- `src/cli/args.rs` — `is_subcommand()`, `build_subcommand()`, `help_text()`,
  `help_command_lines()`, `capabilities_manifest()`
- `src/cli/args_tests.rs` — `help_advertises_only_invocable_commands`,
  `help_group_members_parse`, `help_command_parts_are_structural`,
  `capabilities_manifest_matches_help`
- `src/cli/run.rs` — `print_capabilities` (prints the generated manifest)
- `docs/commands.md` — command reference (kept in sync by rule 11's
  post-change documentation check)
- `skills/kotlin-lsp/SKILL.md` — published agent skill
