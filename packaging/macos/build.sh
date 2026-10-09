#!/usr/bin/env bash
# Builds "Vera View.app" (universal: Apple Silicon + Intel) and dist/VeraView-<version>.dmg.
# Run on a Mac with Xcode command-line tools and rustup.
#
# Optional environment variables:
#   BUNDLE_ID             reverse-DNS app id (default below; use one you own before publishing)
#   MACOS_SIGN_IDENTITY   "Developer ID Application: Name (TEAMID)". Without it the app is
#                         ad-hoc signed: it runs on this Mac but Gatekeeper blocks it elsewhere.
#   MACOS_NOTARY_PROFILE  keychain profile from `xcrun notarytool store-credentials`;
#                         when set (with a signing identity) the DMG is notarized and stapled.
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
bundle_id=${BUNDLE_ID:-com.example.veraview}   # TODO: replace with your own identifier

# Refresh the open-source license notices (fails on licenses not accepted in about.toml).
if command -v cargo-about >/dev/null; then
    cargo about generate installer/notices.hbs -o THIRD-PARTY-NOTICES.txt
else
    echo "cargo-about not installed; using the committed THIRD-PARTY-NOTICES.txt" >&2
fi

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

app="dist/Vera View.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/vera-view" \
    target/aarch64-apple-darwin/release/vera-view \
    target/x86_64-apple-darwin/release/vera-view
sed -e "s/__VERSION__/$version/g" -e "s/__BUNDLE_ID__/$bundle_id/g" \
    packaging/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
cp assets/vera-view.icns "$app/Contents/Resources/"
cp THIRD-PARTY-NOTICES.txt README.md "$app/Contents/Resources/"

if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    # Hardened runtime + timestamp are required for notarization.
    codesign --force --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" "$app"
else
    codesign --force --sign - "$app"
fi
codesign --verify --strict --verbose=2 "$app"

# Disk image with a shortcut to /Applications for drag-to-install.
staging=$(mktemp -d)
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
dmg="dist/VeraView-$version.dmg"
rm -f "$dmg"
hdiutil create -volname "Vera View" -srcfolder "$staging" -ov -format UDZO "$dmg"
rm -rf "$staging"

if [ -n "${MACOS_SIGN_IDENTITY:-}" ]; then
    codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$dmg"
    if [ -n "${MACOS_NOTARY_PROFILE:-}" ]; then
        xcrun notarytool submit "$dmg" --keychain-profile "$MACOS_NOTARY_PROFILE" --wait
        xcrun stapler staple "$dmg"
    fi
fi

ls -l dist/
