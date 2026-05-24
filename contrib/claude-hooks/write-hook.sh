#!/bin/bash
# Claude Code PostToolUse Hook: auto-check syntax when writing Kotlin/Java/Swift files
# Install: copy to .claude/hooks/ or symlink

FILE="$1"
EXT="${FILE##*.}"

case "$EXT" in
  kt|kts|java|swift)
    if command -v kotlin-lsp &>/dev/null; then
      RESULT=$(kotlin-lsp check "$FILE" 2>&1)
      if [ $? -ne 0 ]; then
        echo "<file_diagnostics>"
        echo "$RESULT"
        echo "</file_diagnostics>"
      fi
    fi
    ;;
esac
