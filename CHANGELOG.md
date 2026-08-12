## 0.32.0 (2026-08-12)

### feat: codegraph-parity semantic search (acronym segmentation, field filters, generated down-ranking) (#298, #299, #302, #305)

- **Identifier segmentation** matches codegraph's `splitIdentifierSegments`:
  acronym runs split (`HTMLParser` → html/parser), digits glue to their word
  (`base64Encode` → base64/encode), any non-alphanumeric separates, and
  2–32 char / 12-per-name bounds drop minified names. Query-side expansion
  keeps raw casing so typed names like `HTMLParser` match their segments.
- **Field-qualified query filters**: `kind:function name:auth path:src/api
  authenticate` — filters narrow the candidate set, TF-IDF scores within it.
  `lang:`/`language:` accept kotlin|java|swift; unknown prefixes (`TODO:`)
  pass through as plain text; quoted values keep spaces; filters-only queries
  (`search "kind:method path:src/api"`) list all matches by name. `--kind`
  ORs onto query-string filters (was silently dropped).
- **Generated-file detection** (path conventions: `.pb.kt`/`_grpc.kt`/
  `grpc.kt`/`*OuterClass.java`/`.g.java`/`_mock.kt`/`.generated.kt` + a
  head-of-file banner): generated stubs rank below same-name real
  implementations and print `(generated)`.
- `search`/symbol-query commands now honor `--root` and `--no-stdlib`
  (both were parsed but ignored; the latter avoided re-parsing
  `~/.kotlin-lsp/sources` on every invocation). Real-world search quality
  corpus battery guards all of the above.
- **Stemmer fix**: the `-tion` rule mangled short words (`option` → `ope`,
  which prefix-matched `opening`); it now requires length 9–14 and strips
  `ion` + e (`resolution` → `resolute`). `search "kind:class Option"`
  ranks the Option class first again.

### feat: check false-positive suppression for 11+ grammar phantom classes (#300, #306, #308, #309, #310)

tree-sitter-kotlin-sg misparses valid Kotlin into ERROR nodes; each shape
gets a conservative suppression in `collect_syntax_errors`, pinned by
`fp_*` tests with real-error controls:

- single-line class/interface bodies (`class X { fun f() {} }`)
- `fun interface` cascades (already handled, now tested)
- `catch<T>` generic Flow-operator calls
- nullable callable references (`String?::plus`)
- semicolon empty class bodies (`class X : B() {;`)
- local `suspend fun` + callable-reference blocks
- Kotlin 2.x context receivers (call form, function-type receiver form,
  trailing annotations, generic args)
- parenthesized callable statements (`(fn)(call, it)`)
- generic receiver function heads (`Raise<NonEmptyList<Error>>.zipOrAccumulate`)
- detached constructors with KDoc (`@JvmOverloads constructor(...)`, `: this`)

dogfood sweep (check): coroutines 11→0, ktor 61→18, arrow 139→22,
sqldelight 20→0.

### perf: strip raw source lines from the disk cache (#304, #311)

`FileData.lines` replaced with `LazyLines`: parse fills eagerly, cache hits
deserialize empty and fill from disk on first actual use (hover/complete/
find), iter commands never touch disk. index.bin 857→565 KiB on the arrow
fixture (8485→5457 KiB decompressed); cached-hit search ~2.5s→~2.1s.
CACHE_VERSION 21.

### fix: call reach resolves chained calls through return types + delegated properties (#295, #296)

