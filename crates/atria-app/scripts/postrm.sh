#!/bin/sh
# Refresh caches after removal so stale desktop entries/icons disappear.
set -e
if [ "$1" = "remove" ] || [ "$1" = "deconfigure" ]; then
  update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
  gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
exit 0
