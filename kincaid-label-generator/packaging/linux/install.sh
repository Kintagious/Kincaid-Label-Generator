#!/bin/sh
# Builds Kincaid Label Generator and adds it to your app menu (with the K icon).
# Needs Rust (https://rustup.rs) and GTK 3 headers (Debian/Ubuntu: sudo apt install libgtk-3-dev).
set -e
cd "$(dirname "$0")/../.."
cargo build --release
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/icons/hicolor/256x256/apps" "$HOME/.local/share/applications"
cp target/release/kincaid-label-generator "$HOME/.local/bin/"
cp assets/icon_256.png "$HOME/.local/share/icons/hicolor/256x256/apps/kincaid-label-generator.png"
cat > "$HOME/.local/share/applications/kincaid-label-generator.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Kincaid Label Generator
Comment=Make Build-To label sheets as PDF
Exec=$HOME/.local/bin/kincaid-label-generator %f
Icon=kincaid-label-generator
Terminal=false
Categories=Office;
MimeType=text/csv;
StartupWMClass=kincaid-label-generator
DESKTOP
echo "Installed. Look for Kincaid Label Generator in your app menu."
