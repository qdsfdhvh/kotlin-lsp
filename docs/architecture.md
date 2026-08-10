# Architecture

## Overview

`kotlin-lsp` is a no-JVM symbol engine for Kotlin, Java, and Swift — CLI-first,
LSP protocol features (goto-definition, hover, completion, diagnostics, etc.)
with an LSP transport for editor compatibility.

## Hexagonal Architecture

```
┌──────────────────────────────────────────────────────────┐
│                      CLI Layer                           │
│  (args.rs → run.rs → batch.rs / templates.rs / etc.)    │
└──────────────┬───────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────┐
│                     LSP Layer                            │
│  (backend/mod.rs — LanguageServer trait impl)            │
│  ├── handlers.rs — feature implementations              │
│  ├── helpers.rs — diagnostics                            │
│  ├── actions.rs — code actions                           │
│  ├── nav.rs — goto-definition / type-definition         │
│  └── format.rs — hover markdown formatting              │
└──────────────┬───────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────┐
│                   Index Layer                            │
│  (indexer/)                                              │
│  ├── scope.rs — file/workspace queries                  │
│  ├── lookup.rs — definition/resolution                  │
│  ├── resolution.rs — symbol enrichment                  │
│  ├── cache.rs — on-disk serialization                   │
│  ├── scan.rs — workspace scanning                        │
│  ├── apply.rs — merging results                         │
│  ├── infer/ — type inference (cst_cursor, sig, subst)   │
│  └── live_tree.rs — live document tracking              │
└──────────────┬───────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────┐
│                   Parser Layer                           │
│  (parser.rs — tree-sitter dispatch)                     │
│  ├── queries.rs — node kind constants (contract: docs/codebase/QUERIES.md)   │
│  ├── str_ext.rs — string utilities                       │
│  └── lines_ext.rs — line-based parsing                  │
└──────────────┬───────────────────────────────────────────┘
               │
┌──────────────▼───────────────────────────────────────────┐
│                   Domain Types                           │
│  (types.rs — SymbolEntry, FileData, ImportEntry, etc.)  │
└──────────────────────────────────────────────────────────┘
```

## Key Data Structures

### `Indexer`
Central shared state holding parsed file data, symbol definitions, subtype mappings,
and live document trees. Thread-safe via `Arc<DashMap<K, V>>` and `RwLock`.

```rust
pub(crate) struct Indexer {
    pub files: Arc<DashMap<String, FileData>>,
    pub definitions: Arc<DashMap<String, Vec<Location>>>,
    pub subtypes: Arc<DashMap<String, Vec<Location>>>,
    pub live_lines: Arc<DashMap<String, Vec<String>>>,
    // ...
}
```

### `SymbolEntry`
Per-symbol cached data from tree-sitter parsing:

```rust
pub(crate) struct SymbolEntry {
    pub name: String,
    pub kind: SymbolKind,
    pub range: Range,
    pub selection_range: Range,
    pub detail: String,          // Full signature (e.g. "fun foo(x: Int): String")
    pub type_params: Vec<String>,
    pub extension_receiver: String,
    pub deprecated: bool,
}
```

### `FileData`
All data extracted from a single parsed file:

```rust
pub(crate) struct FileData {
    pub uri: String,
    pub lines: Vec<String>,
    pub symbols: Vec<SymbolEntry>,
    pub imports: Vec<ImportEntry>,
    pub syntax_errors: Vec<SyntaxError>,
    pub supers: Vec<(usize, String, Vec<String>)>,
    pub content_hash: u64,
}
```

## Edge Index System

kotlin-lsp extracts typed relationship edges during CST parsing and stores them
in pre-built DashMap indexes for O(1) lookup. No tree-sitter re-parse at query
时间 — query engine reads directly from edge maps.

### Edge types

| Edge | Key → Value | CST node | CLI command |
|------|------------|----------|-------------|
| `call_edges` | callee_name → [(caller_file, caller_name)] | `call_expression` | `callers`, `callees` |
| `import_edges` | imported_fqn → [(importing_file, local_name)] | `import_header` | `imports-of` |
| `override_edges` | method_name → [(overriding_file, class)] | `function_declaration` with `override` modifier | `type-hierarchy` |
| `annotation_edges` | annotation_name → [(file, symbol_name)] | `annotation` | `annotated` |

### Data flow

```
CST parse (parser.rs)
    │  extract_*_edges() walks CST nodes, returns Vec<(String, String)>
    ▼
FileData (types.rs)
    │  edges stored as #[serde(default)] Vec fields
    ▼
apply.rs
    │  merged into Indexer DashMaps by key
    ▼
SymbolGraph / WorkspaceQueryEngine
    │  typed query API (callers_of, imports_of, annotations_of, ...)
    ▼
CLI / LSP
```

### Annotation edges (since 2026-07-16)

CST structure:
```
function_declaration
  modifiers
    annotation
      "@"
      user_type
        type_identifier  ← annotation name (e.g. "Composable")
  simple_identifier     ← annotated symbol name (e.g. "MyScreen")
```

Extraction walks CST for `annotation` nodes, extracts the `type_identifier`
child as annotation name, and walks up to the nearest declaration node to find
`simple_identifier` as the annotated symbol.

Supported parent declaration kinds:
`function_declaration`, `class_declaration`, `object_declaration`,
`property_declaration`, `class_method`, `type_alias`, `enum_class`,
`interface_declaration`.

Query API:
```rust
// WorkspaceQueryEngine
let results: Vec<(String, String)> = engine.annotations_of("Composable");
// → [("src/MyScreen.kt", "MyScreen"), ("src/Other.kt", "OtherScreen")]
```

## Resolution Pipeline

1. **CST parsing** — tree-sitter produces a concrete syntax tree
2. **Symbol extraction** — walk the CST to find declarations
3. **Edge extraction** — walk the CST for relationships (calls, imports, overrides, annotations)
4. **Cross-file resolution** — match symbol names across files via `definitions` map
5. **Type substitution** — resolve generic type parameters for subclass contexts
6. **Rg fallback** — `rg` (ripgrep) for cold-start / unindexed symbols

## Concurrency

- The `Backend` holds the `Indexer` behind `Arc`
- LSP event handlers run on Tokio (async)
- File indexing runs via `tokio::task::spawn_blocking`
- Live document trees updated synchronously (single-threaded LSP dispatch)

## Testing Strategy

- **Unit tests** alongside production code (e.g., `inlay_hints_tests.rs`)
- **Smoke tests** in `tests/lsp_smoke.rs` (end-to-end LSP via stdio)
- **CLI tests** in `tests/` (grammar validation)
- All tests run via `cargo test`
