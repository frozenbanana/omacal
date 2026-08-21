#!/usr/bin/env bash
# Install Omacal locally for Omarchy / Arch
# Usage: ./packaging/install.sh [--skip-build] [--help]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

SKIP_BUILD=0
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    --help|-h)
      echo "Usage: $0 [--skip-build]"
      echo "  --skip-build  Reuse existing src-tauri/target/release/omacal (no npm/tauri build)"
      exit 0
      ;;
    *) echo "Unknown arg: $arg" >&2; exit 1 ;;
  esac
done

if [[ $SKIP_BUILD -eq 0 ]]; then
  echo "==> Building Omacal"
  npm install
  # Always use `tauri build` so the UI is embedded (plain `cargo build` points at localhost:1420)
  npx tauri build --bundles deb
else
  echo "==> Skipping build (--skip-build)"
fi

BIN="$ROOT/src-tauri/target/release/omacal"
install -Dm755 "$BIN" "$HOME/.local/bin/omacal"
# Alias for 1 release (omarcal → omacal)
ln -sf omacal "$HOME/.local/bin/omarcal"
install -Dm755 "$ROOT/packaging/omarchy-omacal" "$HOME/.local/bin/omarchy-omacal"
ln -sf omarchy-omacal "$HOME/.local/bin/omarchy-omarcal"
install -Dm755 "$ROOT/packaging/omacal-waybar" "$HOME/.local/bin/omacal-waybar"
ln -sf omacal-waybar "$HOME/.local/bin/omarcal-waybar"
install -Dm644 "$ROOT/packaging/omacal.desktop" "$HOME/.local/share/applications/omacal.desktop"
ln -sf omacal.desktop "$HOME/.local/share/applications/omarcal.desktop" 2>/dev/null || cp "$HOME/.local/share/applications/omacal.desktop" "$HOME/.local/share/applications/omarcal.desktop"
# Hyprland — install both legacy conf and Lua (Lua preferred on Omarchy Quattro+)
install -Dm644 "$ROOT/packaging/hypr/omacal.conf" "$HOME/.config/hypr/omacal.conf"
if [[ -f "$ROOT/packaging/hypr/omacal.lua" ]]; then
  mkdir -p "$HOME/.config/hypr/apps"
  install -Dm644 "$ROOT/packaging/hypr/omacal.lua" "$HOME/.config/hypr/apps/omacal.lua" 2>/dev/null || true
fi
mkdir -p "$HOME/.config/systemd/user"
install -Dm644 "$ROOT/packaging/systemd/omacal-daemon.service" "$HOME/.config/systemd/user/omacal-daemon.service"
# Canonical name alias
install -Dm644 "$ROOT/packaging/systemd/omacal.service" "$HOME/.config/systemd/user/omacal.service" 2>/dev/null || cp "$HOME/.config/systemd/user/omacal-daemon.service" "$HOME/.config/systemd/user/omacal.service"
# Legacy alias (1 release)
ln -sf omacal.service "$HOME/.config/systemd/user/omarcal.service" 2>/dev/null || cp "$HOME/.config/systemd/user/omacal.service" "$HOME/.config/systemd/user/omarcal.service"
ln -sf omacal-daemon.service "$HOME/.config/systemd/user/omarcal-daemon.service" 2>/dev/null || true

# Point desktop entry at local bin (idempotent)
if grep -q "^Exec=omacal " "$HOME/.local/share/applications/omacal.desktop" 2>/dev/null; then
  sed -i "s|^Exec=omacal |Exec=$HOME/.local/bin/omacal |" "$HOME/.local/share/applications/omacal.desktop"
elif grep -q "^Exec=$HOME/.local/bin/omacal" "$HOME/.local/share/applications/omacal.desktop" 2>/dev/null; then
  : # already patched
else
  sed -i "s|^Exec=omacal|Exec=$HOME/.local/bin/omacal|" "$HOME/.local/share/applications/omacal.desktop" 2>/dev/null || true
fi

# Install app icons into hicolor so launchers show the icon (alias for 1 release)
ICON_ROOT="$ROOT/src-tauri/icons"
install -Dm644 "$ICON_ROOT/32x32.png" "$HOME/.local/share/icons/hicolor/32x32/apps/omacal.png"
install -Dm644 "$ICON_ROOT/128x128.png" "$HOME/.local/share/icons/hicolor/128x128/apps/omacal.png"
install -Dm644 "$ICON_ROOT/128x128@2x.png" "$HOME/.local/share/icons/hicolor/256x256/apps/omacal.png"
install -Dm644 "$ICON_ROOT/icon.png" "$HOME/.local/share/icons/hicolor/512x512/apps/omacal.png"
for sz in 32x32 128x128 256x256 512x512; do
  ln -sf omacal.png "$HOME/.local/share/icons/hicolor/$sz/apps/omarcal.png" 2>/dev/null || cp "$HOME/.local/share/icons/hicolor/$sz/apps/omacal.png" "$HOME/.local/share/icons/hicolor/$sz/apps/omarcal.png"
done
ln -sf omacal.png "$HOME/.local/share/icons/hicolor/scalable/apps/omacal.png" 2>/dev/null || true
ln -sf omacal.png "$HOME/.local/share/icons/hicolor/scalable/apps/omarcal.png" 2>/dev/null || true

# Refresh MIME / desktop / icon databases and make Omacal the .ics handler
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$HOME/.local/share/applications" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v xdg-mime >/dev/null 2>&1; then
  xdg-mime default omacal.desktop text/calendar || true
fi

echo ""
echo "Installed:"
echo "  ~/.local/bin/omacal"
echo "  ~/.local/bin/omarchy-omacal"
echo "  ~/.local/bin/omacal-waybar"
echo "  ~/.local/share/applications/omacal.desktop"
echo "  ~/.local/share/icons/hicolor/{32,128,256,512}x*/apps/omacal.png"
echo "  ~/.config/hypr/omacal.conf (+ apps/omacal.lua)"
echo "  ~/.config/systemd/user/omacal.service (alias omacal-daemon.service)"
echo ""
echo "Optional:"
echo "  # Hyprland Lua (Omarchy Quattro+): already in ~/.config/hypr/apps/omacal.lua"
echo "  # Legacy: source = ~/.config/hypr/omacal.conf"
echo "  # Quickshell bar widget: cp -r packaging/quickshell/henry.omacal ~/.config/omarchy/plugins/"
echo "  # App menu: merge packaging/omarchy-menu.jsonc into ~/.config/omarchy/extensions/omarchy-menu.jsonc"
echo "  # Waybar (legacy): add custom/omacal (see packaging/waybar/omacal.jsonc)"
echo "  systemctl --user enable --now omacal.service  # or omacal-daemon.service"
echo ""
echo "To uninstall: ./packaging/uninstall.sh"
echo ""
echo "IMPORTANT: disable vdirsyncer pairs for calendars you sync in Omacal."
