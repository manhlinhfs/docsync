#!/usr/bin/env bash
set -euo pipefail

OWNER="${DOCSYNC_GITHUB_OWNER:-manhlinhfs}"
REPO="${DOCSYNC_GITHUB_REPO:-docsync}"
VERSION="${1:-latest}"
INSTALL_DIR="${DOCSYNC_INSTALL_DIR:-$HOME/.local/bin}"

detect_os() {
  case "$(uname -s)" in
    Linux) echo "unknown-linux-gnu" ;;
    Darwin) echo "apple-darwin" ;;
    *) echo "unsupported"; return 1 ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64) echo "x86_64" ;;
    arm64|aarch64) echo "aarch64" ;;
    *) echo "unsupported"; return 1 ;;
  esac
}

OS="$(detect_os)"
ARCH="$(detect_arch)"
ASSET="docsync-${ARCH}-${OS}.tar.gz"

if [[ "$VERSION" == "latest" ]]; then
  URL="https://github.com/${OWNER}/${REPO}/releases/latest/download/${ASSET}"
else
  URL="https://github.com/${OWNER}/${REPO}/releases/download/${VERSION}/${ASSET}"
fi

mkdir -p "$INSTALL_DIR"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$URL" -o "$TMP_DIR/$ASSET"
tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
install -m 0755 "$TMP_DIR/docsync" "$INSTALL_DIR/docsync"

echo "Installed docsync to $INSTALL_DIR/docsync"
