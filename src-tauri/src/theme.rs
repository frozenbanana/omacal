use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeColors {
    pub accent: String,
    pub foreground: String,
    pub background: String,
    pub cursor: String,
    pub selection_foreground: String,
    pub selection_background: String,
    pub muted: String,
    pub colors: Vec<String>,
    pub light: bool,
    pub name: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            accent: "#82FB9C".into(),
            foreground: "#ddf7ff".into(),
            background: "#0B0C16".into(),
            cursor: "#ddf7ff".into(),
            selection_foreground: "#0B0C16".into(),
            selection_background: "#ddf7ff".into(),
            muted: "#4c566a".into(),
            colors: (0..16).map(|i| format!("#{i:02x}{i:02x}{i:02x}")).collect(),
            light: false,
            name: "default".into(),
        }
    }
}

/// Candidate theme directories, newest Omarchy layout first.
///
/// Omarchy Quattro+ writes the live theme to `~/.local/state/omarchy/current/theme`
/// (with `theme.name` and `mode = "dark"|"light"` inside `colors.toml`). Older
/// Omarchy used `~/.config/omarchy/current/theme` with a `light.mode` file and
/// `color0..15` keys. We try the state path first, then fall back to the legacy
/// config path so both are supported.
fn omarchy_theme_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(state) = dirs::state_dir() {
        paths.push(state.join("omarchy/current/theme"));
    }
    if let Some(config) = dirs::config_dir() {
        paths.push(config.join("omarchy/current/theme"));
    }
    paths
}

fn omarchy_theme_name_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(state) = dirs::state_dir() {
        paths.push(state.join("omarchy/current/theme.name"));
    }
    if let Some(config) = dirs::config_dir() {
        paths.push(config.join("omarchy/current/theme.name"));
    }
    paths
}

fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.exists()).cloned()
}

