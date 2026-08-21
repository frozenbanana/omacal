# Omacal

> Omarchy-native CalDAV calendar for Linux (Hyprland / Arch).

A fast, local-first calendar that syncs natively with **Nextcloud CalDAV** — no
vdirsyncer, no Evolution Data Server. Themed live from your active Omarchy
`colors.toml`, with desktop notifications via Mako, a tray icon, and a Waybar
helper.

![License](https://img.shields.io/github/license/frozenbanana/omacal)
![Build](https://img.shields.io/github/actions/workflow/status/frozenbanana/omacal/build.yml?branch=main)

---

## Features

- Day / week / month / year views (FullCalendar + Luxon)
- Native CalDAV sync: discovery, `sync-collection`, ETag-aware writes
- Create / edit / delete events, drag-move and resize
- All-day events and **config-driven timezone** — wall times stay `06:00` across CET/CEST, DST-safe
- Recurrence with **structured builder** (Daily / Weekly / Weekdays, interval + ends never/on/after) and **instance delete** (This event / This & future / Entire series via `EXDATE`/`UNTIL`)
- Recurrence expanded with wall-time semantics (e.g. weekly `06:00 Stockholm` stays `06:00`, `04:00Z` summer / `05:00Z` winter)
- Full-day timeline `00:00–24:00` with scroll to `08:00`, no clipping of night events
- Reminders → desktop notifications (Mako)
- Attendees + Accept / Maybe / Decline for pending invites
- Import `.ics` files — double-click and confirm
- Free-busy lookups
- Search, multi-calendar colors, tray icon, Waybar next-event module
- Live Omarchy theme reload

## Stack

- **UI:** Tauri 2 + React + TypeScript + FullCalendar (+ `@fullcalendar/luxon3` + `luxon` for IANA timezones)
- **Core:** Rust — reqwest CalDAV client, SQLite, system keyring, notify, `chrono-tz` + `rrule`
- **Calendar:** iCalendar (ICS), RRULE (wall-time expansion), alarms, PARTSTAT

## Requirements

- Linux with the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
  (webkit2gtk-4.1, etc.)
- Rust (stable) and Node.js ≥ 20 to build from source
- A Nextcloud instance with CalDAV enabled

**Non-goals (v1):** tasks/VTODO, travel time, find-a-time, Google/Outlook
OAuth, full email iMIP client, macOS/Windows builds.

## Install

### Arch / Omarchy (pacman)

```bash
# From omarchy-pkgs (when published)
sudo pacman -S omacal

# Or build locally
makepkg -si  # from repo root (uses PKGBUILD)
```

### Debian / Ubuntu (`.deb`)

Download the latest `.deb` from the
[Actions → Build](https://github.com/frozenbanana/omacal/actions/workflows/build.yml)
workflow artifacts:

```bash
sudo apt install ./Omacal_0.3.0_amd64.deb
```

### From source (local install, `~/.local`)

```bash
chmod +x packaging/install.sh packaging/omarchy-omacal packaging/omacal-waybar
./packaging/install.sh          # also installs icons + desktop entry to ~/.local
# Or: ./packaging/install.sh --skip-build   # reuse existing target/release/omacal
```

This registers the desktop entry and sets Omacal as the default handler for `.ics` files.

> **Note:** never install a plain `cargo build --release` binary — without
> Tauri's bundling it loads `http://localhost:1420` and shows a white screen.

### Omarchy launcher

Launch from the app menu (`Omacal`) or `omarchy-omacal` for a floating Hyprland
window. Optional systemd daemon keeps the tray, sync, and alarms alive:

```bash
systemctl --user enable --now omacal.service
# legacy name still works: omacal-daemon.service
```

## Development

```bash
npm install
npm run app:dev     # tauri dev + Vite on :1420
```

Useful checks:

```bash
npm run build                                # tsc + vite
cd src-tauri && cargo test --lib && cargo check
```

## First run

1. Open **Accounts** and add your Nextcloud CalDAV URL, e.g.
   `https://your.server/remote.php/dav/`
2. Use a Nextcloud **app password** — stored in the system keyring, never in
   plain config.
3. Add the email addresses used on invites so RSVP matching works.
4. Click **Sync**.

Data lives in:

| Kind | Path |
|---|---|
| Config | `~/.config/omacal/config.toml` |
| Database | `~/.local/share/omacal/omacal.db` |
| Secrets | system keyring (service `omacal`) |
| Theme | `~/.local/state/omarchy/current/theme/colors.toml` (fallback `~/.config/omarchy/current/theme/colors.toml`) |

## Importing `.ics` files

Omacal registers the `text/calendar` MIME type. Double-clicking any `.ics`
in the file manager launches (or wakes) Omacal and opens the event editor
prefilled with the event — pick a calendar and press **Save**. Imports always
create a fresh event and can be cancelled without changes.

## Omarchy integration

| Piece | Location |
|---|---|
| Theme | Reads `~/.local/state/omarchy/current/theme/colors.toml` live (fallback legacy) |
| Floating launch | `packaging/omarchy-omacal` |
| Hyprland rules | `packaging/hypr/omacal.lua` (legacy `omacal.conf` still shipped) |
| Bar widget | `~/.config/omarchy/plugins/henry.omacal` (Quickshell, replaces Waybar `custom/omacal`) |
| Waybar (legacy) | `packaging/omacal-waybar` (see `packaging/waybar/omacal.jsonc`) |
| App menu | `packaging/omarchy-menu.jsonc` snippet |
| Daemon | `packaging/systemd/omacal.service` (alias `omacal-daemon.service`, keeps alarm/sync with tray) |

## Shortcuts

| Key | Action |
|---|---|
| `n` | New event |
| `e` | Edit selected |
| `t` | Today |
| `←` / `→` | Prev / next period |

## Migration from vdirsyncer / khal

Omacal owns CalDAV sync. **Do not** run `vdirsyncer` against the same
calendars at the same time (edit races).

1. `systemctl --user disable --now vdirsyncer-sync.timer`
2. Comment out the matching pairs in `~/.config/vdirsyncer/config`
3. Add the same accounts in Omacal and sync
4. Keep `khal` only on a *separate* copy of data, not shared with Omacal

Move plaintext passwords out of the vdirsyncer config and into the keyring /
Nextcloud app passwords.

## Architecture

```
React UI --invoke--> Commands --> Db (SQLite)
     (Luxon tz)                   |          ^
                             v          |
                        SyncEngine <-> CalDavClient (reqwest)
                             |
                        Nextcloud CalDAV
```

- Source of truth for an event is `raw_ics`; index columns (`dtstart` as UTC RFC3339, etc.) are derived.
- Timed events stored as UTC, but **floating** `DTSTART:20260126T060000` is interpreted as `config.locale.timezone` (`Europe/Stockholm` by default). `WithTimezone` `DTSTART;TZID=...:20260126T060000` is converted via `chrono-tz`.
- Recurring events keep wall time across DST: expansion uses `DTSTART;TZID=…:wall` via `rrule` so `06:00 Stockholm` is `05:00Z` winter / `04:00Z` summer (auto-repaired on next sync via `ics_tz_fix_v2`). `EXDATE` (single occurrence delete) and `UNTIL` truncation (this & future) are filtered during expansion.
- Editing a recurring instance edits the **entire series** (v1) — `master_start` is kept so DTSTART doesn’t shift to the clicked occurrence.
- Background loops: theme watcher, alarm notifications, interval sync, tray.
- Single-instance guard forwards opened `.ics` files to the running app.
- Full-day week view: `slotMinTime 00:00–24:00`, `slotDuration 00:30`, `scrollTime 08:00`, `expandRows`, `timeZone = config.locale.timezone` via Luxon.

## License

[MIT](LICENSE) © Henry Bergstrom
