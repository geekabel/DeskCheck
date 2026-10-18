#!/bin/sh
# Removes what data/install-dev.sh installed for your user: the .desktop
# file and the D-Bus service of DeskCheck. Your settings
# (~/.config/deskcheck) and the build in target/ are kept.
#
#   ./data/uninstall-dev.sh
set -eu

ID=io.github.geekabel.DeskCheck
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"

removed=0
for file in "$DATA/applications/$ID.desktop" "$DATA/dbus-1/services/$ID.service"; do
    if [ -e "$file" ]; then
        rm -f "$file"
        echo "Removed $file"
        removed=1
    fi
done
if [ "$removed" -eq 0 ]; then
    echo "Nothing to remove: DeskCheck was not installed with install-dev.sh."
fi
