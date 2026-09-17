---
name: kotlin-lsp
description: Use the `kotlin-lsp` CLI for precise symbol lookup in Kotlin/Java/Swift projects — faster than grep/rg and returns typed answers (declarations, refs, signatures) instead of raw text matches. Saves tokens because results are scoped and structured.
---

# kotlin-lsp

`kotlin-lsp` is a tree-sitter–backed CLI for Kotlin / Java / Swift symbol queries — no daemon, no JVM. It returns *declaration locations* and *type-aware references*, not text matches.

```bash
kotlin-lsp --version   # tool version + tree-sitter grammar versions (kotlin/java/swift)
```

`capabilities --json` includes a `grammars` object with the same three
resolved grammar crate versions, so behavior differences can be attributed to
the grammar build.

## When to use kotlin-lsp vs rg

```
Query is about Kotlin/Java/Swift symbols?
├─ No → rg / Read
└─ Yes:
   ├─ Symbol is unique AND in this repo → rg --type kotlin (faster)
   ├─ Symbol is generic (handle, Event, …) → kotlin-lsp find/refs --module … --limit
   ├─ Symbol lives in library (Compose, AndroidX) → kotlin-lsp find
   ├─ Cross-module ref filtering needed → kotlin-lsp refs --module / --owner
   ├─ One-stop info (def + sig + doc) → kotlin-lsp context
   ├─ Syntax check → kotlin-lsp check
   ├─ Format check → kotlin-lsp format check
   ├─ Direct callers/callees → kotlin-lsp call hierarchy
   ├─ Implementation tree → kotlin-lsp type hierarchy
   ├─ Composable analysis → kotlin-lsp android composables <file> --call-graph/--state/--preview
   ├─ Batch queries → echo '[...]' | kotlin-lsp tool query --json
   ├─ Import analysis → kotlin-lsp search imports
   ├─ Annotation query → kotlin-lsp search annotated
   ├─ Signature search → kotlin-lsp search docs
   ├─ Semantic search → kotlin-lsp search semantic "login repo"
   ├─ Cached summaries → kotlin-lsp search summarize <name> --cached | search cache-stats
   └─ Full project snapshot → kotlin-lsp tool snapshot / tool graph
```

## How it saves tokens

| Naive | Better with kotlin-lsp |
|---|---|
| `rg 'class MyViewModel'` returns every text match including doc comments | `kotlin-lsp find MyViewModel --limit 5` returns only declaration sites |
| `rg 'MyViewModel'` to find usages, then manually filter | `kotlin-lsp refs MyViewModel --limit 20` returns real references |
| Open file, read 200 lines to figure out return type | `kotlin-lsp hover Foo.kt 42 10` returns just the signature |

Output defaults:
- Text mode groups by file with structural annotation (path, module, sourceSet)
- `--json` for structured data
- `--relative` auto-enabled when stdout is piped (agent context)

## Quick reference — most-used commands

