# Contributing

Thanks for helping make Omacal the official Omarchy calendar.

## Requirements

- Linux + [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) (`webkit2gtk-4.1` etc.)
- Rust stable + Node.js ≥ 20

## Development

```bash
npm install
npm run app:dev      # tauri dev (Vite :1420)
npm run build        # tsc + vite → dist/
npm test             # TZ=Europe/Stockholm vitest
npm run lint         # eslint --max-warnings 0
npm run format:check # prettier
npm run typecheck    # tsc --noEmit
cd src-tauri && cargo fmt -- --check && cargo clippy -- -D warnings && cargo test
npx tauri build --bundles deb   # → src-tauri/target/release/bundle/deb/*.deb
makepkg -si                     # Arch package (from repo root)
```

### Commit style

- Conventional commits if possible (`feat:`, `fix:`, `chore:`). `CHANGELOG.md` is curated manually on release.
- Keep `package.json` / `src-tauri/Cargo.toml` / `src-tauri/tauri.conf.json` versions in sync.

### Pull requests

1. Fork, branch, `npm run lint && npx tsc --noEmit && cargo clippy -- -D warnings`.
2. Add tests for CalDAV/XML or `ics.rs` changes with realistic Nextcloud fixtures.
3. Do not edit plan files under `.cursor/plans/` unless asked; prefer extending `caldav.rs`/`ics.rs`/`commands.rs`.
4. Ensure `packaging/install.sh` and `PKGBUILD` stay in sync for bin/desktop/icons/mime.

## Packaging

- Debian: `tauri build --bundles deb` (AppImage `linuxdeploy` is known-broken, use `deb` only).
- Arch: `PKGBUILD` installs to `/usr/bin` `/usr/share/applications` `/usr/share/icons/hicolor/*/apps` `/usr/share/metainfo` `/usr/lib/systemd/user`.
- After Rust changes for the installed app: `npx tauri build --bundles deb && install -Dm755 src-tauri/target/release/omacal ~/.local/bin/omacal && pkill -x omacal`.

## Security

- Passwords only in `keyring` service `omacal` (`account:{id}:password`), never `config.toml`/git/logs.
- Do not commit `~/.config/vdirsyncer/config`.
- Report security issues privately via GitHub Security Advisories.

## Code of Conduct

Be kind, respect Omarchy conventions, keep the scope focused (see non-goals in `README.md`).
