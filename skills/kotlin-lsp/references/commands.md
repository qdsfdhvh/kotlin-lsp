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

## `search` — Symbol discovery

| Command | Description |
|---------|-------------|
| `kotlin-lsp search semantic "query"` | TF-IDF semantic search |
| `kotlin-lsp search docs "query"` | KDoc / signature search |
| `kotlin-lsp search summarize <name>` | Symbol summary (cached) |
| `kotlin-lsp search cache-stats` | Cached summary statistics |
| `kotlin-lsp search imports <name>` | Files importing this symbol |
| `kotlin-lsp search annotated <annotation>` | Annotated with @annotation |
| `kotlin-lsp search find-test <file> <line> <col>` | Find related tests |
| `kotlin-lsp search expect-actual <name>` | KMP expect/actual declarations |

## Call & type hierarchy

| Command | Description |
|---------|-------------|
| `kotlin-lsp call hierarchy <file> <line> <col>` | Incoming + outgoing call chain |
| `kotlin-lsp impact <file> <line> <col>` | What depends on this symbol? |
| `kotlin-lsp type hierarchy <name> [--subtypes\|--supertypes]` | Supertype/subtype tree |
| `kotlin-lsp type sealed <name>` | Sealed class subclasses |

## Module & package introspection

| Command | Description |
|---------|-------------|
| `kotlin-lsp module list` | List modules |
| `kotlin-lsp module deps <name>` | Module dependency graph |
| `kotlin-lsp module files <name>` | Files in a module |
| `kotlin-lsp module packages <name>` | Packages in a module |

## `edit` — Code modification

| Command | Description |
|---------|-------------|
| `kotlin-lsp edit rename <file> <line> <col> <new>` | Rename symbol |
| `kotlin-lsp edit batch <file>` | Batch from JSON rules |
| `kotlin-lsp edit imports <file>` | Batch add missing imports |
| `kotlin-lsp edit inject <file>` | Batch-resolve type signatures |
| `kotlin-lsp edit insert <file> <line> --content --kind` | Semantic insertion |
| `kotlin-lsp edit new <template> <name>` | File from template |
| `kotlin-lsp edit organize <file>...` | Sort, dedup, remove unused imports |

## `tool` — Debug / introspection

| Command | Description |
|---------|-------------|
| `kotlin-lsp tool inspect <file>` | File diagnostics for agents |
| `kotlin-lsp tool graph` | Symbol dependency graph |
| `kotlin-lsp tool snapshot` | Full workspace snapshot |
| `kotlin-lsp tool bench` | Performance benchmarks |
| `kotlin-lsp tool doctor` | System diagnostics |
| `kotlin-lsp tool workspace` | Workspace overview |
| `kotlin-lsp tool query` | Batch query (pipe JSON) |
| `kotlin-lsp tool skills list\|read` | Agent skills |
| `kotlin-lsp tool code-action <file> <line> <col>` | List/apply code actions |
| `kotlin-lsp tool tokens <file>` | Semantic tokens (debug) |
| `kotlin-lsp tool tree <file>` | Parse tree dump (debug) |

## Infrastructure

| Command | Description |
|---------|-------------|
| `kotlin-lsp check <file>...` | Syntax errors + warnings |
| `kotlin-lsp format check\|apply <file>...` | Code formatting |
| `kotlin-lsp index [--root <dir>]` | Build workspace cache |
| `kotlin-lsp index-jars [root]` | Index library JARs |
| `kotlin-lsp extract-sources` | Unpack `*-sources.jar` |
| `kotlin-lsp sources [--explain]` | Auto-discovered source roots |
| `kotlin-lsp cache stats` | Cache diagnostics |
| `kotlin-lsp gradle-deps` | Parsed Gradle dependencies |

## Android

| Command | Description |
|---------|-------------|
| `kotlin-lsp android activities` | Manifest activities |
| `kotlin-lsp android composables <file> [--call-graph] [--state] [--preview]` | Composable analysis |

## Output conventions

- **Text (default):** grouped by file with `<path>\n  line:col[name] text`. Pipe-safe.
- **`--json`:** compact JSON — pipe to `jq` for reading.
- **`--relative`:** workspace-relative paths. **Auto-enabled when stdout is piped.**
- **`--absolute`:** override the non-TTY auto-relative.
- **`--flat`:** legacy grep-style `<path>:<line>:<col>: <name>`.
