---
name: kotlin-lsp
description: Use the `kotlin-lsp` CLI for precise symbol lookup in Kotlin/Java/Swift projects — faster than grep/rg and returns typed answers (declarations, refs, signatures) instead of raw text matches. Saves tokens because results are scoped and structured.
---

# kotlin-lsp

`kotlin-lsp` is a tree-sitter–backed language server that ships a scriptable CLI (no daemon, no JVM). Reach for it when working with Kotlin, Java, or Swift symbols, especially in Android / KMP projects — it returns *declaration locations* and *type-aware references*, not text matches.

Check installation:

```bash
kotlin-lsp --version
```

If missing, suggest the install one-liner from the project README; do not auto-install without asking.

## When to use kotlin-lsp vs rg

```
Query is about Kotlin/Java/Swift symbols?
├─ No → rg / Read
└─ Yes:
   ├─ Symbol name is unique AND in this repo → rg --type kotlin is fine (and faster)
   ├─ Symbol name is generic (handle, String, Event, …) → kotlin-lsp find/refs --module … --limit
   ├─ Symbol lives in library (Compose, AndroidX, 3rd-party) → kotlin-lsp find (rg cannot reach)
   ├─ Symbol lives in generated or ignored code → kotlin-lsp find (plain rg may miss it)
   ├─ Need cross-module ref filtering (--module / --source-set / --owner) → kotlin-lsp refs
   ├─ Need one-stop symbol info (def + sig + doc) → kotlin-lsp context <file> <line> <col>
   ├─ Need syntax check on edited files → kotlin-lsp check <file>
   ├─ Need formatting check (like spotlessCheck) → kotlin-lsp format check <file/dir>...
   ├─ Need formatting apply (like spotlessApply) → kotlin-lsp format apply <file/dir>...
   ├─ Need callee tree (what does this call?) → kotlin-lsp callees <file> <line> <col>
   ├─ Need implementation tree → kotlin-lsp implementations <Name>
   ├─ Need subclass tree → kotlin-lsp subclasses <Name>
   ├─ Need fuzzy search → kotlin-lsp find --fuzzy "<query>"
   ├─ Need call hierarchy → kotlin-lsp call-hierarchy <file> <line> <col>
   ├─ Need class hierarchy → kotlin-lsp type-hierarchy <Name>
   ├─ Need batch of queries → echo '[...]' | kotlin-lsp query --json
   ├─ Need which files import X → kotlin-lsp imports-of <Name>
   ├─ Need symbols by annotation → kotlin-lsp annotated <Name>
   ├─ Need package deps → kotlin-lsp package-deps <package>
   ├─ Need full relationship graph → kotlin-lsp symbol-graph --json
   ├─ Need to search signatures → kotlin-lsp docs <query>
   ├─ Imports are messy → kotlin-lsp organize-imports <file>
   ├─ Need batch type injection for a file → kotlin-lsp inject <file>
   ├─ Need signature/type at a declaration → kotlin-lsp hover <file> <line> <col>
   └─ Need signature at a call site → kotlin-lsp find <name> (jump to decl), then hover the decl
```

## How it saves tokens

| Naive approach | Better with kotlin-lsp |
|---|---|
| `rg 'class MyViewModel'` returns every text match including doc comments and imports | `kotlin-lsp find MyViewModel --limit 5` returns only declaration sites |
| `rg 'MyViewModel'` to find usages, then manually filter | `kotlin-lsp refs MyViewModel --limit 20` returns real references |
| Open file, read 200 lines to figure out what `foo.bar(x)` returns | `kotlin-lsp hover Foo.kt 42 10` returns just the signature |

**Output is AI-tuned by default:**
- Text mode (default) for `find`/`refs` is **grouped by file with structural annotation** — path on its own line followed by `[<module> <sourceSet>]` when known, then one `line:col[ kind]` per match, blank line between file groups.
- `--json` emits **compact** JSON. Use when you need structured data for `jq` or when `signature` / `relativePath` matter downstream.
- `--relative` (workspace-relative paths) is **auto-enabled when stdout is piped** (always true in agent context). Pass `--absolute` to opt out.

