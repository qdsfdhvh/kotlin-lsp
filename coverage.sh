#!/usr/bin/env bash
# coverage.sh — generate llvm-cov report for kotlin-lsp
# Usage:
#   ./coverage.sh                          → show targeted files (resolution, complete, indexer_tests)
#   ./coverage.sh src/indexer/resolution.rs → show specific file
#   ./coverage.sh --all                     → show everything
set -euo pipefail

LLVM_TOOLS="$(rustup run stable rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin"

echo "==> Cleaning..."
rm -f coverage-*.profraw coverage.profdata

echo "==> Building with -C instrument-coverage..."
RUSTFLAGS="-C instrument-coverage" rustup run stable cargo test --no-run --quiet

echo "==> Running tests..."
BIN=$(ls target/debug/deps/kotlin_lsp-* 2>/dev/null | grep -v '\.d$' | head -1)
if [ -z "$BIN" ]; then echo "ERROR: no test binary"; exit 1; fi
LLVM_PROFILE_FILE="coverage-%p-%m.profraw" "$BIN" --quiet 2>/dev/null

echo "==> Merging profiles..."
$LLVM_TOOLS/llvm-profdata merge -sparse coverage-*.profraw -o coverage.profdata

echo "==> Report:"
echo ""

# Select filter
if [ "${1:-}" = "--all" ]; then
    FILTER="."
else
    FILTER="${*:-resolution\.rs|complete\.rs|indexer_tests\.rs}"
fi

$LLVM_TOOLS/llvm-cov report \
    -ignore-filename-regex='/.cargo/' \
    -instr-profile=coverage.profdata \
    "$BIN" target/debug/kotlin-lsp 2>/dev/null | \
    awk -v re="$FILTER" '
        NR <= 2 { print; next }
        NR == 3 { print; next }
        $0 ~ re || NF == 0
    '

echo ""
rm -f coverage-*.profraw coverage.profdata
echo "Done."
