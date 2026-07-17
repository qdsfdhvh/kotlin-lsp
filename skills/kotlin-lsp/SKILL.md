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
   ├─ Search by name → kotlin-lsp find <name>
   ├─ Batch queries → echo '[...]' | kotlin-lsp query --json
   ├─ Import analysis → kotlin-lsp imports-of
   ├─ Annotation query → kotlin-lsp annotated
   ├─ Signature search → kotlin-lsp docs
   ├─ Semantic search → kotlin-lsp search "find where token is refreshed"
   ├─ Cached summaries → kotlin-lsp summarize <name> --cached | summary-cache
   └─ Full project snapshot → kotlin-lsp snapshot / symbol-graph
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
# Find declarations
kotlin-lsp find <Name> [--limit N] [--module <frag>] [--kind class,fun]
kotlin-lsp search "login repo" [--limit N] [--json]

# Find references
kotlin-lsp refs <Name> [--limit N] [--exclude-imports]

# Hover / context
kotlin-lsp hover <file> <line> <col>
kotlin-lsp context <file> <line> <col>

# Call graph (tree output)
kotlin-lsp call hierarchy <file> <line> <col>

# Inheritance
kotlin-lsp type hierarchy <Name> [--subtypes|--supertypes]

# Semantic search
kotlin-lsp search "find where token is refreshed" [--limit N]
kotlin-lsp search "login view model" --json

# Cached summaries
kotlin-lsp summarize <name> --cached
kotlin-lsp summary-cache

# Batch
echo '[...]' | kotlin-lsp query --json

# Project snapshot
kotlin-lsp snapshot
kotlin-lsp symbol-graph --json

# Syntax / format
kotlin-lsp check <file>...
kotlin-lsp format check <dir>...

# Code edits
kotlin-lsp organize-imports <file>
kotlin-lsp batch-imports <file> [--apply]
kotlin-lsp rename <file> <line> <col> <newName>

# Doctor
kotlin-lsp doctor [--json]
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
| Signature search | `kotlin-lsp docs "query"` |
| Syntax check | `kotlin-lsp check <file>...` |
| Format check | `kotlin-lsp format check <dir>...` |
| Format apply | `kotlin-lsp format apply <dir>...` |
| Code actions | `kotlin-lsp code-action <file> <line> <col>` |
| Organize imports | `kotlin-lsp organize-imports <file>...` |
| Batch imports | `kotlin-lsp batch-imports <file>` |
| Rename | `kotlin-lsp rename <file> <line> <col> <new>` |
| Semantic insert | `kotlin-lsp insert <file> <kind>` |
| Inject types | `kotlin-lsp inject <file>` |
| Call hierarchy | `kotlin-lsp call hierarchy <file> <line> <col>` |
| Impact analysis | `kotlin-lsp impact <file> <line> <col>` |
| Type hierarchy | `kotlin-lsp type hierarchy <name>` |
| Module list | `kotlin-lsp module list` |
| Module deps | `kotlin-lsp module deps <name>` |
| Module files | `kotlin-lsp module files <name>` |
| Module packages | `kotlin-lsp module packages [name]` |
| Android activities | `kotlin-lsp android activities` |
| Android composables | `kotlin-lsp android composables <file>` |
| Import analysis | `kotlin-lsp imports-of <name>` |
| Annotation query | `kotlin-lsp annotated <name>` |
| Symbol summary | `kotlin-lsp summarize <name>` |
| Summary cache | `kotlin-lsp summary-cache` |
| Find tests | `kotlin-lsp find-test <file> <line> <col>` |
| KMP expect/actual | `kotlin-lsp expect-actual <name>` |
| Index workspace | `kotlin-lsp index [--root <dir>] [--gradle]` |
| Index JARs | `kotlin-lsp index-jars [root]` |
| Gradle deps | `kotlin-lsp gradle-deps` |
| Extract sources | `kotlin-lsp extract-sources [lib...]` |
| Source roots | `kotlin-lsp sources` |
| Cache stats | `kotlin-lsp cache stats` |
| Doctor | `kotlin-lsp doctor [--json]` |
| Workspace overview | `kotlin-lsp workspace` |
| Snapshot | `kotlin-lsp snapshot` |
| Symbol graph | `kotlin-lsp symbol-graph` |
| Batch query | `echo '[...]' \| kotlin-lsp query --json` |
| File inspect | `kotlin-lsp inspect <file>` |
| Tokens (debug) | `kotlin-lsp tokens <file>` |
| Parse tree (debug) | `kotlin-lsp tree <file>` |
| New file | `kotlin-lsp new-file <template> <name>` |
| Benchmark | `kotlin-lsp benchmark` |
| Agent skills | `kotlin-lsp skills list \| read <name>` |

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
