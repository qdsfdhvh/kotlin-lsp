---
name: kotlin-lsp
description: Use the `kotlin-lsp` CLI for precise symbol lookup in Kotlin/Java/Swift projects — faster than grep/rg and returns typed answers (declarations, refs, signatures) instead of raw text matches. Saves tokens because results are scoped and structured.
---

# kotlin-lsp

`kotlin-lsp` is a tree-sitter–backed CLI for Kotlin / Java / Swift symbol queries — no daemon, no JVM. It returns *declaration locations* and *type-aware references*, not text matches.

```bash
kotlin-lsp --version
```

## When to use kotlin-lsp vs rg

```
Query is about Kotlin/Java/Swift symbols?
├─ No → rg / Read
└─ Yes:
   ├─ Symbol is unique AND in this repo → rg --type kotlin (faster)
   ├─ Symbol is generic (handle, Event, …) → kotlin-lsp find/refs --module … --limit
   ├─ Symbol lives in library (Compose, AndroidX) → kotlin-lsp find
   ├─ Cross-module ref filtering needed → kotlin-lsp refs --module / --owner
   ├─ One-stop info (def + sig + doc) → kotlin-lsp context
   ├─ Syntax check → kotlin-lsp check
   ├─ Format check → kotlin-lsp format check
   ├─ Caller/callee tree → kotlin-lsp call hierarchy
   ├─ Implementation tree → kotlin-lsp type hierarchy
   ├─ Composable analysis → kotlin-lsp android composables <file> --call-graph/--state/--preview
   ├─ Batch queries → echo '[...]' | kotlin-lsp query --json
   ├─ Import analysis → kotlin-lsp search imports
   ├─ Annotation query → kotlin-lsp search annotated
   ├─ Signature search → kotlin-lsp search docs
   ├─ Semantic search → kotlin-lsp search semantic "login repo"
   ├─ Cached summaries → kotlin-lsp search summarize <name> --cached | search cache-stats
   └─ Full project snapshot → kotlin-lsp tool snapshot / tool graph
```

Full command reference → references/commands.md

## How it saves tokens

| Naive | Better with kotlin-lsp |
|---|---|
| `rg 'class MyViewModel'` returns every text match including doc comments | `kotlin-lsp find MyViewModel --limit 5` returns only declaration sites |
| `rg 'MyViewModel'` to find usages, then manually filter | `kotlin-lsp refs MyViewModel --limit 20` returns real references |
| Open file, read 200 lines to figure out return type | `kotlin-lsp hover Foo.kt 42 10` returns just the signature |

Output defaults:
- Text mode groups by file with structural annotation (path, module, sourceSet)
- `--json` for structured data
- `--relative` auto-enabled when stdout is piped (agent context)

## Quick reference — most-used commands

```bash
# Find declarations / references
kotlin-lsp find <Name> [--limit N] [--kind class,fun]
kotlin-lsp refs <Name> [--limit N]
kotlin-lsp hover <file> <line> <col>
kotlin-lsp context <file> <line> <col>

# Search group (symbol discovery)
kotlin-lsp search semantic "login repo" [--limit N]
kotlin-lsp search summarize <name> --cached
kotlin-lsp search docs <query>
kotlin-lsp search cache-stats
kotlin-lsp search imports <name>
kotlin-lsp search annotated <annotation>
kotlin-lsp search find-test <file> <line> <col>
kotlin-lsp search expect-actual <name>

# Call graph
kotlin-lsp call hierarchy <file> <line> <col>

# Type hierarchy
kotlin-lsp type hierarchy <Name>
kotlin-lsp type sealed <Name>

# Edit group (code modification)
kotlin-lsp edit organize <file>...
kotlin-lsp edit imports <file> [--apply]
kotlin-lsp edit rename <file> <line> <col> <newName>
kotlin-lsp edit inject <file>
kotlin-lsp edit new <template> <Name>

# Tool group (debug / introspection)
kotlin-lsp tool bench
kotlin-lsp tool doctor [--json]
kotlin-lsp tool inspect <file>
kotlin-lsp tool snapshot [--include-libraries] [--limit <n>]
kotlin-lsp tool graph --json
kotlin-lsp tool code-action <file> <line> <col>

# Syntax / format
kotlin-lsp check <file>...
kotlin-lsp format check <dir>...
kotlin-lsp format apply <dir>...

# Android / Compose
kotlin-lsp android composables <file> --call-graph/--state/--preview
```
## All commands

