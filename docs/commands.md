# CLI Commands

`kotlin-lsp` works standalone — no editor, no daemon. All commands share common
flags: `--json`, `--fast`, `--smart`, `--root <dir>`, `--limit <n>`, `--kind`,
`--module`, `--source-set`, `--owner`.

**Output is AI-tuned by default:** text mode is minimal (grouped by file,
structural annotation), `--json` emits compact JSON, and `--relative` is
auto-enabled when stdout is piped.

## Quick examples

```bash
kotlin-lsp find MyViewModel              # search declarations
kotlin-lsp refs MyViewModel              # find all references
kotlin-lsp hover src/Foo.kt 42 10        # hover info at line 42, col 10
kotlin-lsp complete src/Foo.kt 42 --dot  # completions after last '.' on line 42
kotlin-lsp context src/Foo.kt 42 10      # one-stop: def + sig + doc + refs
kotlin-lsp search "login repo"          # semantic search
kotlin-lsp check src/Foo.kt              # syntax + import + deprecation diagnostics
kotlin-lsp call hierarchy src/Foo.kt 42 10  # incoming + outgoing call chain
kotlin-lsp type hierarchy Activity       # super/subtype tree
kotlin-lsp organize-imports src/Foo.kt   # sort, dedup, remove unused
kotlin-lsp inject src/Foo.kt             # batch-resolve all type signatures
kotlin-lsp code-action src/Foo.kt 42 10  # list applicable code actions
kotlin-lsp batch-imports src/Foo.kt      # scan for import candidates
kotlin-lsp new-file activity Activity    # generate file from template
kotlin-lsp index --root ./android        # pre-build cache
kotlin-lsp sources --root ./android      # list detected source roots
kotlin-lsp extract-sources               # unpack library sources from Gradle cache
kotlin-lsp index-jars                    # index library symbols from *-sources.jar
kotlin-lsp cache stats                   # show index cache diagnostics
kotlin-lsp benchmark                     # run performance benchmarks
```

## Common flags

| Flag | Behaviour |
|------|-----------|
| _(none)_ | Auto: use cached index if available, fall back to fast `rg`/`fd` |
| `--fast` | Always use `rg`/`fd`; instant, no index needed |
| `--smart` | Require index; build it if missing |
| `--json` | Compact JSON output (no whitespace); pipe to `jq` for human reading |
| `--relative` | Print workspace-relative paths. **Auto-enabled when stdout isn't a TTY** |
| `--absolute` | Force absolute paths; opt out of the non-TTY auto-relative default |
| `--flat` | Use legacy grep-style `<path>:<line>:<col>: <name>` format |
| `--module <frag>` | Filter results by module path fragment |
| `--source-set <set>` | Filter by source set (e.g. `commonMain`, comma-separated for OR) |
| `--owner <name>` | Filter results by enclosing class/interface/object name |
| `--kind class,fun` | Filter by symbol kind |
| `--limit <n>` | Cap result count after filtering |
| `--root <dir>` | Workspace root (default: nearest `.git` dir) |

## Library sources

Library symbols (Compose, AndroidX, coroutines, stdlib, …) are resolved
automatically once you extract them:

```bash
kotlin-lsp extract-sources        # one-time: unpack *-sources.jar from Gradle cache
kotlin-lsp index-jars             # one-time: index library symbols
```

- **Android SDK** (`Activity`, `Context`, `View`, …) — detected from
  `local.properties` → `$ANDROID_HOME` → `$ANDROID_SDK_ROOT`
- **Gradle library sources** — extracted from `*-sources.jar` in the Gradle cache
- **IntelliJ/Android Studio projects** — `workspace.json` source roots are picked
  up automatically


## Symbol lookup

| Command | Description |
|---------|-------------|
| `find <name>` | Declaration search — qualified, `--owner`, `--kind`, `--module` |
| `refs <name>` | All references — same filters, `--explain` for provenance |
| `hover <file> <line> <col>` | Signature, KDoc, deprecation, data class props |
| `complete <file> <line> [col]` | Dot-completion, auto-import, scored ranking |
| `context <file> <line> <col>` | One-stop: def + sig + doc + refs, `--expand` |
| `search "query"` | TF-IDF semantic search over symbol names + KDoc + signatures |
| `docs "query"` | Search by name or signature |
| `summarize <name>` | Rich summary: kind, signature, members, KDoc, cached |
| `summary-cache` | Manage cached symbol summaries |

