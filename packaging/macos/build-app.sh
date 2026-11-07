#!/bin/sh
# Builds target/release/Reclaimer.app — double-click to run, drag to
# Applications to install. Needs only Xcode's command line tools.
set -eu
cd "$(dirname "$0")/../.."

cargo build --release
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n 1)
APP=target/release/Reclaimer.app

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/Reclaimer "$APP/Contents/MacOS/Reclaimer"
cp icons/icon.icns "$APP/Contents/Resources/Reclaimer.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>                 <string>Reclaimer</string>
    <key>CFBundleDisplayName</key>          <string>Reclaimer</string>
    <key>CFBundleIdentifier</key>           <string>com.dincertekin.reclaimer</string>
    <key>CFBundleExecutable</key>           <string>Reclaimer</string>
    <key>CFBundleIconFile</key>             <string>Reclaimer</string>
    <key>CFBundlePackageType</key>          <string>APPL</string>
    <key>CFBundleShortVersionString</key>   <string>$VERSION</string>
    <key>CFBundleVersion</key>              <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>       <string>11.0</string>
    <key>LSApplicationCategoryType</key>    <string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key>      <true/>
    <key>NSHumanReadableCopyright</key>     <string>MIT License</string>
</dict>
</plist>
PLIST

# Ad-hoc signature: required to run on Apple silicon. Not notarized, so
# the first launch on another Mac needs right-click → Open.
codesign --force --sign - "$APP"
echo "Built $APP"
