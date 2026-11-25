#!/usr/bin/env bash
# Writes the winget manifests for a release of pdvim, which install the
# program from the release's .zip for Windows as a portable package, with
# `pdvim` on the PATH.
#
# usage: packaging/winget/manifest.sh VERSION SHA256SUMS DIR [URL]
#
#   VERSION     the release's version, as 0.1.31
#   SHA256SUMS  the release's SHA256SUMS file, which has the checksum
#   DIR         where to write the three manifests; made if missing
#   URL         where the release's files are; by default, the release
#               on GitHub. Given only to try the manifests out.
#
# winget-pkgs keeps a package as three files: the version, the installer
# and the default locale. `wingetcreate submit DIR` sends them there.
set -euo pipefail

usage="usage: packaging/winget/manifest.sh VERSION SHA256SUMS DIR [URL]"
version=${1:?$usage}
sums=${2:?$usage}
dir=${3:?$usage}
url=${4:-"https://github.com/djmbritt/pdvim/releases/download/v$version"}
url=${url%/}

id=djmbritt.pdvim
schema=1.10.0
target=x86_64-pc-windows-msvc
name="pdvim-$version-$target"
file="$name.zip"

sha=$(awk -v file="$file" '$2 == file || $2 == "*" file { print toupper($1) }' "$sums")
if [ -z "$sha" ]; then
    echo "manifest.sh: no checksum for $file in $sums" >&2
    exit 1
fi

mkdir -p "$dir"
header="# Made by packaging/winget/manifest.sh in djmbritt/pdvim for each release."

cat >"$dir/$id.yaml" <<YAML
$header
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.version.$schema.schema.json

PackageIdentifier: $id
PackageVersion: $version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $schema
YAML

# Rust's MSVC builds link the Visual C++ runtime as a DLL, so it is asked
# for as a dependency.
cat >"$dir/$id.installer.yaml" <<YAML
$header
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.installer.$schema.schema.json

PackageIdentifier: $id
PackageVersion: $version
MinimumOSVersion: 10.0.0.0
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
- RelativeFilePath: $name\\pdvim.exe
  PortableCommandAlias: pdvim
Dependencies:
  PackageDependencies:
  - PackageIdentifier: Microsoft.VCRedist.2015+.x64
Installers:
- Architecture: x64
  InstallerUrl: $url/$file
  InstallerSha256: $sha
ManifestType: installer
ManifestVersion: $schema
YAML

cat >"$dir/$id.locale.en-US.yaml" <<YAML
$header
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.defaultLocale.$schema.schema.json

PackageIdentifier: $id
PackageVersion: $version
PackageLocale: en-US
Publisher: David Britt
PublisherUrl: https://github.com/djmbritt
PublisherSupportUrl: https://github.com/djmbritt/pdvim/issues
Author: David Britt
PackageName: pdvim
PackageUrl: https://github.com/djmbritt/pdvim
License: AGPL-3.0-or-later
LicenseUrl: https://github.com/djmbritt/pdvim/blob/v$version/LICENSE
ShortDescription: A minimal, keyboard-driven PDF reader
Description: A minimal, keyboard-driven PDF reader with vi keybindings, tabs, search, visual mode and highlights.
Moniker: pdvim
Tags:
- pdf
- reader
- viewer
- vim
- epub
ReleaseNotesUrl: https://github.com/djmbritt/pdvim/releases/tag/v$version
ManifestType: defaultLocale
ManifestVersion: $schema
YAML

ls -l "$dir"
