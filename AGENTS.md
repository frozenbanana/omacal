# AGENTS.md — Omarcal

Guidance for AI agents and humans working on this repository.

## What this is

**Omarcal** is a Linux-first (Omarchy / Arch / Hyprland) desktop calendar:

- Tauri 2 + React + TypeScript UI (Apple Calendar–inspired)
- Native **Nextcloud CalDAV** sync (not vdirsyncer, not Evolution Data Server)
- Omarchy theme integration, Mako notifications, tray, Waybar helper
- Local-first SQLite store; passwords in the system keyring only

Target user already runs Omarchy with Nextcloud CalDAV (and historically khal/vdirsyncer).

## Non-goals (v1)

Do not implement unless explicitly asked:

- Google / Outlook OAuth
- Full IMAP/SMTP iMIP email invite client
- VTODO / tasks, NL quick-add, travel time, find-a-time / availability search
- macOS / Windows builds
- Sharing the same on-disk store with khal/vdirsyncer

## Repository layout

```
omarcal/
  src/                      # React + TS frontend
    App.tsx                 # FullCalendar shell, shortcuts, sync/UI wiring
    store.ts                # Zustand + Tauri invoke helpers
    App.css                 # Omarchy CSS variables + layout
    components/
      EventEditor.tsx
      EventDetail.tsx
      SettingsModal.tsx     # Accounts / CalDAV wizard
  src-tauri/
    src/
      lib.rs                # App entry, tray, background sync, theme/alarm loops
      main.rs
      commands.rs           # Tauri IPC commands
      caldav.rs             # CalDAV client + XML parsers (critical)
      sync.rs               # Sync orchestration
      db.rs                 # SQLite schema + queries
      ics.rs                # iCalendar parse/build/RRULE/alarms/PARTSTAT
      config.rs             # ~/.config/omarcal/config.toml
      secrets.rs            # keyring (service: omarcal)
      theme.rs              # Omarchy colors.toml watcher
      alarms.rs             # Desktop notification loop (notify-rust / Mako)
    tauri.conf.json
    Cargo.toml              # MUST keep custom-protocol feature (see below)
  packaging/
    install.sh              # Local install to ~/.local/bin
    omarchy-omarcal         # Floating Hyprland launcher
    omarcal-waybar          # Waybar custom module script
    omarcal.desktop
    hypr/omarcal.conf
    systemd/omarcal-daemon.service
  README.md
```

## Runtime paths

| Kind | Path |
|---|---|
| Config | `~/.config/omarcal/config.toml` |
| DB | `~/.local/share/omarcal/omarcal.db` |
| Secrets | keyring service `omarcal`, key `account:{id}:password` |
| Omarchy theme | `~/.config/omarchy/current/theme/colors.toml` (+ `theme.name`, optional `light.mode`) |
| Installed binary | `~/.local/bin/omarcal` |

Config holds account URL/username/addresses only — **never** passwords.

## Architecture

```
React UI  --invoke-->  commands.rs  -->  Db (SQLite)
                              |               ^
                              v               |
                         SyncEngine  <-->  CalDavClient (reqwest)
                              |
                         Nextcloud CalDAV
```

Background threads (from `lib.rs` setup):

1. Theme watcher → emits `theme-changed`
2. Alarm loop → `notify-rust` + emits `alarm-fired`
3. Interval sync → `sync_all` + emits `sync-finished`
4. Tray (show / sync now / quit); `OMARCAL_START_HIDDEN=1` hides window on start

Frontend listens via `@tauri-apps/api/event` in `store.ts` (`bootListeners`).

## Build & run (critical)

### Development

```bash
npm install
npm run app:dev          # tauri dev (Vite on :1420)
```

### Production / install

```bash
npm run app:build        # tauri build --bundles deb
./packaging/install.sh   # preferred full install
```

**Never** install a binary from plain `cargo build --release` for end-user use.

