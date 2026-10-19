#!/bin/sh
# Builds Budlink and installs it for the current user (binary, menu entry and icons).
#
#   ./install.sh              install into ~/.local
#   ./install.sh --uninstall  remove it again
#
# Set PREFIX to install somewhere else (e.g. PREFIX=/usr/local, run with sudo).

set -eu

APP_ID=io.github.neptunonet.Budlink
PREFIX=${PREFIX:-$HOME/.local}

BIN=$PREFIX/bin/budlink
DESKTOP=$PREFIX/share/applications/$APP_ID.desktop
ICONS=$PREFIX/share/icons/hicolor
ICON=$ICONS/scalable/apps/$APP_ID.svg
SYMBOLIC=$ICONS/symbolic/apps/$APP_ID-symbolic.svg

refresh_caches() {
    gtk-update-icon-cache -q -t -f "$ICONS" 2>/dev/null || true
    update-desktop-database -q "$PREFIX/share/applications" 2>/dev/null || true
}

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$BIN" "$DESKTOP" "$ICON" "$SYMBOLIC"
    refresh_caches
    echo "Budlink removed from $PREFIX"
    exit 0
fi

cd "$(dirname "$0")"

cargo build --release

install -Dm755 target/release/budlink "$BIN"
install -Dm644 "data/icons/$APP_ID.svg" "$ICON"
install -Dm644 "data/icons/$APP_ID-symbolic.svg" "$SYMBOLIC"

# Point the menu entry at the installed binary, so it works even when
# $PREFIX/bin is not in the PATH of the desktop session.
mkdir -p "$(dirname "$DESKTOP")"
sed "s|^Exec=budlink|Exec=$BIN|" "data/$APP_ID.desktop" > "$DESKTOP"
chmod 644 "$DESKTOP"

refresh_caches
echo "Budlink installed in $PREFIX. Look for it in the app menu."
