# CLI Commands

`kotlin-lsp` works standalone — no editor, no daemon.

Flags are **command-specific**, not global behavior promises. See the
[applicability table](#flag-applicability) and individual command contracts.
`capabilities --json` lists group flag **unions**: a flag on `search` or `tool`
does not imply that every member supports it.

## Quick examples

```bash
# ── core lookup ──
kotlin-lsp find ViewModel              # declarations
kotlin-lsp refs ViewModel              # references
kotlin-lsp hover Foo.kt 42 10          # signature + doc
kotlin-lsp context Foo.kt 42 10        # def + sig + doc + refs
kotlin-lsp complete Foo.kt 42 --dot    # completions

# ── groups ──
kotlin-lsp docs "parse"               # KDoc search (top-level alias)
kotlin-lsp search "login"            # semantic search (shorthand)
kotlin-lsp search semantic "login"  # semantic search (explicit)
kotlin-lsp search "kind:class name:ViewModel"  # field-filtered search
kotlin-lsp search "kind:function path:src/api"  # filters-only (list all matching)
kotlin-lsp search summarize User      # symbol summary
kotlin-lsp search docs "parse"        # KDoc search
kotlin-lsp search imports UserRepo     # who imports this
kotlin-lsp edit rename Foo.kt 42 10 X # rename symbol
kotlin-lsp edit imports Foo.kt       # add missing imports
kotlin-lsp edit organize Foo.kt       # clean imports
kotlin-lsp edit inject Foo.kt         # resolve types
kotlin-lsp edit new activity Login    # file from template
kotlin-lsp tool code-action F.kt 1 1  # list code actions
kotlin-lsp tool inspect Foo.kt        # file diagnostics
kotlin-lsp tool snapshot              # workspace symbols as JSON (home sources excluded)
kotlin-lsp tool bench                 # performance
kotlin-lsp capabilities --json        # CLI capability manifest (incl. grammar versions)
kotlin-lsp tool doctor                # system health
kotlin-lsp call hierarchy F.kt 42 10  # direct callers / callees
kotlin-lsp call diff HEAD~1 main      # call-tree diff (inferred entries)
kotlin-lsp call reach entry --to target  # all call paths entry→target
kotlin-lsp type hierarchy User        # super/subtype tree
kotlin-lsp type sealed Result         # sealed subclasses
kotlin-lsp android composables F.kt   # composable analysis
kotlin-lsp module list                # list modules

# ── infrastructure ──
kotlin-lsp check Foo.kt               # syntax + warnings
kotlin-lsp format check src/          # formatting
kotlin-lsp index --root ./            # build cache
kotlin-lsp gradle-deps                # parsed dependencies
kotlin-lsp cache stats                # cache info
```

## Command groups

| Group | Subcommands | What they do |
|-------|-------------|-------------|
| **search** | `semantic`, `docs`, `summarize`, `cache-stats`, `imports`, `annotated`, `find-test`, `expect-actual` | Symbol discovery and analysis |
| **edit** | `rename`, `batch`, `imports`, `inject`, `insert`, `new`, `organize` | Code modification |
| **tool** | `inspect`, `graph`, `snapshot`, `bench`, `doctor`, `workspace`, `query`, `skills`, `code-action`, `tokens`, `tree` | Debug / introspection |
| **call** | `hierarchy`, `diff`, `reach` | Call graph / call-tree diff / call paths |
| **type** | `hierarchy`, `sealed` | Type hierarchy |
| **module** | `list`, `deps`, `files`, `packages` | Module structure |
| **android** | `activities`, `composables` | Android resources |
| **format** | `check`, `apply` | Code formatting |

## Top-level commands

| Command | Description |
|---------|-------------|
| `find <name>` | Declaration search |
| `refs <name>` | All references |
| `hover <file> <line> <col>` | Signature, KDoc, deprecation |
| `complete <file> <line> [col]` | Dot-completion, auto-import |
| `context <file> <line> <col>` | One-stop: def + sig + doc + refs |
| `impact <file> <line> <col>` | Impact / risk analysis |
| `check <file>...` | Syntax errors, imports, deprecation; exits 1 on syntax errors or missing/unreadable inputs, source-less dirs reported as `empty_dirs` (exit 0 unless other inputs fail); `--diagnose` requires at least one checkable file, otherwise exits 1 with an input diagnostic in JSON `errors` |
| `index [--root <dir>] [--gradle] [--lang <lang>]` | Build workspace cache; `--gradle` enables Gradle dependency resolution, `--lang kotlin\|java\|swift` for a per-language cache (`index-<lang>.bin`) |
| `index-jars [root]` | Index library JARs |
| `extract-sources` | Unpack `*-sources.jar` |
| `sources` | List auto-discovered source roots |
| `cache stats` | Cache diagnostics |
| `gradle-deps` | Parsed Gradle dependencies |

## Indexed hover

`hover <file> <line> <col> --smart` preserves declaration signature/KDoc output.
At a reference, its conservative fallback accepts an unqualified CST identifier
only when the indexed name has one distinct definition, including enabled library
candidates after a warm start. Qualified or ambiguous reference fallbacks remain
unresolved; comments, literal text and punctuation are not reference matches.
A missing result exits 1; JSON success remains `{"signature":"…"}`.
This is name-based lookup, not compiler binding. Positions are 1-based UTF-16;
file operands retain their cwd-relative base (`--root` selects the index).

## Direct call hierarchy (`call hierarchy`)

```bash
kotlin-lsp call hierarchy entry --json --root ./project --no-stdlib
kotlin-lsp call hierarchy Service.send --incoming
kotlin-lsp call hierarchy src/Calls.kt 2 15 --outgoing --root ./project
```

Lookup is an **exact function/method name**, or a unique `Class.method`, not fuzzy
symbol search. For nested types, `Class` is the nearest enclosing type's simple
name (`Inner.entry`, not `Outer.entry` for a method inside `Outer.Inner`). The
position form selects a function/method declaration (including overrides) or a
call identifier; line/column are **1-based UTF-16**. Override properties remain
non-callable. A declaration position can select between same-named declarations.
Call identifiers use name-based lookup and may remain ambiguous even when a
compiler could bind them. Missing files, invalid
cursors/roots, non-callable declarations, unindexed files and missing/ambiguous
names exit 1; ambiguity includes sorted candidates with real file/line/column.
Runtime JSON errors contain `error` (plus `candidates` for ambiguity); argument
errors use stderr. No arbitrary first definition or fabricated location.

`--incoming` selects callers, `--outgoing` selects callees. Neither flag (default)
or both flags selects both. Unselected JSON arrays remain empty. This is **one
hop**, not a recursive tree: depth/extra operands are rejected; use `call reach`
for paths. Directions are only accepted by `call hierarchy`.

Identity limits: declaration outgoing can separate different caller files, but
same-file overloads are refused because edges lack declaration ranges. `call
reach` can merge same-named bodies across packages, overloads and source sets;
`Class.method` does not disambiguate identical class names in different packages.
`tool graph` retains caller files, while snapshot relationship pairs lose that
identity. Treat these as candidate relationships, not compiler-resolved paths.
See [graph boundary evidence](codebase/GRAPH_IDENTITY.md) for exact repros and
cold/warm behavior.

`--root` overrides cwd for indexing and relative file operands. Without it,
workspace discovery is unchanged and relative files use cwd. `--no-stdlib` skips
home sources (`~/.kotlin-lsp/sources`). Name lookup and both directions share one
index, including cached library data when enabled.

For `fun entry() { target() }`, querying `entry --json` returns compact JSON:

```json
{"incoming":[],"name":"entry","outgoing":["target"]}
```

The top-level `name`/`incoming`/`outgoing` contract and incoming **string-array**
type are preserved. Entries now mean sorted, deduplicated **graph keys** (e.g.
bare name or `Class.method`) in both directions, replacing incoming rg snippets. They
are not locations, call-site counts or unique symbol IDs. Repeated calls collapse;
self/cycle edges appear once without recursive expansion. Outgoing keys can be
external/unresolved (including constructor or synthetic chained-call keys); a key
is not proof of an indexed callee definition. Hierarchy uses the stored graph keys,
not reach's return-type/implementor expansion. Declarations, comments and string
text are not call edges.

The graph is grammar/name-based, not compiler-grade package/overload/source-set
binding or dynamic dispatch. Incoming edges that could bind multiple indexed
callables fail with candidates instead of merging them. Outgoing edges are
filtered by caller file and key; same-file overloads sharing that key fail even
at a declaration position because the stored edges cannot distinguish bodies.
The bundled Kotlin grammar rejects some legal identifiers (e.g. `fun 目标()`);
unsupported grammar constructs are not repaired by hierarchy lookup. UTF-16
positions around Unicode text and Swift Unicode callable names are supported.

## Batch symbol queries (`tool query`)

```bash
printf '%s\n' '[{"type":"definition","name":"target"},{"type":"references","name":"target","refKind":"call"}]' |
  kotlin-lsp tool query --json --root ./project --no-stdlib
```

Reads a JSON array from stdin, loads one index for all items, and emits a compact
JSON array in request order. `--root` selects the indexed workspace regardless
of cwd; `--no-stdlib` skips `~/.kotlin-lsp/sources`. Relative file operands resolve
against an explicit `--root`, otherwise cwd. File paths are canonicalized;
`line` and `col` are **1-based UTF-16**, not byte offsets.

| `type` | Fields | Success fields (besides `type`) |
|--------|--------|--------------------------------|
| `definition` | `name` | `results`: file/line/col locations |
| `references` | `name`, optional `refKind` | `results`: file/line/col locations, `filter_applied` |
| `hover` | `file`, `line`, `col` | `name`, `signature` (null if unavailable or ambiguous) |
| `summarize` | `name` | `name`, `kind`, `visibility`, `signature`, `deprecated` |
| `callers` | `file`, `line`, `col`, optional `depth` | `name`, `callers`: name/file entries, `depth` |
| `implementations`, `subclasses` | `name` | `name`, `results`: file URI/line entries |

`refKind` accepts `call`, `read`, `write`, `override`, `import`, `type-use`, or
`declaration`. Omitted, `all`, and `reference` follow normal smart `refs`:
include declarations and usages, exclude imports/package clauses. Use `import`
explicitly to find import occurrences (including import-only files). Candidates
come from the existing scoped reference search, verified as CST identifiers;
comments and string text are excluded. `call` selects callee identifiers, not
declarations, receivers, or arguments. These are name-based queries, not
package/overload-resolved identities. Interpolation expressions remain references
(including Kotlin and Swift strings). Swift property bindings are declarations,
not reads; simple and compound assignment targets are writes, including adjacent
statements.

`callers` supports only omitted `depth` or `depth: 1`; other depths error.
It uses existing name-based call edges, capped at 20 callers. Subtype queries
retain their 50-entry cap. Locations/edges are sorted and deduplicated before
these caps; the existing item fields and batch array shape are unchanged.

Invalid items (including invalid filters, depths, files, or positions) return
`{"type":"…","error":"…"}` in place without discarding other results. Any item
error exits 1; malformed JSON or a non-array input fails before execution.
Empty definition/reference results are valid; a missing hover signature is null.

## Shared-engine edit safety

These guarantees apply to **`edit rename --apply`, `edit imports --apply`, and
`tool code-action --apply`**, not to every edit/format command. Plain `edit insert`,
`edit batch`, organize/new/format keep their separate implementations; `edit inject`
is a read-only type query. File operands remain cwd-relative; existing root/index
and library policies are unchanged. Rename and code-action apply enforce their
resolved workspace root. Imports still edits its explicitly supplied file.

- The engine reads **every requested file before writing**, requiring valid UTF-8,
  regular files, valid ranges and root containment (when supplied). Duplicate
  canonical targets, including symlink aliases, are rejected for the entire batch.
- `TextEdit` ranges are **0-based LSP UTF-16**, end-exclusive. CLI cursor operands
  remain **1-based UTF-16**. Reversed/out-of-range ranges, surrogate-interior
  positions, overlapping replacements and inserts inside replacements fail.
  Adjacent edits are valid; equal-position inserts retain request order and precede
  a replacement starting there. Imports retains its existing reversed candidate
  insertion order, so its preview and apply agree.
- Untouched bytes, LF/CRLF/mixed endings and final-newline state are retained.
  `new_text` is verbatim, including explicit CR/LF; the engine never normalizes it
  or restores a removed EOF newline. Generated import statements use LF, so adding
  imports to CRLF text can intentionally produce mixed endings.
- Rename without `--apply` is a dry run. Imports with `--json` is a preview even
  with `--apply`; `--dry-run` also prevents writes. Dry runs create no temporary
  files and change neither content nor permissions. In an edit summary,
  `files_modified`/`edits_applied` are **prospective** for `dry_run:true`, otherwise
  actual successful replacements. Byte-identical edits return `noop`, not success.
  Imports' separate JSON preview retains its `unique`/`ambiguous`/`unknown` and
  `preview.old_lines/new_lines` fields (display lines, not a lossless byte encoding).
- All-file preflight failure writes nothing. Content, permissions, canonical paths,
  root/parent/target identities are rechecked before the first commit and immediately
  before each replacement. Detected conflicts fail rather than overwrite.
- Each changed file uses a create-new unique temporary file in its target directory
  and atomic replacement, preserving ordinary permissions. In-root symlinks retain
  the link and update the intended target; outside-root targets are rejected.
  Read-only changed targets fail. Atomic replacement changes inode/file identity;
  other hard links keep their old content.
- There is **no cross-file transaction**: late failures retain earlier successful
  writes and stop subsequent writes. Existing `files_modified` and per-file
  `ok`/`error`/`noop` summary fields remain; unattempted files have `error` with a
  reason. Direct CLI consumers print their report and exit 1 on any edit failure.
- Normal success/error paths clean their own temporary files. If temporary or
  directory identity changes, cleanup is deliberately skipped rather than trusting
  a stale pathname; the error reports a possibly retained temporary file. Cleanup
  permission failures are also reported. Inspect these paths manually, not by
  blindly deleting a similarly named file.

Limits: final check-to-rename/unlink races remain; there is no crash-durability,
ACL/ownership/xattr preservation, compiler-grade rename, or universal filesystem
identity guarantee. Windows uses the existing `same-file` identity implementation
(with its documented file-ID/ReFS limitations); Windows/Linux execution is not
validated by the local macOS tests. A pure-CJK Kotlin rename probe currently fails
in reference discovery before reaching the edit engine; this slice does not change
that grammar/query behavior.

## Removed flat aliases

The flat names below are rejected by the parser: exit 1, stderr
`error: unknown subcommand '<name>'` plus usage, with no JSON error envelope
even if `--json` is given. Internal dead handlers do not make aliases callable.
Use the grouped names below. `docs <query>` remains a supported alias for
`search docs <query>`; `search <query>` remains semantic-search shorthand
(equivalent to `search semantic <query>`). Neither is a removed alias.

| Old | New |
|-----|-----|
| `summarize` | `search summarize` |
| `summary-cache` | `search cache-stats` |
| `imports-of` | `search imports` |
| `annotated` | `search annotated` |
| `find-test` | `search find-test` |
| `expect-actual` | `search expect-actual` |
| `rename` | `edit rename` |
| `batch` / `batch-imports` | `edit batch` / `edit imports` |
| `inject` | `edit inject` |
| `insert` / `insert-*` | `edit insert` |
| `new-file` | `edit new` |
| `organize-imports` | `edit organize` |
| `tokens` / `tree` | `tool tokens` / `tool tree` |
| `inspect` | `tool inspect` |
| `symbol-graph` | `tool graph` |
| `snapshot` | `tool snapshot [--include-libraries] [--limit <n>]` |
| `benchmark` | `tool bench` |
| `doctor` | `tool doctor` |
| `workspace` | `tool workspace` |
| `query` | `tool query` |
| `skills` | `tool skills` |
| `code-action` | `tool code-action` |
| `callers` / `callees` | `call hierarchy --incoming/--outgoing` |
| `call-hierarchy` | `call hierarchy` |
| `implementations` / `subclasses` | `type hierarchy --subtypes` |
| `type-hierarchy` | `type hierarchy` |
| `modules` / `module-deps` | `module list` / `module deps` |
| `android-activities` / `android-composables` | `android activities` / `android composables` |

## Flag applicability

| Flag | Supported use / behavior |
|------|--------------------------|
| `--fast` / `--smart` | `find`, `refs`: rg/fd-only or require a pre-built index; `hover --smart` requires an index and `hover --fast` fails. Not semantic-search modes |
| `--json` | Commands with documented JSON output; not a universal text-to-JSON conversion |
| `--json-envelope` | Semantic search only, requires `--json`; see contract below |
| `--root <dir>` | Selects workspace/index for the query commands below, `complete`, `index`, semantic search, `tool query`, `call hierarchy`; does not universally rebase file operands |
| `--no-stdlib` | Supported index consumers below skip canonical `~/.kotlin-lsp/sources`, retaining configured nonhome external paths; applies to `find`/`refs`/`hover` only when using an index |
| `--relative` / `--absolute` | `find`, `refs`: workspace-relative or absolute paths (relative auto-enabled when piped) |
| `--flat` | `find`, `refs`: grep-style `path:line:col: name` text |
| `--limit <n>` | `find`, `refs`, semantic search, `tool snapshot`: cap result count |
| `--kind class,fun` | `find`, `refs`, semantic search: filter by symbol kind |
| `--module <frag>` | `find`, `refs`: filter by module path |
| `--owner <name>` | `find`, `refs`: filter by enclosing class |
| `--source-set <set>` | `find`, `refs`: filter by source set |

Root/no-stdlib applicability is not inferred from parsing a flag or from a group
manifest entry. See the bounded workspace and file-only contracts below.

### Workspace and file-only options

| Commands | Root policy |
|---|---|
| `module list/deps/files` | `--root` selects the exact project, including settings, module dependencies and files. Without it, discover from Gradle-settings ancestors (even without `.git`), then cwd. File discovery retains `.kt`/`.kts`/`.java` scope. |
| `tool graph/workspace/snapshot`, `android activities` | `--root` selects all project/module/source/manifest metadata, not just the symbol index. Without it, retain existing `.git`/cwd discovery and nested Gradle module discovery. |
| `tool tree`, `android composables`, `format check/apply`, ordinary `check` | File operands stay cwd-relative. Root does not select a different file. `check --diagnose` uses root only for its diagnostic index. |
| File-only `edit` operands | Cwd-relative; existing indexing and containment policies remain unchanged. `edit inject` retains its existing discovery. |
| `capabilities`, `tool skills` | No workspace operation. |
| `extract-sources` | Rejects root; use `--gradle-home`/`--output`. |

For the seven workspace operations, relative roots are cwd-relative. Missing or
file roots fail on stderr before results; an existing empty directory is a valid
empty project and never falls back to cwd or a Gradle ancestor. No flags or file
bases are made universal. `tool workspace` remains a lightweight line-based
overview, not a new tree-sitter symbol extractor. Graph retains its existing
source inclusion; snapshot has the separate inclusion policy below.

### Indexed query options

| Commands | Index/source policy |
|---|---|
| `context`, `impact`, `search find-test`, `tool inspect` | Honor `--root` and `--no-stdlib` |
| `search summarize`, including `--cached`; `search expect-actual` | Honor `--root` and `--no-stdlib` |
| `type hierarchy` | Honors `--root`; keeps its workspace-only default (home sources excluded), even without `--no-stdlib` |
| `find`, `refs`, `hover` | Honor `--root`; indexed modes honor `--no-stdlib`. `--smart` requires a pre-built workspace index. Auto find/refs use fast mode if no index exists; auto hover builds one. Fast find/refs search the workspace, not home libraries; hover rejects fast mode |

For these file-oriented queries, **relative file operands remain cwd-relative**;
`--root B` selects B's index, not B as a file base. Absolute operands stay absolute.
A cwd file outside B's index may yield no indexed symbol (`context`, `impact`,
`search find-test`) or empty symbols (`tool inspect`); hover can index the explicitly
requested file on demand. Neither behavior silently substitutes B's same-named
file. Without `--root`, file queries discover a nearest `.git` from the operand,
then fall back to cwd discovery; name queries discover from cwd. `tool query`
and `call hierarchy` are the documented **explicit root-relative exceptions**.