```bash
# Find declarations / references
kotlin-lsp find <Name> [--limit N] [--kind class,fun]
kotlin-lsp refs <Name> [--limit N]
kotlin-lsp refs <Name> explain [--limit N]  # label each hit with its reference kind (type-use, call, read, write, …)
kotlin-lsp hover <file> <line> <col>
kotlin-lsp context <file> <line> <col>

# Search group (symbol discovery)
kotlin-lsp search semantic "login repo" [--limit N]
kotlin-lsp search "login repo" --json --json-envelope --limit 1  # same semantic query
# --json alone: compact array, default limit 20. Envelope: {results,truncated}.
# truncated means eligible matches exceed the limit; at 0, true iff any match.
# Envelope requires --json and is rejected by all non-semantic commands.
kotlin-lsp search semantic "kind:class name:ViewModel login" [--kind class]  # field filters + flag
# Field filters: kind: (class|fun|method|...), lang:/language: (kotlin|java|swift),
# path:/name: (case-insensitive substring); unknown prefixes are plain text;
# filters-only queries ("kind:method path:src/api") list all matches by name.
# Generated stubs (protobuf/kapt/mock, or a "Code generated ... DO NOT EDIT"
# header) rank below same-name real implementations and print "(generated)".
kotlin-lsp search summarize <name> --cached
kotlin-lsp search docs <query>
kotlin-lsp search cache-stats
kotlin-lsp search imports <name>
kotlin-lsp search annotated <annotation>
kotlin-lsp search find-test <file> <line> <col>
kotlin-lsp search expect-actual <name>

# Call graph
kotlin-lsp call hierarchy <file> <line> <col>
kotlin-lsp call hierarchy <name> [--incoming] [--outgoing] --root <project> --no-stdlib
# Exact function/method name or unique Class.method (nearest enclosing type).
# Declarations, including override methods, and call identifiers use 1-based
# UTF-16 positions. Relative files use explicit --root, otherwise cwd.
# Neither/both direction flags = both; one flag = that direction. One hop only;
# depth is rejected (use call reach). Missing/invalid/ambiguous queries exit 1.
# JSON keeps name/incoming/outgoing; both arrays are sorted unique string graph
# keys, not locations or call-site counts. Incoming no longer contains rg snippets.
# Outgoing can include unresolved/external keys. Ambiguity lists candidates;
# use a declaration position, not a guessed first match. Same-file overload bodies
# cannot be separated. Grammar/name-based, not package/overload/source-set binding.
# Full lookup/output boundaries: docs/commands.md → Direct call hierarchy.
kotlin-lsp call diff [<ref1> [<ref2>]] [<name>]  # call-tree diff (git-diff style; inferred entries when name omitted)
kotlin-lsp call reach <entry> [--to <target>]  # every call path entry→target
# Type hierarchy
kotlin-lsp type hierarchy <Name> [--subtypes|--supertypes]
kotlin-lsp type sealed <Name>

# Edit group (code modification)
kotlin-lsp edit organize <file>...
kotlin-lsp edit imports <file> [--apply]
kotlin-lsp edit rename <file> <line> <col> <newName>
kotlin-lsp edit inject <file>
kotlin-lsp edit new <template> <Name>

# Tool group (debug / introspection)
kotlin-lsp tool bench
kotlin-lsp tool doctor [--json]
kotlin-lsp tool inspect <file>
kotlin-lsp tool snapshot [--include-libraries] [--limit <n>]
kotlin-lsp tool graph --json
kotlin-lsp tool code-action <file> <line> <col>

# Syntax / format
kotlin-lsp check <file>...
kotlin-lsp format check <file/dir>...
kotlin-lsp format apply <file/dir>...

# Android / Compose
kotlin-lsp android composables <file> --call-graph/--state/--preview
```
### Applying shared-engine edits

For `edit rename --apply`, `edit imports --apply`, and `tool code-action --apply`,
inspect the report and exit status: preflight errors write nothing; late errors
can leave accurately reported partial success. Rename without `--apply` and
imports with `--json` are previews; dry-run summary counts are prospective, not
writes. Cursor arguments are 1-based UTF-16; generated `TextEdit` ranges are
0-based UTF-16. Replacements preserve untouched bytes and use `new_text` verbatim;
equal-position inserts keep request order. Per-file replacement is atomic, not a
batch transaction. Identity conflicts can deliberately leave a temporary file
rather than delete through an untrusted path. Full guarantees, root applicability,
newline/permission behavior and remaining races: `docs/commands.md` →
**Shared-engine edit safety**. Other edit/format routes do not inherit these guarantees.

## Installation and command compatibility

Install/update from GitHub Release prebuilt assets; see `README.md` →
**Install / update** for asset names and older installer caveats. Local Cargo
builds are development/test binaries, not machine installations.
`docs <query>` is a live alias for `search docs <query>`; `search <query>` is
semantic-search shorthand. Removed flat names (including `benchmark`, `inject`,
`call-hierarchy`, `query`, `snapshot`) fail with exit 1 and an unknown-subcommand
message on stderr, not a JSON envelope. Use the grouped commands below; dead
internal handlers are not available aliases.

For call-graph package/overload/source-set collisions, consult
`docs/codebase/GRAPH_IDENTITY.md`. Hierarchy refuses ambiguous names/same-file
outgoing overloads; reach may merge bodies and snapshot relationship pairs lose
file identity. These are not compiler-resolved paths.

## All commands

