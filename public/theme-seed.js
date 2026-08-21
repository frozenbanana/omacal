// Seed Omarchy (Nord-default) theme vars before Vite loads to avoid a flash.
// The Rust get_snapshot theme later overrides these precisely.
(function () {
  var dark =
    !window.matchMedia || !window.matchMedia("(prefers-color-scheme: light)").matches;
  var root = document.documentElement;
  if (dark) {
    root.style.setProperty("--bg", "#2e3440");
    root.style.setProperty("--fg", "#d8dee9");
    root.style.setProperty("--accent", "#81a1c1");
    root.style.setProperty("--muted", "#4c566a");
    root.style.colorScheme = "dark";
  } else {
    root.style.setProperty("--bg", "#eff1f5");
    root.style.setProperty("--fg", "#4c4f69");
    root.style.setProperty("--accent", "#1e66f5");
    root.style.setProperty("--muted", "#9ca0b0");
    root.style.colorScheme = "light";
  }
})();