## Commands

### Symbol lookup

#### find — declaration search

```bash
kotlin-lsp find <Name> [--limit N] [--module <fragment>] [--source-set <set>] [--kind class,fun] [--owner <name>]
```

- `--limit N` caps noise.
- `--module <frag>` narrows to a Gradle module (substring match on path).
- `--source-set <name>` filters KMP code (comma-separated for OR).
- `--kind class,fun,interface` filters by symbol kind.
- `--owner <name>` filters by enclosing class/interface/object.

```bash
kotlin-lsp find LoginViewModel --limit 3
kotlin-lsp find Event --module play-domain --limit 5
kotlin-lsp find HomeScreen --source-set commonMain,androidMain
```

#### refs — reference search

```bash
kotlin-lsp refs <Name> [--limit N] [--module <fragment>] [--source-set <set>] [--owner <name>]
```

Same filters as `find`. Add `--json` when you need `relativePath` / `module` / `sourceSet` / `owner` as parseable fields.

Use `--exclude-imports` to strip import-statement matches from results.
Useful for common names like `Event`, `Result`, `State` that appear in
thousands of import lines.

```bash
kotlin-lsp refs Click --module auth --limit 20
kotlin-lsp refs Refresh --owner ScreenAction --limit 20
kotlin-lsp refs ScreenAction.Refresh              # auto-detect owner
```

#### hover — signature at position

```bash
kotlin-lsp hover <file> <line> <col>
```

Line and column are 1-based, like editor cursors. Returns the type and surrounding signature.

**Context resolves call sites too**: `context` now works at call sites via `cst_call_info`, returning function_name, qualifier, and active_parameter. Use `--expand N` for surrounding source context.

```bash
# Step 1: locate the declaration
kotlin-lsp find login --limit 1
#   features/auth/src/commonMain/kotlin/Auth.kt:42:5: fun login

# Step 2: hover the declaration
kotlin-lsp hover features/auth/src/commonMain/kotlin/Auth.kt 42 5
```

Two calls are still cheaper than reading the file.

### Navigation & hierarchy

#### complete — cursor completions

```bash
kotlin-lsp complete <file> <line> [col] [--dot|--eol] [--no-stdlib]
```

Pass `col`, or use `--dot` to place the cursor after the last `.` on the line, or `--eol` to use the end of trimmed line content. Text output is tab-separated: `label\tkind\tdetail\timport`. JSON output: `[{label, kind, detail?, import?}]`.

```bash
kotlin-lsp complete shared/src/commonMain/kotlin/data/Repo.kt 87 --dot
```

#### context — one-stop symbol info

```bash
kotlin-lsp context <file> <line> <col>
```

Returns definition + signature + doc comment in a single call. Good for: "tell me everything about this symbol".

#### callers — who calls this function

```bash
kotlin-lsp callers <file> <line> <col>         # direct callers only (depth 1)
kotlin-lsp callers <file> <line> <col> 3       # transitive callers up to depth 3
kotlin-lsp callers <file> <line> <col> --json  # JSON tree output
```

Returns a **tree** of callers, not a flat list. Each node shows the caller function
name, kind, file, and position. Use `--depth N` for transitive traversal; default is 1.
Cycles are detected and skipped. JSON output includes nested `children` arrays.

#### callees — what this function calls

```bash
kotlin-lsp callees <file> <line> <col>         # direct callees only
kotlin-lsp callees <file> <line> <col> 3       # transitive callees up to depth 3
kotlin-lsp callees <file> <line> <col> --json  # JSON tree output
```

Parses the function body with tree-sitter to find call expressions, resolves
each callee to its declaration location, and returns a tree of callees.

