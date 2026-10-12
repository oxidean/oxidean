#!/bin/sh
# ox — Oxidean CLI installer. Served by the instance itself:
#   curl -fsSL @OXIDEAN_ORIGIN@/cli/install.sh | sh
set -eu

BASE="@OXIDEAN_ORIGIN@"
DEST="${OXIDEAN_INSTALL_DIR:-$HOME/.local/bin}"
# Binaries are served by the instance at $BASE/cli/bin; point OXIDEAN_BIN_BASE
# at any flat mirror instead (e.g. a GitHub release's download URL).
BIN_BASE="${OXIDEAN_BIN_BASE:-$BASE/cli/bin}"

have() { command -v "$1" >/dev/null 2>&1; }

fetch() {
    if have curl; then curl -fsSL "$1"; elif have wget; then wget -qO- "$1"; else
        echo "error: need curl or wget" >&2; exit 1; fi
}

os=$(uname -s | tr '[:upper:]' '[:lower:]')
arch=$(uname -m)
exe=""
case "$os" in
    linux)
        # musl systems (Alpine, Void-musl, …) need the musl build — the gnu
        # binary won't run there. `ldd --version` reports musl; fall back to
        # the loader path when ldd is absent.
        if { ldd --version 2>&1 || true; } | grep -qi musl \
            || ls /lib/ld-musl-* >/dev/null 2>&1; then
            os_part=unknown-linux-musl
        else
            os_part=unknown-linux-gnu
        fi
        ;;
    darwin)  os_part=apple-darwin ;;
    freebsd) os_part=unknown-freebsd ;;
    mingw*|msys*|cygwin*)
        # Git Bash / MSYS2 / Cygwin — the native Windows build works here.
        os_part=pc-windows-msvc; exe=".exe" ;;
    *)
        echo "error: unsupported OS '$os' — on native Windows use the PowerShell installer:" >&2
        echo "  irm $BASE/cli/install.ps1 | iex" >&2
        echo "or build from source: cargo install --git <repo> oxidean-cli" >&2
        exit 1
        ;;
esac
case "$arch" in
    x86_64|amd64)  arch_part=x86_64 ;;
    aarch64|arm64) arch_part=aarch64 ;;
    armv7l|armv7)  arch_part=armv7 ;;
    *) echo "error: unsupported architecture '$arch'" >&2; exit 1 ;;
esac

if [ "$arch_part" = armv7 ] && [ "$os" = linux ]; then
    # Rust's armv7 Linux triples carry an ABI suffix the mapping must keep.
    if [ "$os_part" = unknown-linux-musl ]; then
        TARGET="armv7-unknown-linux-musleabihf"
    else
        TARGET="armv7-unknown-linux-gnueabihf"
    fi
else
    TARGET="$arch_part-$os_part"
fi

echo "installing ox for $TARGET from $BASE"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
BIN="ox$exe"

if ! fetch "$BIN_BASE/ox-$TARGET" > "$tmp/$BIN"; then
    echo "error: no prebuilt ox for $TARGET at $BIN_BASE." >&2
    echo "build from source instead: cargo install --git <repo> oxidean-cli" >&2
    exit 1
fi
fetch "$BIN_BASE/ox-$TARGET.sha256" > "$tmp/ox.sha256" || true

expected=$(awk '{print $1}' "$tmp/ox.sha256" 2>/dev/null || true)
if [ -n "$expected" ]; then
    if have sha256sum; then actual=$(sha256sum "$tmp/$BIN" | awk '{print $1}');
    elif have shasum; then actual=$(shasum -a 256 "$tmp/$BIN" | awk '{print $1}');
    elif have openssl; then actual=$(openssl dgst -sha256 "$tmp/$BIN" | awk '{print $NF}');
    else actual=""; fi
    if [ -n "$actual" ] && [ "$actual" != "$expected" ]; then
        echo "error: checksum mismatch — aborting" >&2; exit 1
    fi
fi

chmod +x "$tmp/$BIN"
mkdir -p "$DEST"
if ! mv "$tmp/$BIN" "$DEST/$BIN" 2>/dev/null; then
    echo "cannot write $DEST — retry with: sudo mv $tmp/$BIN /usr/local/bin/$BIN" >&2
    exit 1
fi

echo "installed ox to $DEST/$BIN"
"$DEST/$BIN" --version || true
case ":$PATH:" in
    *":$DEST:"*) ;;
    *) echo "note: $DEST is not on your PATH" ;;
esac
echo "next: ox auth login --instance $BASE --web"
echo "  or: ox auth login --instance $BASE --token <pat>"
