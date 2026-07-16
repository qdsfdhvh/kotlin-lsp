# Command Reference

Detailed flags and examples for each `kotlin-lsp` command.

## Symbol lookup

### find — declaration search

```bash
kotlin-lsp find <Name> [--limit N] [--module <fragment>] [--source-set <set>] [--kind class,fun] [--owner <name>] [--fuzzy] [--visibility <vis>] [--modifier <mod>]
```

- `--limit N` caps noise.
- `--module <frag>` narrows to a Gradle module (substring match on path).
- `--source-set <name>` filters KMP code (comma-separated for OR).
- `--kind class,fun,interface` filters by symbol kind.
- `--owner <name>` filters by enclosing class/interface/object.
- `--fuzzy` enables subsequence matching (splits query on whitespace, scores by token match). `--limit` defaults to 20.
- `--visibility public|internal|protected|private`
- `--modifier suspend|inline|data|sealed|abstract|open`

```bash
kotlin-lsp find LoginViewModel --limit 3
kotlin-lsp find Event --module play-domain --limit 5
kotlin-lsp find HomeScreen --source-set commonMain,androidMain
kotlin-lsp find --fuzzy "login repo" --limit 5
kotlin-lsp find <Name> --visibility public --modifier suspend
```

### refs — reference search

```bash
kotlin-lsp refs <Name> [--limit N] [--module <fragment>] [--source-set <set>] [--owner <name>] [--exclude-imports] [--ref-kind call|read|write|override|import|type-use] [--explain]
```

Same filters as `find`. Add `--json` for `relativePath` / `module` / `sourceSet` / `owner`.
`--exclude-imports` strips import-statement matches.
`--ref-kind` filters references by usage type; `--explain` annotates each result with its kind.

```bash
kotlin-lsp refs Click --module auth --limit 20
kotlin-lsp refs Refresh --owner ScreenAction --limit 20
kotlin-lsp refs ScreenAction.Refresh              # auto-detect owner
```

### hover — signature at position

```bash
kotlin-lsp hover <file> <line> <col>
```

Line and column are 1-based. Context resolves call sites via `cst_call_info` (returns function_name, qualifier, active_parameter). `--expand N` for surrounding source context.

### context — one-stop symbol info

```bash
kotlin-lsp context <file> <line> <col>
```

Returns definition + signature + doc comment. Best for: "tell me everything about this symbol".

### complete — cursor completions

```bash
kotlin-lsp complete <file> <line> [col] [--dot|--eol] [--no-stdlib]
```

`--dot` places cursor after last `.` on line. `--eol` uses end of trimmed line. `--no-stdlib` skips library sources.

## Navigation & hierarchy

### callers / callees

```bash
kotlin-lsp callers <file> <line> <col> [depth] [--json]
kotlin-lsp callees <file> <line> <col> [depth] [--json]
```

Tree-structured output (not flat). Cycles detected and skipped. JSON includes nested `children`.

**Important**: `callers`/`callees` return agent-tuned trees; `call-hierarchy` targets LSP protocol shapes. Prefer `callers`/`callees`.

### call-hierarchy / type-hierarchy

```bash
kotlin-lsp call-hierarchy <file> <line> <col>
kotlin-lsp type-hierarchy <Name> [--subtypes] [--supertypes] [--graph] [--depth N]
```

### implementations / subclasses

```bash
kotlin-lsp implementations <Name> [depth] [--json]
kotlin-lsp subclasses <Name> [depth] [--json]
```

Recursive tree with cycle detection.

### query — batch

```bash
echo '[...]' | kotlin-lsp query --json
```

Accepts JSON array on stdin. Types: `"definition"`, `"references"`, `"hover"`, `"summarize"`, `"callers"`. Single index load for all queries.

### imports-of / annotated / package-deps / docs