**Important**: these commands differ from `call-hierarchy` — `callers`/`callees`
return tree-structured output designed for AI agents, while `call-hierarchy`
targets LSP protocol shapes (flat lists). For agent workflows, prefer
`callers`/`callees`.

#### fuzzy — fuzzy search (new)

```bash
kotlin-lsp find --fuzzy "<tokens>" [--limit N]
```

Splits query on whitespace, matches each token as a subsequence.
Returns results sorted by relevance. `--limit` defaults to 20.

```bash
kotlin-lsp find --fuzzy "login repo" --limit 5
```

#### implementations — interface implementations (new)

```bash
kotlin-lsp implementations <Name> [depth] [--json]
```

Tree of all classes implementing the given interface.

```bash
kotlin-lsp implementations Repository
kotlin-lsp implementations ViewModel 2 --json
```

#### subclasses — class subclasses (new)

```bash
kotlin-lsp subclasses <Name> [depth] [--json]
```

Tree of all subclasses of the given class.

```bash
kotlin-lsp subclasses Activity --json
```

#### query — batch query (new)

```bash
echo '[...]' | kotlin-lsp query --json
```

Accepts JSON array on stdin, loads index once, returns results.
Types: "definition", "references", "hover", "summarize", "callers".

```bash
echo '[{"type":"definition","name":"LoginViewModel"}]' | kotlin-lsp query --json
```

#### imports-of / annotated / package-deps / docs (new)

```bash
kotlin-lsp imports-of com.example.Foo --json       # files importing Foo
kotlin-lsp annotated Composable --json              # @Composable symbols
kotlin-lsp package-deps com.example --json          # package dependencies
kotlin-lsp docs "login" --json                      # search signatures
```

#### visibility / modifier filters (new)

```bash
kotlin-lsp find <Name> --visibility public --modifier suspend
```

#### call-hierarchy — caller lookup

```bash
kotlin-lsp call-hierarchy <file> <line> <col>
```

Finds callers of a function via `rg` across the workspace.

#### type-hierarchy — class hierarchy

```bash
kotlin-lsp type-hierarchy <Name> [--subtypes] [--supertypes]
```

Shows subtypes (classes implementing/extending) and/or supertypes. Default: subtypes only.

### Project analysis

#### symbol-graph — full relationship export (new)

```bash
kotlin-lsp symbol-graph [--json]
```

Exports the complete symbol relationship graph:
calls (who calls whom), inheritance (subtype ↔ supertype),
imports (who imports what), overrides (method overrides).

```bash
kotlin-lsp symbol-graph --json
```

#### check — syntax validation

```bash
kotlin-lsp check <file> [file...]
```

Parses files with tree-sitter and reports syntax errors. No index needed. Exit code 1 if errors found.

#### inject — batch type injection

#### doctor — project diagnostics

```bash
kotlin-lsp doctor [--verbose]
```

Checks workspace health: root existence, source file counts, library sources
extraction status, index cache, and runtime tools (rg, fd). Use `--verbose`
for directory-level cache size and untracked source directory warnings.

```bash
kotlin-lsp inject <file>
```

Reads a file, extracts all referenced type names, and returns their signatures in one batch. One call replaces N context calls.

#### format check — check formatting violations (like spotlessCheck)

```
kotlin-lsp format check <file/dir>... [--json]
```

Runs `ktlint` in lint-only mode on the given files and directories.
Reports violations with unified diff context. Exits non-zero if any
violations are found. Use `--json` for machine-readable output.

Requires `ktlint` to be installed on PATH.

Examples:
```
kotlin-lsp format check src/main.kt              # check one file
kotlin-lsp format check src/                      # check all .kt/.kts in src/
kotlin-lsp format check src/ --json               # machine-readable
```

#### format apply — apply formatting in-place (like spotlessApply)

```
kotlin-lsp format apply <file/dir>... [--dry-run]
```

Runs `ktlint --format` on the given files and directories in-place.
Reports which files were modified and any errors. Exits non-zero if
unfixable violations or errors occur.

