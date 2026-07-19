# kotlin-lsp

[![release](https://img.shields.io/github/v/release/qdsfdhvh/kotlin-lsp)](https://github.com/qdsfdhvh/kotlin-lsp/releases/latest)
[![build](https://img.shields.io/github/actions/workflow/status/qdsfdhvh/kotlin-lsp/ci.yml)](https://github.com/qdsfdhvh/kotlin-lsp/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/qdsfdhvh/kotlin-lsp)](LICENSE)

A fast, no-JVM **symbol engine** for Kotlin, Java, and Swift — with a
scriptable CLI and LSP transport.  Built with
[tree-sitter](https://tree-sitter.github.io/) — instant startup, low memory,
zero external runtime.

---

## Install

### macOS / Linux

```bash
# Primary: build from source via cargo (→ ~/.cargo/bin)
cargo install --git https://github.com/qdsfdhvh/kotlin-lsp --tag v0.29.0

# Fallback: pre-built binary
curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh | bash

kotlin-lsp --version
```

### Windows

```powershell
# PowerShell
iwr -useb https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.ps1 | iex
kotlin-lsp --version
```

### Manual

Download from [releases](https://github.com/qdsfdhvh/kotlin-lsp/releases/latest)
and place the binary on your `PATH`.

### Build from source

```bash
git clone https://github.com/qdsfdhvh/kotlin-lsp
cd kotlin-lsp
cargo build --release
# binary at target/release/kotlin-lsp
```


**Recommended:** install `fd` and `rg` (ripgrep) for faster file discovery.

---

## Usage

`kotlin-lsp` works standalone — no editor, no daemon.

- **[docs/commands.md](docs/commands.md)** — full command reference, examples, flags

---

## For AI agents

```bash
npx skills add https://github.com/qdsfdhvh/kotlin-lsp
```

The bundled skill teaches your agent to prefer `kotlin-lsp find` / `refs` /
`hover` over text-grep for Kotlin/Java/Swift symbols. Re-run after updates.
