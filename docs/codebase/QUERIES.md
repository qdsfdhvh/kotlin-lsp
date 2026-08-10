# Query & Extraction Contract — per-language symbol analysis

> **Who this is for**: anyone adding a new language or extending an existing
> one. Violating this contract compiles clean and silently produces wrong
> symbols (missing kinds, wrong owner attribution, false `check` errors).
> Modeled on calldiff's `languages/CONTRACT.md`, adapted to kotlin-lsp's
> query/parse architecture.

## 1. Three languages, three strategies

| Language | Strategy | Implementation |
|---|---|---|
| Kotlin | S-expression `Query` over `tree-sitter-kotlin-sg` | `KOTLIN_DEFINITIONS` (`src/queries.rs:26`), pattern-index → `SymbolKind` via `def_pattern_meta()` (`queries.rs:234`) |
| Swift | S-expression `Query` with field names | `SWIFT_DEFINITIONS` (`queries.rs:279`), `swift_def_pattern_meta()` |
| Java | Hand-written CST walk (no `Query`) | `extract_java()` (`src/parser.rs:1898`) + `KIND_*` constants |

Each strategy must produce `SymbolEntry` values with: `name`, `kind`
(`SymbolKind`), `visibility`, `range`, `selection_range`, `detail`, and the
optional metadata (`type_params`, `deprecated`, `documentation`, …).

## 2. Mandatory coverage per language

A language extractor must handle, in order of importance:

1. **Free functions** and **type/class methods** (distinct `SymbolKind`:
   `FUNCTION` vs `METHOD`; Java: `METHOD`).
2. **Type declarations**: class, interface, enum (+ enum members/entries),
   object, record/struct, annotation, typealias — each a distinct `SymbolKind`
   where the grammar distinguishes them.
3. **Constructors** (`KIND_CTOR_DECL` / Swift `init_declaration`).
4. **Properties / fields** (`val`/`var`, `KIND_FIELD_DECL`), including
   destructuring and primary-constructor parameters (Kotlin patterns 14–19).
5. **Imports / packages** (package + import edges for module queries).
6. **Call edges** (`extract_call_edges` in `parser.rs`): caller→callee pairs
   from `call_expression`/`navigation_expression`, used by `call hierarchy`,
   `call reach`, `call diff`, `impact`, and snapshot.

## 3. `KIND_*` constants — never hardcode node strings

All node-kind strings live in `src/queries.rs` (`KIND_*` constants, ~101 of
them). Project rule 2: **no hardcoded node kind strings** in parser/indexer
code. Reasons: (a) grammar upgrades rename nodes — one edit point; (b) the
constants double as documentation of which grammar nodes the tool depends on.
When adding a language, add its node kinds as `KIND_*` constants even if the
language uses a Query (queries may still need them for `extract_call_edges`
style walking).

## 4. Definition-query contract (Kotlin & Swift)

`KOTLIN_DEFINITIONS` / `SWIFT_DEFINITIONS` are single combined queries with
**ordered patterns**. Rules:

- **Every pattern emits exactly two captures**: `@def` (full declaration node
  → `SymbolEntry::range`) and `@name` (identifier node →
  `SymbolEntry::selection_range` + symbol text). The test
  `parser::tests` relies on this invariant.
- **Pattern order is significant** — later patterns only match what earlier
  ones did not (tree-sitter `Query` semantics: first pattern wins per node).
  Ordering notes embedded in `queries.rs` as comments:
  - `enum class` MUST precede plain `class` (both parse as
    `class_declaration`; `enum_class_body` disambiguates).
  - `data class` MUST precede plain `class` (subset match).
  - `operator fun` MUST precede plain `fun` (top-level and method variants).
- **Pattern index → `SymbolKind`** mapping lives in `def_pattern_meta()`
  (`Kotlin`) / `swift_def_pattern_meta()` (`Swift`). Add a row for every new
  pattern; keep indices stable once published (cache invalidation keys off
  `CACHE_VERSION`, but the mapping must stay exhaustive).
- Swift queries use field names (`name: (type_identifier)`) where available;
  Kotlin's grammar has **no field names** (`child_by_field_name` always
  returns `None` — verified in `queries.rs` header comments), so Kotlin
  patterns match structurally.

## 5. Java CST-walk contract

`extract_java()` matches `node.kind()` against `KIND_*` constants in a
`match`. Rules:

- Every declaration kind is handled via `push_named` (name + selection from
  `first_identifier`) or a dedicated pusher (`push_field_declaration`,
  `push_java_import`).
- `extract_supers_java()` runs in the same walk for supertype edges
  (inheritance).
- Java has no extension receivers — `extension_receiver` stays empty.
- Keep the walk iterating the whole tree (`queue` in `parse_java`); do not
  switch to a Query for Java without re-checking capture semantics.

## 6. Call-edge extraction contract

`extract_call_edges(source, lang)` (`parser.rs:2250`):

- Finds each `call_expression` (and language equivalent — Kotlin
  `navigation_expression`, Swift member calls) and the **enclosing function
  declaration**, emitting `(caller_name, callee_name)`.
- **Nested lambdas / anonymous functions do NOT attribute calls to the outer
  caller** — a call inside a lambda belongs to the lambda's own (anonymous)
  scope or is skipped, mirroring calldiff's rule. Verify with the `fp_*`
  regression tests before changing.
- Dynamic/computed callees (e.g. `map[fn]()`, reflection) are ignored when
  obvious — this is a syntactic index, not a typechecker.

## 7. Adding a new language — checklist

1. Add grammar crate to `Cargo.toml` + `build.rs` version list (see
   `build.rs` `GRAMMAR_CRATES`).
2. Add `KIND_*` constants for the new grammar's nodes.
3. Implement definitions (Query or CST walk) + `def_pattern_meta` mapping.
4. Wire `extract_call_edges` for the language (`crate::Language`).
5. Add a `*_tests.rs` with at least: one declaration-kind test per mandatory
   category (§2), one call-edge test, one nesting/lambda attribution test.
6. Bump `CACHE_VERSION` (`src/indexer/cache.rs`) — parser output shape changed.
7. Update `docs/commands.md` / `skills/kotlin-lsp/SKILL.md` if CLI
   language-surface text mentions supported languages.