Requires `ktlint` to be installed on PATH.

Examples:
```
kotlin-lsp format apply src/main.kt               # format one file
kotlin-lsp format apply src/                       # format all .kt/.kts in src/
```
#### impact — change risk analysis

```bash
kotlin-lsp impact <file> <line> <col> [--json]
```

Returns risk score, references breakdown by kind, callers, and test coverage.
Use before refactoring to gauge blast radius.

#### summarize — symbol overview

```bash
kotlin-lsp summarize <Name> [--expand]
```

Returns kind, signature, members list, return type, parameters, and KDoc.
One call replaces N hover + context calls. Use `--expand` for full member signatures.

#### find-test — locate tests

```bash
kotlin-lsp find-test <file> <line> <col>
```

Finds test files/methods for the symbol at cursor. Matches by naming convention,
imports, and source set layout.

#### expect-actual — KMP expect/actual navigation

```bash
kotlin-lsp expect-actual <Name>
```

Resolves `expect` → all `actual` implementations across KMP source sets (and vice versa).

#### modules / module-deps / module-files — Gradle module graph

```bash
kotlin-lsp modules                           # list all detected Gradle modules
kotlin-lsp module-deps <module> [--incoming|--outgoing]
kotlin-lsp module-files <module>             # list source files in a module
```

Uses `settings.gradle*` to discover modules. Direction defaults to `--outgoing`.

#### android-activities / android-composables — Android resource checks

```bash
kotlin-lsp android-activities [--root <dir>] # list Activities from AndroidManifest
kotlin-lsp android-composables <file>        # find @Composable functions
```

#### check --diagnose — call-argument validation

```bash
kotlin-lsp check <file>... --diagnose
```

Same as `check` but also validates argument counts and types at call sites.

### Code manipulation

#### code-action — inspect or apply quick fixes

```bash
kotlin-lsp code-action <file> <line> <col> [--apply]
```

List available code actions first. Use `--apply` only when the intended edit is obvious.

#### organize-imports — import cleanup

```bash
kotlin-lsp organize-imports <file> [file...]
```

Sorts, deduplicates, and removes unused imports from Kotlin/Java files.

#### insert — code insertion

```bash
kotlin-lsp insert <file> <line> (--before|--after) --content "..." [--in-place]
```

Inserts text before or after a given line. With `--in-place`, writes back to file.

#### batch — cross-file batch modifications

```bash
kotlin-lsp batch <rule.json> [--dry-run]
```

Applies find-replace and insert operations across multiple files atomically via JSON rules. Use `--dry-run` to preview.

#### batch-imports — resolve missing imports

```bash
kotlin-lsp batch-imports <file> [--apply] [--json] [--output <path>]
```

Scans for uppercase identifiers, resolves each against the index, and reports
unique (auto-importable), ambiguous (multiple FQN matches), and unknown
identifiers. Defaults to dry-run; use `--apply` to write.

```bash
kotlin-lsp batch-imports src/Feature.kt              # preview only
kotlin-lsp batch-imports src/Feature.kt --apply       # write imports
```

#### insert-import — add a single import

```bash
kotlin-lsp insert-import <file> <fqn>               # auto-generates `import <fqn>`
kotlin-lsp insert-import <file> <fqn> --content <text>  # custom import line
```

Auto-generates `import <fqn>` from the second positional argument when `--content`
is not provided.  Detects and skips duplicate imports.  Respects blank-line gaps
after `package` declarations.

#### insert-member, insert-function — class member insertion

```bash
kotlin-lsp insert-member <file> <owner> --content <text>
kotlin-lsp insert-function <file> <owner> --content <text>
```

Finds the class body via tree-sitter and inserts before the closing `}` with
proper indentation matching existing members.

#### insert-override — generate override boilerplate

```bash
kotlin-lsp insert-override <file> <owner> --name <method>
kotlin-lsp insert-override <file> <owner> --content <text>
```