These queries reject missing/file-as-root explicit roots. Unreadable file operands
and out-of-range file-query positions fail with stderr and no successful results;
positions are 1-based UTF-16. A readable but unindexed file is not an I/O error.
JSON shapes are unchanged; whitespace is command-specific (summaries/context and
several related commands still pretty-print). Semantic search, indexed hover and
batch query retain their documented compact output.

`--no-stdlib` also remains wired for semantic search, `search docs/imports/annotated/
cache-stats`, top-level `docs`, `module packages`, `type sealed`, `call reach/hierarchy`,
`complete`, `index` and `tool query`. It does not remove configured nonhome external
sources. Existing workspace-only defaults in `check --diagnose`, `tool tokens
--resolve`, `tool code-action` and `tool bench` are unchanged. Library inclusion
is not a promise that every query enumerates libraries: refs/impact candidates
and find-test/expect-actual discovery retain workspace-scoped rg searches.
Source-less diagnostics and heuristic/qualified-resolution limitations remain.

## Semantic search output

```bash
kotlin-lsp search "kind:class login" --json --limit 1
kotlin-lsp search semantic "kind:class login" --json --json-envelope --limit 1
```

Shorthand and explicit semantic search have the same contract. `--json` remains
a **compact array** of result objects with unchanged fields: `name`, `kind`,
`file`, `line` (1-based), `signature`, `doc` (nullable), `generated`, `score`.
`--json --json-envelope` opts into the compact object
`{"results":[...],"truncated":true}` with exactly those two keys; `results` is
the same array. No total count is computed. The default limit remains **20**.
`truncated` is true only when eligible matches exceed the limit after filtering:
limit 0 is true iff a match exists, exactly N matches at limit N is false, and
no hits is always false. Limits must be non-negative integers representable on
the platform. The envelope requires `--json` and is rejected before execution
for every other command, including `search docs` and `search summarize`.

