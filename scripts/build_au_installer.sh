#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$ROOT_DIR"

# 1. Ensure the AudioUnit .component bundle is up to date
"$SCRIPT_DIR/build_audiounit.sh"

APP_VERSION="$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' Cargo.toml | head -n 1)"
if [[ -z "$APP_VERSION" ]]; then
    APP_VERSION="1.5.2"
fi

COMPONENT_NAME="TapeDigizerDenoiser"
COMPONENT_BUNDLE="$ROOT_DIR/${COMPONENT_NAME}.component"
PKG_NAME="${COMPONENT_NAME}-Installer.pkg"
OUTPUT_PKG="$ROOT_DIR/$PKG_NAME"

STAGING_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tapedigizer_au_pkg.XXXXXX")"
trap 'rm -rf "$STAGING_DIR"' EXIT

PAYLOAD_DIR="$STAGING_DIR/payload/Library/Audio/Plug-Ins/Components"
SCRIPTS_DIR="$STAGING_DIR/scripts"
RESOURCES_DIR="$STAGING_DIR/resources"
COMPONENT_PKG="$STAGING_DIR/${COMPONENT_NAME}-component.pkg"
DIST_XML="$STAGING_DIR/Distribution.xml"

mkdir -p "$PAYLOAD_DIR" "$SCRIPTS_DIR" "$RESOURCES_DIR"

echo "=== 4. Preparing installer payload & scripts ==="
cp -R "$COMPONENT_BUNDLE" "$PAYLOAD_DIR/${COMPONENT_NAME}.component"

# Remove extended attributes / resource forks so pkgbuild produces a clean bundle
xattr -cr "$PAYLOAD_DIR/${COMPONENT_NAME}.component" 2>/dev/null || true
codesign --force --deep --sign - "$PAYLOAD_DIR/${COMPONENT_NAME}.component"

# Create postinstall script
cat << 'EOF' > "$SCRIPTS_DIR/postinstall"
#!/usr/bin/env bash
set -e

COMPONENT_NAME="TapeDigizerDenoiser.component"
INSTALLED_COMP="$3/Library/Audio/Plug-Ins/Components/$COMPONENT_NAME"

# Determine the active console user so we can also sync to ~/Library/Audio/Plug-Ins/Components
# and initialize the shared NoiseProfiles preset folder with user write permissions.
CONSOLE_USER="$(stat -f "%Su" /dev/console 2>/dev/null || true)"
if [[ -z "$CONSOLE_USER" || "$CONSOLE_USER" == "root" ]]; then
    CONSOLE_USER="${USER:-}"
fi

if [[ -d "$INSTALLED_COMP" ]]; then
    xattr -cr "$INSTALLED_COMP" 2>/dev/null || true
    /usr/bin/codesign --force --deep --sign - "$INSTALLED_COMP" 2>/dev/null || true
fi

if [[ -n "$CONSOLE_USER" && "$CONSOLE_USER" != "root" ]]; then
    USER_HOME="$(dscl . -read "/Users/$CONSOLE_USER" NFSHomeDirectory 2>/dev/null | awk '{print $2}' || true)"
    if [[ -n "$USER_HOME" && -d "$USER_HOME" ]]; then
        USER_COMP_DIR="$USER_HOME/Library/Audio/Plug-Ins/Components"
        USER_PRESETS_DIR="$USER_HOME/Library/Application Support/TapeDigizer/NoiseProfiles"

        mkdir -p "$USER_COMP_DIR"
        mkdir -p "$USER_PRESETS_DIR"

        if [[ -d "$INSTALLED_COMP" && "$INSTALLED_COMP" != "$USER_COMP_DIR/$COMPONENT_NAME" ]]; then
            rm -rf "$USER_COMP_DIR/$COMPONENT_NAME"
            cp -R "$INSTALLED_COMP" "$USER_COMP_DIR/$COMPONENT_NAME"
            xattr -cr "$USER_COMP_DIR/$COMPONENT_NAME" 2>/dev/null || true
            /usr/bin/codesign --force --deep --sign - "$USER_COMP_DIR/$COMPONENT_NAME" 2>/dev/null || true
        fi

        chown -R "$CONSOLE_USER:staff" "$USER_COMP_DIR/$COMPONENT_NAME" 2>/dev/null || true
        chown -R "$CONSOLE_USER:staff" "$USER_HOME/Library/Application Support/TapeDigizer" 2>/dev/null || true
    fi
fi

# Refresh macOS AudioComponentRegistrar so Logic Pro immediately detects the plugin
/usr/bin/killall -9 AudioComponentRegistrar 2>/dev/null || true

exit 0
EOF
chmod +x "$SCRIPTS_DIR/postinstall"

