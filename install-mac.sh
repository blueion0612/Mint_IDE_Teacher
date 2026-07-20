#!/bin/bash
# MINT Grader — macOS Source Build Installer
#
# 서명 인증서 없이 CI에서 빌드된 .dmg/.pkg는 macOS에서 실행 시 크래시가
# 보고되어, 학생용 IDE와 동일하게 소스 빌드 방식으로 설치한다.
# Usage:
#   curl -sL https://raw.githubusercontent.com/blueion0612/Mint_IDE_Teacher/main/install-mac.sh | bash

set -e

REPO="blueion0612/Mint_IDE_Teacher"
BUILD_DIR="$HOME/MINT_Grader_Source"
INSTALL_DIR="/Applications"

echo ""
echo "=============================="
echo "  MINT Grader — Source Build"
echo "=============================="
echo ""

check() { command -v "$1" &>/dev/null; }
# /usr/bin/gcc, git, etc. are ALWAYS-present xcrun stubs even with no CLT
# installed — `xcode-select -p` is the real probe.
have_clt() { xcode-select -p &>/dev/null && [ -e "$(xcode-select -p 2>/dev/null)" ]; }

# ─── 1. Xcode Command Line Tools ───
if ! have_clt; then
    echo "[1/5] Installing Xcode Command Line Tools..."
    echo "       (설치 팝업이 뜨면 '설치'를 누르고 기다리세요.)"
    xcode-select --install 2>/dev/null || true
    for i in $(seq 1 360); do have_clt && break; sleep 5; done
    if ! have_clt; then
        echo "  [FAIL] Xcode Command Line Tools 설치가 완료되지 않았습니다. 'xcode-select --install'를 직접 실행한 뒤 다시 시도하세요."
        exit 1
    fi
fi
echo "[1/5] Xcode CLT: OK"

# ─── 2. Homebrew ───
if ! check brew; then
    echo "[2/5] Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
fi
if [ -x "/opt/homebrew/bin/brew" ]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
elif [ -x "/usr/local/bin/brew" ]; then
    eval "$(/usr/local/bin/brew shellenv)"
fi
echo "[2/5] Homebrew: OK ($(brew --prefix))"

# ─── 3. Build tools (Node + Rust) ───
echo "[3/5] Installing build tools (Node / Rust)..."
NEED=""
check node  || NEED="$NEED node"
check rustc || NEED="$NEED rust"
if [ -n "$NEED" ]; then
    brew install $NEED
fi
echo "[3/5] Tools: OK"

# ─── 4. Source clone (idempotent — wipe + reclone for clean retry) ───
echo "[4/5] Cloning source to $BUILD_DIR ..."
rm -rf "$BUILD_DIR"
git clone "https://github.com/$REPO.git" "$BUILD_DIR"
cd "$BUILD_DIR"

# ─── 5. Build + install ───
echo "[5/5] Building (3~7 minutes, downloads Rust crates on first run)..."
npm install
# --bundles app: .app만 생성. 기본값(dmg 포함)은 bundler가 AppleScript로
# Finder를 조작해 curl|bash 환경에서 Automation 프롬프트/실패를 유발한다.
npm run tauri build -- --bundles app

APP_PATH=$(find "$BUILD_DIR/src-tauri/target/release/bundle/macos" -maxdepth 1 -name "*.app" | head -1)
if [ -z "$APP_PATH" ]; then
    echo "  [FAIL] Build produced no .app at expected location."
    exit 1
fi
APP_NAME=$(basename "$APP_PATH" .app)
echo "Installing $APP_NAME.app to $INSTALL_DIR ..."
rm -rf "$INSTALL_DIR/$APP_NAME.app"
cp -R "$APP_PATH" "$INSTALL_DIR/"
xattr -cr "$INSTALL_DIR/$APP_NAME.app"

echo ""
echo "=============================="
echo "  Build complete!"
echo "=============================="
echo ""
echo "  App:    $INSTALL_DIR/$APP_NAME.app"
echo "  Source: $BUILD_DIR"
echo ""

open "$INSTALL_DIR/$APP_NAME.app"
