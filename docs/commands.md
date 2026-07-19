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
kotlin-lsp search semantic "login repo"  # semantic search
kotlin-lsp search summarize Activity     # symbol summary
kotlin-lsp check src/Foo.kt              # syntax + import + deprecation diagnostics
kotlin-lsp call hierarchy src/Foo.kt 42 10  # incoming + outgoing call chain
kotlin-lsp type hierarchy Activity       # super/subtype tree
kotlin-lsp type sealed Result            # list sealed class subclasses
kotlin-lsp edit organize src/Foo.kt      # sort, dedup, remove unused
kotlin-lsp edit inject src/Foo.kt        # batch-resolve all type signatures
kotlin-lsp tool code-action src/Foo.kt 42 10  # list applicable code actions
kotlin-lsp edit imports src/Foo.kt       # scan for import candidates
kotlin-lsp edit new activity Activity    # generate file from template
kotlin-lsp index --root ./android        # pre-build cache
kotlin-lsp sources --root ./android      # list detected source roots
kotlin-lsp extract-sources               # unpack library sources from Gradle cache
kotlin-lsp index-jars                    # index library symbols from *-sources.jar
kotlin-lsp cache stats                   # show index cache diagnostics
kotlin-lsp tool bench                    # run performance benchmarks
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

## Groups

Commands are organized into groups. Old names still work alongside the new grouped names.

### `search` — Symbol discovery

| Command | Description |
|---------|-------------|
| `search docs <query>` | KDoc / docs search |
| `search semantic <query>` | TF-IDF semantic search |
| `search summarize <name>` | Rich summary: kind, signature, members, KDoc |
| `search cache-stats` | Show AI summary cache stats |
| `search imports <name>` | Files importing the given symbol |
| `search annotated <annotation>` | Symbols annotated with @annotation |
| `search find-test <file> <line> <col>` | Find tests for a symbol |
| `search expect-actual <name>` | KMP expect/actual declarations |

### `edit` — Code modification

| Command | Description |
|---------|-------------|
| `edit rename <file> <line> <col> <new>` | Rename symbol |
| `edit batch <file>` | Batch from JSON rules |
| `edit imports <file>` | Batch add missing imports |
| `edit inject <file>` | Batch-resolve type signatures |
| `edit insert <file> <line> --content --kind` | Semantic insertion |
| `edit new <template> <name>` | Generate file from template |
| `edit organize <file>...` | Sort, dedup, remove unused imports |

### `tool` — Debug / introspection

| Command | Description |
|---------|-------------|
| `tool tokens <file>` | Syntax tokens (debug) |
| `tool tree <file>` | Dump parse tree (debug) |
| `tool inspect <file>` | Detailed file diagnostics |
| `tool graph` | Build symbol dependency graph |
| `tool snapshot` | Workspace snapshot |
| `tool bench` | Performance benchmarks |
| `tool doctor` | System diagnostics |
| `tool workspace` | Workspace overview |
| `tool query` | Batch query engine (pipe JSON) |
| `tool skills list` | List bundled agent skills |
| `tool code-action <file> <line> <col>` | List/apply code actions |

### Other groups

| Group | Subcommands |
|-------|-------------|
| `call hierarchy` | Incoming/outgoing call chain |
| `type hierarchy` | Supertype/subtype tree |
| `type sealed <name>` | List sealed class subclasses |
| `module` | list / deps / files / packages |
| `android` | activities / composables |
| `format` | check / apply |
| `gradle-deps` | Show parsed Gradle dependencies |

## Top-level commands (standalone)

| Command | Description |
|---------|-------------|
| `find <name>` | Declaration search |
| `refs <name>` | All references |
| `hover <file> <line> <col>` | Signature, KDoc, deprecation |
| `complete <file> <line> [col]` | Dot-completion, auto-import |
| `context <file> <line> <col>` | One-stop def + sig + doc + refs |
| `impact <file> <line> <col>` | Impact analysis |
| `check <file>...` | Syntax errors, imports, deprecation |
| `index [--root <dir>]` | Index workspace |
| `index-jars [root]` | Index library symbols from JARs |
| `extract-sources [lib...]` | Unpack *-sources.jar |
| `sources [--explain]` | Auto-discovered source roots |
| `cache stats` | Index cache statistics |

### Deprecated aliases (kept for backward compatibility)

`summarize` → `search summarize`, `docs` → `search docs`, `search` → `search semantic`,
`imports-of` → `search imports`, `annotated` → `search annotated`,
`find-test` → `search find-test`, `expect-actual` → `search expect-actual`,
`summary-cache` → `search cache-stats`,
`rename` → `edit rename`, `batch` / `batch-imports` → `edit batch` / `edit imports`,
`inject` → `edit inject`, `insert` → `edit insert`, `new-file` → `edit new`,
`organize-imports` → `edit organize`,
`tokens` → `tool tokens`, `tree` → `tool tree`, `inspect` → `tool inspect`,
`symbol-graph` → `tool graph`, `snapshot` → `tool snapshot`,
`benchmark` → `tool bench`, `doctor` → `tool doctor`, `workspace` → `tool workspace`,
`query` → `tool query`, `skills` → `tool skills`, `code-action` → `tool code-action`,
`callers` → `call hierarchy --incoming`, `callees` → `call hierarchy --outgoing`,
`call-hierarchy` → `call hierarchy`, `implementations` → `type hierarchy --subtypes`,
`subclasses` → `type hierarchy --subtypes`, `type-hierarchy` → `type hierarchy`,
`modules` → `module list`, `module-deps` → `module deps`, `module-files` → `module files`,
`package-deps` → `module packages`,
`android-activities` → `android activities`, `android-composables` → `android composables`

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
`~/.cache/kotlin-lsp/`. The cache is populated automatically by
`kotlin-lsp index` and refreshed on file changes.