- Without Tauri’s production path / `custom-protocol`, the app loads `http://localhost:1420` → white screen + connection refused when Vite is down.
- `Cargo.toml` must keep:

```toml
[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

- Prefer `--bundles deb` (AppImage/`linuxdeploy` has failed in this environment).
- After backend changes: `npx tauri build --bundles deb` then  
  `install -Dm755 src-tauri/target/release/omarcal ~/.local/bin/omarcal`
- Fully quit old instance (including tray) before testing: `pkill -x omarcal`

### Useful checks

```bash
cd src-tauri && cargo test --lib caldav::tests
cd src-tauri && cargo check
npm run build            # tsc + vite only
```

## Tauri commands (IPC)

Defined in `commands.rs`, registered in `lib.rs`:

| Command | Purpose |
|---|---|
| `get_snapshot` | Config + calendars + events + invites + theme + sync meta |
| `get_theme` / `get_config` / `save_config` | Theme / config |
| `add_account` / `remove_account` / `test_account` | Account wizard |
| `sync_now` | Full sync |
| `list_calendars` / `set_calendar_visible` / `set_calendar_color` | Calendar chrome |
| `list_events` / `search_events` / `pending_invites` / `next_event` | Queries |
| `save_event` / `delete_event` | CRUD + CalDAV PUT/DELETE |
| `respond_invite` | PARTSTAT update + PUT |
| `freebusy` | Best-effort free-busy REPORT |

Frontend arg naming: Tauri camelCases command args; nested serde structs use **snake_case** field names in JSON (`event_id`, `caldav_url`, etc.) as written in `store.ts`.

## CalDAV / Nextcloud (read this before touching sync)

Implementation: custom client in `caldav.rs` (reqwest + quick-xml). Do not assume `fast-dav-rs` is wired in.

### Discovery flow

1. PROPFIND `current-user-principal` on `/remote.php/dav/`
2. PROPFIND `calendar-home-set` on principal
3. PROPFIND Depth:1 on home → calendars
4. Sync via `REPORT sync-collection` (fallback `calendar-query`)

URL normalization: scheme optional (`https://` added); trailing slashes stripped then re-added where needed.

### XML parsing pitfalls (already burned us)

1. **Namespace prefixes** — Nextcloud uses `d:`, `cal:`, etc. `local_name()` must strip both `{ns}` Clark notation and `prefix:` forms.
2. **Self-closing tags** — Calendars are `<cal:calendar/>`. quick-xml emits `XmlEvent::Empty`, not `Start`. Parsers must handle **Empty** or calendars appear as zero.
3. Skip schedule `inbox` / `outbox`; ignore deleted-calendar / trash-bin / bare home collection.
4. On auth failure return clear 401/403 messages (don’t claim “no principal”).

Unit tests live in `caldav.rs` (`parses_nextcloud_principal_xml`, calendar list with empty tags, optional `cal_list_fixture.xml`). Prefer adding fixtures when changing parsers.

### Invites

v1 = CalDAV scheduling / `PARTSTAT=NEEDS-ACTION` on events matching account `addresses`. Not full email iMIP. Nextcloud may also expose schedule-inbox (partial support).

### Dual sync warning

Omarcal owns sync. Running **vdirsyncer** on the same calendars causes races. Document in UX/README; don’t re-enable dual-write.

## Data model (SQLite)

Tables (see `db.rs` migrate):

- `calendars` — account_id, href, displayname, color, sync_token, ctag, visible, readonly
- `objects` — raw ICS + indexed fields (uid, dtstart/end, rrule, attendees_json, alarms_json, my_partstat)
- `outbox` — reserved for durable writes
- `alarm_log` — de-dupe fired alarms
- `meta` — last_sync / last_sync_error

**Source of truth for an event is `raw_ics`.** Index columns are derived for UI/query.

Writes: build/update ICS → CalDAV PUT with If-Match ETag → upsert local row.

