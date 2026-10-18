#!/bin/sh
# Installs the .desktop file and the D-Bus service of DeskCheck for your
# user, pointing at the app built in this repository. GNOME needs them to
# show the notifications of DeskCheck (with their Undo button), and to
# list DeskCheck in Settings > Notifications.
#
#   ./data/install-dev.sh            build (release) and install
#   ./data/install-dev.sh --remove   remove them
set -eu

ID=io.github.geekabel.DeskCheck
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(dirname "$HERE")"

if [ "${1:-}" = "--remove" ]; then
    rm -f "$DATA/applications/$ID.desktop" "$DATA/dbus-1/services/$ID.service"
    echo "Removed $ID.desktop and $ID.service."
    exit 0
fi

cargo build --release -p deskcheck-app --manifest-path "$ROOT/Cargo.toml"
BINDIR="$ROOT/target/release"

mkdir -p "$DATA/applications" "$DATA/dbus-1/services"
sed "s#^Exec=deskcheck-app#Exec=$BINDIR/deskcheck-app#" "$HERE/$ID.desktop" \
    > "$DATA/applications/$ID.desktop"
sed "s#@BINDIR@#$BINDIR#" "$HERE/$ID.service.in" > "$DATA/dbus-1/services/$ID.service"
if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DATA/applications/$ID.desktop"
fi
echo "Installed $ID.desktop and $ID.service. Start DeskCheck from the Activities."
