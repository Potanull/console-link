#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
target="${CONSOLE_LINK_TARGET:-aarch64-apple-darwin}"
cargo build --locked --release --target "$target" --bin console-link
target_dir="${CARGO_TARGET_DIR:-target}"
bundle="dist/Console Link.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$target_dir/$target/release/console-link" "$bundle/Contents/MacOS/console-link"
cp packaging/Info.plist "$bundle/Contents/Info.plist"
cp assets/icon.icns "$bundle/Contents/Resources/icon.icns"
cp LICENSE THIRD_PARTY.md THIRD_PARTY_LICENSES.txt "$bundle/Contents/Resources/"
codesign --force --sign - "$bundle"
codesign --verify --strict "$bundle"
ditto -c -k --sequesterRsrc --keepParent "$bundle" "dist/console-link-macos-arm64.zip"
echo "Created dist/Console Link.app (ad-hoc signed, not notarized)."
