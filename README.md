# kotlin-lsp

[![release](https://img.shields.io/github/v/release/qdsfdhvh/kotlin-lsp)](https://github.com/qdsfdhvh/kotlin-lsp/releases/latest)
[![build](https://img.shields.io/github/actions/workflow/status/qdsfdhvh/kotlin-lsp/ci.yml)](https://github.com/qdsfdhvh/kotlin-lsp/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/qdsfdhvh/kotlin-lsp)](LICENSE)

A fast, no-JVM **symbol engine** for Kotlin, Java, and Swift — with a
scriptable CLI and LSP transport.  Built with
[tree-sitter](https://tree-sitter.github.io/) — instant startup, low memory,
zero external runtime.

---

## Install / update

Install and update only from [GitHub Release prebuilt assets](https://github.com/qdsfdhvh/kotlin-lsp/releases/latest).
Local builds are for development/testing, not machine installation.

### macOS / Linux

Download the matching archive from the Release page:

| Environment | Asset |
|---|---|
| macOS Apple Silicon (native arm64 shell) | `kotlin-lsp-darwin-aarch64.tar.gz` |
| Linux x86_64 | `kotlin-lsp-linux-x86_64.tar.gz` |
| Linux arm64 | `kotlin-lsp-linux-aarch64.tar.gz` |

Extract it, then place the contained `kotlin-lsp-<os>-<arch>` binary on PATH as
`kotlin-lsp`. For example, after downloading the Apple Silicon archive:

```bash
tar -xzf kotlin-lsp-darwin-aarch64.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 0755 kotlin-lsp-darwin-aarch64 "$HOME/.local/bin/kotlin-lsp"
"$HOME/.local/bin/kotlin-lsp" --version
```

Add `$HOME/.local/bin` to PATH if needed. Repeat with the new Release asset to
update, and verify the exact destination's version matches the selected tag.
There is no current Intel macOS asset; Rosetta cannot run arm64 binaries on Intel.
Prefer this manual Release path until the updated `scripts/install.sh` is
published: older Release installer scripts try Cargo first and may verify an
unrelated binary on PATH. The repository script now uses Release assets only.

### Windows

```powershell
iwr -useb https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.ps1 | iex
& "$env:USERPROFILE\.kotlin-lsp\bin\kotlin-lsp.exe" --version
```

The installer selects `kotlin-lsp-windows-x86_64.zip` or
`kotlin-lsp-windows-aarch64.zip`. Manual extraction contains
`<asset>/kotlin-lsp.exe`; place that binary on PATH. Re-run for updates.

### Local development (not installation)

```bash
cargo build
cargo test --test benches
# Run the development binary in place; do not copy it into an install directory.
./target/debug/kotlin-lsp --help
```

**Recommended:** install `fd` and `rg` (ripgrep) for faster file discovery.

---

## Usage

`kotlin-lsp` works standalone — no editor, no daemon.

- **[docs/commands.md](docs/commands.md)** — full command reference, examples, flags

Semantic search keeps `--json` as a compact array. Opt into truthful limit
metadata with `kotlin-lsp search "login repo" --json --json-envelope --limit 1`
(`{results,truncated}`). Flags are command-specific; capability group flags are
unions, not promises for every member.

Call-graph tooling for agents (tree-sitter based, no JVM):

```bash
kotlin-lsp call reach entry --to target    # every call path entry→target
kotlin-lsp call diff                        # call-tree diff HEAD vs worktree (branch-aware, inferred entries)
kotlin-lsp call diff main feature --entry boot
kotlin-lsp call hierarchy F.kt 42 10        # direct callers / callees (both by default)
kotlin-lsp call hierarchy entry --outgoing  # exact callable name; one hop
kotlin-lsp --version                        # tool + tree-sitter grammar versions
```

---

## For AI agents

```bash
npx skills add https://github.com/qdsfdhvh/kotlin-lsp
```

The bundled skill teaches your agent to prefer `kotlin-lsp find` / `refs` /
`hover` over text-grep for Kotlin/Java/Swift symbols. Re-run after updates.

### Agent mode

Limit what the agent indexes by placing a `tools/kotlin-lsp` file in your project root:

```
# tools/kotlin-lsp — paths to index in agent mode
src/
build.gradle.kts
```

Then start kotlin-lsp with `--agent`. Only listed paths are indexed, reducing memory and startup time for agent workflows.