```bash
kotlin-lsp imports-of com.example.Foo --json       # files importing Foo
kotlin-lsp annotated Composable --json              # @Composable symbols
kotlin-lsp annotated Inject --kind class --json     # @Inject-annotated classes
kotlin-lsp package-deps com.example --json          # package dependencies
kotlin-lsp docs "login" --json                      # search signatures/KDoc
```

## Project analysis

### symbol-graph

```bash
kotlin-lsp symbol-graph [--json]
```

Full relationship graph: calls, inheritance, imports, overrides.

### snapshot

```bash
kotlin-lsp snapshot [--filter kind=class,fun] [--exclude-relationships] [--json]
```

Complete workspace JSON: symbols, modules, relationships, entry points.

### impact — change risk

```bash
kotlin-lsp impact <file> <line> <col> [--json]
```

Risk score, refs by kind, callers, tests.

### summarize

```bash
kotlin-lsp summarize <Name> [--expand]
```

Kind, signature, members, return type, parameters, KDoc. `--expand` for full member signatures.

### find-test

```bash
kotlin-lsp find-test <file> <line> <col>
```

Matches by naming convention, imports, source set layout.

### expect-actual

```bash
kotlin-lsp expect-actual <Name>
```

`expect` → `actual` across KMP source sets (and vice versa).

### modules / module-deps / module-files

```bash
kotlin-lsp modules                           # list Gradle modules
kotlin-lsp module-deps <module> [--incoming|--outgoing]
kotlin-lsp module-files <module>             # source files in module
```

### android-activities / android-composables

```bash
kotlin-lsp android-activities [--root <dir>]
kotlin-lsp android-composables <file>
```

### check / check --diagnose

```bash
kotlin-lsp check <file> [file...]
kotlin-lsp check <file>... --diagnose         # also validates call arguments
```

Syntax errors via tree-sitter. Exit code 1 if errors.

### format check / format apply

```bash
kotlin-lsp format check <file/dir>... [--json]
kotlin-lsp format apply <file/dir>... [--dry-run]
```

Uses `ktlint`. Requires `ktlint` on PATH.

### inject

```bash
kotlin-lsp inject <file>
```

Extracts all referenced type names, returns signatures in one batch.

## Code manipulation

### code-action

```bash
kotlin-lsp code-action <file> <line> <col> [--apply]
```

### organize-imports

```bash
kotlin-lsp organize-imports <file> [file...]
```

Sorts, deduplicates, removes unused imports.

### insert / insert-import / insert-member / insert-function / insert-override

```bash
kotlin-lsp insert <file> <line> (--before|--after) --content "..." [--in-place]
kotlin-lsp insert-import <file> <fqn> [--content <text>]
kotlin-lsp insert-member <file> <owner> --content <text>
kotlin-lsp insert-function <file> <owner> --content <text>
kotlin-lsp insert-override <file> <owner> --name <method>
kotlin-lsp insert-override <file> <owner> --content <text>
```

### batch / batch-imports

```bash
kotlin-lsp batch <rule.json> [--dry-run]
kotlin-lsp batch-imports <file> [--apply] [--json]
```

### rename / refs-at

```bash
kotlin-lsp rename <file> <line> <col> <newName> [--apply] [--json]
kotlin-lsp refs-at <file> <line> <col> [--json]
```

### inspect

```bash
kotlin-lsp inspect <file> [--json] [--expand N]
```

Package, imports, symbols, syntax errors in one command.

### doctor

```bash
kotlin-lsp doctor [--verbose] [--json]
```

Structured workspace health checks.

### search — semantic search

TF-IDF ranked semantic search over symbols. Indexes names, KDoc, signatures,
and return types. No external ML deps.

```bash
kotlin-lsp search <query> [--limit N] [--json]
```

```bash
kotlin-lsp search "find where token is refreshed" --limit 10
kotlin-lsp search "login view model" --json
```

### summarize --cached / summary-cache

Pre-computed AI-friendly symbol summaries. Agents load cached summaries without
re-parsing source files.

```bash
kotlin-lsp summarize <name> --cached [--json]
kotlin-lsp summary-cache
```