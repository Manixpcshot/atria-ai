#!/bin/sh
# Refresh the desktop-menu and icon caches after install.
# (Icons live under the hicolor theme; without this, some panels show a
# stale/missing taskbar icon until the caches happen to be rebuilt.)
set -e
update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
exit 0
