# CLI Commands

`kotlin-lsp` works standalone — no editor, no daemon. All commands share common
flags: `--json`, `--fast`, `--smart`, `--root <dir>`, `--limit <n>`, `--kind`,
`--module`, `--source-set`, `--owner`.

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
