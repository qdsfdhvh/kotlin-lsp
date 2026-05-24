#!/bin/bash
# Claude Code PostToolUse Hook: auto-inject types when reading Kotlin/Java/Swift files
# Install: copy to .claude/hooks/ or symlink

FILE="$1"
EXT="${FILE##*.}"

case "$EXT" in
  kt|kts|java|swift)
    if command -v kotlin-lsp &>/dev/null; then
      kotlin-lsp inject "$FILE" 2>/dev/null
    fi
    ;;
esac