Recurrence: expand with `rrule` crate for UI/alarms (`build_events` / `alarms.rs`). Occurrence UIDs may be `uid::timestamp`; strip `::…` before save/edit.

## Frontend conventions

- State: Zustand in `store.ts`; durable data only via invoke.
- Views: FullCalendar (`dayGridMonth`, `timeGridWeek`, `timeGridDay`, `multiMonthYear`).
- Theme: CSS variables `--bg`, `--fg`, `--accent`, `--color0`… from Omarchy; no hardcoded purple/cream AI palettes.
- Fonts: monospace stack (JetBrains Mono / Iosevka / Cascadia) to fit Omarchy.
- Shortcuts: `n` new, `e` edit, `t` today, arrows navigate (ignore when typing in inputs).

## Omarchy packaging

| Artifact | Role |
|---|---|
| `packaging/omarchy-omarcal` | Float + size + focus Hyprland window |
| `packaging/hypr/omarcal.conf` | Window rules snippet |
| `packaging/omarcal-waybar` | JSON for Waybar next-event module (reads SQLite) |
| `packaging/systemd/omarcal-daemon.service` | Keep app/tray alive (`OMARCAL_START_HIDDEN=1`) |
| `packaging/omarcal.desktop` | Walker / app menu |

Theme load: parse TOML keys `accent`, `foreground`, `background`, `cursor`, `selection_*`, `color0`–`color15`.

## Locale defaults

Match existing user prefs unless told otherwise:

- Timezone: `Europe/Stockholm`
- 24h time, week starts Monday
- CalDAV URLs shaped like `https://host/remote.php/dav/`

## Security

- Store only app passwords in keyring; never write secrets to `config.toml`, git, logs, or fixtures.
- Do not commit `~/.config/vdirsyncer/config` contents (may contain plaintext passwords).
- Don’t echo credentials in agent transcripts or commit messages.

## Working style for agents

1. Prefer extending existing modules (`caldav.rs`, `ics.rs`, `commands.rs`, React components) over new frameworks.
2. After CalDAV/XML changes: add/adjust unit tests with realistic Nextcloud XML (prefixed + Empty tags).
3. After Rust changes meant for the installed app: **tauri build + install** to `~/.local/bin/omarcal`, then ask user to fully restart.
4. Keep IPC surface stable; update `store.ts` when command payloads change.
5. Don’t expand scope into non-goals without an explicit request.
6. Don’t edit plan files under `.cursor/plans/` unless asked.

## Known failure symptoms → likely cause

| Symptom | Likely cause |
|---|---|
| White screen + connection refused | Binary built without production/`custom-protocol`; still pointing at Vite `:1420` |
| `no current-user-principal in response` | XML local-name/prefix bug or bad URL/auth (check 401 path) |
| `OK — found calendars: (none)` | Calendar list parser missing `XmlEvent::Empty` for `<cal:calendar/>` |
| Sync races / duplicate edits | vdirsyncer still running against same calendars |
| Theme doesn’t update | Watcher not seeing Omarchy theme symlink swap; check `theme.rs` paths |
| Alarms missing when window closed | Need tray/daemon (`omarcal-daemon.service` or keep tray running) |

## Quick file index for common tasks

| Task | Start here |
|---|---|
| CalDAV discovery / list / sync XML | `src-tauri/src/caldav.rs` |
| Sync orchestration | `src-tauri/src/sync.rs` |
| Event CRUD IPC | `src-tauri/src/commands.rs` |
| ICS / RRULE / RSVP | `src-tauri/src/ics.rs` |
| SQLite | `src-tauri/src/db.rs` |
| UI / calendar views | `src/App.tsx` |
| Account wizard | `src/components/SettingsModal.tsx` |
| Omarchy colors | `src-tauri/src/theme.rs`, `src/App.css` |
| Install / Hyprland / Waybar | `packaging/` |
