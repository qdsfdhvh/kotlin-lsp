# kotlin-lsp — Command Reference

Install: `curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh | bash`

Common flags: `--json`, `--fast`/`--smart`, `--root <dir>`, `--limit <n>`, `--kind class,fun`, `--module <frag>`, `--owner <name>`, `--relative`/`--absolute`, `--flat`, `--gradle`

## Symbol lookup

| Command | Description |
|---------|-------------|
| `kotlin-lsp find <name>` | Declaration search |
| `kotlin-lsp refs <name>` | All references |
| `kotlin-lsp hover <file> <line> <col>` | Signature + KDoc |
| `kotlin-lsp complete <file> <line> [col]` | Completions with auto-import |
| `kotlin-lsp context <file> <line> <col>` | One-stop: def + sig + doc + refs |
| `kotlin-lsp search "query"` | TF-IDF semantic search |
| `kotlin-lsp docs "query"` | Search by name or signature |
| `kotlin-lsp summarize <name>` | Symbol summary (cached) |
| `kotlin-lsp summary-cache` | Manage cached summaries |

## Call & type hierarchy

| Command | Description |
|---------|-------------|
| `kotlin-lsp call hierarchy <file> <line> <col>` | Incoming + outgoing call chain |
| `kotlin-lsp impact <file> <line> <col>` | What depends on this symbol? |
| `kotlin-lsp type hierarchy <name> [--subtypes\|--supertypes]` | Supertype/subtype tree |

## Module & package introspection

| Command | Description |
|---------|-------------|
| `kotlin-lsp module list` | List all project modules |
| `kotlin-lsp module deps <name>` | Module dependency graph |
| `kotlin-lsp module files <name>` | Files in a module |
| `kotlin-lsp module packages [name]` | Package-level import deps |
| `kotlin-lsp imports-of <name>` | Files importing this symbol |
| `kotlin-lsp annotated <name>` | Symbols with @annotation |

## Android

| Command | Description |
|---------|-------------|
| `kotlin-lsp android activities` | AndroidManifest activities |
| `kotlin-lsp android composables <file>` | @Composable functions |

## Editing & code actions

| Command | Description |
|---------|-------------|
| `kotlin-lsp code-action <file> <line> <col>` | List/apply code actions |
| `kotlin-lsp organize-imports <file>...` | Sort + dedup + remove unused |
| `kotlin-lsp batch-imports <file>` | Batch add missing imports |
| `kotlin-lsp rename <file> <line> <col> <new>` | Rename symbol |
| `kotlin-lsp inject <file>` | Batch-resolve type signatures |
| `kotlin-lsp insert <file> <kind>` | Semantic insertion |
| `kotlin-lsp new-file <template> <name>` | Generate from template |
| `kotlin-lsp format check\|apply <dir>...` | ktfmt/ktlint format |

## Diagnostics & introspection

| Command | Description |
|---------|-------------|
| `kotlin-lsp check <file>...` | Syntax errors, unused imports, deprecation |
| `kotlin-lsp doctor [--json]` | System diagnostics |
| `kotlin-lsp cache stats` | Index cache statistics |
| `kotlin-lsp sources` | Detected source roots |
| `kotlin-lsp inspect <file>` | Detailed file diagnostics |
| `kotlin-lsp tokens <file>` | Syntax tokens (debug) |
| `kotlin-lsp tree <file>` | Parse tree (debug) |
| `kotlin-lsp benchmark` | Performance benchmarks |

## Indexing & library sources

| Command | Description |
|---------|-------------|
| `kotlin-lsp index [--root <dir>] [--gradle]` | Index workspace |
| `kotlin-lsp index-jars [root]` | Index library symbols from JARs |
| `kotlin-lsp extract-sources [lib...]` | Unpack *-sources.jar |
| `kotlin-lsp gradle-deps` | Show parsed Gradle dependencies |

## Workspace & batch

| Command | Description |
|---------|-------------|
| `kotlin-lsp workspace` | Workspace overview |
| `kotlin-lsp snapshot` | Workspace snapshot |
| `kotlin-lsp symbol-graph` | Symbol dependency graph |
| `echo '[...]' \| kotlin-lsp query --json` | Batch query engine |

## Cross-platform

| Command | Description |
|---------|-------------|
| `kotlin-lsp expect-actual <name>` | KMP expect/actual |
| `kotlin-lsp find-test <file> <line> <col>` | Find related test files |

## Agent skills

| Command | Description |
|---------|-------------|
| `kotlin-lsp skills list \| read <name>` | Bundled agent skills |
