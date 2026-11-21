#!/usr/bin/env bash
# Packages a release build of pdvim for one target into dist/, after
#
#     cargo build --release --locked --target TARGET
#
# usage: packaging/dist.sh TARGET
#
#   Linux    pdvim-VERSION-TARGET.tar.gz: the program, its desktop entry and
#            icon, its manual pages, and the documents
#   macOS    the same as a .tar.gz, and pdvim.app in pdvim-VERSION-TARGET.dmg
#   Windows  pdvim-VERSION-TARGET.zip: pdvim.exe and the documents
#
# It runs in bash on all three, Git Bash on Windows.
set -euo pipefail

target=${1:?usage: packaging/dist.sh TARGET}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
name="pdvim-$version-$target"
build="target/$target/release"
work="target/dist"
stage="$work/$name"
out="$root/dist"

rm -rf "$stage"
mkdir -p "$stage" "$out"
cp README.md LICENSE defaults.toml "$stage/"

case "$target" in
*-windows-*)
    cp "$build/pdvim.exe" "$stage/"
    rm -f "$out/$name.zip"
    (cd "$work" && 7z a -tzip "$out/$name.zip" "$name" >/dev/null)
    ;;

*-apple-darwin)
    cp "$build/pdvim" docs/man/pdvim.1 docs/man/pdvim.5 "$stage/"
    tar -C "$work" -czf "$out/$name.tar.gz" "$name"

    # An application bundle, so that pdvim can be started from Finder,
    # Launchpad and the Dock.
    app="$work/dmg/pdvim.app"
    rm -rf "$work/dmg"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$build/pdvim" "$app/Contents/MacOS/pdvim"
    if command -v rsvg-convert >/dev/null && command -v iconutil >/dev/null; then
        iconset="$work/pdvim.iconset"
        rm -rf "$iconset"
        mkdir -p "$iconset"
        for size in 16 32 128 256 512; do
            double=$((size * 2))
            rsvg-convert -w "$size" -h "$size" packaging/pdvim.svg \
                -o "$iconset/icon_${size}x${size}.png"
            rsvg-convert -w "$double" -h "$double" packaging/pdvim.svg \
                -o "$iconset/icon_${size}x${size}@2x.png"
        done
        iconutil -c icns "$iconset" -o "$app/Contents/Resources/pdvim.icns"
    else
        echo "dist.sh: rsvg-convert or iconutil missing; the app has no icon" >&2
    fi
    cat >"$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>pdvim</string>
    <key>CFBundleDisplayName</key>
    <string>pdvim</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.djmbritt.pdvim</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>pdvim</string>
    <key>CFBundleIconFile</key>
    <string>pdvim</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST
    # Not signed with a developer certificate: an ad-hoc signature keeps
    # the bundle whole, and Gatekeeper still asks before the first start.
    codesign --force --deep --sign - "$app"
    ln -s /Applications "$work/dmg/Applications"
    rm -f "$out/$name.dmg"
    hdiutil create -volname pdvim -srcfolder "$work/dmg" -format UDZO \
        -ov "$out/$name.dmg" >/dev/null
    ;;

*-linux-*)
    cp "$build/pdvim" packaging/pdvim.desktop packaging/pdvim.svg \
        docs/man/pdvim.1 docs/man/pdvim.5 "$stage/"
    tar -C "$work" -czf "$out/$name.tar.gz" "$name"
    ;;

*)
    echo "dist.sh: no packaging for $target" >&2
    exit 1
    ;;
esac

ls -l "$out"
