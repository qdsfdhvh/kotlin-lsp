# CLI Commands

`kotlin-lsp` works standalone — no editor, no daemon.

Common flags on all commands: `--json`, `--fast`, `--smart`, `--root <dir>`,
`--limit <n>`, `--kind`, `--module`, `--source-set`, `--owner`.

## Quick examples

```bash
# ── core lookup ──
kotlin-lsp find ViewModel              # declarations
kotlin-lsp refs ViewModel              # references
kotlin-lsp hover Foo.kt 42 10          # signature + doc
kotlin-lsp context Foo.kt 42 10        # def + sig + doc + refs
kotlin-lsp complete Foo.kt 42 --dot    # completions

# ── groups ──
kotlin-lsp search "login"            # semantic search (shorthand)
kotlin-lsp search semantic "login"  # semantic search (explicit)
kotlin-lsp search summarize User      # symbol summary
kotlin-lsp search docs "parse"        # KDoc search
kotlin-lsp search imports UserRepo     # who imports this
kotlin-lsp edit rename Foo.kt 42 10 X # rename symbol
kotlin-lsp edit import Foo.kt         # add missing imports
kotlin-lsp edit organize Foo.kt       # clean imports
kotlin-lsp edit inject Foo.kt         # resolve types
kotlin-lsp edit new activity Login    # file from template
kotlin-lsp tool code-action F.kt 1 1  # list code actions
kotlin-lsp tool inspect Foo.kt        # file diagnostics
kotlin-lsp tool bench                 # performance
kotlin-lsp tool doctor                # system health
kotlin-lsp call hierarchy F.kt 42 10  # call chains
kotlin-lsp type hierarchy User        # super/subtype tree
kotlin-lsp type sealed Result         # sealed subclasses
kotlin-lsp android composables F.kt   # composable analysis
kotlin-lsp module list                # list modules

# ── infrastructure ──
kotlin-lsp check Foo.kt               # syntax + warnings
kotlin-lsp format check src/          # formatting
kotlin-lsp index --root ./            # build cache
kotlin-lsp gradle-deps                # parsed dependencies
kotlin-lsp cache stats                # cache info
```

## Command groups

| Group | Subcommands | What they do |
|-------|-------------|-------------|
| **search** | `semantic`, `docs`, `summarize`, `cache-stats`, `imports`, `annotated`, `find-test`, `expect-actual` | Symbol discovery and analysis |
| **edit** | `rename`, `batch`, `imports`, `inject`, `insert`, `new`, `organize` | Code modification |
| **tool** | `inspect`, `graph`, `snapshot`, `bench`, `doctor`, `workspace`, `query`, `skills`, `code-action`, `tokens`, `tree` | Debug / introspection |
| **call** | `hierarchy` | Call graph |
| **type** | `hierarchy`, `sealed` | Type hierarchy |
| **module** | `list`, `deps`, `files`, `packages` | Module structure |
| **android** | `activities`, `composables` | Android resources |
| **format** | `check`, `apply` | Code formatting |

## Top-level commands

| Command | Description |
|---------|-------------|
| `find <name>` | Declaration search |
| `refs <name>` | All references |
| `hover <file> <line> <col>` | Signature, KDoc, deprecation |
| `complete <file> <line> [col]` | Dot-completion, auto-import |
| `context <file> <line> <col>` | One-stop: def + sig + doc + refs |
| `impact <file> <line> <col>` | Impact / risk analysis |
| `check <file>...` | Syntax errors, imports, deprecation |
| `index [--root <dir>]` | Build workspace cache |
| `index-jars [root]` | Index library JARs |
| `extract-sources` | Unpack `*-sources.jar` |
| `sources` | List auto-discovered source roots |
| `cache stats` | Cache diagnostics |
| `gradle-deps` | Parsed Gradle dependencies |

## Deprecated aliases

Old names still work with `[WARN]`. Prefer the new grouped names.

| Old | New |
|-----|-----|
| `summarize` | `search summarize` |
| `docs` / `search` | `search semantic` / `search docs` |
| `summary-cache` | `search cache-stats` |
| `imports-of` | `search imports` |
| `annotated` | `search annotated` |
| `find-test` | `search find-test` |
| `expect-actual` | `search expect-actual` |
| `rename` | `edit rename` |
| `batch` / `batch-imports` | `edit batch` / `edit imports` |
| `inject` | `edit inject` |
| `insert` / `insert-*` | `edit insert` |
| `new-file` | `edit new` |
| `organize-imports` | `edit organize` |
| `tokens` / `tree` | `tool tokens` / `tool tree` |
| `inspect` | `tool inspect` |
| `symbol-graph` | `tool graph` |
| `snapshot` | `tool snapshot` |
| `benchmark` | `tool bench` |
| `doctor` | `tool doctor` |
| `workspace` | `tool workspace` |
| `query` | `tool query` |
| `skills` | `tool skills` |
| `code-action` | `tool code-action` |
| `callers` / `callees` | `call hierarchy --incoming/--outgoing` |
| `call-hierarchy` | `call hierarchy` |
| `implementations` / `subclasses` | `type hierarchy --subtypes` |
| `type-hierarchy` | `type hierarchy` |
| `modules` / `module-deps` | `module list` / `module deps` |
| `android-activities` / `android-composables` | `android activities` / `android composables` |

## Common flags

| Flag | Behaviour |
|------|-----------|
| `--fast` | Always `rg`/`fd` — instant, no index |
| `--smart` | Require pre-built index |
| `--json` | Machine-readable output |
| `--relative` | Workspace-relative paths (auto when stdout piped) |
| `--flat` | Grep-style `path:line:col: name` |
| `--limit <n>` | Cap result count |
| `--kind class,fun` | Filter by symbol kind |
| `--module <frag>` | Filter by module path |
| `--owner <name>` | Filter by enclosing class |
| `--source-set <set>` | Filter by source set |

## Library sources

```bash
kotlin-lsp extract-sources    # one-time: unpack *-sources.jar from Gradle cache
kotlin-lsp index-jars         # one-time: index extracted library symbols
```

## What gets indexed

JDK, Kotlin stdlib, and Android SDK symbols are available without source JARs.

Source files are cached in `~/.cache/kotlin-lsp/`. Cache populated by `kotlin-lsp index`,
refreshed on file changes.
