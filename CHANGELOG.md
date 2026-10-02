# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-10-02

### Added
- Edit individual recurring events: editing one occurrence edits the master while keeping `master_start`, with a `ConfirmRecurrenceEdit` dialog to scope changes to a single instance, this-and-future, or the whole series.
- WebKitGTK `WEBKIT_DISABLE_DMABUF_RENDERER=1` default in `main.rs` to avoid startup kills on hybrid Intel/NVIDIA systems (respects a user-supplied value).

### Changed
- Faster calendar writes: reduced latency in CalDAV PUT / sync path.
- CI: Rust job now installs system deps (glib-2.0) and builds the frontend (`npm ci` + `npm run build`) so the Tauri context resolves `frontendDist`.

### Fixed
- Splash no longer hangs: null CSP + 5s ready fallback.
- Lint gate clean (`--max-warnings 0`) and removed stale `omarcal` target path in cargo clean.

## [0.3.0] - 2026-08-21

### Changed
- Rename project `omarcal` → `omacal` (6 chars, `oma`+`cal` — `omacalc` parallel, official `omarchy` style). Binary `omacal`, identifier `com.henry.omacal`, config `~/.config/omacal`, DB `~/.local/share/omacal/omacal.db`, keyring service `omacal`, widget `henry.omacal`, launchers `omarchy-omacal`/`omacal-waybar`. Migration fallback reads legacy `omarcal` DB/config/keyring and `OMARCAL_START_HIDDEN` env; alias symlinks `omarcal`→`omacal` kept for 1 release.
- Version bump `0.2.0` → `0.3.0`.

## [0.2.0] - 2026-08-21

### Added
- Omarchy-native theme live-reload from `~/.local/state/omarchy/current/theme/colors.toml` with legacy `~/.config` fallback; derives xterm `color0…15` palette from semantic `accent/background/foreground/selection/muted` keys.
- Recurrence structured builder (Daily/Weekly/Weekdays) with `EXDATE`/`UNTIL` instance delete (single / future / entire series) — wall-time DST-safe.
- Quickshell bar widget `henry.omacal` (next-event) replacing Waybar `custom/omacal`; `omarchy-menu.jsonc` app menu provider.
- Arch packaging: `PKGBUILD` + `.SRCINFO`, `metainfo.xml`, `man/omacal.1`, `hicolor` 32/128/256/512 + scalable icons, AppStream metadata.
- Hyprland Lua rule `packaging/hypr/omacal.lua` (legacy `.conf` still shipped) and `omarchy-omacal` launcher via `jq`.
- Click-to-mark (single click highlights, double-click edits), paste-anchor day highlight, `Ctrl+C/X/V` at last marked place, Apple-style arrow navigation (`←/→` event, `Cmd/Ctrl+←/→` period).
- Pulsating calendar splash + Nord fallback + theme-seed inline script; `tauri.conf` `backgroundColor #2e3440`.
- Lint/format gates: `eslint --max-warnings 0`, `prettier --check`, `tsc --noEmit`, `cargo fmt --check`, `cargo clippy -- -D warnings`, `vitest --coverage`.

### Changed
- Systemd user service `WantedBy=graphical-session.target` + `PartOf=graphical-session.target`, `ConditionEnvironment=WAYLAND_DISPLAY`.
- Inputs/labels contrast and focus ring for Nord readability.
- `omacal.desktop` adds `TryExec`, `StartupWMClass`, file associations.

### Fixed
- Calendar list parsing handles `XmlEvent::Empty` for `<cal:calendar/>`, namespace prefix stripping.
- Floating `DTSTART` interpreted as `config.locale.timezone` wall time, not UTC.
- `EXDATE` dedup and `UNTIL` UTC conversion for `rrule` crate.

## [0.1.0] - 2026-08-10

### Added
- Initial public release: Tauri 2 + React + TypeScript + FullCalendar + Rust CalDAV (reqwest + quick-xml) + SQLite + keyring.
- Day/week/month/year views, CalDAV `sync-collection` + ETag writes, ICS RRULE wall-time, Mako alarms, tray, Waybar helper, `.ics` import.
