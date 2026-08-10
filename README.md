# Omarcal

> Omarchy-native CalDAV calendar for Linux (Hyprland / Arch).

A fast, local-first calendar that syncs natively with **Nextcloud CalDAV** — no
vdirsyncer, no Evolution Data Server. Themed live from your active Omarchy
`colors.toml`, with desktop notifications via Mako, a tray icon, and a Waybar
helper.

![License](https://img.shields.io/github/license/frozenbanana/omarcal)
![Build](https://img.shields.io/github/actions/workflow/status/frozenbanana/omarcal/build.yml?branch=main)

---

## Features

- Day / week / month / year views (FullCalendar)
- Native CalDAV sync: discovery, `sync-collection`, ETag-aware writes
- Create / edit / delete events, drag-move and resize
- All-day events and timezone-aware times
- Recurrence (RRULE) with expanded occurrences in UI and alarms
- Reminders → desktop notifications (Mako)
- Attendees + Accept / Maybe / Decline for pending invites
- Import `.ics` files — double-click and confirm
- Free-busy lookups
- Search, multi-calendar colors, tray icon, Waybar next-event module
- Live Omarchy theme reload

## Stack

- **UI:** Tauri 2 + React + TypeScript + FullCalendar
- **Core:** Rust — reqwest CalDAV client, SQLite, system keyring, notify
- **Calendar:** iCalendar (ICS), RRULE, alarms, PARTSTAT

## Requirements

- Linux with the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
  (webkit2gtk-4.1, etc.)
- Rust (stable) and Node.js ≥ 20 to build from source
- A Nextcloud instance with CalDAV enabled

**Non-goals (v1):** tasks/VTODO, travel time, find-a-time, Google/Outlook
OAuth, full email iMIP client, macOS/Windows builds.

## Install

### From a CI build (recommended)

Download the latest `.deb` from the
[Actions → Build](https://github.com/frozenbanana/omarcal/actions/workflows/build.yml)
workflow artifacts and install it:

```bash
sudo apt install ./Omarcal_0.1.0_amd64.deb
```

### From source (local install)

```bash
chmod +x packaging/install.sh packaging/omarchy-omarcal packaging/omarcal-waybar
./packaging/install.sh
```

This builds the app, installs it to `~/.local/bin`, registers the desktop entry,
and sets Omarcal as the default handler for `.ics` files.

> **Note:** never install a plain `cargo build --release` binary — without
> Tauri's bundling it loads `http://localhost:1420` and shows a white screen.

### Arch / Omarchy launcher

Launch with Walker (`Omarcal`) or `omarchy-omarcal` for a floating Hyprland
window. Optional systemd daemon keeps the tray, sync, and alarms alive:

```bash
systemctl --user enable --now omarcal-daemon.service
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
| Config | `~/.config/omarcal/config.toml` |
| Database | `~/.local/share/omarcal/omarcal.db` |
| Secrets | system keyring (service `omarcal`) |
| Theme | `~/.config/omarchy/current/theme/colors.toml` |

## Importing `.ics` files

Omarcal registers the `text/calendar` MIME type. Double-clicking any `.ics`
in the file manager launches (or wakes) Omarcal and opens the event editor
prefilled with the event — pick a calendar and press **Save**. Imports always
create a fresh event and can be cancelled without changes.

## Omarchy integration

| Piece | Location |
|---|---|
| Theme | Reads `~/.config/omarchy/current/theme/colors.toml` live |
| Floating launch | `packaging/omarchy-omarcal` |
| Hyprland rules | `packaging/hypr/omarcal.conf` |
| Waybar | `packaging/omarcal-waybar` (see `packaging/waybar/omarcal.jsonc`) |
| Daemon | `packaging/systemd/omarcal-daemon.service` (keeps alarm/sync with tray) |

## Shortcuts

| Key | Action |
|---|---|
| `n` | New event |
| `e` | Edit selected |
| `t` | Today |
| `←` / `→` | Prev / next period |

## Migration from vdirsyncer / khal

Omarcal owns CalDAV sync. **Do not** run `vdirsyncer` against the same
calendars at the same time (edit races).

1. `systemctl --user disable --now vdirsyncer-sync.timer`
2. Comment out the matching pairs in `~/.config/vdirsyncer/config`
3. Add the same accounts in Omarcal and sync
4. Keep `khal` only on a *separate* copy of data, not shared with Omarcal

Move plaintext passwords out of the vdirsyncer config and into the keyring /
Nextcloud app passwords.

## Architecture

```
React UI --invoke--> Commands --> Db (SQLite)
                             |          ^
                             v          |
                        SyncEngine <-> CalDavClient (reqwest)
                             |
                        Nextcloud CalDAV
```

- Source of truth for an event is `raw_ics`; index columns are derived.
- Background loops: theme watcher, alarm notifications, interval sync, tray.
- Single-instance guard forwards opened `.ics` files to the running app.

## License

[MIT](LICENSE) © Henry Bergstrom