| Need | Command |
|------|---------|
| Find definition | `kotlin-lsp find <name>` |
| Find references | `kotlin-lsp refs <name>` |
| Hover / signature | `kotlin-lsp hover <file> <line> <col>` |
| Completions | `kotlin-lsp complete <file> <line> [col]` |
| One-stop context | `kotlin-lsp context <file> <line> <col>` |
| Semantic search | `kotlin-lsp search "query"` |
| KDoc search | `kotlin-lsp search docs "query"` |
| Syntax check | `kotlin-lsp check <file>...` |
| Format check | `kotlin-lsp format check <dir>...` |
| Format apply | `kotlin-lsp format apply <dir>...` |
| Code actions | `kotlin-lsp tool code-action <file> <line> <col>` |
| Organize imports | `kotlin-lsp edit organize <file>...` |
| Batch imports | `kotlin-lsp edit imports <file>` |
| Rename | `kotlin-lsp edit rename <file> <line> <col> <new>` |
| Semantic insert | `kotlin-lsp edit insert <file> <kind>` |
| Inject types | `kotlin-lsp edit inject <file>` |
| Call hierarchy | `kotlin-lsp call hierarchy <file> <line> <col>` |
| Impact analysis | `kotlin-lsp impact <file> <line> <col>` |
| Type hierarchy | `kotlin-lsp type hierarchy <name>` |
| Module list | `kotlin-lsp module list` |
| Module deps | `kotlin-lsp module deps <name>` |
| Module files | `kotlin-lsp module files <name>` |
| Module packages | `kotlin-lsp module packages [name]` |
| Android activities | `kotlin-lsp android activities` |
| Android composables | `kotlin-lsp android composables <file> [--call-graph] [--state] [--preview]` |
| Import analysis | `kotlin-lsp search imports <name>` |
| Annotation query | `kotlin-lsp search annotated <name>` |
| Symbol summary | `kotlin-lsp search summarize <name>` |
| Summary cache | `kotlin-lsp search cache-stats` |
| Find tests | `kotlin-lsp search find-test <file> <line> <col>` |
| KMP expect/actual | `kotlin-lsp search expect-actual <name>` |
| Index workspace | `kotlin-lsp index [--root <dir>] [--gradle]` |
| Index JARs | `kotlin-lsp index-jars [root]` |
| Gradle deps | `kotlin-lsp gradle-deps` |
| Extract sources | `kotlin-lsp extract-sources [lib...]` |
| Source roots | `kotlin-lsp sources` |
| Cache stats | `kotlin-lsp cache stats` |
| Doctor | `kotlin-lsp tool doctor [--json]` |
| Workspace overview | `kotlin-lsp tool workspace` |
| Snapshot | `kotlin-lsp tool snapshot` (workspace symbols; add `--include-libraries` for the ~/.kotlin-lsp/sources library cache, `--limit <n>` to cap) |
| Symbol graph | `kotlin-lsp tool graph` |
| Batch query | `echo '[...]' \| kotlin-lsp tool query --json` |
| File inspect | `kotlin-lsp tool inspect <file>` |
| Tokens (debug) | `kotlin-lsp tool tokens <file>` |
| Parse tree (debug) | `kotlin-lsp tool tree <file>` |
| New file | `kotlin-lsp edit new <template> <name>` |
| Benchmark | `kotlin-lsp tool bench` |
| Agent skills | `kotlin-lsp tool skills list \| read <name>` |

Full command reference → references/commands.md

## Performance modes

| Mode | When |
|------|------|
| _(default)_ | Auto — cached index or `rg`/`fd` fallback |
| `--fast` | Always `rg`/`fd`; instant |
| `--smart` | Require pre-built index |

Indexing setup → https://github.com/qdsfdhvh/kotlin-lsp/blob/main/docs/features.md

## Anti-patterns

- **Don't** `rg 'class FooBar'` when `kotlin-lsp find FooBar` will do.
- **Don't** read the entire file for a signature; use `hover`.
- **Don't** omit `--limit` on `refs` for common names like `String` or `Result`.
- **Don't** invoke `kotlin-lsp` recursively inside an LSP context.
