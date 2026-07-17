# Comparison with Official Kotlin Language Server

This document compares [qdsfdhvh/kotlin-lsp](https://github.com/qdsfdhvh/kotlin-lsp) (this project)
with the [official Kotlin Language Server](https://github.com/Kotlin/kotlin-lsp) maintained by
JetBrains.

## Quick Summary

| Dimension | This project | Official |
|-----------|-------------|----------|
| **Language** | Rust (no JVM) | Kotlin (JVM-based) |
| **Parser** | tree-sitter (CST) | IntelliJ IDEA Kotlin Plugin (full compiler frontend) |
| **Startup** | ~milliseconds | ~seconds (JVM warmup) |
| **Memory** | Low (~tens of MB) | High (~hundreds of MB to GB) |
| **Interface** | **CLI** (40+ commands) | **LSP** (editor integration) |
| **Type checking** | No (syntax-level only) | Yes (full Kotlin compiler) |
| **Multi-language** | Kotlin, Java, Swift | Kotlin only |
| **Agent/tooling** | **Rich** (skills, structured JSON, batch queries) | Minimal |
| **License** | MIT | Partially closed-source |

## Philosophy

**This project** is a **symbol engine** for AI agents and CLI tooling — no JVM,
no daemon, instant startup. The CLI is the primary surface.

**Official** is an **LSP server** built on IntelliJ IDEA for full IDE features.

> **This project**: fast symbol lookup for agents, CI, headless workflows.
> **Official**: full IntelliJ-grade completion and type checking in an editor.

## Feature coverage

### Unique to this project

| Feature | Why it matters |
|---------|---------------|
| **40+ CLI commands** | `find`, `refs`, `hover`, `complete`, `search`, `check`, `context`, `call hierarchy`, `type hierarchy`, `summarize`, `organize-imports`, `inject`, `code-action`, `batch-imports`, `skills`, `benchmark`, and more |
| **Agent skills** | Bundled `SKILL.md` for AI coding agents |
| **Structured output** | `--json`, `--flat`, `--relative` for scripts |
| **Swift support** | Navigate Swift alongside Kotlin/Java |
| **Cross-file type inference** | Resolve types from returns, constructors, class literals |
| **Gradle dep resolution** | Shallow parse `libs.versions.toml` + `build.gradle.kts` without daemon |

### Unique to official (not in this project)

| Feature | Why it matters |
|---------|---------------|
| **Full type checking** | Real-time errors for mismatches, null safety |
| **Rich completion** | Type-aware with expected-type filtering |
| **Quick fixes** | Auto-import, add type annotation, convert to expression body |
| **Signature help** | Parameter hints as you type |

### Shared

| Feature | This project | Official |
|---------|-------------|----------|
| Goto definition | Index lookup + `rg` fallback | IntelliJ index |
| Find references | `rg --word-regexp` | IntelliJ semantic search |
| Hover | KDoc/Javadoc from source | IntelliJ doc resolver |
| Completion | CST-based + scored | IntelliJ type-aware |
| Rename | Project-wide + reindex | IntelliJ refactoring engine |
| Call hierarchy | `rg` incoming + CST outgoing | IntelliJ analysis |

## Performance

| Metric | This project | Official |
|--------|-------------|----------|
| Cold start | ~1s | ~5-10s |
| Subsequent start | ~50ms | ~5-10s |
| 100-file index | ~200ms | ~2-5s |
| Idle memory | ~15-30 MB | ~300-800 MB |

## Limitations of this project

1. **No type checking** — CST-level syntax errors only
2. **No built-in formatting** — Delegates to `ktfmt` / `ktlint` on `$PATH`
3. **Completion quality** — Good for names, no type-aware ranking
4. **Best-effort type inference** — Complex generics may not resolve
