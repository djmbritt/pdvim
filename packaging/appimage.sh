#!/usr/bin/env bash
# Packages a release build of pdvim for Linux as an AppImage in dist/, after
#
#     cargo build --release --locked --target TARGET
#
# usage: packaging/appimage.sh TARGET
#
# linuxdeploy puts the program in an AppDir with its desktop entry, icon and
# AppStream metadata, and the libraries it links to that a distribution
# cannot be counted on to have; the C library, and what has to come from
# the system to work with it (fontconfig, the GPU drivers), are left out.
# Build on the oldest distribution it should run on, as the release
# workflow does, since the AppImage needs a glibc at least as new.
set -euo pipefail

target=${1:?usage: packaging/appimage.sh TARGET}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
arch=${target%%-*}
id=io.github.djmbritt.pdvim
work="target/appimage"
appdir="$work/AppDir"
out="$root/dist"

mkdir -p "$work" "$out"
rm -rf "$appdir"

# linuxdeploy is itself an AppImage, which needs FUSE to run unless it is
# told to unpack itself first; CI runners and containers seldom have it.
export APPIMAGE_EXTRACT_AND_RUN=1
linuxdeploy="$work/linuxdeploy-$arch.AppImage"
if [ ! -x "$linuxdeploy" ]; then
    curl -fsSL -o "$linuxdeploy" \
        "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-$arch.AppImage"
    chmod +x "$linuxdeploy"
fi

# Named by the application's ID, as the AppStream metadata expects. Its
# entry starts the program itself, as linuxdeploy needs to find it there;
# the AppRun below sets WGPU_BACKEND instead.
sed 's/^Exec=env WGPU_BACKEND=vulkan /Exec=/' packaging/pdvim.desktop > "$work/$id.desktop"
install -Dm644 packaging/$id.metainfo.xml "$appdir/usr/share/metainfo/$id.metainfo.xml"
install -Dm644 -t "$appdir/usr/share/doc/pdvim" README.md LICENSE defaults.toml

LDAI_OUTPUT="$out/pdvim-$version-$arch.AppImage" \
    LDAI_UPDATE_INFORMATION="gh-releases-zsync|djmbritt|pdvim|latest|pdvim-*-$arch.AppImage.zsync" \
    "$linuxdeploy" \
    --appdir "$appdir" \
    --executable "target/$target/release/pdvim" \
    --desktop-file "$work/$id.desktop" \
    --custom-apprun packaging/AppRun \
    --icon-file packaging/pdvim.svg \
    --output appimage
# The file AppImageUpdate fetches changes by, which linuxdeploy leaves
# where it ran.
mv "pdvim-$version-$arch.AppImage.zsync" "$out/"

ls -l "$out"