fn read_str(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub fn load_omarchy_theme() -> ThemeColors {
    let mut name = None;
    for name_path in omarchy_theme_name_paths() {
        if let Some(n) = read_str(&name_path).map(|s| s.trim().to_string()) {
            name = Some(n);
            break;
        }
    }
    match first_existing(&omarchy_theme_paths()) {
        Some(dir) => parse_theme(&dir, name.as_deref()),
        None => {
            let mut theme = ThemeColors::default();
            if let Some(n) = name {
                theme.name = n;
            }
            theme
        }
    }
}

fn parse_theme(dir: &Path, name: Option<&str>) -> ThemeColors {
    let mut theme = ThemeColors::default();
    if let Some(n) = name {
        theme.name = n.to_string();
    }

    let colors_path = dir.join("colors.toml");
    let text = match read_str(&colors_path) {
        Some(t) => t,
        None => return theme,
    };
    let val: toml::Value = match text.parse() {
        Ok(v) => v,
        Err(_) => return theme,
    };
    let table = match val.as_table() {
        Some(t) => t,
        None => return theme,
    };

    // Light mode: new Omarchy uses `mode = "light"`, legacy used a `light.mode` file.
    theme.light = match table.get("mode").and_then(|v| v.as_str()) {
        Some("light") => true,
        _ => dir.join("light.mode").exists(),
    };

    let get = |key: &str| table.get(key).and_then(|v| v.as_str()).map(String::from);

    if let Some(v) = get("accent") {
        theme.accent = v;
    }
    if let Some(v) = get("foreground") {
        theme.foreground = v;
    }
    if let Some(v) = get("background") {
        theme.background = v;
    }
    // cursor: legacy explicit key, else fall back to bright_foreground / foreground.
    if let Some(v) = get("cursor") {
        theme.cursor = v;
    } else if let Some(v) = get("bright_foreground") {
        theme.cursor = v;
    } else if let Some(v) = get("foreground") {
        theme.cursor = v;
    }
    // selection: legacy selection_foreground/background, else Omarchy `selection` key
    // (used as the selection background).
    if let Some(v) = get("selection_foreground") {
        theme.selection_foreground = v;
    } else {
        theme.selection_foreground = theme.background.clone();
    }
    if let Some(v) = get("selection_background") {
        theme.selection_background = v;
    } else if let Some(v) = get("selection") {
        theme.selection_background = v;
    }
    if let Some(v) = get("muted") {
        theme.muted = v;
    }

    // Prefer explicit color0..15 (legacy), else derive an xterm-ordered palette from
    // the Omarchy semantic keys so calendar colors come out as expected.
    let mut colors: Vec<String> = Vec::new();
    for i in 0..16 {
        let key = format!("color{i}");
        if let Some(v) = get(&key) {
            colors.push(v);
        }
    }
    if colors.len() != 16 {
        colors = derive_palette(&theme, |k| get(k));
    }
    theme.colors = colors;

    theme
}

/// Derive `color0..15` from Omarchy's semantic color keys, xterm ordering.
fn derive_palette(theme: &ThemeColors, get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut c = Vec::with_capacity(16);
    let pick = |primary: &str, fallback: &str| get(primary).or_else(|| get(fallback));
    c.push(theme.background.clone()); // 0
    c.push(pick("red", "color1").unwrap_or_else(|| theme.foreground.clone())); // 1
    c.push(pick("green", "color2").unwrap_or_else(|| theme.foreground.clone())); // 2
    c.push(pick("yellow", "color3").unwrap_or_else(|| theme.foreground.clone())); // 3
    c.push(pick("blue", "accent").unwrap_or_else(|| theme.accent.clone())); // 4
    c.push(pick("magenta", "color5").unwrap_or_else(|| theme.foreground.clone())); // 5
    c.push(pick("cyan", "color6").unwrap_or_else(|| theme.foreground.clone())); // 6
    c.push(theme.foreground.clone()); // 7
    c.push(theme.muted.clone()); // 8
    c.push(pick("bright_red", "red").unwrap_or_else(|| theme.foreground.clone())); // 9
    c.push(pick("bright_green", "green").unwrap_or_else(|| theme.foreground.clone())); // 10
    c.push(pick("bright_yellow", "yellow").unwrap_or_else(|| theme.foreground.clone())); // 11
    c.push(pick("bright_blue", "blue").unwrap_or_else(|| theme.accent.clone())); // 12
    c.push(pick("bright_magenta", "magenta").unwrap_or_else(|| theme.foreground.clone())); // 13
    c.push(pick("bright_cyan", "cyan").unwrap_or_else(|| theme.foreground.clone())); // 14
    c.push(pick("bright_foreground", "foreground").unwrap_or_else(|| theme.foreground.clone())); // 15
    c
}

pub fn start_theme_watcher(app: AppHandle) {
    thread::spawn(move || {
        let paths = omarchy_theme_paths();
        if paths.is_empty() {
            let _ = app.emit("theme-changed", load_omarchy_theme());
            return;
        }
        let (tx, rx) = mpsc::channel();
        let mut watcher: RecommendedWatcher = match Watcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            notify::Config::default().with_poll_interval(Duration::from_secs(2)),
        ) {
            Ok(w) => w,
            Err(_) => return,
        };

        // Watch the theme dir and its parent to catch the atomic `mv next-theme theme`
        // swap and the `colors.toml` write. Watch all candidates so a migration between
        // config and state layouts is still picked up.
        let mut watched = Vec::new();
        for p in &paths {
            if !watched.contains(p) {
                let _ = watcher.watch(p, RecursiveMode::NonRecursive);
                watched.push(p.clone());
            }
            if let Some(parent) = p.parent() {
                let parent = parent.to_path_buf();
                if !watched.contains(&parent) {
                    let _ = watcher.watch(&parent, RecursiveMode::NonRecursive);
                    watched.push(parent);
                }
            }
        }
        let _ = app.emit("theme-changed", load_omarchy_theme());

        let mut last = String::new();
        while let Ok(res) = rx.recv() {
            if res.is_err() {
                continue;
            }
            // debounce
            thread::sleep(Duration::from_millis(150));
            while rx.try_recv().is_ok() {}
            let theme = load_omarchy_theme();
            let sig = format!(
                "{}-{}-{}-{}",
                theme.name, theme.background, theme.accent, theme.light
            );
            if sig != last {
                last = sig;
                let _ = app.emit("theme-changed", theme);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, content: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), content).unwrap();
    }

    const NORD: &str = r##"
mode = "dark"
accent = "#81a1c1"
selection = "#434c5e"
muted = "#4c566a"
background = "#2e3440"
dark_background = "#222730"
darker_background = "#191c23"
lighter_background = "#3b4252"
foreground = "#d8dee9"
dark_foreground = "#667080"
light_foreground = "#adb5c4"
bright_foreground = "#d8dee9"
red = "#bf616a"
yellow = "#ebcb8b"
orange = "#d5967a"
green = "#a3be8c"
cyan = "#88c0d0"
blue = "#81a1c1"
magenta = "#b48ead"
brown = "#6a4b3d"
bright_red = "#bf616a"
bright_yellow = "#ebcb8b"
bright_green = "#a3be8c"
bright_cyan = "#8fbcbb"
bright_blue = "#81a1c1"
bright_magenta = "#b48ead"
"##;

    #[test]
    fn parses_nord_semantic_keys_and_derives_palette() {
        let dir = std::env::temp_dir().join("omacal-theme-test-nord");
        let _ = fs::remove_dir_all(&dir);
        write(&dir, "colors.toml", NORD);
        let theme = parse_theme(&dir, Some("nord"));

        assert_eq!(theme.name, "nord");
        assert!(!theme.light);
        assert_eq!(theme.background, "#2e3440");
        assert_eq!(theme.foreground, "#d8dee9");
        assert_eq!(theme.accent, "#81a1c1");
        assert_eq!(theme.muted, "#4c566a");
        // cursor falls back to bright_foreground
        assert_eq!(theme.cursor, "#d8dee9");
        // selection key maps to selection_background; selection_foreground defaults to bg
        assert_eq!(theme.selection_background, "#434c5e");
        assert_eq!(theme.selection_foreground, "#2e3440");

        // derived xterm palette
        assert_eq!(theme.colors[0], "#2e3440"); // background
        assert_eq!(theme.colors[1], "#bf616a"); // red
        assert_eq!(theme.colors[2], "#a3be8c"); // green
        assert_eq!(theme.colors[3], "#ebcb8b"); // yellow
        assert_eq!(theme.colors[4], "#81a1c1"); // blue/accent
        assert_eq!(theme.colors[7], "#d8dee9"); // foreground
        assert_eq!(theme.colors[8], "#4c566a"); // muted
        assert_eq!(theme.colors[15], "#d8dee9"); // bright foreground
        assert_eq!(theme.colors.len(), 16);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_light_mode_and_cursor_key() {
        let dir = std::env::temp_dir().join("omacal-theme-test-light");
        let _ = fs::remove_dir_all(&dir);
        write(
            &dir,
            "colors.toml",
            r##"
mode = "light"
accent = "#1e66f5"
background = "#eff1f5"
foreground = "#4c4f69"
cursor = "#8839ef"
selection = "#ccd0da"
"##,
        );
        let theme = parse_theme(&dir, Some("catppuccin-latte"));

        assert!(theme.light);
        assert_eq!(theme.background, "#eff1f5");
        assert_eq!(theme.cursor, "#8839ef"); // explicit cursor key wins
        assert_eq!(theme.colors.len(), 16);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_default_when_no_colors_file() {
        let dir = std::env::temp_dir().join("omacal-theme-test-missing");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let theme = parse_theme(&dir, Some("nothing"));
        assert_eq!(theme.background, "#0B0C16"); // default
        assert_eq!(theme.name, "nothing");
        let _ = fs::remove_dir_all(&dir);
    }
}
