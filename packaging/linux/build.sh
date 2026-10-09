#!/usr/bin/env bash
# Builds Vera View for Linux (x86_64) and packages it as:
#   dist/VeraView-<version>-x86_64.AppImage   runs on most distributions, no install needed
#   dist/vera-view_<version>-1_amd64.deb      Debian / Ubuntu package (needs cargo-deb)
#
# Build dependencies on Debian/Ubuntu:
#   sudo apt install build-essential libxkbcommon-dev libwayland-dev libxcb-render0-dev \
#                    libxcb-shape0-dev libxcb-xfixes0-dev file
#   cargo install cargo-deb
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
mkdir -p dist

# Refresh the open-source license notices (fails on licenses not accepted in about.toml).
if command -v cargo-about >/dev/null; then
    cargo about generate installer/notices.hbs -o THIRD-PARTY-NOTICES.txt
else
    echo "cargo-about not installed; using the committed THIRD-PARTY-NOTICES.txt" >&2
fi

cargo build --release

# --- .deb (metadata is in Cargo.toml under [package.metadata.deb]) ---
if command -v cargo-deb >/dev/null; then
    cargo deb --no-build --output dist/
else
    echo "cargo-deb not installed; skipping .deb (cargo install cargo-deb)" >&2
fi

# --- AppImage ---
appdir=target/VeraView.AppDir
rm -rf "$appdir"
install -Dm755 target/release/vera-view "$appdir/usr/bin/vera-view"
install -Dm644 packaging/linux/vera-view.desktop "$appdir/usr/share/applications/vera-view.desktop"
install -Dm644 packaging/linux/vera-view.xml "$appdir/usr/share/mime/packages/vera-view.xml"
install -Dm644 assets/icon-256.png "$appdir/usr/share/icons/hicolor/256x256/apps/vera-view.png"
install -Dm644 assets/icon-512.png "$appdir/usr/share/icons/hicolor/512x512/apps/vera-view.png"
install -Dm644 THIRD-PARTY-NOTICES.txt "$appdir/usr/share/doc/vera-view/THIRD-PARTY-NOTICES.txt"
install -Dm644 README.md "$appdir/usr/share/doc/vera-view/README.md"
# AppImage expects the desktop file and icon at the top level, and an AppRun entry point.
cp packaging/linux/vera-view.desktop "$appdir/"
cp assets/icon-256.png "$appdir/vera-view.png"
ln -s usr/bin/vera-view "$appdir/AppRun"

tool=target/appimagetool-x86_64.AppImage
if [ ! -x "$tool" ]; then
    curl -fsSL -o "$tool" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$tool"
fi
# --appimage-extract-and-run avoids needing FUSE (e.g. in containers and CI).
ARCH=x86_64 "$tool" --appimage-extract-and-run "$appdir" "dist/VeraView-$version-x86_64.AppImage"

ls -l dist/
