#!/bin/sh
# Installs the .desktop file and the D-Bus service of DeskCheck for your
# user, pointing at the app built in this repository. GNOME needs them to
# show the notifications of DeskCheck (with their Undo button), and to
# list DeskCheck in Settings > Notifications.
#
#   ./data/install-dev.sh            build (release) and install
#   ./data/uninstall-dev.sh          remove them (or install-dev.sh --remove)
set -eu

ID=io.github.geekabel.DeskCheck
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(dirname "$HERE")"

if [ "${1:-}" = "--remove" ]; then
    exec "$HERE/uninstall-dev.sh"
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
echo "Installed $DATA/applications/$ID.desktop"
echo "Installed $DATA/dbus-1/services/$ID.service"
echo "Start DeskCheck from the Activities. To remove: data/uninstall-dev.sh"
