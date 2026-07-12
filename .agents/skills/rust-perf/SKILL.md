# Rust Performance Optimization

> Curated from [actionbook/rust-skills/m10-performance](https://skills.sh/actionbook/rust-skills/m10-performance)
> and [mohitmishra786/low-level-dev-skills/rust-profiling](https://www.skills.sh/mohitmishra786/low-level-dev-skills/rust-profiling).
> Tailored for kotlin-lsp: CLI-first, cold-start-critical, large JAR symbol index.

## Triggers

- "optimize performance", "make it faster", "reduce startup time"
- "profile", "flamegraph", "benchmark"
- "binary too large", "reduce binary size"

## Optimization Priority

```
1. Algorithm choice      (10x – 1000x)   ← symbol index, lazy loading
2. Data structure        (2x – 10x)      ← Vec over HashMap (small N)
3. Allocation reduction  (2x – 5x)       ← with_capacity, avoid clone
4. Cache optimization    (1.5x – 3x)     ← contiguous layout, SmallVec
5. SIMD / Parallelism    (2x – 8x)       ← rayon for batch parsing
```

## Before Optimizing

**Measure first. Never guess.**

```bash
# Profile cold start (kotlin-lsp specific)
cargo build --profile profiling
samply record ./target/profiling/kotlin-lsp benchmark

# Binary size analysis
cargo bloat --release --crates
ls -lh target/release/kotlin-lsp

# Micro-benchmark
cargo bench
```

## Common Techniques

| Technique | When | How | Applied in kotlin-lsp |
|-----------|------|-----|----------------------|
| Pre-allocation | Known size | `Vec::with_capacity(n)` | Symbol index HashMap |
| Avoid clone | Hot paths | References, `Cow<T>`, `Arc<String>` | `sort_by` borrow instead of `sort_by_key` clone |
| Lazy loading | Expensive data | Load on first access | `lazy_load_library_file()` |
| Index pre-compute | Repeated lookups | Compact serialization | Symbol index (2.8MB zstd) |
| Batch operations | Many small ops | Collect then process | Library batch restore |
| SmallVec | Usually < 8 items | `smallvec::SmallVec<[T; 4]>` | Not yet applied |
| Zero-copy deser | Large caches | `rkyv`, `capnp`, mmap | Future: library cache |

## Binary Size

```toml
[profile.release]
opt-level = "s"     # "z" for max size, "3" for max speed
lto = "fat"         # full LTO (slower build, smaller binary)
codegen-units = 1   # better optimization
strip = true        # remove debug symbols
panic = "abort"     # no unwinding tables
```

**Current kotlin-lsp settings:** `opt-level=3, lto=fat, strip=true, panic=abort` → 14.8MB

Trade-off table:

| opt-level | Binary | Cold start | Best for |
|-----------|--------|-----------|----------|
| `3` | ~15MB | 1.0s | CLI tools (current) |
| `s` | ~13MB | 1.5s | Space-constrained |
| `z` | ~12MB | 6.0s | ❌ Never for CLI |

## Profiling Workflow

### 1. Build for profiling
```bash
cargo build --profile profiling  # inherits release, debug=2, strip=false, lto=false
```

### 2. Record with samply (macOS)
```bash
samply record ./target/profiling/kotlin-lsp benchmark
# Opens in Firefox Profiler
```

### 3. Generate flamegraph (Linux)
```bash
cargo install flamegraph
cargo flamegraph --bin kotlin-lsp -- benchmark
```

### 4. Binary bloat analysis
```bash
cargo install cargo-bloat
cargo bloat --release --crates    # by crate
cargo bloat --release -n 20       # top 20 functions
```

## kotlin-lsp Specifics

### Cold start bottleneck

The library cache (53MB zstd → 344MB deserialized, 50K+ JAR files) is the dominant factor.
**Solution applied:** Compact symbol index (2.8MB zstd) for definitions, lazy FileData loading.

### Hot paths

| Path | Frequency | Optimization |
|------|-----------|-------------|
| Completion sort/dedup | Per keystroke | Avoided `label.clone()` (2 allocs → 0) |
| `definition_locations` | Per lookup | DashMap O(1), symbol index pre-load |
| `get_file()` | Per library lookup | Transparent lazy loading |
| Library cache load | Per CLI invocation | Symbol index skip (0.5s saved) |

### Future optimizations (deferred)

- **SmallVec** for `CompactLoc` vecs (most symbols have 1-2 definitions)
- **String interning** for symbol names (billions of repeats across 50K files)
- **Per-package cache sharding** — only load cache for referenced packages
- **mmap zero-copy deserialization** via `rkyv` for library cache
- **Parallel completion** — rayon for multi-file symbol search
