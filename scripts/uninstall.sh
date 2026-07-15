#!/usr/bin/env bash
# uninstall.sh — remove kotlin-lsp binary, cache, sources, and config.
set -euo pipefail

BOLD=$(tput bold 2>/dev/null || echo "")
RESET=$(tput sgr0 2>/dev/null || echo "")

echo "${BOLD}kotlin-lsp uninstall${RESET}"
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

# 4. Project-local caches (.kotlin-lsp/cache in all repos)
echo ""
echo "  Looking for project-local caches (.kotlin-lsp/cache)..."
for kid in $(find "$HOME" -maxdepth 5 -type d -name ".kotlin-lsp" 2>/dev/null); do
    if [ -d "$kid/cache" ]; then
        size=$(du -sh "$kid" 2>/dev/null | cut -f1)
        rm -rf "$kid"
        echo "    ✅ $kid ($size)"
        total=$((total + 1))
    fi
done

echo ""
echo "${BOLD}Done — $total items removed.${RESET}"
