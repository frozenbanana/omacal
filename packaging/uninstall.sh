#!/usr/bin/env bash
# Uninstall Omacal local files installed by packaging/install.sh
set -euo pipefail

echo "==> Stopping user service (if running)"
systemctl --user disable --now omacal.service 2>/dev/null || true
systemctl --user disable --now omacal-daemon.service 2>/dev/null || true
systemctl --user disable --now omarcal.service 2>/dev/null || true
systemctl --user disable --now omarcal-daemon.service 2>/dev/null || true

echo "==> Removing files"
rm -f "$HOME/.local/bin/omacal" "$HOME/.local/bin/omarcal"
rm -f "$HOME/.local/bin/omarchy-omacal" "$HOME/.local/bin/omarchy-omarcal"
rm -f "$HOME/.local/bin/omacal-waybar" "$HOME/.local/bin/omarcal-waybar"
rm -f "$HOME/.local/share/applications/omacal.desktop" "$HOME/.local/share/applications/omarcal.desktop"
rm -f "$HOME/.local/share/icons/hicolor/32x32/apps/omacal.png" "$HOME/.local/share/icons/hicolor/32x32/apps/omarcal.png"
rm -f "$HOME/.local/share/icons/hicolor/128x128/apps/omacal.png" "$HOME/.local/share/icons/hicolor/128x128/apps/omarcal.png"
rm -f "$HOME/.local/share/icons/hicolor/256x256/apps/omacal.png" "$HOME/.local/share/icons/hicolor/256x256/apps/omarcal.png"
rm -f "$HOME/.local/share/icons/hicolor/512x512/apps/omacal.png" "$HOME/.local/share/icons/hicolor/512x512/apps/omarcal.png"
rm -f "$HOME/.local/share/icons/hicolor/scalable/apps/omacal.png" "$HOME/.local/share/icons/hicolor/scalable/apps/omarcal.png"
rm -f "$HOME/.config/hypr/omacal.conf" "$HOME/.config/hypr/omarcal.conf"
rm -f "$HOME/.config/hypr/apps/omacal.lua" "$HOME/.config/hypr/apps/omarcal.lua"
rm -f "$HOME/.config/systemd/user/omacal.service" "$HOME/.config/systemd/user/omarcal.service"
rm -f "$HOME/.config/systemd/user/omacal-daemon.service" "$HOME/.config/systemd/user/omarcal-daemon.service"
# Quickshell widget (if copied by user)
# Leave ~/.config/omarchy/plugins/henry.omacal untouched unless --full
if [[ "${1:-}" == "--full" ]]; then
  rm -rf "$HOME/.config/omarchy/plugins/henry.omacal"
  echo "Removed Quickshell widget ~/.config/omarchy/plugins/henry.omacal (--full)"
fi

echo "==> Refreshing caches"
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v update-mime-database >/dev/null 2>&1; then
  update-mime-database "$HOME/.local/share/mime" >/dev/null 2>&1 || true
fi

# Keep DB/config by default (user data)
echo ""
echo "Removed Omacal binaries and integration files."
echo "Kept: ~/.config/omacal/config.toml and ~/.local/share/omacal/omacal.db"
echo "To also remove data: rm -rf ~/.config/omacal ~/.local/share/omacal && rm -f ~/.local/share/icons/hicolor/*/apps/omacal.png"
echo "System package: sudo pacman -R omacal (if installed via pacman)"
