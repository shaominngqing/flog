#!/bin/bash
# =============================================================================
# flog — One-line Installer
# =============================================================================
# Install:
#   curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | bash
#
# Install to a specific directory:
#   curl -fsSL https://raw.githubusercontent.com/shaominngqing/flog/master/install.sh | FLOG_INSTALL_DIR=/usr/local/bin bash
#
# Uninstall:
#   rm $(which flog)
# =============================================================================

set -euo pipefail

FLOG_VERSION="0.5.2"
REPO="shaominngqing/flog"

NC='\033[0m'; BOLD='\033[1m'; DIM='\033[2m'
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
C1='\033[38;5;39m'; C2='\033[38;5;45m'

ok()   { echo -e "  ${GREEN}✓${NC} $1"; }
warn() { echo -e "  ${YELLOW}⚠${NC} $1"; }
fail() { echo -e "  ${RED}✗${NC} $1"; exit 1; }
step() { echo -e "\n  ${C1}${BOLD}▸${NC} ${BOLD}$1${NC}"; }

_gradient() {
    local text="$1" start="${2:-39}" count="${3:-6}"
    for ((i=0; i<${#text}; i++)); do
        local c=$(( start + (i % count) ))
        printf "\033[1;38;5;${c}m%s" "${text:$i:1}"
    done
    printf "${NC}"
}

# ── Banner ──
echo ""
{
cat << 'BANNER'
███████╗██╗      ██████╗  ██████╗
██╔════╝██║     ██╔═══██╗██╔════╝
█████╗  ██║     ██║   ██║██║  ███╗
██╔══╝  ██║     ██║   ██║██║   ██║
██║     ███████╗╚██████╔╝╚██████╔╝
╚═╝     ╚══════╝ ╚═════╝  ╚═════╝
BANNER
} | while IFS= read -r line; do
    printf "  "
    _gradient "$line" 39 6
    printf "\n"
done
echo ""
printf "    ${DIM}Flutter Log Viewer — see your logs, finally.${NC}\n"
echo ""

# ── Select version ──
step "Select release"

VERSION="${FLOG_VERSION_OVERRIDE:-$FLOG_VERSION}"
if [[ "$VERSION" == v* ]]; then
    LATEST_TAG="$VERSION"
    VERSION="${VERSION#v}"
else
    LATEST_TAG="v${VERSION}"
fi
ok "v${VERSION}"

# ── Detect platform ──
step "Detect platform"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    darwin) OS_LABEL="macOS"; OS_NAME="macos" ;;
    linux)  OS_LABEL="Linux"; OS_NAME="linux" ;;
    mingw*|msys*|cygwin*) OS_LABEL="Windows"; OS_NAME="windows" ;;
    *) fail "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH_NAME="x86_64" ;;
    aarch64|arm64) ARCH_NAME="aarch64" ;;
    *) fail "Unsupported architecture: $ARCH" ;;
esac

ok "$OS_LABEL $ARCH_NAME"

# ── Download binary ──
step "Download flog binary"

ASSET_NAME="flog-${OS_NAME}-${ARCH_NAME}"
if [ "$OS_NAME" = "windows" ]; then
    ASSET_NAME="${ASSET_NAME}.zip"
else
    ASSET_NAME="${ASSET_NAME}.tar.gz"
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ASSET_NAME}"

TMP_DIR=$(mktemp -d)
TMP_ARCHIVE="${TMP_DIR}/${ASSET_NAME}"
trap "rm -rf '$TMP_DIR'" EXIT

