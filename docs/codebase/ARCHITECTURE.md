# kotlin-lsp Architecture

## Overview

```
                 LSP Client (VS Code / Zed / Neovim)
                          |
                    tower-lsp (JSON-RPC)
                          |
                    ┌─────────────────┐
                    │   src/backend/   │
                    │  handlers + mod  │
                    └────────┬────────┘
                             |
              ┌──────────────┼──────────────┐
              ▼              ▼              ▼
        src/indexer/   src/resolver/   src/parser.rs
        scan, apply,   find, complete, tree-sitter
        cache, infer   infer, mod      queries
              │              │              │
              └──────────────┼──────────────┘
                             ▼
                       src/types.rs
                       SymbolEntry, FileData
```

## Key components

### src/backend/ — LSP protocol layer
- `mod.rs` — `LanguageServer` trait impl, `Backend` struct, capabilities
- `handlers.rs` — hover, completion, definition, references, folding, inlay hints
- `actions.rs` — code action quick-fixes
- `format.rs` — hover Markdown formatting

### src/indexer/ — Indexing & parsing
- Tree-sitter parsing for Kotlin/Java/Swift
- In-memory `DashMap`-based index
- Disk cache via bincode; `FileData.lines` is a `LazyLines` once-cell that is
  **not** serialized — parse fills eagerly, cache hits fill from disk on first
  actual use (hover/complete/find), iter commands never touch disk
- Library caches retain native-path keys (version 21); `get_file` decodes file
  URIs for lookup and keeps URI keys in memory. Fast start loads compact
  declarations on demand; first file access hydrates only that file's source
  lines. Inspect, find-kind enrichment and summaries use this accessor rather
  than assuming library files are already materialized.
- Generated-file detection (path conventions + header banner) persists
  `FileData.generated`, used to down-rank stubs in search
- File discovery via fd/walkdir

### src/resolver/ — Symbol resolution
- Multi-tier resolution: local → import → same package → star import → rg fallback
- Type inference for lambda params, `it`/`this`
- Completion scoring + auto-import
- Supertype hierarchy walking

### src/cli/ — Standalone CLI
- `find`, `refs`, `hover`, `complete` — one-shot queries
- `edit inject`, `context` — AI agent tools
- `check`, `edit organize` — code quality
- `call hierarchy`, `type hierarchy` — navigation
- Graph identity is name/file-based, not compiler binding. Before interpreting
  package/overload/source-set paths, read [GRAPH_IDENTITY.md](GRAPH_IDENTITY.md):
  hierarchy has defensive ambiguity checks, but reach/export can lose identity.

### src/parser.rs — Tree-sitter integration
- Query execution for Kotlin/Java/Swift grammars
- Symbol extraction: classes, functions, properties, imports
- `collect_syntax_errors()` suppresses 11+ grammar phantom classes
  (single-line bodies, `catch<T>`, context receivers, detached constructors,
  …) — each pinned by an `fp_*` regression test with a real-error control
- Deprecated annotation detection

## Data flow

1. **Startup**: `index_workspace()` → discover files → parse → build index
2. **DidOpen**: `store_live_document_state()` → `index_content()` → publish diagnostics
3. **Completion**: `completions()` → line-scan → index lookup → score → return items
4. **Hover**: `hover_impl()` → resolve symbol → enrich → format Markdown
5. **CLI**: build index → execute command → output text/JSON

## Shared CLI edit engine

`src/cli/edit.rs` prepares all requested raw UTF-8 texts and strict UTF-16 edits
before writing, then rechecks contents, permissions and retained filesystem
identities. Each changed target is replaced through a same-directory create-new
`tempfile`; `same-file` handles stay open during comparisons. Destination handles
are released after the final check so Windows can replace the target; parent and
temporary-file handles remain for cleanup checks. Temporary cleanup is
identity-checked, including persist failures; uncertain paths are retained with
an explicit error instead of blindly removed. The internal generic commit hook
is a zero-cost no-op in production and permits deterministic real-filesystem
conflict tests, without CLI/environment test switches.

Direct routes are rename, missing imports and code-action apply. The semantic
insert dispatcher also uses this engine, but its old flat parser branches are not
registered by `is_subcommand`; plain grouped `edit insert` is a different writer.
The line-only helper is test compatibility code; disk preview/apply share the raw
text implementation. `edit inject` performs no edit. No index/cache/schema change.
See `docs/commands.md` → **Shared-engine edit safety** for actual guarantees and
limits (per-file, not cross-file atomicity; remaining TOCTOU; no ACL/crash promise).
