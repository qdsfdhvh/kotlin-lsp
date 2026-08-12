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
- `inject`, `list-types`, `context` — AI agent tools
- `check`, `organize-imports` — code quality
- `call-hierarchy`, `type-hierarchy` — navigation

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
