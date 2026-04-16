#!/bin/bash
# MINT Grader — macOS one-line installer
# Usage: curl -sL https://raw.githubusercontent.com/blueion0612/Mint_IDE_Teacher/main/install-mac.sh | bash

set -e

APP_NAME="MINT Grader"
REPO="blueion0612/Mint_IDE_Teacher"
INSTALL_DIR="/Applications"

echo ""
echo "=== MINT Grader Installer ==="
echo ""

ARCH=$(uname -m)
if [ "$ARCH" = "arm64" ]; then
    DMG_PATTERN="aarch64.dmg"
    echo "Detected: Apple Silicon (M1/M2/M3/M4)"
else
    DMG_PATTERN="x64.dmg"
    echo "Detected: Intel Mac"
fi

echo "Finding latest release..."
DMG_URL=$(curl -sL "https://api.github.com/repos/$REPO/releases/latest" | \
    grep "browser_download_url.*${DMG_PATTERN}" | \
    head -1 | \
    cut -d '"' -f 4)

if [ -z "$DMG_URL" ]; then
    echo "Error: Could not find DMG download URL"
    exit 1
fi

echo "Downloading: $(basename "$DMG_URL")"
TMPDIR=$(mktemp -d)
DMG_PATH="$TMPDIR/mint-grader.dmg"

curl -L "$DMG_URL" -o "$DMG_PATH" --progress-bar

echo "Installing..."
MOUNT_POINT=$(hdiutil attach "$DMG_PATH" -nobrowse -quiet | tail -1 | awk '{print $NF}')

APP_FOUND=$(find "$MOUNT_POINT" -name "*.app" -maxdepth 1 | head -1)
if [ -n "$APP_FOUND" ]; then
    ACTUAL_NAME=$(basename "$APP_FOUND" .app)
    rm -rf "$INSTALL_DIR/$ACTUAL_NAME.app"
    cp -R "$APP_FOUND" "$INSTALL_DIR/"
else
    echo "Error: No .app found in DMG"
    hdiutil detach "$MOUNT_POINT" -quiet
    exit 1
fi

hdiutil detach "$MOUNT_POINT" -quiet
xattr -cr "$INSTALL_DIR/$ACTUAL_NAME.app"
rm -rf "$TMPDIR"

echo ""
echo "=== Installation complete! ==="
echo "App installed to: $INSTALL_DIR/$ACTUAL_NAME.app"
echo ""

open "$INSTALL_DIR/$ACTUAL_NAME.app"