## Call & type hierarchy

| Command | Description |
|---------|-------------|
| `call hierarchy <file> <line> <col>` | Incoming/outgoing call chain |
| `call impact <file> <line> <col>` | Impact analysis: what depends on this? |
| `type hierarchy <name> [--subtypes\|--supertypes]` | Supertype/subtype tree |

### Deprecated aliases
`callers` → `call hierarchy --incoming`, `callees` → `call hierarchy --outgoing`,
`call-hierarchy` → `call hierarchy`, `implementations` → `type hierarchy --subtypes`,
`subclasses` → `type hierarchy --subtypes`, `type-hierarchy` → `type hierarchy`

## Module & package introspection

| Command | Description |
|---------|-------------|
| `module list` | List all project modules |
| `module deps <name> [direction]` | Show module dependency graph |
| `module files <name>` | List files in a module |
| `module packages [name]` | Package-level import dependencies |
| `imports-of <name>` | Files importing the given symbol |
| `annotated <name>` | Symbols annotated with @name |

### Deprecated aliases
`modules` → `module list`, `module-deps` → `module deps`,
`module-files` → `module files`, `package-deps` → `module packages`

## Android

| Command | Description |
|---------|-------------|
| `android activities` | List Android activities from AndroidManifest |
| `android composables <file>` | Find @Composable functions |

### Deprecated aliases
`android-activities` → `android activities`, `android-composables` → `android composables`

## Editing & code actions

| Command | Description |
|---------|-------------|
| `code-action <file> <line> <col>` | List/apply code actions |
| `organize-imports <file>...` | Sort, dedup, remove unused imports |
| `rename <file> <line> <col> <new>` | Rename symbol |
| `format check\|apply <file>...` | ktfmt/ktlint format |
| `insert <file> <kind> [--owner]` | Semantic insertion |
| `batch-imports <file>` | Batch add missing imports |
| `inject <file>` | Batch-resolve type signatures |
| `new-file <template> <name>` | Generate file from template |

## Diagnostics & introspection

| Command | Description |
|---------|-------------|
| `check <file>...` | Syntax errors, unused imports, deprecation, redundant vals |
| `index [--root <dir>] [--gradle]` | Index workspace |
| `doctor` | System diagnostics |
| `cache stats` | Index cache statistics |
| `sources [--explain]` | Auto-discovered source roots |
| `tokens <file>` | Syntax tokens (debug) |
| `tree <file>` | Dump parse tree (debug) |
| `inspect <file>` | Detailed file diagnostics |
| `benchmark` | Performance benchmarks |

## Library & workspace

| Command | Description |
|---------|-------------|
| `extract-sources [lib...]` | Unpack *-sources.jar from Gradle cache |
| `index-jars [root]` | Index library symbols from JARs |
| `gradle-deps` | Show parsed Gradle dependencies |
| `workspace` | Workspace overview |
| `snapshot` | Workspace snapshot |
| `symbol-graph` | Build symbol dependency graph |
| `query` | Batch query engine (pipe JSON) |

## Cross-platform

| Command | Description |
|---------|-------------|
| `expect-actual <name>` | KMP expect/actual declarations |
| `find-test <file> <line> <col>` | Find related test files |

## Agent skills

| Command | Description |
|---------|-------------|
| `skills list` | List bundled agent skills |
| `skills read <name>` | Print full SKILL.md for a skill |

## What gets indexed

**JDK standard library** (`java.*`, `javax.*`, `jakarta.*`),
**Kotlin standard library** (`kotlin.*`), and **Android SDK**
(`android.*`, `androidx.*`) symbols are available via the `kotlin-lsp`
[`stdlib`](https://github.com/qdsfdhvh/kotlin-lsp/blob/main/src/stdlib.rs)
module using tree-sitter-compatible signatures. No source JARs needed for
stdlib resolution.

See [Library sources](#library-sources) above for details on extracting
third-party library symbols.

All source files under the detected workspace are cached in
`~/.kotlin-lsp/cache/`. The cache is populated automatically by
`kotlin-lsp index` and refreshed on file changes.