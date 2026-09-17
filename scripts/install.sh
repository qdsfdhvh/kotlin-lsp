#!/usr/bin/env bash
# kotlin-lsp installer for Linux and macOS.
#
# Usage:
#   curl -fsSL https://github.com/qdsfdhvh/kotlin-lsp/releases/latest/download/install.sh | bash
#
# Installs or updates exclusively from GitHub Release prebuilt assets.
#
# Environment variables:
#   KOTLIN_LSP_VERSION   release tag. Default: latest release.
#   KOTLIN_LSP_REPO      release repo (default: qdsfdhvh/kotlin-lsp).
#   KOTLIN_LSP_PREFIX    install directory. Default: $HOME/.local/bin.
# KOTLIN_LSP_FORCE_BINARY is no longer needed; all installs use Release assets.
set -euo pipefail

REPO="${KOTLIN_LSP_REPO:-qdsfdhvh/kotlin-lsp}"
REPO_URL="https://github.com/${REPO}"
VERSION="${KOTLIN_LSP_VERSION:-latest}"
PREFIX="${KOTLIN_LSP_PREFIX:-$HOME/.local/bin}"

err() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }
info() { printf '\033[36m::\033[0m %s\n' "$*"; }

# ── Release binary download ───────────────────────────────────────
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
    x86_64|amd64)
      if [ "$os" = "darwin" ]; then
        err "no Release asset for a Darwin x86_64 environment; on Apple Silicon use a native arm64 shell. Intel Macs are unsupported (Rosetta cannot run arm64 on Intel)."
      else
        arch="x86_64"
      fi
      ;;
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
  local bin="$PREFIX/kotlin-lsp"
  if ! "$bin" --version >/dev/null 2>&1; then
    err "binary did not run cleanly — try '$bin --version' to debug"
  fi
  info "$("$bin" --version)"

  # PATH hint
  local dir="$PREFIX"
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
download_binary
verify
cat <<'EOF'

Next: wire up your editor — see docs at
  https://github.com/qdsfdhvh/kotlin-lsp#setup

EOF