Scored queries preserve relevance and defer generated stubs when a same-name
real implementation exists. Filters-only queries retain name/generated order.
Remaining ties use name, file, line, signature, kind, documentation and generated
status, not index traversal order; matching prefix terms accumulate in stable
order. Generated-only matches remain visible. `--root` selects the indexed
workspace; `--no-stdlib` excludes home sources without changing workspace defaults.

## Library sources

```bash
kotlin-lsp extract-sources    # one-time: unpack *-sources.jar from Gradle cache
kotlin-lsp index-jars         # one-time: index extracted library symbols
```

`extract-sources` has no workspace role and **rejects `--root`** before scanning
or writing. Use `--gradle-home <dir>` for the input Gradle cache, `--output <dir>`
for the extraction destination, optional library substring filters, and
`--dry-run` to preview without writing. Defaults remain `$GRADLE_USER_HOME` or
`~/.gradle` for input and `~/.kotlin-lsp/sources` for output.

`tool snapshot` emits workspace symbols and configured nonhome external sources
by default. The extracted
library cache (`~/.kotlin-lsp/sources`) is deliberately excluded — including it
made a one-file project emit 773 MB of JSON (issue #242). Pass
`--include-libraries` to include library symbols (output can be hundreds of MB;
a warning is printed to stderr), and `--limit <n>` to cap the symbol count.
Selected library symbol metadata is materialized on both cold and warm calls.
Home library files do not contribute workspace relationships, even with inclusion.
`--exclude-relationships` omits that field; it does not change symbol selection.
Snapshot does not consume `--no-stdlib`: the tool group flag union is not a
promise of support by snapshot, and no mixed-flag precedence is defined.

## What gets indexed

JDK, Kotlin stdlib, and Android SDK symbols are available without source JARs.

Source files are cached in `~/.cache/kotlin-lsp/`. Cache populated by `kotlin-lsp index`,
refreshed on file changes.
