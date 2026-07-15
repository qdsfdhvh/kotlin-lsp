#!/usr/bin/env bash
# dogfood.sh — smoke-test kotlin-lsp against real-world Kotlin projects.
# Usage: ./tests/dogfood.sh [--release]
set -euo pipefail

BIN="$(cd "$(dirname "$0")/.." && pwd)/target/release/kotlin-lsp"
if [ "${1:-}" = "--release" ] || [ ! -x "$BIN" ]; then
    echo "==> Building release binary..."
    rustup run stable cargo build --release --quiet || cargo build --release --quiet
fi

echo "BIN=$BIN"

TMPDIR="/tmp/kotlin-lsp-dogfood"
CACHE="$TMPDIR/cache"
mkdir -p "$CACHE"

PASS=0
FAIL=0
PANICS=0

report() { printf "  %-8s %s\n" "$1" "$2"; }
ok()    { report "✅" "$1"; PASS=$((PASS + 1)); }
fail()  { report "❌" "$1"; FAIL=$((FAIL + 1)); }
panic() { report "💥" "$1 (PANIC)"; PANICS=$((PANICS + 1)); }

run_cmd() {
    local out
    set +e
    out=$(XDG_CACHE_HOME="$CACHE" timeout 30 "$BIN" "$@" 2>&1)
    local rc=$?
    set -e
    # Panic = signal 6 (SIGABRT) or "panicked" in output
    if [ $rc -eq 134 ] || echo "$out" | grep -q "panicked at"; then
        echo "$out"
        return 134
    fi
    # Non-zero exit with explicit error message = handled
    if [ $rc -ne 0 ]; then
        if echo "$out" | grep -qE "^error:|^No |^Usage:"; then
            echo "$out"
            return $rc
        fi
        echo "$out"
        return 1
    fi
    echo "$out"
    return 0
}

test_project() {
    local name="$1" url="$2" symbol="$3"
    local dir="$TMPDIR/$name"

    echo ""
    echo "━━━ $name ━━━"

    # Clone
    echo -n "  clone... "
    if git clone --depth 1 "$url" "$dir" 2>&1 | tail -1; then
        echo "  ok ($(find "$dir" -name '*.kt' | wc -l | tr -d ' ') files)"
    else
        echo "FAILED"
        fail "$name: clone"
        return
    fi

    # Index
    echo -n "  index... "
    local out
    out=$(cd "$dir" && XDG_CACHE_HOME="$CACHE" "$BIN" index 2>&1)
    if echo "$out" | grep -q "Index ready"; then
        local syms=$(echo "$out" | grep "Index ready" | grep -oE '[0-9]+ symbols')
        echo "ok ($syms)"
    else
        echo "FAILED"
        echo "$out" | tail -3
        fail "$name: index"
        return
    fi

    # Find
    echo -n "  find $symbol... "
    if out=$(cd "$dir" && run_cmd find "$symbol" --limit 3); then
        if echo "$out" | grep -q "$symbol"; then
            ok "$name: find $symbol"
        else
            fail "$name: find $symbol (not found)"
        fi
    elif [ $? -eq 134 ]; then
        panic "$name: find $symbol"
    else
        fail "$name: find $symbol"
    fi

    # find --kind fun
    echo -n "  find $symbol --kind fun... "
    if out=$(cd "$dir" && run_cmd find "$symbol" --limit 3 --kind fun); then
        ok "$name: find $symbol --kind fun"
    elif [ $? -eq 134 ]; then
        panic "$name: find $symbol --kind fun"
    else
        fail "$name: find $symbol --kind fun"
    fi

    # refs
    echo -n "  refs $symbol... "
    if out=$(cd "$dir" && run_cmd refs "$symbol" --limit 3); then
        ok "$name: refs $symbol"
    elif [ $? -eq 134 ]; then
        panic "$name: refs $symbol"
    else
        fail "$name: refs $symbol"
    fi

    # symbol-graph
    echo -n "  symbol-graph... "
    if out=$(cd "$dir" && run_cmd symbol-graph); then
        ok "$name: symbol-graph"
    elif [ $? -eq 134 ]; then
        panic "$name: symbol-graph"
    else
        fail "$name: symbol-graph"
    fi

    # snapshot
    echo -n "  snapshot --json... "
    if out=$(cd "$dir" && run_cmd snapshot --json); then
        ok "$name: snapshot"
    elif [ $? -eq 134 ]; then
        panic "$name: snapshot"
    else
        fail "$name: snapshot"
    fi
}

# ── test projects ──────────────────────────────────────────────────────────

test_project "ktor"        "https://github.com/ktorio/ktor.git"            "HttpStatusCode"
test_project "nowinandroid" "https://github.com/android/nowinandroid.git"   "NiaApp"

# ── summary ────────────────────────────────────────────────────────────────

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
printf "  Passed:  %d\n" "$PASS"
printf "  Failed:  %d\n" "$FAIL"
printf "  Panics:  %d\n" "$PANICS"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

[ "$PANICS" -eq 0 ] && [ "$FAIL" -eq 0 ] && exit 0
exit 1