# Welcome HTML
cat << EOF > "$RESOURCES_DIR/welcome.html"
<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="utf-8">
<style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif; font-size: 13px; line-height: 1.5; color: #222; margin: 16px; }
    h2 { color: #d97706; margin-top: 0; }
    .box { background: rgba(217, 119, 6, 0.08); border-left: 3px solid #d97706; padding: 10px 14px; border-radius: 4px; margin: 12px 0; }
    code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12px; }
</style>
</head>
<body>
    <h2>TapeDigizer: Tape Denoiser &amp; DC Filter (AudioUnit v${APP_VERSION})</h2>
    <p>Dieser Installer richtet das macOS AudioUnit-Plug-In <strong>TapeDigizerDenoiser.component</strong> für <strong>Logic Pro</strong>, GarageBand, MainStage und andere AudioUnit-fähige DAWs ein.</p>
    <div class="box">
        <strong>Funktionen des AudioUnit-Plug-Ins:</strong>
        <ul>
            <li>Spektraler Tonband- &amp; Kassetten-Rauschfilter (2048-Punkt STFT mit Soft-Wiener-Glättung)</li>
            <li>Statische DC-Offset-Korrektur &amp; zweipoliger Butterworth-Subsonic-Hochpass (10–60 Hz)</li>
            <li>Direkter Zugriff auf alle in <strong>TapeDigizer</strong> gespeicherten Rausch-Samples (<code>NoiseProfiles</code>)</li>
            <li><strong>Live-Learn im DAW-Track:</strong> Lernt Bandrauschen auf Wunsch direkt in Logic Pro</li>
            <li>Vorhör-Modi: <em>Cleaned (Wet)</em>, <em>Bypass (Original)</em> und <em>Noise Only (Delta)</em></li>
        </ul>
    </div>
    <p>Installationsort: <code>Library/Audio/Plug-Ins/Components/TapeDigizerDenoiser.component</code></p>
</body>
</html>
EOF

# Conclusion HTML
cat << EOF > "$RESOURCES_DIR/conclusion.html"
<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="utf-8">
<style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif; font-size: 13px; line-height: 1.5; color: #222; margin: 16px; }
    h2 { color: #16a34a; margin-top: 0; }
    .box { background: rgba(22, 163, 74, 0.08); border-left: 3px solid #16a34a; padding: 10px 14px; border-radius: 4px; margin: 12px 0; }
    code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12px; }
</style>
</head>
<body>
    <h2>Installation erfolgreich abgeschlossen!</h2>
    <p>Das AudioUnit-Plug-In <strong>TapeDigizer: Tape Denoiser &amp; DC Filter</strong> ist nun installiert und sofort einsatzbereit.</p>
    <div class="box">
        <strong>So findest du das Plug-In in Logic Pro:</strong>
        <ol>
            <li>Öffne einen Audio-Channel-Strip in Logic Pro.</li>
            <li>Klicke auf einen Effekt-Slot (<em>Audio FX</em>).</li>
            <li>Wähle <strong>Audio Units &rarr; TapeDigizer &rarr; Tape Denoiser &amp; DC Filter</strong>.</li>
        </ol>
    </div>
    <p>Alle in der <strong>TapeDigizer</strong>-App gespeicherten Rausch-Samples stehen automatisch unter:<br>
    <code>~/Library/Application Support/TapeDigizer/NoiseProfiles/</code> als Presets bereit.</p>
</body>
</html>
EOF

# Component plist (disable bundle relocation so macOS Installer always puts it in Audio/Plug-Ins/Components)
COMP_PLIST="$STAGING_DIR/component.plist"
pkgbuild --analyze --root "$STAGING_DIR/payload" "$COMP_PLIST"
/usr/libexec/PlistBuddy -c "Set :0:BundleIsRelocatable false" "$COMP_PLIST" 2>/dev/null || true

echo "=== 5. Building component package with pkgbuild ==="
pkgbuild \
    --root "$STAGING_DIR/payload" \
    --component-plist "$COMP_PLIST" \
    --scripts "$SCRIPTS_DIR" \
    --identifier "com.andreas.tapedigizer.pkg.audiounit" \
    --version "$APP_VERSION" \
    --install-location "/" \
    "$COMPONENT_PKG"

# Distribution XML supporting both User Home installation (no root needed) and System installation
cat << EOF > "$DIST_XML"
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>TapeDigizer: Tape Denoiser &amp; DC Filter (AudioUnit)</title>
    <organization>com.andreas.tapedigizer</organization>
    <domains enable_anywhere="false" enable_currentUserHome="true" enable_localSystem="true"/>
    <options customize="never" require-scripts="true" rootVolumeOnly="false" hostArchitectures="arm64,x86_64"/>
    <welcome file="welcome.html" mime-type="text/html"/>
    <conclusion file="conclusion.html" mime-type="text/html"/>
    <choices-outline>
        <line choice="default">
            <line choice="com.andreas.tapedigizer.pkg.audiounit"/>
        </line>
    </choices-outline>
    <choice id="default"/>
    <choice id="com.andreas.tapedigizer.pkg.audiounit" visible="false">
        <pkg-ref id="com.andreas.tapedigizer.pkg.audiounit"/>
    </choice>
    <pkg-ref id="com.andreas.tapedigizer.pkg.audiounit" version="${APP_VERSION}" onConclusion="none">${COMPONENT_NAME}-component.pkg</pkg-ref>
</installer-gui-script>
EOF

echo "=== 6. Building final macOS Installer (${PKG_NAME}) with productbuild ==="
rm -f "$OUTPUT_PKG"
productbuild \
    --distribution "$DIST_XML" \
    --resources "$RESOURCES_DIR" \
    --package-path "$STAGING_DIR" \
    --identifier "com.andreas.tapedigizer.installer.audiounit" \
    --version "$APP_VERSION" \
    "$OUTPUT_PKG"

echo "=== Successfully created macOS AudioUnit Installer: $OUTPUT_PKG ==="
