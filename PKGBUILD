# Maintainer: Henry Bergstrom <https://github.com/frozenbanana>
pkgname=omacal
pkgver=0.3.0
pkgrel=1
pkgdesc="Omarchy-native CalDAV calendar (Tauri + React, Nextcloud sync, live theme)"
arch=('x86_64')
url="https://github.com/frozenbanana/omacal"
license=('MIT')
depends=('webkit2gtk-4.1' 'gtk3' 'libayatana-appindicator' 'sqlite' 'openssl' 'hicolor-icon-theme')
makedepends=('rust' 'cargo' 'nodejs' 'npm' 'git')
optdepends=('mako: desktop notifications'
            'noto-fonts: recommended fonts')
provides=('omacal' 'omarcal')
conflicts=('omacal-bin' 'omarcal' 'omarcal-bin')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

prepare() {
  cd "$pkgname-$pkgver"
  # npm ci needs package-lock.json
  npm ci || npm install
}

build() {
  cd "$pkgname-$pkgver"
  # Tauri requires embedded frontend; custom-protocol is default feature
  npm run build
  npx tauri build --bundles deb
  # Binary at src-tauri/target/release/omacal will be installed in package()
}

package() {
  cd "$pkgname-$pkgver"

  # Binary (alias omarcal → omacal for 1 release)
  install -Dm755 "src-tauri/target/release/omacal" "$pkgdir/usr/bin/omacal"
  ln -sf omacal "$pkgdir/usr/bin/omarcal"
  install -Dm755 "packaging/omarchy-omacal" "$pkgdir/usr/bin/omarchy-omacal"
  ln -sf omarchy-omacal "$pkgdir/usr/bin/omarchy-omarcal"
  install -Dm755 "packaging/omacal-waybar" "$pkgdir/usr/bin/omacal-waybar"
  ln -sf omacal-waybar "$pkgdir/usr/bin/omarcal-waybar"

  # Desktop + MIME (fileAssociations also in tauri.conf)
  install -Dm644 "packaging/omacal.desktop" "$pkgdir/usr/share/applications/omacal.desktop"
  ln -sf omacal.desktop "$pkgdir/usr/share/applications/omarcal.desktop"

  # Icons — hicolor (Tauri deb also installs these; explicit ensures local builds)
  install -Dm644 "src-tauri/icons/32x32.png" "$pkgdir/usr/share/icons/hicolor/32x32/apps/omacal.png"
  install -Dm644 "src-tauri/icons/128x128.png" "$pkgdir/usr/share/icons/hicolor/128x128/apps/omacal.png"
  install -Dm644 "src-tauri/icons/128x128@2x.png" "$pkgdir/usr/share/icons/hicolor/256x256/apps/omacal.png"
  install -Dm644 "src-tauri/icons/icon.png" "$pkgdir/usr/share/icons/hicolor/512x512/apps/omacal.png"
  # Scalable fallback (use 512 as scalable if no SVG yet)
  install -Dm644 "src-tauri/icons/icon.png" "$pkgdir/usr/share/icons/hicolor/scalable/apps/omacal.png"
  ln -sf omacal.png "$pkgdir/usr/share/icons/hicolor/32x32/apps/omarcal.png"
  ln -sf omacal.png "$pkgdir/usr/share/icons/hicolor/128x128/apps/omarcal.png"
  ln -sf omacal.png "$pkgdir/usr/share/icons/hicolor/256x256/apps/omarcal.png"
  ln -sf omacal.png "$pkgdir/usr/share/icons/hicolor/512x512/apps/omarcal.png"
  ln -sf omacal.png "$pkgdir/usr/share/icons/hicolor/scalable/apps/omarcal.png"

  # AppStream (alias for old ID)
  install -Dm644 "packaging/com.henry.omacal.metainfo.xml" "$pkgdir/usr/share/metainfo/com.henry.omacal.metainfo.xml"
  ln -sf com.henry.omacal.metainfo.xml "$pkgdir/usr/share/metainfo/com.henry.omarcal.metainfo.xml"

  # Hyprland — Lua (preferred) + legacy conf
  install -Dm644 "packaging/hypr/omacal.lua" "$pkgdir/usr/share/omarchy/default/hypr/apps/omacal.lua"
  install -Dm644 "packaging/hypr/omacal.conf" "$pkgdir/usr/share/doc/omacal/omacal.conf.example"

  # Systemd user service — canonical name omacal.service, alias omacal-daemon.service
  install -Dm644 "packaging/systemd/omacal.service" "$pkgdir/usr/lib/systemd/user/omacal.service"
  # Compatibility file copies for old names (alias 1 release)
  install -Dm644 "packaging/systemd/omacal.service" "$pkgdir/usr/lib/systemd/user/omacal-daemon.service"
  ln -sf omacal.service "$pkgdir/usr/lib/systemd/user/omarcal.service"
  ln -sf omacal.service "$pkgdir/usr/lib/systemd/user/omarcal-daemon.service"

  # Waybar example (legacy) + Quickshell widget + menu snippet
  install -Dm644 "packaging/waybar/omacal.jsonc" "$pkgdir/usr/share/doc/omacal/waybar-omacal.jsonc.example"
  install -Dm644 "packaging/omarchy-menu.jsonc" "$pkgdir/usr/share/doc/omacal/omarchy-menu.jsonc.example"
  # Quickshell bar widget — installed directly so `omarchy-shell` can load it
  install -Dm644 "packaging/quickshell/henry.omacal/manifest.json" "$pkgdir/usr/share/omarchy/shell/plugins/henry.omacal/manifest.json"
  install -Dm644 "packaging/quickshell/henry.omacal/BarWidget.qml" "$pkgdir/usr/share/omarchy/shell/plugins/henry.omacal/BarWidget.qml"

  # Man page + license
  install -Dm644 "man/omacal.1" "$pkgdir/usr/share/man/man1/omacal.1"
  install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
