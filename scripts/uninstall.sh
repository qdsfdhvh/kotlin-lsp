#!/usr/bin/env bash
# uninstall.sh — remove kotlin-lsp binary, cache, sources, and config.
set -euo pipefail

BOLD=$(tput bold 2>/dev/null || echo "")
RESET=$(tput sgr0 2>/dev/null || echo "")

echo "${BOLD}kotlin-lsp uninstall${RESET}"
echo ""
echo "This will remove:"
echo "  • kotlin-lsp binary"
echo "  • Library sources (~/.kotlin-lsp)"
echo "  • Global cache (~/.cache/kotlin-lsp)"
echo "  • Current project cache (.kotlin-lsp)"
echo ""
read -p "Continue? [y/N] " confirm
if [ "$confirm" != "y" ] && [ "$confirm" != "Y" ] && [ "${1:-}" != "--force" ]; then
    echo "Cancelled."
    exit 0
fi
echo ""

total=0

# 1. Binary
BIN=$(which kotlin-lsp 2>/dev/null || echo "")
if [ -n "$BIN" ] && [ -f "$BIN" ]; then
    size=$(ls -lh "$BIN" | awk '{print $5}')
    rm "$BIN"
    echo "  ✅ Binary removed: $BIN ($size)"
    total=$((total + 1))
else
    echo "  ⏭️  Binary not found"
fi

# 2. Library sources (~/.kotlin-lsp/sources)
if [ -d "$HOME/.kotlin-lsp" ]; then
    size=$(du -sh "$HOME/.kotlin-lsp" 2>/dev/null | cut -f1)
    rm -rf "$HOME/.kotlin-lsp"
    echo "  ✅ Library sources: $HOME/.kotlin-lsp ($size)"
    total=$((total + 1))
fi

# 3. Legacy global cache (~/.cache/kotlin-lsp)
if [ -d "$HOME/.cache/kotlin-lsp" ]; then
    size=$(du -sh "$HOME/.cache/kotlin-lsp" 2>/dev/null | cut -f1)
    rm -rf "$HOME/.cache/kotlin-lsp"
    echo "  ✅ Global cache: $HOME/.cache/kotlin-lsp ($size)"
    total=$((total + 1))
fi

# 4. Current project cache (.kotlin-lsp/cache in CWD)
local_cache="$(pwd)/.kotlin-lsp"
if [ -d "$local_cache/cache" ]; then
    size=$(du -sh "$local_cache" 2>/dev/null | cut -f1)
    rm -rf "$local_cache"
    echo "  ✅ Project cache: $local_cache ($size)"
    total=$((total + 1))
else
    echo "  ⏭️  No project cache at $local_cache"
fi