| Need | Command |
|------|---------|
| Find definition | `kotlin-lsp find <name>` |
| Find references | `kotlin-lsp refs <name>` |
| Reference kinds | `kotlin-lsp refs <name> explain` |
| Hover / signature | `kotlin-lsp hover <file> <line> <col>` |
| Completions | `kotlin-lsp complete <file> <line> [col]` |
| One-stop context | `kotlin-lsp context <file> <line> <col>` |
| Semantic search | `kotlin-lsp search "query"` |
| KDoc search | `kotlin-lsp search docs "query"` |
| KDoc search (alias) | `kotlin-lsp docs <query>` |
| Syntax check | `kotlin-lsp check <file>...` — exits 1 on syntax errors or missing/unreadable inputs; source-less dirs → `empty_dirs`, not an error unless `--diagnose` has no checkable file (exit 1, JSON `errors`) |
| Format check | `kotlin-lsp format check <file/dir>...` |
| Format apply | `kotlin-lsp format apply <file/dir>...` |
| Code actions | `kotlin-lsp tool code-action <file> <line> <col>` |
| Organize imports | `kotlin-lsp edit organize <file>...` |
| Batch imports | `kotlin-lsp edit imports <file>` |
| Rename | `kotlin-lsp edit rename <file> <line> <col> <new>` |
| Batch edit | `kotlin-lsp edit batch <rule-json>` |
| Insert snippet | `kotlin-lsp edit insert <file> <line> --after --content <text>` |
| Inject types | `kotlin-lsp edit inject <file>` |
| Call hierarchy | `kotlin-lsp call hierarchy <file> <line> <col>` |
| Call hierarchy (by name) | `kotlin-lsp call hierarchy <name>` |
| Call-tree diff | `kotlin-lsp call diff [<ref1> [<ref2>]] [<name>]` — branch-aware call-tree diff between git refs (HEAD vs worktree by default) |
| Call reach | `kotlin-lsp call reach <entry> [--to <target>]` — all call paths from an entrypoint |
| Impact analysis | `kotlin-lsp impact <file> <line> <col>` |
| Type hierarchy | `kotlin-lsp type hierarchy <name> [--subtypes\|--supertypes]` |
| Module list | `kotlin-lsp module list` |
| Module deps | `kotlin-lsp module deps <name>` |
| Module files | `kotlin-lsp module files <name>` |
| Module packages | `kotlin-lsp module packages [name]` |
| Android activities | `kotlin-lsp android activities` |
| Android composables | `kotlin-lsp android composables <file> [--call-graph] [--state] [--preview]` |
| Import analysis | `kotlin-lsp search imports <name>` |
| Annotation query | `kotlin-lsp search annotated <name>` |
| Symbol summary | `kotlin-lsp search summarize <name>` |
| Summary cache | `kotlin-lsp search cache-stats` |
| Find tests | `kotlin-lsp search find-test <file> <line> <col>` |
| KMP expect/actual | `kotlin-lsp search expect-actual <name>` |
| Index workspace | `kotlin-lsp index [--root <dir>] [--gradle] [--lang kotlin\|java\|swift]` — `--lang` builds a per-language cache (`index-<lang>.bin`), handy when only Kotlin (or Swift) matters |
| Index JARs | `kotlin-lsp index-jars [root]` |
| Gradle deps | `kotlin-lsp gradle-deps` |
| Extract sources | `kotlin-lsp extract-sources [lib...] [--gradle-home <dir>] [--output <dir>] [--dry-run]` |
| Source roots | `kotlin-lsp sources` |
| Cache stats | `kotlin-lsp cache stats` |
| Doctor | `kotlin-lsp tool doctor [--json]` |
| CLI capabilities | `kotlin-lsp capabilities [--json]` |
| Workspace overview | `kotlin-lsp tool workspace` |
| Snapshot | `kotlin-lsp tool snapshot` (workspace symbols; add `--include-libraries` for the ~/.kotlin-lsp/sources library cache, `--limit <n>` to cap) |
| Symbol graph | `kotlin-lsp tool graph` |
| Batch query | `echo '[{"type":"definition","name":"MyViewModel"}]' \| kotlin-lsp tool query --json --root ./project --no-stdlib` |
| File inspect | `kotlin-lsp tool inspect <file>` |
| Tokens (debug) | `kotlin-lsp tool tokens <file>` |
| Parse tree (debug) | `kotlin-lsp tool tree <file>` |
| New file | `kotlin-lsp edit new <template> <name>` |
| Benchmark | `kotlin-lsp tool bench` |
| Agent skills | `kotlin-lsp tool skills list \| read <name>` |

Full command reference → references/commands.md

## Batch queries

Use `tool query --json --root <project> [--no-stdlib]` to share one index across
a stdin JSON array. Consume the compact result array in request order; preserve
successful items even if the process exits 1 for another item's `error`.

- `definition`, `references`, `summarize`, `implementations`, `subclasses`: supply `name`.
- `hover`, `callers`: supply `file`, `line`, `col` (1-based **UTF-16**). Relative files
  use explicit `--root`, otherwise cwd. Unavailable/ambiguous hover signatures are null.