When `--name` is provided, generates `override fun <method>() { TODO(...) }`
at the correct position in the class body.  When `--content` is provided,
inserts the raw content as-is (for custom overrides with full signatures).

```bash
kotlin-lsp insert-member <file> <owner> --content <text>
kotlin-lsp insert-function <file> <owner> --content <text>
```

Finds the class body range from the index and inserts before the closing `}`.

#### rename — symbol-aware rename

```bash
kotlin-lsp rename <file> <line> <col> <newName> [--apply] [--json]
```

Renames all references to the symbol at the cursor within the file.
Dry-run by default. Uses `rename_in_scope()` for word-boundary matching.

#### refs-at — filtered references by declaration context

```bash
kotlin-lsp refs-at <file> <line> <col> [--json]
```

Resolves the symbol identity at the cursor and filters reference candidates
by declaration package context — reduces false positives.

#### inspect — one-stop file snapshot for agents

```bash
kotlin-lsp inspect <file> [--json] [--expand N]
```

Returns package, imports, symbols, and syntax error count in one command.

#### doctor --json — structured diagnostics

```bash
kotlin-lsp doctor [--verbose] [--json]
```

With `--json`, returns structured checks with `name`, `status`, and `message`.

## Performance flags

| Flag | When |
|---|---|
| _(none)_ | Auto — use cached index if available, else fast `rg`/`fd` fallback |
| `--fast` | Always use `rg`/`fd`; instant, no index needed |
| `--smart` | Require a pre-built index; run `kotlin-lsp index` first |
| `--root <dir>` | Override workspace root (default: nearest `.git` directory) |
| `--no-stdlib` | For `complete`: skip extracted stdlib/library sources for faster workspace-only suggestions |

## Indexing & library sources

- **KMP source sets** are detected structurally — anything under `src/<name>/{kotlin,java}` counts. Custom names like `jvmCommonMain` work automatically.
- **Android SDK sources** are auto-detected from `local.properties` → `$ANDROID_HOME` → `$ANDROID_SDK_ROOT`.
- **Gradle library sources** (Compose, coroutines, AndroidX): run once:
  ```bash
  kotlin-lsp extract-sources
  ```
  Subsequent queries pick them up.
- **Pre-build index** for faster first-lookup:
  ```bash
  kotlin-lsp index --root ./android
  ```
- **Cache diagnostics**:
  ```bash
  kotlin-lsp cache stats
  ```

## Anti-patterns

- **Don't** use `rg 'class FooBar'` when `kotlin-lsp find FooBar` will do — the LSP filters out string literals, comments, and imports.
- **Don't** read the entire file just to see a function signature; use `hover` instead.
- **Don't** omit `--limit` on `refs` for common names like `String` or `Result` — they have hundreds of hits.
- **Don't** invoke `kotlin-lsp` recursively inside a script that's already inside an LSP context; the CLI is for one-shot queries.

## Reporting pain points

When using `kotlin-lsp` on a project task, keep track of concrete tool pain points such as false-positive `check` output, overly broad `refs`, missing narrowing flags, slow commands, or confusing output. If the user allows or requests upstream feedback, file an issue against `qdsfdhvh/kotlin-lsp` with a small sanitized repro and the expected/actual behavior.

**Privacy rule**: never include the current project's repo name, file paths, package names, class/function names, logs, business terminology, screenshots, or source snippets in upstream issues unless the user explicitly provides public repro material. Rewrite examples as generic placeholders like `example-domain`, `FeatureViewModel`, `ScreenState`, and `path/to/File.kt`; prefer minimal synthetic code that reproduces the tool behavior.

## Help and debug commands

Run `kotlin-lsp --help` for the exhaustive command list. Keep routine agent output focused on the task; do not paste full help text unless the user asks. Debug commands such as `tokens`, `tree`, and `benchmark` are primarily for kotlin-lsp development, not ordinary project navigation.
