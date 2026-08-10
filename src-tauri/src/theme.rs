use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
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
            colors: (0..16).map(|i| format!("#{i:02x}{i:02x}{i:02x}")).collect(),
            light: false,
            name: "default".into(),
        }
    }
}

pub fn omarchy_theme_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omarchy/current/theme")
}

pub fn load_omarchy_theme() -> ThemeColors {
    let dir = omarchy_theme_dir();
    let colors_path = dir.join("colors.toml");
    let mut theme = ThemeColors::default();

    let name_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("omarchy/current/theme.name");
    if let Ok(n) = fs::read_to_string(name_path) {
        theme.name = n.trim().to_string();
    }

    theme.light = dir.join("light.mode").exists();

    if let Ok(text) = fs::read_to_string(&colors_path) {
        if let Ok(val) = text.parse::<toml::Value>() {
            if let Some(table) = val.as_table() {
                if let Some(v) = table.get("accent").and_then(|v| v.as_str()) {
                    theme.accent = v.to_string();
                }
                if let Some(v) = table.get("foreground").and_then(|v| v.as_str()) {
                    theme.foreground = v.to_string();
                }
                if let Some(v) = table.get("background").and_then(|v| v.as_str()) {
                    theme.background = v.to_string();
                }
                if let Some(v) = table.get("cursor").and_then(|v| v.as_str()) {
                    theme.cursor = v.to_string();
                }
                if let Some(v) = table.get("selection_foreground").and_then(|v| v.as_str()) {
                    theme.selection_foreground = v.to_string();
                }
                if let Some(v) = table.get("selection_background").and_then(|v| v.as_str()) {
                    theme.selection_background = v.to_string();
                }
                let mut colors = Vec::new();
                for i in 0..16 {
                    let key = format!("color{i}");
                    if let Some(v) = table.get(&key).and_then(|v| v.as_str()) {
                        colors.push(v.to_string());
                    }
                }
                if colors.len() == 16 {
                    theme.colors = colors;
                }
            }
        }
    }
    theme
}

pub fn start_theme_watcher(app: AppHandle) {
    thread::spawn(move || {
        let path = omarchy_theme_dir();
        if !path.exists() {
            // still emit once
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

        let _ = watcher.watch(&path, RecursiveMode::NonRecursive);
        // also watch parent for symlink swap of theme
        if let Some(parent) = path.parent() {
            let _ = watcher.watch(parent, RecursiveMode::NonRecursive);
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
            let sig = format!("{}-{}", theme.name, theme.background);
            if sig != last {
                last = sig;
                let _ = app.emit("theme-changed", theme);
            }
        }
    });
}