- `references.refKind`: `call|read|write|override|import|type-use|declaration`.
  Omitted/`all`/`reference` follows normal smart refs (includes declarations,
  excludes imports/packages and comments/string text). Use `call` for invocations,
  `import` for imports. Interpolation expressions remain references; Swift property
  bindings are declarations and assignment targets are writes. Invalid filters are
  errors, not ignored flags.
- `callers.depth`: omit or use `1`; other depths error. Results use name-based
  call edges (max 20), not overload-resolved identities. Subtype results cap at 50.

`--no-stdlib` excludes `~/.kotlin-lsp/sources`; `--root` selects the indexed
workspace even when cwd differs. See the project's `docs/commands.md` for item fields.

## Flag scope

Use command-specific contracts, not parser acceptance: capability group flags
are unions, not a promise for each member. Semantic search supports `--root`,
`--no-stdlib`, `--kind`, `--limit` and the opt-in JSON envelope. Its array fields
and scoring stay unchanged; generated-only matches remain searchable. Remaining
ties and prefix scoring are deterministic across processes/cache states.
For `context`, `impact`, `search summarize` (also `--cached`), `search find-test`,
`search expect-actual` and `tool inspect`, use `--root` to select the index and
`--no-stdlib` to skip canonical home sources. Configured nonhome external paths
remain included. For `find`/`refs`/`hover`, `--no-stdlib` applies to indexed modes
only: `--smart` requires a pre-built index; auto find/refs fall back to fast
workspace search without one. Hover auto-builds an index and rejects `--fast`.
`type hierarchy --root` keeps its workspace-only default (home sources excluded).
So do `check --diagnose`, `tool tokens --resolve`, `tool code-action` and `tool bench`.
Library inclusion does not expand workspace-scoped refs/impact/find-test/expect-actual
candidate discovery. JSON whitespace is command-specific, not globally compact.

**File base:** these queries keep relative operands cwd-relative even with an
alternate `--root`; absolute operands remain absolute. A cwd file outside the
selected index may have no indexed symbol; hover can index the requested file
on demand. Neither silently rebases to a same-named file under the root. Missing
roots, unreadable files and invalid positions fail rather than return successful
results. Without `--root`, nearest `.git`/cwd discovery is retained. Only `tool query`
and `call hierarchy` use their documented explicit root-relative operand base.

`extract-sources` rejects `--root`: it scans a Gradle cache, not a workspace.
Use `--gradle-home` / `--output` to select input/destination, and `--dry-run` to
preview.

**Workspace selection:** `module list/deps/files`, `tool graph/workspace/snapshot`
and `android activities` honor `--root` throughout modules, sources and project
metadata. Relative roots use cwd. Missing/file roots fail; empty directories are
valid empty projects. Without root, module commands retain Gradle-settings
ancestor discovery; tools/activities keep existing `.git`/cwd discovery and
nested Gradle module discovery. Module file discovery stays Kotlin/Java-only.
Snapshot defaults to workspace plus configured nonhome external sources, excluding
home sources. `--include-libraries` includes home symbol metadata cold and warm;
`--exclude-relationships` omits relationships. Home files never contribute workspace
relationships, even with inclusion. Snapshot does not use `--no-stdlib`.

**File-only/utility boundaries:** `tool tree`, `android composables`, format,
ordinary check and file-only edits keep cwd-relative operands. Root only affects
already-defined indexing/containment (for example check `--diagnose`); existing
`edit inject` policy is unchanged. Capabilities and `tool skills` have no workspace
operation. Group flags are unions, not every-member promises. See
`docs/commands.md` → Workspace and file-only options / Indexed query options.

Indexed `hover --smart` resolves declarations and uniquely named unqualified
identifier references. Qualified/ambiguous reference fallbacks remain unresolved
(exit 1), rather than guessing a declaration. Use 1-based UTF-16 positions;
file operands remain cwd-relative. See `docs/commands.md` → Indexed hover.

## Performance modes

Indexing and library sources → references/indexing.md

## Anti-patterns

- **Don't** `rg 'class FooBar'` when `kotlin-lsp find FooBar` will do.
- **Don't** read the entire file for a signature; use `hover`.
- **Don't** omit `--limit` on `refs` for common names like `String` or `Result`.
- **Don't** invoke `kotlin-lsp` recursively inside an LSP context.
