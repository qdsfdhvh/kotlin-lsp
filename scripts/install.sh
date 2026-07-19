#!/usr/bin/env bash
# kotlin-lsp installer for Linux and macOS.
#
# Usage:
#   curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh | bash
#
# Tries in order:
#   1. cargo install (preferred — builds from source, puts in ~/.cargo/bin)
#   2. GitHub Release binary download (fallback if cargo isn't available)
#
# Environment variables:
#   KOTLIN_LSP_VERSION   pin a version (e.g. v0.29.0). Default: latest release.
#   KOTLIN_LSP_REPO      override the source repo (default: qdsfdhvh/kotlin-lsp).
#   KOTLIN_LSP_PREFIX    install directory for binary download. Default: $HOME/.local/bin
#                        (falls back to /usr/local/bin if writable and HOME/.local/bin is not on PATH).
#   KOTLIN_LSP_FORCE_BINARY  set to 1 to force GitHub Release download even if cargo is present.
set -euo pipefail

REPO="${KOTLIN_LSP_REPO:-qdsfdhvh/kotlin-lsp}"
REPO_URL="https://github.com/${REPO}"
VERSION="${KOTLIN_LSP_VERSION:-latest}"
PREFIX="${KOTLIN_LSP_PREFIX:-$HOME/.local/bin}"
FORCE_BINARY="${KOTLIN_LSP_FORCE_BINARY:-0}"

err() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
info() { printf '\033[36m::\033[0m %s\n' "$*"; }
warn() { printf '\033[33m!\033[0m %s\n' "$*"; }

# ── check for cargo first (preferred) ──────────────────────────────
try_cargo() {
  if [ "$FORCE_BINARY" = "1" ]; then
    info "KOTLIN_LSP_FORCE_BINARY=1 — skipping cargo, using binary download"
    return 1
  fi
  # Detect the real cargo binary (not the rustup wrapper symlink)
  local cargo_bin=""
  if command -v cargo >/dev/null 2>&1; then
    # Prefer toolchain-direct cargo binary to avoid rustup proxy issues
    local rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"
    local toolchain_dir=$(ls -d "$rustup_home/toolchains"/stable-*/bin/cargo 2>/dev/null | head -1)
    if [ -x "$toolchain_dir" ]; then
      cargo_bin="$toolchain_dir"
    else
      cargo_bin="cargo"
    fi
  fi
  if [ -z "$cargo_bin" ] || ! "$cargo_bin" --version >/dev/null 2>&1; then
    warn "cargo not found — falling back to binary download"
    info "  install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    return 1
  fi
  if ! command -v pkg-config >/dev/null 2>&1; then
    warn "pkg-config not found — cargo build may fail; falling back to binary"
    return 1
  fi

  local pkg="kotlin-lsp"
  if [ "$VERSION" != "latest" ]; then
    pkg="kotlin-lsp@${VERSION#v}"
  fi
  local url="${REPO_URL}.git"
  info "installing via cargo from ${url}"
  "$cargo_bin" install --git "$url" --tag "$VERSION" "$pkg" 2>&1 || {
    warn "cargo install failed — falling back to binary download"
    return 1
  }
  info "installed via cargo → $(command -v kotlin-lsp 2>/dev/null || echo '~/.cargo/bin/kotlin-lsp')"
  return 0
}

# ── binary download fallback ───────────────────────────────────────
download_binary() {
  # detect platform
  local uname_s="$(uname -s)"
  local uname_m="$(uname -m)"
  local os=""
  case "$uname_s" in
    Linux)  os="linux" ;;
    Darwin) os="darwin" ;;
    *) err "unsupported OS: $uname_s (use install.ps1 on Windows)" ;;
  esac
  local arch=""
  case "$uname_m" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) err "unsupported architecture: $uname_m" ;;
  esac
  local asset="kotlin-lsp-${os}-${arch}"
  info "platform: ${os}/${arch} → ${asset}"

  # resolve download URL
  local url=""
  if [ "$VERSION" = "latest" ]; then
    url="${REPO_URL}/releases/latest/download/${asset}.tar.gz"
  else
    url="${REPO_URL}/releases/download/${VERSION}/${asset}.tar.gz"
  fi
  info "downloading ${url}"

  local tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp:-}"' EXIT

  if command -v curl >/dev/null 2>&1; then
    curl -fSL --retry 3 -o "$tmp/asset.tar.gz" "$url" \
      || err "download failed — check that release ${VERSION} exists and includes ${asset}.tar.gz"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$tmp/asset.tar.gz" "$url" \
      || err "download failed — check that release ${VERSION} exists and includes ${asset}.tar.gz"
  else
    err "need either curl or wget"
  fi

  # extract
  tar -xzf "$tmp/asset.tar.gz" -C "$tmp"
  local bin_src=""
  if [ -f "$tmp/$asset" ]; then
    bin_src="$tmp/$asset"
  elif [ -f "$tmp/kotlin-lsp" ]; then
    bin_src="$tmp/kotlin-lsp"
  else
    err "tarball did not contain the kotlin-lsp binary (looked for $asset and kotlin-lsp)"
  fi
  chmod +x "$bin_src"

  # install
  mkdir -p "$PREFIX" 2>/dev/null || true
  if [ ! -w "$PREFIX" ]; then
    if [ -w /usr/local/bin ]; then
      PREFIX="/usr/local/bin"
    elif command -v sudo >/dev/null 2>&1; then
      info "elevating to write to /usr/local/bin"
      SUDO="sudo"
      PREFIX="/usr/local/bin"
    else
      err "no writable install prefix; set KOTLIN_LSP_PREFIX or rerun with sudo"
    fi
  fi

  local dest="$PREFIX/kotlin-lsp"
  ${SUDO:-} install -m 0755 "$bin_src" "$dest"
  info "installed → ${dest}"
}

# ── verify ─────────────────────────────────────────────────────────
verify() {
  local bin="kotlin-lsp"
  if ! command -v "$bin" >/dev/null 2>&1; then
    # maybe just installed via cargo and not yet on PATH in this shell
    if [ -f "$HOME/.cargo/bin/kotlin-lsp" ]; then
      bin="$HOME/.cargo/bin/kotlin-lsp"
    else
      err "kotlin-lsp not found on PATH"
    fi
  fi
  if ! "$bin" --version >/dev/null 2>&1; then
    err "binary did not run cleanly — try '$bin --version' to debug"
  fi
  info "$("$bin" --version)"

  # PATH hint
  local dir="$(dirname "$(command -v "$bin" 2>/dev/null || echo "$bin")")"
  case ":${PATH:-}:" in
    *":$dir:"*) ;;
    *)
      cat <<EOF

\033[33m!\033[0m $dir is not in your PATH. Add it with:

    echo 'export PATH="$dir:\$PATH"' >> ~/.zshrc

EOF
      ;;
  esac
}

# ── main ───────────────────────────────────────────────────────────
if try_cargo; then
  verify
  cat <<'EOF'

Next: wire up your editor — see docs at
  https://github.com/qdsfdhvh/kotlin-lsp#setup

EOF
  exit 0
fi

download_binary
verify
cat <<'EOF'

Next: wire up your editor — see docs at
  https://github.com/qdsfdhvh/kotlin-lsp#setup

EOF
