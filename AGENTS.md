# kotlin-lsp — Agent Instructions

> For AI coding agents (Claude Code, Copilot, Codex, Cursor, etc.)

## Project Identity

**kotlin-lsp** is a Rust-based, tree-sitter-backed LSP server for Kotlin, Java, and Swift.  
Zero JVM, instant startup, <200MB RAM. Designed for agentic use.

## Quick Start

```sh
cargo build --release
cargo test
cargo clippy -- -D warnings
```

## Non-Negotiable Rules

1. **Zero warnings** — fix clippy/fmt, never `#[allow]` without a comment
2. **No hardcoded node kind strings** — use `KIND_*` constants from `src/queries.rs`
3. **Prefer generics over `Box<dyn Trait>`** — static dispatch, zero cost
4. **No `unwrap()` in production** — use `expect("reason")` or `?`
5. **Tests in `*_tests.rs` files** — not inline `mod tests {}`
6. **`#[serde(default)]` on new `SymbolEntry` fields** — bump `CACHE_VERSION` too

## When to Use LSP vs CLI

| Need | Tool |
|------|------|
| Find definition | `kotlin-lsp find <NAME>` |
| Find all references | `kotlin-lsp refs <NAME>` |
| Get hover/signature | `kotlin-lsp hover <FILE> <LINE> <COL>` |
| Get completions | `kotlin-lsp complete <FILE> <LINE> [COL]` |
| One-stop context | `kotlin-lsp context <FILE> <LINE> <COL>` |
| Check syntax errors | `kotlin-lsp check <FILE>...` |
| Call hierarchy | `kotlin-lsp call-hierarchy <FILE> <LINE> <COL>` |
| Type hierarchy | `kotlin-lsp type-hierarchy <NAME>` |
| Organize imports | `kotlin-lsp organize-imports <FILE>...` |

## Common Error Fixes

| Error | Cause | Fix |
|-------|-------|-----|
| E0382 | Clone everywhere | Use refs or proper ownership |
| E0597 | Lifetime too short | Restructure data |
| E0502 | Borrow conflict | Split borrows |
| `unused variable` | Renamed in refactor | Remove or prefix with `_` |
| `this if has identical blocks` | Copied code | Merge conditions |