# Progress bar
_draw_progress() {
    local cur="$1" total="$2" width=32
    local pct=0 filled=0 empty="$width"
    if [ "$total" -gt 0 ]; then
        pct=$(( cur * 100 / total ))
        filled=$(( cur * width / total ))
        empty=$(( width - filled ))
    fi
    local size_mb total_mb
    size_mb=$(echo "scale=1; $cur / 1048576" | bc 2>/dev/null || echo "?")
    total_mb=$(echo "scale=1; $total / 1048576" | bc 2>/dev/null || echo "?")
    local bar="" i
    for ((i=0; i<filled; i++)); do bar="${bar}\033[38;5;39m━"; done
    for ((i=0; i<empty;  i++)); do bar="${bar}\033[2m━"; done
    printf "\r  ${bar}${NC} ${DIM}%s/%sMB${NC} %3d%%" "$size_mb" "$total_mb" "$pct" >&2
}

DOWNLOADED=false
CURL_RETRY=(--retry 5 --retry-delay 1 --connect-timeout 15)
if curl --help all 2>/dev/null | grep -q -- '--retry-all-errors'; then
    CURL_RETRY+=(--retry-all-errors)
fi

if command -v curl >/dev/null 2>&1; then
    TOTAL_SIZE=$(curl -fLIs "${CURL_RETRY[@]}" "$DOWNLOAD_URL" 2>/dev/null \
        | awk 'tolower($1) == "content-length:" { gsub("\r", "", $2); len = $2 } END { if (len != "") print len; else print 0 }' \
        || true)
    TOTAL_SIZE="${TOTAL_SIZE:-0}"

    if [ "$TOTAL_SIZE" -gt 0 ]; then
        _draw_progress 0 "$TOTAL_SIZE"
        curl -fL "${CURL_RETRY[@]}" "$DOWNLOAD_URL" -o "$TMP_ARCHIVE" 2>/dev/null &
        DL_PID=$!
        while kill -0 "$DL_PID" 2>/dev/null; do
            if [ -f "$TMP_ARCHIVE" ]; then
                CUR_SIZE=$(wc -c < "$TMP_ARCHIVE" 2>/dev/null | tr -d ' ')
                _draw_progress "${CUR_SIZE:-0}" "$TOTAL_SIZE"
            fi
            sleep 0.15
        done
        wait "$DL_PID" && DOWNLOADED=true || true
        if [ "$DOWNLOADED" = true ] && [ -s "$TMP_ARCHIVE" ]; then
            _draw_progress "$TOTAL_SIZE" "$TOTAL_SIZE"
        fi
        printf "\n" >&2
    else
        warn "Could not determine download size; downloading without progress..."
        if curl -fL "${CURL_RETRY[@]}" --progress-bar "$DOWNLOAD_URL" -o "$TMP_ARCHIVE"; then
            DOWNLOADED=true
        fi
    fi
elif command -v wget >/dev/null 2>&1; then
    if wget -q "$DOWNLOAD_URL" -O "$TMP_ARCHIVE" 2>/dev/null; then
        DOWNLOADED=true
    fi
fi

if [ "$DOWNLOADED" != true ] || [ ! -s "$TMP_ARCHIVE" ]; then
    if [ "${FLOG_BUILD_FROM_SOURCE:-}" != "1" ]; then
        fail "Failed to download ${ASSET_NAME}. Check network access to github.com, then retry. To build from source instead, run with FLOG_BUILD_FROM_SOURCE=1."
    fi

    warn "Pre-built binary download failed; building from source..."

    if ! command -v cargo >/dev/null 2>&1; then
        fail "Rust toolchain required. Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    fi

    step "Build from source"
    BUILD_DIR="${TMP_DIR}/flog-src"
    if command -v git >/dev/null 2>&1; then
        git clone --depth 1 --branch "$LATEST_TAG" "https://github.com/${REPO}.git" "$BUILD_DIR"
    else
        curl -fsSL "https://github.com/${REPO}/archive/${LATEST_TAG}.tar.gz" | tar -xz -C "$TMP_DIR"
        BUILD_DIR="${TMP_DIR}/flog-${VERSION}"
    fi
    (cd "$BUILD_DIR" && cargo build --release 2>&1 | tail -3)
    cp "$BUILD_DIR/target/release/flog" "${TMP_DIR}/flog"
    chmod +x "${TMP_DIR}/flog"
    ok "Built from source"
