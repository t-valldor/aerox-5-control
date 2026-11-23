#!/usr/bin/env bash
#
# build-app.sh - builds aerox5-gui and packages it as a macOS .app bundle
#               that can be launched directly from Finder (double-click).
#
# Usage:
#   ./scripts/build-app.sh              Build release + create dist/Aerox 5 Control.app
#   ./scripts/build-app.sh --install    Also copy the app to /Applications
#   ./scripts/build-app.sh --open       Open the app after building
#   ./scripts/build-app.sh --no-sign    Skip ad-hoc code signing
#   ./scripts/build-app.sh --help       Show help
#
# Optional app icon: place a square PNG at assets/AppIcon.png (at least 1024x1024
# is recommended). The script then creates a .icns file automatically with iconutil.
#
set -euo pipefail

# ---------------------------------------------------------------------------
# Settings (change here to use a different name/id)
# ---------------------------------------------------------------------------
# This script lives in scripts/, but Cargo.toml, assets/ and dist/ are at the repo root.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/Cargo.toml"
BINARY_NAME="aerox5-gui"
APP_NAME="Aerox 5 Control"
BUNDLE_ID="se.aerox5.control"
MIN_MACOS="11.0"
DIST_DIR="$ROOT/dist"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
ICON_SOURCE="$ROOT/assets/AppIcon.png"

# ---------------------------------------------------------------------------
# Arguments
# ---------------------------------------------------------------------------
INSTALL=false
OPEN_APP=false
SIGN=true

usage() {
    cat <<EOF
Usage: $(basename "$0") [flags]

Flags:
  --install     Copy the finished app to /Applications
  --open        Open the app after building
  --no-sign     Skip ad-hoc code signing
  -h, --help    Show this help

Output: $DIST_DIR/$APP_NAME.app
EOF
}

for arg in "$@"; do
    case "$arg" in
        --install) INSTALL=true ;;
        --open)    OPEN_APP=true ;;
        --no-sign) SIGN=false ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown argument: $arg" >&2; usage >&2; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Helper functions
# ---------------------------------------------------------------------------
info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mVarning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# 1. Sanity checks
# ---------------------------------------------------------------------------
[[ "$(uname -s)" == "Darwin" ]] || die "This script is for macOS only."
[[ -f "$MANIFEST" ]] || die "Could not find Cargo.toml in $ROOT"
command -v cargo >/dev/null 2>&1 || die "cargo was not found in PATH. Install Rust (https://rustup.rs)."

VERSION="$(awk -F'"' '/^version[[:space:]]*=/ {print $2; exit}' "$MANIFEST")"
[[ -n "$VERSION" ]] || VERSION="0.0.0"

# Respect CARGO_TARGET_DIR if it is set.
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
BINARY_PATH="$TARGET_DIR/release/$BINARY_NAME"

# ---------------------------------------------------------------------------
# 2. Build
# ---------------------------------------------------------------------------
info "Building $BINARY_NAME v$VERSION (release)..."
cargo build --release --manifest-path "$MANIFEST"
[[ -x "$BINARY_PATH" ]] || die "Build succeeded but $BINARY_PATH does not exist."

# ---------------------------------------------------------------------------
# 3. Create the .app structure
# ---------------------------------------------------------------------------
info "Creating $APP_NAME.app in $DIST_DIR ..."
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS" "$APP_BUNDLE/Contents/Resources"
cp "$BINARY_PATH" "$APP_BUNDLE/Contents/MacOS/$BINARY_NAME"
chmod +x "$APP_BUNDLE/Contents/MacOS/$BINARY_NAME"

# ---------------------------------------------------------------------------
# 4. Icon (optional)
# ---------------------------------------------------------------------------
ICON_KEYS=""
if [[ -f "$ICON_SOURCE" ]]; then
    info "Creating app icon from $ICON_SOURCE ..."
    ICONSET="$(mktemp -d "${TMPDIR:-/tmp}/aerox5-iconset.XXXXXX")"
    # iconutil requires the directory to have the .iconset suffix — otherwise
    # the conversion fails with "Invalid Iconset" even when all PNG files exist.
    ICONSET_DIR="$ICONSET/AppIcon.iconset"
    mkdir -p "$ICONSET_DIR"
    trap 'rm -rf "$ICONSET"' EXIT

    # iconutil requires all of these sizes. sips scales them from the original.
    while read -r size file; do
        sips -z "$size" "$size" "$ICON_SOURCE" --out "$ICONSET_DIR/$file" >/dev/null
    done <<'ICONS'
16   icon_16x16.png
32   icon_16x16@2x.png
32   icon_32x32.png
64   icon_32x32@2x.png
128  icon_128x128.png
256  icon_128x128@2x.png
256  icon_256x256.png
512  icon_256x256@2x.png
512  icon_512x512.png
1024 icon_512x512@2x.png
ICONS

    iconutil -c icns "$ICONSET_DIR" -o "$APP_BUNDLE/Contents/Resources/AppIcon.icns"
    ICON_KEYS="    <key>CFBundleIconFile</key>
    <string>AppIcon</string>"
else
    warn "No icon was found (assets/AppIcon.png). Using the default icon."
fi

# ---------------------------------------------------------------------------
# 5. Info.plist
# ---------------------------------------------------------------------------
cat > "$APP_BUNDLE/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>$BINARY_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>$MIN_MACOS</string>
    <key>NSHighResolutionCapable</key>
    <true/>
$ICON_KEYS
</dict>
</plist>
EOF

if command -v plutil >/dev/null 2>&1; then
    plutil -lint "$APP_BUNDLE/Contents/Info.plist" >/dev/null
fi

# ---------------------------------------------------------------------------
# 6. Code signing (ad-hoc) - avoids local "app is damaged" warnings
# ---------------------------------------------------------------------------
if [[ "$SIGN" == true ]] && command -v codesign >/dev/null 2>&1; then
    info "Signing the app (ad-hoc)..."
    if ! codesign --force --deep --sign - "$APP_BUNDLE" 2>/dev/null; then
        warn "Code signing failed. The app may still run locally."
    fi
fi

# ---------------------------------------------------------------------------
# 7. Install / open (optional)
# ---------------------------------------------------------------------------
if [[ "$INSTALL" == true ]]; then
    info "Copying to /Applications ..."
    if cp -R "$APP_BUNDLE" /Applications/ 2>/dev/null; then
        info "Installed: /Applications/$APP_NAME.app"
    else
        warn "Could not write to /Applications. Run with sudo or copy manually."
    fi
fi

if [[ "$OPEN_APP" == true ]]; then
    # Make sure macOS registers the newly built icon before opening the app —
    # otherwise the Dock may keep showing a stale cached icon.
    touch "$APP_BUNDLE"
    LSREG="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
    if [[ -x "$LSREG" ]]; then
        "$LSREG" -f "$APP_BUNDLE" >/dev/null 2>&1 || true
    fi
    info "Opening $APP_NAME ..."
    open "$APP_BUNDLE"
fi

# ---------------------------------------------------------------------------
# Done
# ---------------------------------------------------------------------------
cat <<EOF

Done! The app is here:
  $APP_BUNDLE

Double-click it in Finder to launch, or drag it to the Dock.
Run  ./scripts/build-app.sh --install  to place it in /Applications.
EOF
