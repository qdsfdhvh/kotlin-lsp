#!/usr/bin/env bash
# dogfood.sh — comprehensive smoke-test of kotlin-lsp against real projects.
# Usage: ./tests/dogfood.sh
set -euo pipefail

BIN="$(cd "$(dirname "$0")/.." && pwd)/target/release/kotlin-lsp"
if [ ! -x "$BIN" ]; then
    echo "==> Building release binary..."
    rustup run stable cargo build --release --quiet || cargo build --release --quiet
fi

TMPDIR="/tmp/kotlin-lsp-dogfood"
CACHE="$TMPDIR/cache"
rm -rf "$TMPDIR" && mkdir -p "$CACHE"

PASS=0; FAIL=0; PANICS=0

ok()    { printf "  ✅ %s\n" "$1"; PASS=$((PASS + 1)); }
fail()  { printf "  ❌ %s\n" "$1"; FAIL=$((FAIL + 1)); }
panic() { printf "  💥 %s (PANIC)\n" "$1"; PANICS=$((PANICS + 1)); }

ATTR_RESET=$(tput sgr0 2>/dev/null || echo "")
ATTR_BOLD=$(tput bold 2>/dev/null || echo "")

run() {
    local rc out
    set +e
    out=$(XDG_CACHE_HOME="$CACHE" "$BIN" "$@" 2>&1)
    rc=$?
    set -e
    echo "$out"
    return $rc
}

# Run a command inside the project dir. Returns true if it didn't panic.
# Panic = exit 134 or "panicked at" in output.
check() {
    local label="$1" dir="$2"; shift 2
    local out rc
    set +e
    out=$(cd "$dir" && XDG_CACHE_HOME="$CACHE" "$BIN" "$@" 2>&1)
    rc=$?
    set -e
    if [ $rc -eq 134 ] || echo "$out" | grep -q "panicked at"; then
        printf "%s\n" "$out" | tail -5
        panic "$label"
        return 1
    fi
    # Non-zero exit that looks handled (explicit error message) = ok
    if [ $rc -ne 0 ]; then
        if echo "$out" | grep -qE "^(error:|No |Usage:|warning:)" || [ $rc -eq 1 ]; then
            ok "$label"
            return 0
        fi
        printf "%s\n" "$out" | tail -3
        fail "$label"
        return 1
    fi
    ok "$label"
    return 0
}

test_project() {
    local name="$1" url="$2" symbol="$3" class_symbol="${4:-}"
    local dir="$TMPDIR/$name"

    echo ""
    echo "${ATTR_BOLD}━━━ $name ━━━${ATTR_RESET}"

    # ── clone ──
    echo -n "  clone... "
    if git clone --depth 1 --quiet "$url" "$dir" 2>/dev/null; then
        echo "$(find "$dir" -name '*.kt' | wc -l | tr -d ' ') files"
    else
        fail "$name: clone"; return
    fi

    # ── index ──
    echo -n "  index... "
    local out syms
    out=$(cd "$dir" && XDG_CACHE_HOME="$CACHE" "$BIN" index 2>&1)
    if echo "$out" | grep -q "Index ready"; then
        syms=$(echo "$out" | grep "Index ready" | grep -oE '[0-9]+ symbols')
        echo "$syms"
    else
        echo "FAILED"; echo "$out" | tail -3
        fail "$name: index"; return
    fi

    # ── extract position from find --json for position-dependent commands ──
    local find_json file line col
    find_json=$(cd "$dir" && XDG_CACHE_HOME="$CACHE" "$BIN" find "$symbol" --limit 1 --json 2>/dev/null || echo "")
    if [ -n "$find_json" ]; then
        file=$(echo "$find_json" | python3 -c "import sys,json; d=json.load(sys.stdin)[0]; print(d['file'])" 2>/dev/null || echo "")
        line=$(echo "$find_json" | python3 -c "import sys,json; d=json.load(sys.stdin)[0]; print(d['line'])" 2>/dev/null || echo "1")
        col=$(echo  "$find_json" | python3 -c "import sys,json; d=json.load(sys.stdin)[0]; print(d['col'])" 2>/dev/null || echo "1")
    else
        file=""; line="1"; col="1"
    fi

    # ── search commands ──
    check "$name: find $symbol"          "$dir" find "$symbol" --limit 3
    check "$name: find $symbol --kind"   "$dir" find "$symbol" --limit 3 --kind fun
    check "$name: refs $symbol"          "$dir" refs "$symbol" --limit 5
    check "$name: summarize $symbol"     "$dir" summarize "$symbol"

    # ── position-dependent commands ──
    [ -n "$file" ] && {
        check "$name: hover"              "$dir" hover "$file" "$line" "$col"
        check "$name: context"            "$dir" context "$file" "$line" "$col"
        check "$name: code-action"        "$dir" code-action "$file" "$line" "$col"
        check "$name: call-hierarchy"     "$dir" call-hierarchy "$file" "$line" "$col"
        check "$name: impact"             "$dir" impact "$file" "$line" "$col"
        check "$name: find-test"          "$dir" find-test "$file" "$line" "$col"
        check "$name: callers"            "$dir" callers "$file" "$line" "$col"
        check "$name: callees"            "$dir" callees "$file" "$line" "$col"
    }

    # ── workspace-wide commands ──
    check "$name: symbol-graph"          "$dir" symbol-graph
    check "$name: snapshot --json"       "$dir" snapshot --json

    check "$name: modules"               "$dir" modules
    check "$name: cache stats"           "$dir" cache stats

    # ── file-level commands (use the file from find_json above) ──
    [ -n "$file" ] && {
        check "$name: check"              "$dir" check "$file"
        check "$name: inspect"            "$dir" inspect "$file"
        check "$name: format check"       "$dir" format check "$file"
        check "$name: complete"           "$dir" complete "$file" 1 1
    }

    # ── type-hierarchy with a class (if provided) ──



}

# ── read projects from config ──────────────────────────────────────────────

CONF="$(dirname "$0")/dogfood.conf"
if [ ! -f "$CONF" ]; then
    echo "Config not found: $CONF"
    exit 1
fi

while IFS='|' read -r name url symbol class_symbol _; do
    [[ "$name" =~ ^# ]] && continue
    [ -z "$name" ] && continue
    test_project "$name" "$url" "$symbol" "${class_symbol:-}"
done < "$CONF"

# ── summary ────────────────────────────────────────────────────────────────

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
printf "  ✅ Passed:  %d\n" "$PASS"
printf "  ❌ Failed:  %d\n" "$FAIL"
printf "  💥 Panics:  %d\n" "$PANICS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

[ "$PANICS" -eq 0 ] && [ "$FAIL" -eq 0 ] && exit 0
exit 1

