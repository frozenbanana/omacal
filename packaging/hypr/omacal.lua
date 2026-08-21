-- Omacal — Omarchy Hyprland floating rule (Lua, preferred over omacal.conf)
-- Installed to /usr/share/omarchy/default/hypr/apps/omacal.lua by PKGBUILD.
-- For user override, copy to ~/.config/hypr/looknfeel.lua or apps/.
return {
  window_rule = {
    { class = "^omacal$", float = true, size = "1280 800", center = true },
  },
}
