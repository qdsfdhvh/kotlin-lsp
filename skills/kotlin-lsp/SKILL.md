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
   ├─ Caller/callee tree → kotlin-lsp callers / callees
   ├─ Implementation tree → kotlin-lsp implementations
   ├─ Fuzzy search → kotlin-lsp find --fuzzy
   ├─ Batch queries → echo '[...]' | kotlin-lsp query --json
   ├─ Import analysis → kotlin-lsp imports-of
   ├─ Annotation query → kotlin-lsp annotated
   ├─ Signature search → kotlin-lsp docs
   └─ Full project snapshot → kotlin-lsp snapshot / symbol-graph
```

Detailed command flags and examples → [references/commands.md](references/commands.md)

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
kotlin-lsp find --fuzzy "login repo"

# Find references
kotlin-lsp refs <Name> [--limit N] [--exclude-imports]

# Hover / context
kotlin-lsp hover <file> <line> <col>
kotlin-lsp context <file> <line> <col>

# Call graph (tree output)
kotlin-lsp callers <file> <line> <col> [depth]
kotlin-lsp callees <file> <line> <col> [depth]

# Inheritance
kotlin-lsp implementations <Name> [depth]
kotlin-lsp subclasses <Name> [depth]
kotlin-lsp type-hierarchy <Name>

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

Full command catalog → [references/commands.md](references/commands.md)

## Performance modes

| Mode | When |
|------|------|
| _(default)_ | Auto — cached index or `rg`/`fd` fallback |
| `--fast` | Always `rg`/`fd`; instant |
| `--smart` | Require pre-built index |

Indexing setup → [references/indexing.md](references/indexing.md)

## Anti-patterns

- **Don't** `rg 'class FooBar'` when `kotlin-lsp find FooBar` will do.
- **Don't** read the entire file for a signature; use `hover`.
- **Don't** omit `--limit` on `refs` for common names like `String` or `Result`.
- **Don't** invoke `kotlin-lsp` recursively inside an LSP context.
