#!/usr/bin/env bash
# Install Omarcal locally for Omarchy / Arch
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> Building Omarcal"
npm install
# Always use `tauri build` so the UI is embedded (plain `cargo build` points at localhost:1420)
npx tauri build --bundles deb

BIN="$ROOT/src-tauri/target/release/omarcal"
install -Dm755 "$BIN" "$HOME/.local/bin/omarcal"
install -Dm755 "$ROOT/packaging/omarchy-omarcal" "$HOME/.local/bin/omarchy-omarcal"
install -Dm755 "$ROOT/packaging/omarcal-waybar" "$HOME/.local/bin/omarcal-waybar"
install -Dm644 "$ROOT/packaging/omarcal.desktop" "$HOME/.local/share/applications/omarcal.desktop"
install -Dm644 "$ROOT/packaging/hypr/omarcal.conf" "$HOME/.config/hypr/omarcal.conf"
mkdir -p "$HOME/.config/systemd/user"
install -Dm644 "$ROOT/packaging/systemd/omarcal-daemon.service" "$HOME/.config/systemd/user/omarcal-daemon.service"

# Point desktop entry at local bin
sed -i "s|^Exec=omarcal|Exec=$HOME/.local/bin/omarcal|" "$HOME/.local/share/applications/omarcal.desktop"

echo ""
echo "Installed:"
echo "  ~/.local/bin/omarcal"
echo "  ~/.local/bin/omarchy-omarcal"
echo "  ~/.local/bin/omarcal-waybar"
echo "  ~/.local/share/applications/omarcal.desktop"
echo ""
echo "Optional:"
echo "  source packaging/hypr/omarcal.conf from hyprland.conf"
echo "  add custom/omarcal waybar module (see packaging/waybar/omarcal.jsonc)"
echo "  systemctl --user enable --now omarcal-daemon.service"
echo ""
echo "IMPORTANT: disable vdirsyncer pairs for calendars you sync in Omarcal."