else
    # Extract
    if [ "$OS_NAME" = "windows" ]; then
        (cd "$TMP_DIR" && unzip -q "$TMP_ARCHIVE")
    else
        tar xzf "$TMP_ARCHIVE" -C "$TMP_DIR"
    fi
    chmod +x "${TMP_DIR}/flog"

    FILESIZE=$(wc -c < "${TMP_DIR}/flog" | tr -d ' ')
    if [ "$FILESIZE" -ge 1048576 ]; then
        SIZE_LABEL="$(echo "scale=1; $FILESIZE / 1048576" | bc)MB"
    else
        SIZE_LABEL="$(echo "scale=0; $FILESIZE / 1024" | bc)KB"
    fi
    ok "flog v${VERSION} (${SIZE_LABEL})"
fi

# ── Install binary ──
step "Install binary"

INSTALL_DIR=""
ACTIVE_FLOG=$(command -v flog 2>/dev/null || true)

if [ -n "${FLOG_INSTALL_DIR:-}" ]; then
    INSTALL_DIR="$FLOG_INSTALL_DIR"
elif [ -n "$ACTIVE_FLOG" ] && [ "${ACTIVE_FLOG#/}" != "$ACTIVE_FLOG" ]; then
    INSTALL_DIR=$(dirname "$ACTIVE_FLOG")
elif [ -d /opt/homebrew/bin ] && echo "$PATH" | grep -q "/opt/homebrew/bin"; then
    INSTALL_DIR="/opt/homebrew/bin"
elif [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    INSTALL_DIR="/usr/local/bin"
elif [ -d "$HOME/.local/bin" ]; then
    INSTALL_DIR="$HOME/.local/bin"
else
    mkdir -p "$HOME/.local/bin"
    INSTALL_DIR="$HOME/.local/bin"
fi

mkdir -p "$INSTALL_DIR"

if cp "${TMP_DIR}/flog" "$INSTALL_DIR/flog" 2>/dev/null; then
    ok "flog → $INSTALL_DIR/"
elif sudo cp "${TMP_DIR}/flog" "$INSTALL_DIR/flog" 2>/dev/null; then
    ok "flog → $INSTALL_DIR/ (sudo)"
else
    mkdir -p "$HOME/.local/bin"
    cp "${TMP_DIR}/flog" "$HOME/.local/bin/flog"
    INSTALL_DIR="$HOME/.local/bin"
    ok "flog → $INSTALL_DIR/"
    if ! echo "$PATH" | grep -q "$HOME/.local/bin"; then
        warn "Add to PATH: export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
fi

# ── Verify ──
step "Verify"

INSTALLED_BIN="$INSTALL_DIR/flog"
INSTALLED_VERSION=$("$INSTALLED_BIN" --version 2>/dev/null || true)

if printf "%s" "$INSTALLED_VERSION" | grep -q "$VERSION"; then
    ok "$INSTALLED_VERSION at $INSTALLED_BIN"
else
    warn "Installed to $INSTALLED_BIN, but version check did not return v${VERSION}"
fi

ACTIVE_FLOG_AFTER=$(command -v flog 2>/dev/null || true)
if [ -n "$ACTIVE_FLOG_AFTER" ] && [ "$ACTIVE_FLOG_AFTER" != "$INSTALLED_BIN" ]; then
    warn "Your shell currently resolves flog to: $ACTIVE_FLOG_AFTER"
    warn "Run this directly to verify: $INSTALLED_BIN --version"
    warn "Adjust PATH or remove the older binary if needed."
elif ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
    warn "Run: export PATH=\"$INSTALL_DIR:\$PATH\""
fi

# ── Done ──
echo ""
echo -e "  ${GREEN}${BOLD}Done!${NC} Run ${C1}${BOLD}flog${NC} to start."
echo -e "  ${DIM}Then run ${NC}flutter run${DIM} in another terminal.${NC}"
echo ""
