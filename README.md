# kotlin-lsp

[![release](https://img.shields.io/github/v/release/qdsfdhvh/kotlin-lsp)](https://github.com/qdsfdhvh/kotlin-lsp/releases/latest)
[![build](https://img.shields.io/github/actions/workflow/status/qdsfdhvh/kotlin-lsp/ci.yml)](https://github.com/qdsfdhvh/kotlin-lsp/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/qdsfdhvh/kotlin-lsp)](LICENSE)

A fast, no-JVM **symbol engine** for Kotlin, Java, and Swift — with a
scriptable CLI and LSP transport.  
Built with [tree-sitter](https://tree-sitter.github.io/) — instant startup,
low memory, zero external runtime.


---

## Install

### macOS / Linux

Install the latest prebuilt binary:

```bash
curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh | bash
kotlin-lsp --version
```

The installer detects your OS and CPU architecture, downloads the matching
release asset, and writes `kotlin-lsp` to `~/.local/bin` by default.

Pin a specific release:

```bash
curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh \
  | KOTLIN_LSP_VERSION=v0.20.0 bash
```

Install into another directory:

```bash
curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh \
  | KOTLIN_LSP_PREFIX=/usr/local/bin bash
```

If the install directory is not on `PATH`, add it and open a new shell:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
```

### Windows

Install the latest prebuilt binary from PowerShell:

```powershell
iwr -useb https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.ps1 | iex
kotlin-lsp --version
```

The installer writes `kotlin-lsp.exe` to
`%USERPROFILE%\.kotlin-lsp\bin` and adds that directory to your user `PATH`.
Open a new terminal if `kotlin-lsp` is not found immediately.

Pin a specific release:

```powershell
$env:KOTLIN_LSP_VERSION = 'v0.20.0'
iwr -useb https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.ps1 | iex
```

### Update

Run the same installer again. It overwrites the existing binary and verifies
that the new one starts:

```bash
curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh | bash
kotlin-lsp --version
```

```powershell
iwr -useb https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.ps1 | iex
kotlin-lsp --version
```

To update to an exact version, set `KOTLIN_LSP_VERSION` as shown above.

### Manual install

1. Open the [latest release](https://github.com/qdsfdhvh/kotlin-lsp/releases/latest).
2. Download the asset for your platform:
   - macOS Apple Silicon: `kotlin-lsp-darwin-aarch64.tar.gz`
   - macOS Intel: `kotlin-lsp-darwin-x86_64.tar.gz`
   - Linux arm64: `kotlin-lsp-linux-aarch64.tar.gz`
   - Linux x64: `kotlin-lsp-linux-x86_64.tar.gz`
   - Windows arm64: `kotlin-lsp-windows-aarch64.zip`
   - Windows x64: `kotlin-lsp-windows-x86_64.zip`
3. Extract it.
4. Move the binary onto your `PATH`, for example:

```bash
mkdir -p ~/.local/bin
install -m 0755 kotlin-lsp-darwin-aarch64 ~/.local/bin/kotlin-lsp
kotlin-lsp --version
```

On Windows, move `kotlin-lsp.exe` into a directory on your user `PATH`.

### Troubleshooting

- `kotlin-lsp: command not found`: run `which kotlin-lsp` / `Get-Command kotlin-lsp`, then make sure the install directory is on `PATH`.
- Shell still sees an old version: open a new terminal, or check for another earlier `kotlin-lsp` in `PATH`.
- macOS blocks a browser-downloaded binary: run `xattr -d com.apple.quarantine ~/.local/bin/kotlin-lsp`.
- Need to confirm what changed: compare `kotlin-lsp --version` with the [release page](https://github.com/qdsfdhvh/kotlin-lsp/releases).

**Recommended:** Install `fd` and `rg` (ripgrep) for faster file discovery and
cross-file search.

---

## For AI agents

`kotlin-lsp` is designed for AI-agent workflows. Once it's on your `PATH`,
install the bundled agent skill so your AI tool knows when and how to call it:

```bash
npx skills add https://github.com/qdsfdhvh/kotlin-lsp
```

Re-run the same command after updating `kotlin-lsp` so the agent picks up the
latest CLI guidance. If your agent caches skills at startup, restart it after
installing or updating the skill.

The skill teaches the agent to prefer `kotlin-lsp find` / `refs` / `hover`
over text-grep for Kotlin/Java/Swift symbols — saving tokens and returning
structured results. See [`skills/kotlin-lsp/SKILL.md`](skills/kotlin-lsp/SKILL.md)
for the full command reference.

---

## CLI

`kotlin-lsp` works standalone — no editor, no daemon.

See **[docs/commands.md](docs/commands.md)** for the full command reference,
examples, flags, and what gets indexed.



---

## Features

**CLI** — See **[docs/commands.md](docs/commands.md)** for the full command reference,
examples, flags, and what gets indexed.

**LSP** — See **[docs/lsp.md](docs/lsp.md)** for the LSP handler ↔ CLI mapping.

---
