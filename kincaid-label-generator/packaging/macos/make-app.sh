#!/bin/bash
# Builds "Kincaid Label Generator.app" (with the K icon).
#   bash packaging/macos/make-app.sh              # for this Mac
#   bash packaging/macos/make-app.sh --universal  # runs on both Apple Silicon and Intel Macs
# Needs Rust (https://rustup.rs) and Apple's command line tools (xcode-select --install).
set -e
cd "$(dirname "$0")/../.."
NAME="Kincaid Label Generator"
EXE=kincaid-label-generator
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)

if [ "$1" = "--universal" ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/universal
  lipo -create -output target/universal/$EXE \
    target/aarch64-apple-darwin/release/$EXE target/x86_64-apple-darwin/release/$EXE
  BIN=target/universal/$EXE
else
  cargo build --release
  BIN=target/release/$EXE
fi

APP="target/$NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/$EXE"
cp assets/icon.icns "$APP/Contents/Resources/AppIcon.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>com.adamkincaid.labelgenerator</string>
  <key>CFBundleExecutable</key><string>$EXE</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>Developed by Adam Kincaid</string>
</dict>
</plist>
PLIST
# "ad-hoc" signature so macOS will run it on Apple Silicon
codesign --force --deep --sign - "$APP" 2>/dev/null || true
echo
echo "Done: $APP  - drag it into your Applications folder."
