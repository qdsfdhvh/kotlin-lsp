# LSP Capabilities (compatibility transport)

`kotlin-lsp` is CLI-first but speaks LSP over stdio for editor compatibility.
When launched with no arguments it becomes a language server.

All CLI commands are available as LSP handlers. A few are visual-only.

| LSP handler | CLI equivalent | Notes |
|-------------|---------------|-------|
| `textDocument/definition` | `kotlin-lsp find` | |
| `textDocument/typeDefinition` | &mdash; | Resolves `val x: Foo` → `Foo` |
| `textDocument/declaration` | `kotlin-lsp find` | Delegates to definition |
| `textDocument/implementation` | `kotlin-lsp type hierarchy` | Transitive subtype lookup |
| `textDocument/hover` | `kotlin-lsp hover` | |
| `textDocument/completion` | `kotlin-lsp complete` | |
| `textDocument/references` | `kotlin-lsp refs` | |
| `textDocument/documentSymbol` | &mdash; | Outline / workspace symbol |
| `textDocument/codeAction` | `kotlin-lsp code-action` | |
| `textDocument/rename` | &mdash; | Project-wide rename |
| `textDocument/formatting` | &mdash; | Kotlin: ktfmt (default) / ktlint. Java: google-java-format. Swift: swift-format |
| `textDocument/rangeFormatting` | &mdash; | Clips format to requested range |
| `textDocument/callHierarchy` | `kotlin-lsp call hierarchy` | |
| `textDocument/inlayHint` | &mdash; | Configurable inline type hints |
| `textDocument/signatureHelp` | &mdash; | Editor popup |
| `textDocument/semanticTokens` | `kotlin-lsp tokens` | Syntax highlighting — editor only |
| `textDocument/documentHighlight` | &mdash; | Editor occurrence highlight |
| `textDocument/foldingRange` | &mdash; | Code folding |
| `textDocument/selectionRange` | &mdash; | Expression selection |
| `textDocument/onTypeFormatting` | &mdash; | Auto-indent on `}` |
