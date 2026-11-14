#!/usr/bin/env bash
# Local experimental bundle only. Does not install or replace Console Link.
set -euo pipefail

preview_dir="$(cd "$(dirname "$0")/.." && pwd)"
repo_dir="$(cd "$preview_dir/../.." && pwd)"
cd "$preview_dir"

if [[ "$(uname -s)" != Darwin ]]; then
    echo 'This local preview bundle requires macOS.' >&2
    exit 1
fi

cargo build --locked
target_dir="${CARGO_TARGET_DIR:-$preview_dir/target}"
output_dir="${GPUI_PREVIEW_OUTPUT_DIR:-$preview_dir/dist}"
app_dir="$output_dir/Console Link GPUI Preview.app"
mkdir -p "$app_dir/Contents/MacOS" "$app_dir/Contents/Resources"
cp "$target_dir/debug/console-link-gpui-preview" "$app_dir/Contents/MacOS/"
cp "$preview_dir/Info.plist" "$app_dir/Contents/Info.plist"
cp "$repo_dir/assets/icon.icns" "$app_dir/Contents/Resources/icon.icns"
chmod +x "$app_dir/Contents/MacOS/console-link-gpui-preview"
plutil -lint "$app_dir/Contents/Info.plist"
codesign --force --deep --sign - "$app_dir"
codesign --verify --deep --strict "$app_dir"
printf '%s\n' "$app_dir"
