# kotlin-lsp — Claude Code Instructions

## CRITICAL: LSP First, grep last

When investigating Kotlin/Java/Swift code:

1. **`kotlin-lsp find <NAME>`** — find definition
2. **`kotlin-lsp hover <FILE> <LINE> <COL>`** — get signature
3. **`kotlin-lsp refs <NAME>`** — find references
4. **`rg <pattern>`** — last resort

## Pre-Commit Checklist

Every commit must pass:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --no-fail-fast
```

## Source Layout

| Path | Purpose |
|------|---------|
| `src/main.rs` | Entry point, CLI dispatch |
| `src/backend/` | LSP handlers (hover, completion, codeAction, etc.) |
| `src/indexer/` | File discovery, tree-sitter parsing, disk cache |
| `src/parser.rs` | Tree-sitter queries, symbol extraction |
| `src/resolver/` | Cross-file resolution, imports, inference |
| `src/cli/` | Standalone CLI (find, refs, hover, check, etc.) |
| `src/types.rs` | `SymbolEntry`, `FileData`, shared types |

## Key Design Rules

1. **No JVM/Gradle** — pure Rust, tree-sitter only
2. **Cross-file index** — in-memory DashMap, persisted via bincode
3. **`#[serde(default)]`** on new `SymbolEntry` fields, bump `CACHE_VERSION`
4. **Tests in `*_tests.rs`**, not inline `mod tests {}`
5. **Use `KIND_*` constants**, never hardcoded node kind strings
6. **`expect("reason")` not `unwrap()`** in production code

## CI Checks

- `rustfmt --check`
- `clippy -- -D warnings`
- `cargo test` on ubuntu, macos, windows
