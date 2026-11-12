#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$ROOT_DIR"

echo "=== 1. Building TapeDigizer AudioUnit (release cdylib) ==="
cargo build --locked --release --lib

APP_VERSION="$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' Cargo.toml | head -n 1)"
if [[ -z "$APP_VERSION" ]]; then
    APP_VERSION="1.5.2"
fi

# Convert X.Y.Z into integer AudioComponent version (e.g. 1.5.2 -> 66818 = 0x00010502)
IFS='.' read -r V_MAJ V_MIN V_PAT <<< "$APP_VERSION"
AU_VERSION_INT=$(( (V_MAJ << 16) | (V_MIN << 8) | V_PAT ))

COMPONENT_NAME="TapeDigizerDenoiser"
BUNDLE_DIR="$ROOT_DIR/${COMPONENT_NAME}.component"
CONTENTS_DIR="$BUNDLE_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

echo "=== 2. Assembling ${COMPONENT_NAME}.component bundle ==="
rm -rf "$BUNDLE_DIR"
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

cp "$ROOT_DIR/target/release/libtape_digizer.dylib" "$MACOS_DIR/$COMPONENT_NAME"
chmod +x "$MACOS_DIR/$COMPONENT_NAME"

cat << EOF > "$CONTENTS_DIR/Info.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>German</string>
    <key>CFBundleExecutable</key>
    <string>${COMPONENT_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.andreas.tapedigizer.audiounit.denoiser</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${COMPONENT_NAME}</string>
    <key>CFBundleDisplayName</key>
    <string>TapeDigizer Tape Denoiser &amp; DC Filter</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>CFBundleShortVersionString</key>
    <string>${APP_VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${APP_VERSION}</string>
    <key>CSResourcesFileMapped</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>AudioComponents</key>
    <array>
        <dict>
            <key>name</key>
            <string>TapeDigizer: Tape Denoiser &amp; DC Filter</string>
            <key>description</key>
            <string>Spectral Magnetic Tape Hiss &amp; DC/Subsonic Filter with Shared Noise Sample Presets</string>
            <key>factoryFunction</key>
            <string>TapeDigizerDenoiserFactory</string>
            <key>manufacturer</key>
            <string>TpDg</string>
            <key>subtype</key>
            <string>TdNr</string>
            <key>type</key>
            <string>aufx</string>
            <key>version</key>
            <integer>${AU_VERSION_INT}</integer>
            <key>sandboxSafe</key>
            <true/>
            <key>tags</key>
            <array>
                <string>Effects</string>
                <string>Restoration</string>
                <string>Noise Reduction</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
EOF

printf "BNDL????" > "$CONTENTS_DIR/PkgInfo"

echo "=== 3. Ad-hoc Code Signing ${COMPONENT_NAME}.component ==="
codesign --force --deep --sign - "$BUNDLE_DIR"
codesign --verify --verbose "$BUNDLE_DIR"

USER_AU_DIR="$HOME/Library/Audio/Plug-Ins/Components"
mkdir -p "$USER_AU_DIR"
rm -rf "$USER_AU_DIR/${COMPONENT_NAME}.component"
cp -R "$BUNDLE_DIR" "$USER_AU_DIR/${COMPONENT_NAME}.component"
codesign --force --deep --sign - "$USER_AU_DIR/${COMPONENT_NAME}.component"

echo "=== Installed to: $USER_AU_DIR/${COMPONENT_NAME}.component ==="
