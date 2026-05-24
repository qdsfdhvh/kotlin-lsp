# Rust Development Guidelines for kotlin-lsp

> Extracted from actionbook/rust-skills, tailored for this project.

## unwrap() → expect()

Never bare `unwrap()` in production. Use `expect("reason")`:

```rust
// BAD
let root = self.indexer.workspace_root.read().unwrap().clone();

// GOOD
let root = self.indexer.workspace_root.read().expect("workspace_root poisoned").clone();
```

## Prefer generics over Box<dyn Trait>

```rust
// GOOD
fn infer<D: InferDeps>(deps: &D, pos: CursorPos) -> Option<String>

// AVOID
fn infer(deps: &dyn InferDeps, pos: CursorPos) -> Option<String>
```

## No clone() without justification

Clone signals an ownership design issue. Use references or restructure data.

## Newtypes for semantic safety

Adjacent same-type params should be wrapped in a named struct:

```rust
// BAD: swappable
fn resolve(line: usize, col: usize)

// GOOD
fn resolve(pos: CursorPos)
```

## Rule of Three

Don't introduce a generic/trait until ≥2 distinct implementations exist.

## Vec::with_capacity() when size known

```rust
let mut items = Vec::with_capacity(known_size);
```

## Iterator > index loop

```rust
// GOOD
items.iter().filter(|i| i.active).map(|i| &i.name).collect()

// AVOID
for i in 0..items.len() { if items[i].active { ... } }
```

## Error handling decision tree

```
Is failure expected?
├─ Yes → Result<T, E>
│        ├─ Library code → thiserror
│        └─ Application code → anyhow
└─ No → Is it a bug?
         ├─ Yes → panic! / assert!
         └─ No → Option<T>
```

## Pre-commit

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features --no-fail-fast
```
