# Omarcal

Omarchy-native CalDAV calendar for Linux (Hyprland). Syncs with Nextcloud, themed from your active Omarchy `colors.toml`, with desktop notifications via Mako.

## Features

- Day / week / month / year views (FullCalendar)
- Native CalDAV sync (discovery + `sync-collection`)
- Create / edit / delete events (all-day, time zones, drag-move/resize)
- Recurrence (RRULE), reminders → desktop notifications
- Attendees + Accept / Maybe / Decline for pending invites
- Search, multi-calendar colors, tray icon, Waybar helper
- Live Omarchy theme reload

## Stack

- **UI:** Tauri 2 + React + TypeScript + FullCalendar
- **Core:** Rust (reqwest CalDAV, SQLite, keyring, notify)

## Dev

```bash
npm install
npm run app:dev    # tauri dev + Vite
```

## Install (local)

```bash
chmod +x packaging/install.sh packaging/omarchy-omarcal packaging/omarcal-waybar
./packaging/install.sh
```

Use `npm run app:build` (or the install script). Do **not** install a plain `cargo build --release` binary — that loads `http://localhost:1420` and shows a white screen when Vite isn’t running.

Launch with Walker (`Omarcal`) or `omarchy-omarcal` for a floating window.

## First-time setup

1. Open **Accounts** and add each Nextcloud CalDAV URL, e.g.  
   `https://your.server/remote.php/dav/`
2. Use a Nextcloud **app password** — stored in the system keyring, never in config.
3. Add the email addresses used on invites so RSVP matching works.
4. Click **Sync**.

Config: `~/.config/omarcal/config.toml`  
Database: `~/.local/share/omarcal/omarcal.db`

## Migration from vdirsyncer / khal

Omarcal owns CalDAV sync. **Do not** run `vdirsyncer` against the same calendars at the same time (edit races).

1. Stop the timer: `systemctl --user disable --now vdirsyncer-sync.timer`
2. Comment out the matching pairs in `~/.config/vdirsyncer/config`
3. Add the same accounts in Omarcal and sync
4. Keep `khal` only if you still want CLI on a *separate* copy of data (not shared with Omarcal in v1)

Move passwords out of plaintext vdirsyncer config into the keyring / Nextcloud app passwords.

## Omarchy integration

| Piece | Location |
|---|---|
| Theme | Reads `~/.config/omarchy/current/theme/colors.toml` live |
| Floating launch | `omarchy-omarcal` |
| Hyprland rules | `packaging/hypr/omarcal.conf` |
| Waybar | `omarcal-waybar` + snippet in `packaging/waybar/omarcal.jsonc` |
| Systemd | `omarcal-daemon.service` (keeps alarms/sync with tray app) |

## Shortcuts

| Key | Action |
|---|---|
| `n` | New event |
| `e` | Edit selected |
| `t` | Today |
| `←` / `→` | Prev / next period |

## Non-goals (v1)

Quick-add NL, tasks (VTODO), travel time, find-a-time, Google/Outlook OAuth, full email iMIP client.