- **Chained calls** (#295): in `receiver.a().b()`, the second callee is now
  keyed against what `a()` *returns*, not against the root receiver's type.
  The parser emits a compound key (`ExampleApi.fetch.onFailure` for
  `api.fetch().onFailure()`); reach resolves it through the declaration
  index's return types, so `Result`-style chaining no longer fabricates
  nodes like `ExampleApi.onFailure` or drops the real edge. When the return
  type is unknown (generic, stdlib, inferred body), the callee stays bare —
  a dropped edge is recoverable, a wrong edge is not.
- **Delegated properties** (#296): `val x by lazy { ExampleClient() }` and
  `val x by someLazy` (`someLazy: Lazy<ExampleClient>`) now give the
  property the delegate's `getValue` result type (`ExampleClient`), so
  `x.send()` resolves through the real type instead of the delegate's own
  type or nothing.

## 0.31.7 (2026-08-12)

### fix: call reach receiver resolution covers primary-constructor properties and lateinit (#292, PR #293)

- A receiver held as a **primary-constructor property** (`class C(private val
  repo: Repo)`) or a **type-annotated property without initializer**
  (`lateinit var repo: Repo`) now resolves through its declared type — #289
  only read initializers, so DI-injected collaborators still truncated.
  Primary-constructor parameters feed the scope directly; property type
  annotations are found by a full-subtree search (fixing a byte-offset bug
  that skipped lateinit's nested type).

## 0.31.6 (2026-08-11)

### fix: call reach receiver resolution extends to class properties (#289, PR #290)

- `call reach` resolves a receiver held as a **class property**
  (`private val client: ExampleClient`) through the enclosing class body —
  both explicit type annotations and initializers feed the scope. #278's
  declared-type resolution previously only consulted function scope
  (parameters + local `val`s), so DI-injected collaborators truncated real
  call chains at the first property.

## 0.31.5 (2026-08-11)

### fix: call reach interface expansion + CHANGELOG gap (#285, #286; PR #287)

- **call reach** (#285): a call through an interface/abstract-typed receiver
  (`r.process()` with `r: Reader`) now expands the implementors' same-named
  methods — the declared-type resolution (#278) keys the callee as
  `Reader.process`, but the body lives in the implementor, so the path
  previously stopped there.
- **CHANGELOG** (#286): 0.31.4's section omitted PR #282 (#278-#281), the
  substantive part of that release; restored.

## 0.31.4 (2026-08-11)

### fix: 0.31.3 regressions (#278-#281, PR #282)

- **call reach** (#278): callees are now resolved through the receiver's
  declared type (parameter types and `val`/`var` initializers in the enclosing
  function) instead of bare-name uniqueness — false paths when a same-named
  method was unique, silent truncation when it was ambiguous, both fixed.
  #267's type-qualified isolation is preserved.
- **find** (#279): `object` and `interface` declarations no longer vanish from
  the index when the file contains a parameterless `annotation class` — the
  #274 salvage pass now also extracts `object` and `interface` from the
  misparsed infix-expression chains.
- **impact / call hierarchy** (#280): a position on a Kotlin keyword
  (`fun`, `suspend`, `override`, …) now returns `No symbol at cursor` instead
  of a report about the keyword. Also fixed a `/tmp` vs `/private/tmp`
  canonicalization mismatch that made the position forms miss files under
  `/tmp` on macOS.
- **startup** (#281): a `find`/`refs` miss loads only the compact symbol
  index (~19 MB) — the 100+ MB library cache is no longer deserialized on the
  miss path; only completion (auto-import FQNs) loads the full cache.

### fix: propagate two recurring bug patterns (PR #283, proactive)

- **`--no-stdlib` is now honored by every indexed command** — `edit imports`,
  `edit insert`, `context` and the search-family commands previously
  hardcoded `build_index(&root, false)` (same shape as the #257 / reach and
  index fixes), always indexing the 72k+ stdlib sources. `CliArgs.no_stdlib`
  now flows globally; verified `edit imports --no-stdlib` triggers zero
  stdlib indexing.
- **File-path canonicalization** — `batch`, `find-test`, `complete` and
  `context` used `std::path::absolute`, which on macOS yields `/tmp` where the
  indexer stores `/private/tmp`, silently missing the file. They now
  canonicalize so URIs match indexer keys.

## 0.31.3 (2026-08-11)

### fix: 0.31.2 regressions (#273-#275, PR #276)

- **call reach** (#273): bare-name entries resolve to the unique `Class.method`
  key (ambiguous names report an explicit error listing candidates); bare
  callees from variable receivers (`client.send()`) follow the unique
  qualified key. Type-qualified isolation from #267 is preserved.
- **find** (#274): enum class lost its kind once indexed when the file also
  contained an `annotation class` — tree-sitter-kotlin misparses it into an
  infix_expression chain that swallows following declarations. A salvage pass
  now extracts `(enum )?class Name` from such chains.
- **startup** (#275): `find`/`refs` no longer deserialize the 100+ MB library
  cache — symbol-index existence is a cheap stat, definitions populate lazily
  only when a query misses, and completion loads library data on demand.
  `find --smart` on a workspace-local symbol drops from ~6.5s to well under a
  second on a normal machine.

## 0.31.2 (2026-08-11)

### fix: 0.31.1 regressions (#266-#270, PR #271)

- **call reach / call diff call graph** (#266): Java declarations now enter
  the call graph — `extract_call_edges` handles Java `method_invocation`,
  `method_declaration` and `constructor_declaration`; Kotlin→Java static calls
  are followed.
- **call reach** (#267): caller keys are type-qualified (`Class.method`) and
  callee keys are qualified when the receiver is statically known, so
  same-named methods on different types no longer merge — `--to` no longer
  reports an unreachable function as reachable.
- **call diff** (#268): the worktree snapshot now includes new unstaged files
  (`git ls-files --cached --others --exclude-standard`) while still excluding
  gitignored build artifacts (#260).
- **find** (#269): `typealias` gets its own kind so `--kind typealias`
  matches; enum class keeps its kind once indexed.
- **startup** (#270): library cache freshness is checked with pure stat before
  any payload deserialization, and fast start skips bulk FileData restore
  (library FileData loads lazily, deserialized once per process); auto-import
  completion reads library package/symbol metadata from the cached map.
- `index --no-stdlib` is now honored (same shape as #257).

## 0.31.1 (2026-08-10)

### fix: 0.31.0 regressions (#259-#263, PR #264)

- **--help**: rows for `call reach` and `type hierarchy` were joined onto one
  line (missing newline in the help table); the `format apply` row is now
  aligned. A guardrail test prevents joined help rows from shipping again.
- **cache stats/clean**: reported `<root>/.kotlin-lsp/cache/`, a path the tool
  never creates. Now reports the real `{root}/.cache/kotlin-lsp` and shows the
  global library cache; `cache clean` no longer deletes the current-format
  `~/.cache/kotlin-lsp/library-*.bin` (rebuilding it re-indexes 72k+ stdlib
  files), only the pre-.cache legacy dir.
- **find (fast mode)**: returned column 1 and no kind, so `--kind` filters
  matched nothing. The column is now pinned to the symbol name and the kind is
  inferred from the declaration keyword.
- **call diff**: a clean working tree could report a HEAD→worktree diff built
  from a gitignored file that shares a symbol name. Worktree snapshots now use
  `git ls-files` (tracked only); uncommitted edits to tracked files stay
  visible.
- **call reach**: paths crossed the Kotlin/Swift boundary on same-named
  functions. Paths now stay within the language of the entry's call edge.

## 0.31.0 (2026-08-10)

### feat(call): call diff — branch-aware call-tree diff between git refs (#251, #255)

- `call diff [<ref1> [<ref2>]] [<name>]` diffs who-calls-whom between two git
  trees, mirroring calldiff for agentic review of call-flow rewiring.
- Complete implementation (#255): branch-aware trees (if/else-if/else,
  try/catch/finally, `when` arms as first-class tree nodes; nested lambdas are
  not attributed to the outer caller), `⇄` cycle markers, and branch children
  rendered without the continuing rail.
- Entry inference: `--entry` is optional — exported functions whose expanded
  call tree changed are inferred (fallback to any changed function).
- git-diff semantics: no refs = HEAD vs working tree; one ref = that vs
  working tree; trailing on-disk positionals are path filters; worktree
  snapshot reads disk (SKIP_DIRS); default `--max-depth` 12.
- CTA: text mode prints `hint: call diff --entry X` for changed entries.

### feat(call): call reach — enumerate call paths from an entrypoint (#250, #257)

- `call reach <entry> [--to <target>] [--max-depth N]` lists every call path
  from an entry to a target (or all reachable paths), DFS with per-path cycle
  protection, depth cap (default 8) and a 1000-path truncation guard.
- `--no-stdlib` is now honored (#257) — reach previously always indexed the
  72924-file stdlib source cache.

### feat(capabilities): report tree-sitter grammar versions (#253)

- `capabilities --json` gains a top-level `grammars` object
  (`kotlin`/`java`/`swift`); `--version` prints them on a second line. Baked
  in at build time from Cargo.lock via a new `build.rs`.

### docs(codebase): per-language query & extraction contract (#252, #254, #256)

- New `docs/codebase/QUERIES.md` documents the three-strategy architecture
  (Kotlin/Swift query vs Java CST walk), mandatory symbol coverage, KIND_*
  constant rule, ordered-pattern query contract, call-edge extraction rules
  and a new-language checklist.
- README / AGENTS.md / skills docs synced with the new commands.

## 0.30.8 (2026-08-08)

### fix(find): library cache no longer shadows workspace declarations (#247)

- `find` dropped workspace declarations whose name also existed in the
  extracted library sources cache (`State`, `Box`, `Result`, ...). The
  library fast-start symbol index used `insert()`, replacing the whole
  symbol list for a name and silently removing the workspace location.
- Workspace declarations now stay in the results, ranked **first** (ahead
  of library hits), and are reachable via `--source-set` / `--module`.

### fix(kind): data class reports 'class', not 'struct' (#246)

- `data class` was classified `struct`, so `--kind class` never matched it
  (and Kotlin has no `struct`, so the working value was undiscoverable).
  Data classes now report `kind: "class"` (detail stays `"data class"`).
- Caches holding the old kind are rebuilt (CACHE_VERSION / SYMBOL_INDEX_VERSION
  bumped).

## 0.30.7 (2026-08-08)

### fix(snapshot): workspace-only symbols; --include-libraries opt-in; dedupe relationships (#242)

- `tool snapshot` emitted every symbol in the global `~/.kotlin-lsp/sources`
  cache — a one-file project produced 773 MB / 1.27M symbols, of which exactly
  one came from the workspace. It now covers the workspace only (library
  symbols are what `find` already reaches through the cache).
- `--include-libraries` restores the old behaviour deliberately, printing a
  stderr warning that output can be hundreds of MB; `--limit <n>` caps the
  symbol count.
- `relationships` (calls/extends/overrides/imports) are deduplicated and
  scoped to the workspace; library edges are never emitted.

### infra(release): drop darwin-x86_64 assets

- Releases now ship 5 assets (linux × x86_64/aarch64, windows ×
  x86_64/aarch64, darwin-aarch64). Intel Macs get the arm64 build via Rosetta 2
  (`install.sh` falls back with a hint).

## 0.30.6 (2026-08-08)

### fix(module list): only real include(...) calls declare modules (#233)

- Lines like `includeGroupAndSubgroups("androidx")` (a dependency-repository
  helper) and `includeBuild(...)` start with "include" but are NOT modules;
  the old `starts_with("include")` match turned Maven group ids into `(0
  files)` rows at non-existent `<repo>/<group-id>` paths, with duplicates.
- `module list` now matches only `include(...)`, dedupes repeated includes,
  and returns exactly the modules the build declares (kataris: 80 → 67 rows).

### fix(doctor): healthy sources cache, empty index cache, exit code, markers (#234 #237)

- The library-sources check counted `*.jar` files, but `extract-sources`
  unpacks jars into directories — a correctly seeded cache always reported
  `[!]`. Doctor now accepts unpacked source files (.kt/.java/.swift) as
  extracted.
- An empty (0 KB) or missing index cache now reports `[!]` and fails the
  summary instead of showing a green check while `find`/`refs` silently have
  no index.
- Failed checks now exit 1 (was exit 0), and the summary points at the real
  `[!]` marker (it previously referenced `[✗]`, which only one line emitted).

### ci(docs): remove paths-ignore so docs-only PRs can merge

- The main-rules ruleset requires 3 checks on every PR, but CI skipped
  docs-only PRs — their checks never appeared and they could never merge.

### docs(agents): post-release local upgrade from the GitHub Release asset

- releasing/RULE.md step 7 + AGENTS.md rule 8: after publishing, upgrade the
  local binary from the new Release asset (download → verify --version →
  replace), never by local compile.

## 0.30.5 (2026-08-08)

### fix(capabilities): manifest generated from the help table, not hand-written (#231)

- `kotlin-lsp capabilities --json` previously drifted from the parser and
  `--help`: it omitted 25 working subcommands and listed 3 the parser rejects
  (`skills`, `edit new-file`), and reported `subcommands: null` for `search`.
- The manifest is now generated from the same help command table the parser
  is verified against (`args::capabilities_manifest()`), so it cannot drift by
  construction. The help SUBCOMMANDS table is machine-parseable:
  `<cmd> [member] <placeholders>␣␣<description>` (two-space boundary).

### test(cli): help ↔ parser ↔ manifest consistency guardrails (#228 #231)

- `help_advertises_only_invocable_commands` — every `--help` top-level command
  must be accepted by `is_subcommand()`.
- `help_group_members_parse` — every advertised group member must parse, and
  every `search` member must resolve to its intended variant (the catch-all
  silently turned `search summarize`/`search annotated`/… into a semantic
  search for the subcommand word).
- `capabilities_manifest_matches_help` — the flattened manifest set equals the
  `--help` set in both directions.
- `help_command_parts_are_structural` — the help table stays machine-parseable.

### docs(agents): CLI surface single-source-of-truth rules

- `.agents/rules/INDEX.md` (kataris-style task→rule lookup), plus
  `cli-surface-consistency/RULE.md`, `git-workflow/RULE.md`,
  `releasing/RULE.md`; AGENTS.md simplified to a router (rule 13: CLI surface
  has a single source of truth).
- Pre-commit hook and rule 8 now run `cargo clippy --all-targets` — plain
  `cargo clippy` skips `#[cfg(test)]` code, which let a test-code lint slip
  through to macOS CI.

## 0.30.4 (2026-08-08)

### fix(cli): no more nested-tokio-runtime panics in docs/search docs/module packages (#227)

- `kotlin-lsp docs <query>`, `search docs <query>` and `module packages [name]`
  aborted with `Cannot start a runtime from within a runtime` (exit 134) because
  they each created a fresh tokio runtime inside the main runtime.
- The four index-backed query commands (`docs`, `imports-of`, `annotated`,
  `module packages`) are now async and await the shared runtime, so they return
  results instead of aborting the process.

### fix(help): `--help` now advertises only invocable commands (#228)

- The `search` group members documented in docs/commands.md and SKILL.md
  (`summarize`, `cache-stats`, `imports`, `annotated`, `find-test`,
  `expect-actual`) are now real commands instead of silently falling through to
  a semantic search for the subcommand word.
- Removed flat aliases (`summarize`, `imports-of`, `annotated`, `find-test`,
  `expect-actual`, `summary-cache`, `organize-imports`, `batch-imports`,
  `inject`, `inspect`, `skills`, `refs-at`, `benchmark`, `tokens`, `tree`, …)
  from `--help`; the grouped forms (`search …`, `edit …`, `tool …`) are listed
  instead. Duplicate entries for `inspect`/`benchmark`/`tree` removed.
- `--help` now also lists previously-missing invocable commands: `gradle-deps`,
  `tool doctor`, `tool tokens`, `capabilities`, `impact`, `sources`,
  `extract-sources`, `cache stats`, `type sealed`, and every `search`/`edit`/`tool`
  member. `tool tokens` now honors `--resolve`/`--phases`/`--tree` flags.
- Every top-level command shown in `--help` is now accepted by the parser, and
  every accepted command is listed.

## 0.30.3 (2026-07-29)

### fix(check): accept nested annotated function types (#225)

- `kotlin-lsp check` no longer reports four false syntax errors for valid multi-line
  block-body declarations such as `@Composable ((Int) -> Unit) -> Unit`.
- Genuine syntax errors in the same signature, function body, and following declarations
  remain visible.

### ci(windows): install runtime search tools from official releases (#225)

- Windows CI downloads pinned official `ripgrep` and `fd` release assets instead of
  depending on the Chocolatey community feed.
- Runtime-tool installation now fails fast if either executable is unavailable.

## 0.30.2 (2026-07-22)

### feat(call-hierarchy): accept symbol name in addition to file/line/col (#221)

- `kotlin-lsp call hierarchy <name>` resolves unique symbols directly.
- Ambiguous symbols report candidates in JSON or text.
- Existing file/line/col positional form is backward compatible.

### fix(docs): restore `docs` as a top-level alias for `search docs` (#221)

- `kotlin-lsp docs <query>` now works, matching the --help output.

### feat(capabilities): machine-readable CLI capability manifest (#221)

- `kotlin-lsp capabilities --json` outputs a stable JSON manifest with version,
  commands, flags, and subcommands.

## 0.30.1 (2026-07-20)

### fix(search): restore `kotlin-lsp search <query>` shorthand (#216)

- In v0.30.0 the CLI parser rejected `kotlin-lsp search "query"` (required `search semantic <query>`)
  while the help text and bundled SKILL.md documented the shorthand form.
- Unrecognized search subcommands are now treated as semantic search queries instead of erroring.
- `search docs <query>` and `search semantic <query>` continue to work.

## 0.30.0 (2026-07-19)

### feat(lsp): --agent flag for AI coding agent startup (#209)

- `kotlin-lsp --agent` starts with agent-optimized defaults (no diagnostics, semantic tokens, inlay hints, code actions, folding, formatting, etc.)
- `initializationOptions.features` for fine-grained control

### feat(composable): multi-file @Composable call graph (#204)

- `find_composables_multi()` scans multiple files and detects cross-file calls.
- New `external_calls` field on `ComposableInfo`.

### feat(check): --when-exhaustive flag (#201)

- Lightweight CST-level scan for non-exhaustive `when` expressions.
- `kotlin-lsp check --when-exhaustive Foo.kt`

### feat(lsp): index compaction (#205)

- `compact_stale_entries()` on `did_close` and `shutdown`.
- Removes files no longer on disk from the index.

### feat(query): agent batch query — implementations, subclasses (#203)

- `tool query` now accepts `implementations`, `subclasses` types.
- JSON-in/JSON-out: `echo '[{"type":"subclasses","name":"Result"}]' | kotlin-lsp tool query --json`

### perf(stdlib): O(1) HashMap-backed symbol lookup (#210)

- Replaces O(n) linear scan in `hover()` with lazy-init HashMap index.

### feat(lsp): FeatureToggles via initializationOptions (#202)

- Clients can disable features: `{ features: { diagnostics: false, ... } }`.
- Default: all true (full IDE mode).

### chore(cli): remove 43 deprecated CLI aliases from is_subcommand (#211)

- `--help` no longer shows `organize-imports`, `code-action`, `benchmark`, etc.
- Old names still work internally but are hidden.

### fix(install): cargo detection + tmp trap (#200)

- Detects toolchain-direct cargo binary to avoid rustup proxy issues.
- Fixes `tmp: unbound variable` on script exit.

### docs

- All docs updated to grouped command names (#212).
- `AGENTS.md`: CLI grouping rules, CI monitoring patterns, post-change doc checklist, development flow with LSP.

### tests

- 1,259 → 1,301 (+42)
## 0.29.0 (2026-07-19)

### feat(gradle): plugin detection + config-aware deps + android block (#191)

- `kotlin-lsp gradle-deps` now shows plugins, dependency configurations, and Android block settings.
- Plugin parser handles `id("...")` and `kotlin("...")` syntax.
- 9 dependency configurations tracked: implementation, api, kapt, ksp, etc.
- Android block extracts namespace, compileSdk, minSdk, targetSdk, applicationId.

### feat(sealed): is_sealed detection + type sealed command (#191, #193)

- `SymbolEntry.is_sealed` field detected from tree-sitter line prefix.
- `kotlin-lsp type sealed <name>` lists direct subclasses of sealed types.
- CACHE_VERSION bumped 16 → 17.

### feat(cli): command grouping — search, edit, tool (#193)

30+ top-level commands consolidated into 4 groups:

| Group | Subcommands |
|-------|-------------|
| `search` | docs, semantic, summarize, cache-stats, imports, annotated, find-test, expect-actual |
| `edit` | rename, batch, imports, inject, insert, new, organize |
| `tool` | inspect, graph, snapshot, bench, doctor, workspace, query, skills, code-action, tokens, tree |
| `type` | hierarchy, **sealed** (new) |

Old names still work with deprecation warnings.

### perf(cache): Gradle build script staleness detection (#191)

- `GradleDeps.mtime_secs` tracks when `build.gradle.kts` was last parsed.
- `ensure_gradle_indexed()` auto-reparses on build file change.

### fix(check): suppress FP for annotated function type with named params (#194)

`kotlin-lsp check` no longer reports false-positive syntax errors for patterns like
`@Composable (value: String) -> Unit`.

### docs

- `docs/commands.md`: full rewrite with group/standalone/deprecated sections.
- `skills/kotlin-lsp/SKILL.md` + `references/commands.md`: updated with grouped names.
- `AGENTS.md`: CLI grouping rules, CI monitoring patterns, post-change doc checklist.
- `rustfmt.toml`: ignore `tests/dogfood/` fixtures.
- `.githooks/pre-commit`: use `cargo fmt` instead of `find | xargs rustfmt`.

### tests

- 1,259 → 1,301 (+42 tests)
## 0.28.0 (2026-07-17)

### feat(gradle): shallow dependency parsing without daemon

- New `src/gradle/` module: TOML-based `libs.versions.toml` + tree-sitter `build.gradle.kts` parsing.
- `--gradle` flag enables on-demand source JAR indexing for external dependencies.
- `kotlin-lsp gradle-deps` subcommand to inspect parsed dependencies.
- Gradle cache source JAR discovery (`~/.gradle/caches/modules-2/files-2.1/`).

### refactor(cli): group overlapping subcommands into parent commands

New grouped commands. Old names still work with deprecation warnings.

| New | Old (deprecated) |
|-----|-------------------|
| `kotlin-lsp call hierarchy <file> <line> <col>` | `callers`, `callees`, `call-hierarchy` |
| `kotlin-lsp type hierarchy <name>` | `implementations`, `subclasses`, `type-hierarchy` |
| `kotlin-lsp module [list\|deps\|files\|packages]` | `modules`, `module-deps`, `module-files`, `package-deps` |
| `kotlin-lsp android [activities\|composables]` | `android-activities`, `android-composables` |

### fix(skill): correct find --fuzzy → search in bundled SKILL.md (#172)

### fix(search): add stop words + fix stemming false positives (#168)

### docs: documentation overhaul

- README rewritten (55 lines, zero duplication).
- New `docs/commands.md` — authoritative command reference.
- Removed `docs/editors.md`, `docs/lsp.md` — agent-first tooling.
- Updated `docs/comparison-with-official.md`, `docs/architecture.md`.
- AGENTS.md CLI table updated for new grouped commands.
- SKILL.md fully rewritten with new command names and links.

### chore: housekeeping

- Removed `contrib/` — legacy IDE extensions and Python wrappers.
- Updated LICENSE copyright.
- Removed dead code from `call_graph.rs`, `inheritance.rs`.

### feat: semantic search + AI summary cache + SuperKind distinction (#166 #167)

**Semantic Search** (`kotlin-lsp search`)
- TF-IDF semantic search with BM25-inspired scoring over symbol names, KDoc, signatures, and return types.
- Tokenizes on camelCase, snake_case, and whitespace boundaries.
- Prefix matching for incomplete query tokens.
- English stemming: "refreshed" matches "refresh", "running" matches "run".
- `--json` and `--limit N` flags.

**AI Summary Cache** (`summarize --cached` + `summary-cache`)
- Pre-computed structured summaries for all public symbols, built from indexed FileData.
- `kotlin-lsp summarize <name> --cached` — fast symbol overview without re-parsing source.
- `kotlin-lsp summary-cache` — cache statistics (total, docs, signatures, by kind).
- Members field omitted in cached mode (use `--expand` for CST-based members).

**SuperKind Distinction (extends vs implements)**
- New `SuperKind` enum: `Extends`, `Implements`.
- Java supertypes tagged at CST level: `extends` → Extends, `implements` → Implements.
- Kotlin/Swift supertypes default to Extends (cannot distinguish without resolution).
- `CACHE_VERSION` bumped to 16.

**Cache Path Migration**
- Project-local cache moved from `.kotlin-lsp/cache/` to `.cache/kotlin-lsp/`.
- `.cache/` added to `.gitignore`; legacy `.kotlin-lsp/` cleanup preserved.

## 0.26.8 (2026-07-15)

### perf: local dev optimizations (#156)

- `.cargo/config.toml`: Windows `rust-lld` linker (3-5x faster than MSVC `link.exe`).
- `.config/nextest.toml`: `cargo nextest` config with retries, timeouts, and CI profile.

### feat: uninstall script (#155)

- `kotlin-lsp uninstall` command to clean up library sources, caches, and guide binary removal.

## 0.26.7 (2026-07-15)

### Fix: find/refs include library sources (#151)

- FIXED: `find` and `refs` now include declarations from extracted Gradle library sources
  (`~/.kotlin-lsp/sources`). Library symbols are again searchable.

## 0.26.6 (2026-07-15)

### feat: project-local cache + cache management (#148)

- Workspace cache now lives at `{project}/.kotlin-lsp/cache/index.bin` instead of global XDG dir.
- Add `kotlin-lsp cache clean`, `cache list`, `cache stats` commands.

### perf: conservative optimizations (#150)

- Skip library cache load for commands that don't need it (benchmark, diagnose, etc.).
- Skip cache save on warm starts with 0 files parsed.
- AGENTS.md: no-daemon rule.

### test: dogfood.conf with 5 real projects

- ktor, nowinandroid, coroutines, sqldelight, arrow — 100/100 commands pass.

## 0.26.5 (2026-07-15)

### Fix: --kind fun normalization (#147)

- `--kind fun` now matches `function` (short form → full SymbolKind name).

## 0.26.4 (2026-07-15)

### Fix: remaining relative-path panics (#146)

- `impact`, `callers`, `callees`, `find-test`, `batch-imports` no longer panic on relative paths.

## 0.26.3 (2026-07-15)

### Fix: inspect --expand panic on relative paths (#143)

- FIXED: `inspect`, `call-hierarchy`, `code-action`, `refs` now resolve relative paths to absolute
  before constructing a file URL, preventing `.expect("valid file path")` panics.
- 6 CLI regression tests: `./`, `../`, bare filename, call-hierarchy, code-action, inspect --expand

## 0.26.2 (2026-07-15)

### Fix: library source cache can return stale signatures (#141)

- FIXED: `artifact_dir_name` now includes the version (`{group}.{artifact}-{version}`),
  preventing different versions of the same artifact from colliding in the source cache.
- 5 regression tests: includes_version, different_versions, fallback, SNAPSHOT, -sources classifier

## 0.26.1 (2026-07-14)

### Fix: `find --kind fun` drops top-level functions (#139)

- FIXED: `--kind fun` / `--kind class` filtering for `find` and `refs` now works correctly.
  Root cause: `CliResult.kind` was always empty, so kind filters matched nothing.
  Added `enrich_result_kinds()` to backfill `kind` from the indexer's `SymbolEntry.kind`.

### Test Coverage

- `src/indexer/symbol_graph_tests.rs` — 18 unit tests covering all SymbolGraph methods
- `src/query/engine_tests.rs` — 17 unit tests for WorkspaceQueryEngine
- `src/cli/query_engine_tests.rs` — 18 unit tests for IndexQueryEngine (all QueryEngine trait methods)
- `src/cli/snapshot.rs` — 8 unit tests for `is_entry_point` + `collect_relationships`
- 4 regression tests for #139 in `integration_tests.rs`
- 2 symbol-graph CLI integration tests in `tests/cli_commands.rs`

## 0.26.0 (2026-07-14)

### Phase 30 — SymbolGraph + callees refactor (#135)

- `SymbolGraph` struct in `src/indexer/symbol_graph.rs` — typed query API over index edge maps
- `callers` command uses pre-built edge index; `callees` refactored to graph-based lookup (no tree-sitter re-parse)
- `TypeHierarchy` gains `depth` field for future recursive traversal
- CACHE_VERSION bumped 13 → 14
- `.pi/` and `.tours/` added to `.gitignore`

### Phase 31-38 — Already implemented

- `snapshot` — full workspace JSON export with symbols + relationships
- `symbol-graph` — JSON export of call/inherit/import/override edges
- `fuzzy` flag, `imports-of`, `annotated`, `package-deps`, `docs` commands
- `implementations`, `subclasses` with recursive tree traversal
- `query` — batch query from stdin JSON specs

### Phase 39 — Query Engine foundation (#136)

- `WorkspaceQueryEngine` in `src/query/engine.rs` — unified API over Indexer + SymbolGraph
- Single entry point for CLI and LSP queries, eliminating code duplication

---

## 0.25.0 (2026-07-13)

### Phase 29 — Rich Symbol Model (#130)

Extract `return_type`, `parameters`, and `documentation` (KDoc) at parse time.
Previously these fields were always `None`/empty — now populated during
tree-sitter parsing.

- **return_type**: from function/property declarations (`fun foo(): Boolean` → `Some("Boolean")`)
- **parameters**: from function params (`fun login(u: String, p: String)` → `[("u","String"),("p","String")]`)
- **documentation**: KDoc extracted from preceding lines
- `summarize` fast path now reads `documentation` from index
- CACHE_VERSION bumped 12 → 13
- Fixed `extract_detail` to not trim `=` inside parens (default param values)
- Fixed `parse_param_from_sig` to strip default values from type

### Phase 31 — Workspace Snapshot (#131)

New `kotlin-lsp snapshot` command — single JSON export of the complete workspace:
project info, module structure, full symbol metadata, relationship graph
(calls/extends/overrides/imports), and entry points.

```bash
kotlin-lsp snapshot                          # full snapshot as JSON
kotlin-lsp snapshot --exclude-relationships  # symbols + modules only
```

### CLI Subcommand Registration Fix (#130)

26 commands were missing from `is_subcommand()` (including `summarize`,
`callers`, `callees`, `impact`, `modules`, `inspect`, etc.) — causing them to
appear as unknown commands. All 53 subcommands now properly registered.


## 0.24.2 (2026-07-13)

### Phase 23 — Generic Type Param Substitution (#112)

- **Function-level type param substitution** — generic functions now show
  substituted types in hover/completion when called with explicit type args.
  Example: `transform<String, Int>("hello", 42)` → hover shows
  `T → String, R → Int`.
- New `call_site_type_args` on `IndexRead` trait with tree-sitter CST extraction.
- `build_type_param_subst_impl` now falls back to function-level substitution
  after enclosing-class substitution.

### Phase 26 — Implicit Receiver Completion (#112)

- **Bare completion now includes `this` members** — when typing without a dot
  prefix inside a class body or scope-function lambda (`apply`/`run`/`with`),
  non-private members of the implicit receiver are offered.
- New `add_implicit_receiver_completions` resolves `this` type via
  `infer_lambda_param_type_at` (scope functions) + `enclosing_class_at` (class
  bodies). Filters private members, deduplicates against existing items.

### Agent Knowledge Server — Phases 29-39 (#115)

- **Symbol Graph** — export full workspace symbol graph as JSON:
  `kotlin-lsp symbol-graph --root <dir>` includes nodes, import edges,
  override edges, and call edges.
- **Type Hierarchy Graph** — `type-hierarchy --graph` outputs tree-format
  supertype chains.
- **Supertypes Index** — `supertypes_index` for fast forward-edge lookups
  without scanning all files.
- **Rich SymbolEntry fields** — `parent_fq_name`, `return_type`,
  `parameters`, `documentation` populated from tree-sitter signature
  extraction.
- **Enclosing class resolution** — `parent_fq_name` computed via
  range-based class containment for all non-class symbols.

### Fixes

- **Annotated function-type parsing** (#121, #127) — functions with
  `@Composable (Int) -> Unit` parameters are now correctly extracted
  from tree-sitter ERROR nodes via `extract_functions_from_errors`.
- **Unknown subcommand exit code** (#125) — misspelled/unavailable
  subcommands now exit 1 with a clear error message.

### Internal

- `complete_bare` gains new `cursor_line: Option<u32>` parameter for scope-
  aware completion.
- SKILL.md updated with symbol-graph, integration tests, and min_version
  0.24.0.
- `coverage.sh` added for instant targeted or full-project coverage reports.

### Agent Knowledge Server — Phases 13-21 (#109)

- **Call Graph** — `callers` / `callees` with depth control and JSON tree output
- **Reference Classification** — `--ref-kind call|read|write|override|import|type-use`
- **Impact Analysis** — `impact` command: risk score, ref breakdown, callers
- **Module Dependency Graph** — `modules`, `module-deps`, `module-files` with Gradle detection
- **Symbol Summarization** — `summarize` command: kind, signature, members, KDoc
- **Test Finder** — `find-test` command: locate tests by naming, imports, source sets
- **KMP expect/actual Resolution** — `expect-actual` command
- **Android Resource Graph** — `android-activities`, `android-composables`
- **Workspace Graph** — `workspace` command: module→package→symbol overview

### Upstream Ports

- **`this`-inference fixes (Phase 22)** — correct hover for `this` in `apply{}`, `run{}`, `let{}`
- **Companion-object member priority (Phase 24)** — `MyClass.foo` prefers Companion member
- **Nullable receiver diagnostics (Phase 25)** — flag plain `.` access on nullable types
- **`check --diagnose` (Phase 27)** — extend `check` with call-argument validation
- **KDoc UTF-8 fix (Phase 28)** — fix KDoc corruption on non-ASCII characters

### Dependencies

- **Switch to crates.io** — all tree-sitter deps now from crates.io, `[patch.crates-io]` removed
- `tree-sitter` → `0.26`, `tree-sitter-kotlin-sg` → `0.4`, `tree-sitter-java` → `0.23`, `tree-sitter-swift` → `0.7`

### Bug Fixes

- Fix `context` panic with relative file paths
- Fix `PathBuf` import in `diagnose.rs`

## 0.23.0 (2026-07-09)

- **CLI format (Phase 12)** — `format check <files>` and `format apply <files>` subcommands
  for ktlint integration, matching Spotless `spotlessCheck` / `spotlessApply` semantics.
  Supports `--json`, directory recursion, and `--dry-run` preview.
- **Semantic insert enhancements (Phase 4)** — `insert-import` now auto-generates
  import statements from FQNs. Improved indent calculation for `insert-member`
  and `insert-function` using tree-sitter. Added `insert-override` with `--name` support.
- **Performance (Phase 11)** — zstd compression for index and library caches.
  Reduces cache I/O and on-disk footprint for large Android/KMP projects.
## 0.22.0 (2026-06-21)

- **CLI code-action parity** (Phase 1) — `code-action` now lists real LSP actions
  instead of an empty placeholder. Supports `--json`, `--apply`.
- **Safe edit preview/apply engine** (Phase 2) — shared `WorkspaceEdit` → file edit
  engine with dry-run, validation, structured output. Used by code-action --apply.
- **Missing-import workflow** (Phase 3) — `batch-imports` resolves FQNs via index,
  classifies as unique/ambiguous/unknown. Supports `--apply`.
- **Semantic insert primitives** (Phase 4) — `insert-import`, `insert-member`,
  `insert-function` commands. Index-aware insertion positions.
- **Rename/refactor dry-run** (Phase 5) — `rename <file> <line> <col> <newName>`
  with symbol-aware word replacement, dry-run preview, and --apply.
- **Use-site context** (Phase 6) — `context` now resolves call sites via
  `cst_call_info`, returning function_name, qualifier, active_parameter.
- **Reference precision** (Phase 7) — `refs-at <file> <line> <col>` filters
  name-based candidates by declaration package context.
- **Agent snapshot command** (Phase 8) — `inspect <file>` returns package, imports,
  symbols, error count in one command.
- **Machine-readable doctor** (Phase 9) — `doctor --json` returns structured checks
  with name, status (ok/warn/error), and message.
- **Skill docs audit** (Phase 10) — `skills/kotlin-lsp/SKILL.md` updated with all
  new commands.
- **Performance findings** (Phase 11) — measured one-shot CLI latency (~1.4s warm
  cache); library cache deserialization identified as bottleneck.
## 0.21.0

- **Configurable Kotlin formatter** — `--format-tool ktlint|ktfmt` CLI flag.
  ktfmt (native, no JVM) is default; ktlint via `--format-tool ktlint`.
- **CLI flag parsing fix** — `--format-tool` now works in any position
  relative to `--port` / `--index-only`.
- **organize-imports: delegated property fix** (fixes #91) — `getValue`/`setValue`
  operator imports are no longer incorrectly removed when `by` is used.
  `val` delegates keep only `getValue`; `var` delegates keep both.
- **Smoke test fix** — `smoke_inlay_hints` now indexes the source file
  via `sourcePaths: ["src"]`, fixing a long-standing CI failure.
## 0.20.0

- **Named-arg completion** — appends `name =` items when cursor is inside a
  call expression, matching function parameter names from signature lookup.
- **Copy() synthesis for data classes** — compiler-generated `copy()` function
  is now indexed as a `SymbolEntry` for every data class, enabling completion
  and hover. (Port from upstream kmp-lsp)
- **`::class` literal type resolution** — inlay hints resolve types from
  `::class` literal arguments (e.g. `retrofit.create(Api::class.java)` shows
  `: Api`). (Port from upstream kmp-lsp)
- **Bare type parameter fallback** — when CST inference returns a bare type
  param (`T`, `R`), falls through to text-based inference for better results.
- **`create()` added to generic factory allowlist** — `create<SomeType>()` now
  resolves the return type for inlay hints and hover.
- **`call_site_type_arg_strings()` helper** — extracts type arguments from
  call sites for better generic resolution.
- **JAR symbol indexing** — `kotlin-lsp index-jars` extracts symbols from `*-sources.jar`
  for go-to-definition, hover, and completion of library symbols.
- **ScopeContext for completion** — `LambdaScope` + `ScopeContext` structs
  ported from upstream for richer completion context analysis.
- **Codebase refactoring** — `backend/mod.rs` decomposed from 1353→880 lines
  into focused sub-modules: `capabilities.rs`, `init.rs`, `commands.rs`,
  `progress.rs`.
- **Tests** — 926 total (905 → 926, +21 tests).

## 0.19.2

- `textDocument/rangeFormatting` — new LSP handler that reuses existing
  external formatters (ktfmt, google-java-format, swift-format) and returns
  edits limited to the requested range when possible.
- `textDocument/typeDefinition` — real type-definition resolution instead of
  delegating to regular goto-definition. Resolves `val x: Foo` → `Foo`,
  `fun foo(): Bar` → `Bar`, lambda params, `it`/`this`, and falls back to
  regular definition when no type-specific target is available.
- **Inlay hint configuration** — receive `initializationOptions.inlayHints`
  to toggle `lambdaIt`, `lambdaParams`, `thisHints`, `untypedVars` hints.
  All default to `true` when client omits config. (PR #55)
- **"Specify type explicitly" code action** — inserts `: InferredType` for
  untyped `val`/`var` declarations. (PR #56)
- **"Add names to call arguments" code action** — converts positional args
  to named args: `foo(a, b)` → `foo(param1 = a, param2 = b)`. (PR #57)
- **`sources explain`** — CLI diagnostics showing why each source root is
  included. (PR #58)
- **`cache stats`** — CLI command for cache diagnostic information. (PR #60)
- **Duplicate import diagnostics** — warns on repeated import statements. (PR #61)
- **`refs --explain`** — labels each reference as declaration/override/import. (PR #62)
- **`--kind` filter** — filter `find`/`refs` by symbol kind (`class,fun,interface`). (PR #63)
- **Unresolved reference import fixes** — attaches import quick-fix to LSP
  diagnostics. (PR #64)
- **`code-action` CLI** — list/apply code actions from the command line. (PR #65)
- **Android project detection** — detects namespace from `AndroidManifest.xml`
  or `build.gradle.kts`. (PR #66)
- **Deprecated symbol diagnostics** — WARNING on `@Deprecated` declarations. (PR #67)
- **Override fixture tests** — regression tests for annotated override methods. (PR #68)
- **`batch-imports` CLI** — scans files for import candidates. (PR #69)
- **File templates** — `kotlin-lsp new-file <template> <Name>`. (PR #70)
- **Pre-commit hook + architecture docs** — `.githooks/pre-commit`, `docs/architecture.md`. (PR #71)
- **`context --expand` recursive depth** — up to 4-level type chain resolution. (PR #72)
- **Kotlin inspections** — redundant `val x = x` self-assignment detection. (PR #73)
- **Enhanced context JSON** — references count included in output. (PR #74)
- **Spelling diagnostics framework** — placeholder for spelling checks. (PR #75)
- **Cache domain split + benchmark CLI** — `CacheDomain` enum, `kotlin-lsp benchmark`. (PR #76)
- **Tests** — 905 total (889 → 905, +16 tests).

## 0.18.0

- inject — batch type injection for files
- list-types — project-level type listing
- context --expand — type chain tracing
- textDocument/typeDefinition LSP
- Claude Code hooks — auto-inject on read, auto-check on write
- AGENTS.md / CLAUDE.md with release procedure
- +31 tests (889 → 904)
- README trim to 125 lines

## 0.17.0

- Completion: deprecated tag, label_details (inline params + return type)
- Hover: visibility, deprecated warning, data class properties
- CodeAction: add missing import, suppress warning, generate overrides
- New CLI: check, organize-imports, context, call-hierarchy, type-hierarchy
- LSP: call hierarchy (incoming + outgoing), selection range, document formatting, on-type formatting
- Rust: expect() over unwrap(), Vec::with_capacity(), AGENTS.md + CLAUDE.md

## 0.16.1
## 0.14.0

- **`sourceRoots` scoping for rg searches** — `rg`-based references, definitions, and symbol searches are now scoped to the configured `sourceRoots` entries from `workspace.json` (IntelliJ/Android Studio module source roots). Searches no longer scan generated code or build output directories when source roots are configured. All callers (Backend, CLI fast mode, resolver step-5, infer) use a single central `Indexer::rg_scope_for_path` path so scoping is consistent across the board. Fixes [#78](https://github.com/Hessesian/kotlin-lsp/issues/78).

## 0.13.0

- **Zed extension** — `contrib/zed-extension` registers `kotlin-lsp` as a first-class Zed language server for Kotlin, Java, and Swift. Resolves the binary from `$PATH`; no symlinks or `binary.path` overrides required. Install locally with `zed --install-dev-extension contrib/zed-extension` or copy to `~/.config/zed/extensions/kotlin-lsp/`.
- **`complete` CLI subcommand** — `kotlin-lsp complete <file> <line> [col]` returns completion candidates as JSON (`[{label, kind, detail?, import?}]`). Flags: `--dot` (auto-place cursor after last `.` on the line), `--eol` (end of trimmed line), `--no-stdlib` (skip `~/.kotlin-lsp/sources` for ~5× faster project-only completions). Useful for agent/script integration without a running LSP daemon.
- **Library cache** — `sourcePaths`-indexed files are saved to a deterministic on-disk cache (`~/.cache/kotlin-lsp/library-<hash>.bin`). Subsequent restarts skip re-parsing unchanged library sources, making warm startup significantly faster on large projects with many source JARs.
- **Library visibility filtering** — symbols marked `private` or `internal` in library source files are stripped from the index. Only `public` and `protected` symbols are indexed for external libraries (inaccessible members add noise to completions and workspace symbol search).
- **Android SDK auto-detection** — the Android platform sources (`$ANDROID_HOME/sources/android-XX/`) are now indexed automatically. Detection order: `sdk.dir` in `local.properties` → `$ANDROID_HOME` → `$ANDROID_SDK_ROOT`. The highest installed API level is picked. No `sourcePaths` config or `extract-sources` needed for Android SDK classes (`Activity`, `Context`, `View`, etc.).
- **`@` completion trigger** — `@` is now a trigger character so annotation completions (`@Composable`, `@Inject`, `@Override`, …) appear immediately after typing `@`.
- **LSP smoke test suite** — `tests/lsp_smoke.rs` exercises the full server over stdio: initialization, workspace symbol, go-to-definition, hover, and inlay hints. Runs against a temp fixture without a real Android project.
- **Stack overflow fix** — `has_fun_interface_descendant` converted from recursive to iterative to prevent stack overflow on deeply nested class hierarchies.

## 0.12.1

- **Auto-include `~/.kotlin-lsp/sources` in LSP server** — after running `kotlin-lsp extract-sources`, extracted library sources are indexed automatically without any manual `sourcePaths` configuration in the LSP client.
- **Docs overhaul** — README restructured for progressive disclosure (VS Code Quick Start first, condensed config, detailed options moved to `docs/features.md`). `docs/editors.md` reordered with VS Code at the top including platform-specific `.vsix` install commands.

## 0.12.0

- **`extract-sources` CLI** — `kotlin-lsp extract-sources` walks the Gradle cache (`~/.gradle/caches/modules-2/files-2.1`), deduplicates `*-sources.jar` by keeping the latest version per artifact, and extracts `.kt`/`.java` sources to `~/.kotlin-lsp/sources`. Supports `--dry-run`, `--output`, `--gradle-home`, and optional group/artifact filter patterns. CLI commands (`find`, `refs`, `hover`, `index`) now automatically include `~/.kotlin-lsp/sources` so extracted library sources are indexed without any manual configuration.
- **`sources` CLI** — `kotlin-lsp sources` lists auto-discovered source roots and their origin (`workspace.json` or `build-layout`). Prints a tip to run `extract-sources` when build-layout detection is active.
- **Zero-config source root discovery** — the LSP server and CLI now auto-discover source roots from JetBrains `workspace.json` (exported by IntelliJ/Android Studio) and from standard Gradle/Maven build layouts (`src/main/kotlin`, `src/main/java`, per-module subprojects). No manual `sourcePaths` configuration needed for most Android projects.
- **Extension robustness** — fixed hang on large workspaces; `shutdown` is now non-blocking; top-level `object` declarations emit `STATIC` semantic token modifier.

## 0.11.0

- **Semantic tokens** — full `textDocument/semanticTokens/full` implementation with two-phase pipeline: Phase 1 (CST classification via tree-sitter) + Phase 2 (cross-file resolution via index). Supports Kotlin, Java, and Swift.
- **`tokens` CLI command** — `kotlin-lsp tokens <file>` dumps semantic tokens (CST-only by default, 19ms). `--resolve` opts into Phase 2 cross-file resolution.
- **`tree` CLI command** — `kotlin-lsp tree <file>` dumps the tree-sitter parse tree for debugging.
- **VS Code extension** — bundled extension with syntax highlighting, binary auto-discovery, and support for Kotlin, Java, and Swift files. GitHub Actions release workflow builds cross-platform binaries and packages `.vsix`.
- **Performance** — CLI `tokens` defaults to CST-only mode (19ms vs 1.1s with full index). Added `docs/performance.md` with benchmarks and profiling guide.
- **`fd` optional** — file discovery falls back to `walkdir` when `fd` is not installed.

## 0.10.0

- **CLI mode** — `kotlin-lsp find|refs|hover|index` subcommands: use kotlin-lsp as a standalone tool without an editor or daemon
- **Auto mode** — uses cached index when available, falls back to fast rg/fd automatically (no flag needed)
- **`--fast` flag** — pure rg/fd, zero startup cost; useful in scripts and CI
- **`--smart` flag** — builds index if missing, uses full cross-file accuracy
- **`--json` flag** — machine-readable output for piping/scripting
- **`--root` flag** — workspace root override; defaults to nearest `.git` dir or cwd
- **`--help` / `--version`** — standard CLI flags; work before or after subcommand
- **Helpful errors** — `--find` (common mistake) prints `'find' is a subcommand, not a flag`

## 0.9.4

- **Phase 12 refactoring complete** — replaced bool/tuple returns with named `struct`s for clarity (e.g., `ScanResult`, `NamedResult`); downgraded unreachable `pub` to `pub(crate)` across the binary crate; fixed bare `unwrap()` and double-ref anti-patterns; replaced blocking `std::fs::read_to_string` with `tokio::fs` in spawned tasks.
- **Hexagonal architecture cleanup** — replaced `Option<tower_lsp::Client>` in `Indexer` with `ProgressReporter` outbound port trait. `LspProgressReporter` adapter in backend sends `$/progress` notifications; `NoopReporter` used in CLI/tests. Fixes LSP violation where domain layer depended on protocol types.
- **Comprehensive codebase documentation** — 7 new markdown guides in `docs/codebase/` covering architecture, module structure, conventions, integrations, testing, and known concerns. Includes hexagonal layer breakdown, design patterns, concurrency model, and high-churn risk areas.
- **Feature contributor onboarding** — CodeTour (13-step walkthrough) at `.tours/feature-contributor-guide.tour` teaches how to implement a new LSP feature from handler to tests. Covers architecture layers, handler pattern, resolver logic, and test strategy.

## 0.9.3

- **Performance: no more file cap** — the default file limit is now unlimited. Previously the LSP mode only eagerly indexed 2000 files; larger projects (especially iOS) fell back to on-demand `rg` for deeper files. After the query/parser caching fix in 0.9.2, the per-file parse cost is low enough that indexing everything upfront is the right default. Use `KOTLIN_LSP_MAX_FILES` env var to set a custom limit if needed.
- **Performance: cached tree-sitter queries and parsers** — `Query` objects (the compiled S-expression query automaton) are now compiled once per process via `OnceLock` and reused across all file parses. `Parser` objects are reused per worker thread via thread-local storage. Eliminates the dominant CPU cost for large iOS codebases during indexing.

## 0.9.2

- **Generic type parameter substitution** — hover, inlay hints, and completion now resolve generic type parameters to their concrete types when inside a subclass. For example, if `DashboardProductsReducer : FlowReducer<Event, Effect, State>`, then `EffectType` is shown as `Effect` in inlay hints, hover tooltips, and completion detail. Works for:
  - Enclosing class supertypes (e.g. `FlowReducer<Event, Effect, State>`)
  - Member property type hierarchies (e.g. a `val reducer: DashboardProductsReducer` in a ViewModel gives access to `FlowReducer`'s param substitution)
  - Annotated classes where the declaration line is an annotation (scans up to 5 source lines to find the actual `<TypeParams>`)
- **Hover/inlay hint consistency** — `it`/lambda param hover now uses the same import-aware resolution as go-to-definition (`resolve_symbol` → local → imports → same-package → hierarchy), fixing cases where hover showed the wrong type (e.g. a deprecated enum instead of the local data class)
- **Hover applies enclosing-class substitution** — `it`/`this` hover applies the same substitution map as inlay hints (was previously using raw inferred type)
- **`parse_type_params` fix** — now only looks for `<>` before the first `(`, avoiding false matches on constructor parameter generic types

## 0.9.1

- **CST inlay hints** — inlay hint computation replaced with a tree-sitter preorder walk; no longer scans line-by-line. `line_starts` precomputed for O(1) offset lookups; `hint_property` now uses CST initializer inference for untyped `val`/`var`.
- **Live parse trees** — each open document keeps a live tree-sitter parse tree updated on every `didChange`. CST-first paths in `lambda_params_at_col`, `enclosing_class_at`, and `find_it_element_type_in_lines_impl` use the live tree instead of backward character scans.
- **`it` inside nested lambdas no longer shows `: suspend`** — `find_as_call_arg_type` now tracks brace depth; a cursor inside `setState { it }` no longer walks out through the `{` and mis-infers the outer function's `suspend` parameter type.
- **O(1) line access in CST fast paths** — replaced `from_utf8(&doc.bytes).lines().nth(row)` (O(row)) with `live_lines` map lookups (O(1)) in scope and inference hot paths.

## 0.8.0

- **Completion relevance & ranking** — completions are now scored and sorted by match quality: exact prefix match (score 0) → camelCase acronym match (score 1, e.g. typing `CB` matches `ColumnButton`) → substring (score 2, same-file/package only). Results are capped at 150 items with `isIncomplete: true` so the client re-queries as you type, keeping the list tight. Cross-package (auto-import) symbols require a prefix of ≥ 2 characters and only include prefix/acronym matches (no substring flood). Typing after `@` restricts completions to class/annotation kinds (functions and variables are suppressed).
- **Auto-import completion** — selecting an unimported class/interface/object in completion automatically adds the correct `import` statement. Multiple classes with the same name (from different packages) appear as separate items with the package shown in the detail column. Already-imported, same-package, and star-import-covered symbols are shown without a redundant edit.
- **`sourcePaths` configuration** — index extra directories (library sources, Gradle-unpacked stubs) for hover, go-to-definition and autocomplete, while excluding them from `findReferences` and `rename`. Paths can be absolute (including `~/…`) or relative to the workspace root; no hardcoded directory excludes are applied (the user's intent is trusted). Files inside the workspace root are indexed but not excluded from findReferences.
- **`contrib/extract-sources.py`** — cross-platform Python 3 script that finds `*-sources.jar` files in the Gradle cache, deduplicates by keeping the latest version of each artifact, and extracts `.kt`/`.java` sources to `~/.kotlin-lsp/sources/` for use with `sourcePaths`. Supports substring filters (e.g. `androidx.compose`), `--dry-run`, and custom `--gradle-home`/`--output` paths.

## 0.7.1

- **`ignorePatterns` configuration** — exclude directories/files from indexing via `initializationOptions`. Supports gitignore-style globs: bare patterns (e.g. `bazel-*`) match at any depth; path-scoped patterns (e.g. `third-party/**`) match relative to the workspace root. Absolute paths under the workspace root are also accepted. Applied to both `fd` and the `walkdir` fallback, and to the warm-start cached manifest so newly configured patterns take effect without clearing the cache. See [Configuration](#configuration) in the README.
- **Swift hover keyword fix** — Swift functions now correctly show `func` instead of `fun` in hover code blocks.

## 0.7.0

- **`it`/`this` type-directed inference** — when `it` or `this` is used as a call argument (named or positional), the expected parameter type is inferred from the function signature. E.g. `.send(channel = this)` → `SendChannel`, `process(it)` → `Item`
- **`this` in receiver vs regular lambdas** — `this` inside a regular `(T) -> R` lambda now correctly hints the enclosing class instead of the lambda param; only receiver lambdas `T.() -> R` and scope functions (`run`/`apply`/`also`/`let`/`with`) hint the receiver type
- **`fun interface` recognition** — fix tree-sitter not recognising `fun interface` declarations
- **Suspend lambda type inference** — correct type inference for `suspend` lambda parameters
- **Rename regression tests** — 9 tests covering 2/3/4 occurrences same-line, multi-line, substring false-positive, UTF-16 range correctness
- **Copilot extension** — remove overly restrictive `kotlin_rg` pre-hook; all `rg` queries now pass through unconditionally

## 0.6.1

- **`super.method` go-to-def** — must not fall through to an override in the current file; resolves to the parent class declaration

## 0.6.0

- **`super`/`this` go-to-def** — `super` resolves to the parent class; `this.method` resolves via the enclosing class hierarchy
- **Multi-line constructors** — go-to-def works when the constructor spans multiple lines
- **`typealias` support** — indexed and resolved in go-to-def chains
- **Cross-module resolution** — improved supertype priority indexing for cross-module hierarchies

## 0.5.0

- **Workspace pinning** — workspace set once at `initialize` from env var / `~/.config/kotlin-lsp/workspace` / `rootUri`; never overridden at runtime by `did_open`
- **Removed `changeRoot` command** — one LSP instance per workspace; restart to switch projects
- **Outside-root file isolation** — files opened outside the workspace root are skipped for workspace-wide indexing
- **Tiered root auto-detection** — strong project markers (`settings.gradle.kts`, `Cargo.toml`) > `.git` > `Package.swift`; correctly handles mono-repos
- **Cold-start navigation** — `hover`, `goToDefinition`, `documentSymbol` work immediately on first file open via on-demand `index_content`
- **`rg` fallback at cold start** — `lines_for` reads from disk when file not yet indexed
- **Live indexing progress** — `WorkDoneProgress::Report` notifications every 500 ms with percentage
- **Extension tools** — `kotlin_lsp_status`, `kotlin_lsp_set_workspace`

## 0.4.1

- **SOLID refactoring** — pure functions, coordinator pattern, `WorkspaceIndexResult` pipeline
- **Async indexing** — concurrent file parsing with semaphore-guarded `spawn_blocking`
- **iOS indexing fixes** — non-blocking parse, deadlock prevention
- **Cache versioning** — `CACHE_VERSION` bump invalidates stale on-disk indexes
- **`--index-only` CLI mode** — headless one-shot indexing for CI/tooling

## 0.4.0

- **Swift support** — full structural indexing of `.swift` files with all LSP features; SwiftPM `.build` and Xcode `DerivedData` excluded automatically
- **Centralized parser dispatch** — `parse_by_extension()` routes `.kt`/`.java`/`.swift` to the correct tree-sitter parser
- **Dynamic file discovery** — `fd`/`rg` glob patterns and file watchers include all supported extensions

## 0.3.13

- **Inlay hints** — type hints for lambda `it`, named params, `this`, untyped `val`/`var`
- **Go-to-implementation** — transitive subtype lookup via BFS
- **Syntax diagnostics** — tree-sitter `ERROR`/`MISSING` nodes
- **Cross-file lambda resolution** — named-arg lambdas resolve parameter types from constructor signatures
- **Instant feature availability** — all features work immediately via `rg` fallback
- **Race condition fix** — semaphore permit held through `spawn_blocking`
- **Workspace symbol** — dot-qualified queries for extension functions
