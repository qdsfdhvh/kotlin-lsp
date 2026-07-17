# Command Reference

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
