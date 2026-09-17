# Call graph identity boundary

Observed on macOS, debug v0.32.3 working tree, 2026-09-13. This is a boundary
report, **not compiler-binding support** and not a graph-schema migration.
`tests/graph_identity_tests.rs` guards the already-defensive hierarchy behavior
and unique-name reach/export controls. It does not enshrine the false paths below
as correct results. [TESTING.md](TESTING.md#p6-baseline-and-identity-gate) maps the gate.

## Minimal public CLI repro

Create an otherwise empty workspace with these two files:

```kotlin
// a/A.kt
package alpha
fun shared() { alphaLeaf() }
fun alphaLeaf() {}
```

```kotlin
// b/B.kt
package beta
fun shared() { betaLeaf() }
fun betaLeaf() {}
```

Run the development binary in place, with `ROOT` set to that workspace and a
separate temporary HOME, USERPROFILE, XDG_CACHE_HOME, XDG_CONFIG_HOME,
XDG_DATA_HOME and cwd. No home library sources are needed.

```bash
kotlin-lsp check "$ROOT" --json
kotlin-lsp call hierarchy shared --root "$ROOT" --no-stdlib --json
kotlin-lsp call hierarchy a/A.kt 2 5 --outgoing --root "$ROOT" --no-stdlib --json
kotlin-lsp call reach shared --root "$ROOT" --no-stdlib --json
kotlin-lsp tool graph --root "$ROOT" --json
kotlin-lsp tool snapshot --root "$ROOT" --json
```

Expected distinction: `alpha.shared` and `beta.shared` have separate bodies;
a name-only query should disambiguate or refuse, not imply one callable owns
both bodies. Observed hierarchy: name query exits 1 with two file/line/column
candidates; declaration outgoing exits 0 with only `["alphaLeaf"]`. Observed
reach: exits 0 with both `shared → alphaLeaf` and `shared → betaLeaf`, assigning
one shared declaration location to both paths. **That attribution is not reliable.**

## Cold/warm boundary matrix

Each observed command used its own fresh workspace (cache absent), then the same
command again with its persisted `.cache/kotlin-lsp/index.bin`. Each ambiguous
fixture also had `Unique.kt`: `fun uniqueLeaf() {}` followed on line 2 by
`fun uniqueEntry() { uniqueLeaf() }`. The two unique control commands were
`call hierarchy uniqueEntry` and `call reach uniqueEntry --to uniqueLeaf`, with
`--json --root "$ROOT" --no-stdlib`; both exit 0 and show only the single real edge.
92 invocations across seven fixtures were recorded, including exports and these
positive controls. Parsing/defensive CI tests additionally require nonzero valid
file counts. Full sources and exact positions are in `tests/graph_identity_tests.rs`.

| Distinction | Hierarchy name / declaration outgoing | Reach observation (exit 0, cold and warm) | Raw graph / snapshot observation (exit 0) |
|---|---|---|---|
| Two packages, top-level `shared` | exit 1, two candidates / exit 0, only alpha body | merges alpha and beta bodies | graph retains caller file, bare `shared` key; snapshot symbols have different FQNs but relationship pairs lose file identity |
| Two packages, both `Worker.shared` | exit 1 / exit 0, only alpha body | `call reach Worker.shared` merges both bodies; entry file is empty, line 0 | caller key is `Worker.shared`, not package-qualified; symbol `parent` retains package |
| Same-file `shared(Int)` and `shared(String)` on separate lines | exit 1 / exit 1, refuses merged outgoing even at declaration | merges int/string bodies under `shared` | caller file is the same and edges lack declaration range/signature; snapshot symbol signatures are distinct |
| Different-file overloads | exit 1 / exit 0, only selected file's int body | merges both bodies and reuses one declaration location | graph file distinguishes callers; snapshot relationship pairs do not |
| `src/commonMain/kotlin/Same.kt` and `src/jvmMain/kotlin/Same.kt`, same `demo.shared` | exit 1 / exit 0, only common body | merges common/jvm bodies, without source-set selection | file paths distinguish declarations; FQN and relationship names alone do not |
| Both overloads on one line, separated by `;` | exit 1 with columns 5/39 / exit 1 | merges both bodies | **known producer bug:** snapshot emits `fun shared(x: Int)` and Int parameters for both declarations, losing the String signature |
| Unique `uniqueEntry → uniqueLeaf` only | exit 0, exact edge | exact two-node path with correct file URI/lines 2 and 1 | two symbols, one call edge; no cwd decoy |

The source-set probe indexes two plain functions in separate source directories;
it does not claim Gradle source-set selection or compiler expect/actual binding.
Cold/warm ambiguity behavior and path-name sets agree, **not byte-for-byte JSON**:
array ordering can vary, and the wrongly selected shared entry file changed
between cold/warm for packages and source sets. Warm cache does not repair identity.

## Why this boundary exists / recommended next design

`Indexer.call_edges` retains a callee string and `(caller_file, caller_name)`.
Hierarchy combines a declaration catalog with that data: it can filter outgoing
by caller file, and refuses same-file overloads because edges have no declaration
range. Reach builds name-keyed adjacency, with short `Class.method` keys, and
location lookup cannot reliably recover a unique declaration afterward.
`tool graph` exports the available caller file; snapshot symbols carry richer
metadata, but snapshot relationship pairs contain only names. FQN alone is not
an overload/source-set identity, and the existing signature producer is not safe
for same-line overloads.

A future separately approved change should first give call-edge producers a
source identity plus declaration range (including column), preserve it through
cache/adjacency/export, and explicitly represent unresolved/ambiguous callees.
Package, containing type and signature metadata can assist candidate selection;
source-set selection needs its own explicit policy. Fix the same-line producer
before trusting signature-based IDs. Stage resolver/cache/schema compatibility
and any JSON opt-in/versioning deliberately; do not infer identity by taking the
first name match or silently add package prefixes to one consumer only. This P6
change leaves production graph data and JSON unchanged.

Separate limits remain: the P5 pure-CJK rename discovery probe fails before the
edit engine, and action probes cover only the documented shared-engine paths.
Graph evidence does not expand those edit/parser guarantees.
