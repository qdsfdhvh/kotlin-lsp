# Indexing & Library Sources

## Source set detection

- **KMP source sets** are detected structurally — anything under `src/<name>/{kotlin,java}` counts. Custom names like `jvmCommonMain` work automatically.
- **Android SDK sources** are auto-detected from `local.properties` → `$ANDROID_HOME` → `$ANDROID_SDK_ROOT`.

## Library sources

Gradle library sources (Compose, coroutines, AndroidX):

```bash
kotlin-lsp extract-sources
```

Run once after cloning or adding dependencies. Subsequent `find`, `refs`, and `complete` pick them up.

## Pre-built index

For faster first-lookup, pre-build the index:

```bash
kotlin-lsp index --root ./android
```

Without this, the first command in a session pays the indexing cost.

## Cache diagnostics

```bash
kotlin-lsp cache stats
```

Shows cache size, hit rate, and staleness.

## Performance modes

| Mode | Behavior |
|------|----------|
| _(default)_ | Auto — use cached index if available, else fast `rg`/`fd` fallback |
| `--fast` | Always use `rg`/`fd`; instant, no index needed |
| `--smart` | Require a pre-built index; run `kotlin-lsp index` first |
| `--root <dir>` | Override workspace root (default: nearest `.git` directory) |
| `--no-stdlib` | For `complete`: skip library sources for faster workspace-only suggestions |
