#!/usr/bin/env bash
# Prints the Homebrew cask for a release of pdvim, which installs the
# program the release built for the Mac it runs on, Apple silicon or Intel,
# with its manual pages, from the release's .tar.gz.
#
# usage: packaging/homebrew/cask.sh VERSION SHA256SUMS [URL]
#
#   VERSION     the release's version, as 0.1.31
#   SHA256SUMS  the release's SHA256SUMS file, which has the checksums
#   URL         where the release's files are; by default, the release
#               on GitHub. Given only to try the cask out.
#
# A cask rather than a formula: Homebrew lets only casks install a
# different prebuilt download on each kind of processor. Only for macOS:
# the .deb, .rpm, AppImage, Flatpak and Nix flake serve Linux.
#
# The program is not notarized, so macOS would refuse to run it as a
# download it cannot check. The cask takes the quarantine mark off the one
# file it installs, as installing from a tap is the user's own choice.
set -euo pipefail

version=${1:?usage: packaging/homebrew/cask.sh VERSION SHA256SUMS [URL]}
sums=${2:?usage: packaging/homebrew/cask.sh VERSION SHA256SUMS [URL]}
url=${3:-'https://github.com/djmbritt/pdvim/releases/download/v#{version}'}
url=${url%/}

# The checksum of the package for TARGET, from SHA256SUMS.
sum() {
    local file="pdvim-$version-$1.tar.gz"
    local found
    found=$(awk -v file="$file" '$2 == file || $2 == "*" file { print $1 }' "$sums")
    if [ -z "$found" ]; then
        echo "cask.sh: no checksum for $file in $sums" >&2
        exit 1
    fi
    echo "$found"
}

cat <<RUBY
# Made by packaging/homebrew/cask.sh in djmbritt/pdvim for each release;
# changes made here are lost at the next.
cask "pdvim" do
  arch arm: "aarch64", intel: "x86_64"

  version "$version"
  sha256 arm:   "$(sum aarch64-apple-darwin)",
         intel: "$(sum x86_64-apple-darwin)"

  url "$url/pdvim-#{version}-#{arch}-apple-darwin.tar.gz"
  name "pdvim"
  desc "Keyboard-driven PDF reader with vi keybindings"
  homepage "https://github.com/djmbritt/pdvim"

  livecheck do
    url :homepage
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  binary "pdvim-#{version}-#{arch}-apple-darwin/pdvim"
  manpage "pdvim-#{version}-#{arch}-apple-darwin/pdvim.1"
  manpage "pdvim-#{version}-#{arch}-apple-darwin/pdvim.5"

  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", staged_path.to_s]
  end

  zap trash: "~/.config/pdvim"
end
RUBY
