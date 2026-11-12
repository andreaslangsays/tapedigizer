#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$ROOT_DIR"

echo "=== 1. Building release binary ==="
cargo build --locked --release

APP_VERSION="$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' Cargo.toml | head -n 1)"
if [[ -z "$APP_VERSION" ]]; then
    echo "Could not read the package version from Cargo.toml" >&2
    exit 1
fi

APP_NAME="TapeDigizer"
APP_BUNDLE="$ROOT_DIR/${APP_NAME}.app"
CONTENTS_DIR="$APP_BUNDLE/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

echo "=== 3. Assembling ${APP_NAME}.app bundle ==="
rm -rf "$APP_BUNDLE"
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# Copy binary
cp "$ROOT_DIR/target/release/tape_digizer" "$MACOS_DIR/$APP_NAME"
chmod +x "$MACOS_DIR/$APP_NAME"

# Copy Icon
cp "$ROOT_DIR/AppIcon.icns" "$RESOURCES_DIR/AppIcon.icns"

# Write Info.plist
cat << EOF > "$CONTENTS_DIR/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleName</key>
    <string>TapeDigizer</string>
    <key>CFBundleDisplayName</key>
    <string>TapeDigizer</string>
    <key>CFBundleIdentifier</key>
    <string>com.andreas.tapedigizer</string>
    <key>CFBundleVersion</key>
    <string>${APP_VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${APP_VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>TapeDigizer</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>TapeDigizer benötigt Zugriff auf das Audio-Interface / Mikrofon, um Tonbänder und Kassetten in Studioqualität aufzunehmen.</string>
</dict>
</plist>
EOF

# Write PkgInfo
printf "APPL????" > "$CONTENTS_DIR/PkgInfo"

echo "=== 4. Ad-hoc code signing ==="
codesign --force --deep --sign - "$APP_BUNDLE"
codesign --verify --verbose "$APP_BUNDLE"

touch "$APP_BUNDLE"

echo "=== Successfully built $APP_BUNDLE ==="

if [[ -x "$SCRIPT_DIR/build_au_installer.sh" ]]; then
    "$SCRIPT_DIR/build_au_installer.sh"
    if [[ -d "$ROOT_DIR/TapeDigizerDenoiser.component" ]]; then
        rm -rf "$RESOURCES_DIR/TapeDigizerDenoiser.component"
        cp -R "$ROOT_DIR/TapeDigizerDenoiser.component" "$RESOURCES_DIR/TapeDigizerDenoiser.component"
        codesign --force --deep --sign - "$APP_BUNDLE"
    fi
elif [[ -x "$SCRIPT_DIR/build_audiounit.sh" ]]; then
    "$SCRIPT_DIR/build_audiounit.sh"
fi
