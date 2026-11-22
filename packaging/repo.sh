#!/usr/bin/env bash
# Makes the apt and dnf repositories that GitHub Pages serves, from the
# .deb and .rpm packages of a release, signed with the repository's key.
#
# usage: packaging/repo.sh PACKAGES OUT URL
#
#   PACKAGES  a directory holding the release's .deb and .rpm packages
#   OUT       the directory to make the site in; emptied first
#   URL       where the site is served, as https://djmbritt.github.io/pdvim
#
# The key is the one gpg finds by REPO_KEY_ID, already imported: see
# packaging/repo/README.md. It needs apt-ftparchive (apt-utils),
# createrepo_c and rpmsign (rpm).
#
#   OUT/apt/          a flat apt repository: the .debs, Packages, and
#                     Release signed as InRelease and Release.gpg
#   OUT/rpm/          a dnf repository: the .rpms, each signed, and
#                     repodata/ with repomd.xml signed beside it
#   OUT/pdvim.gpg     the public key, for apt's signed-by
#   OUT/pdvim.asc     the public key, armoured, for dnf
#   OUT/pdvim.sources the apt source, to put in /etc/apt/sources.list.d/
#   OUT/pdvim.repo    the dnf repository, to put in /etc/yum.repos.d/
#   OUT/index.html    how to add either
#
# Only the packages given are in the repositories: each release replaces
# the one before, which is all that upgrading needs.
set -euo pipefail

packages=${1:?usage: packaging/repo.sh PACKAGES OUT URL}
out=${2:?usage: packaging/repo.sh PACKAGES OUT URL}
url=${3:?usage: packaging/repo.sh PACKAGES OUT URL}
url=${url%/}
key=${REPO_KEY_ID:?REPO_KEY_ID must name the signing key}

shopt -s nullglob
debs=("$packages"/*.deb)
rpms=("$packages"/*.rpm)
if [ ${#debs[@]} -eq 0 ] || [ ${#rpms[@]} -eq 0 ]; then
    echo "repo.sh: no .deb or no .rpm in $packages" >&2
    exit 1
fi

rm -rf "$out"
mkdir -p "$out/apt" "$out/rpm"
out=$(cd "$out" && pwd)

sign() {
    gpg --batch --yes --local-user "$key" "$@"
}

# apt: a flat repository, so that the packages of every architecture sit
# together, and apt picks the one for its own.
cp "${debs[@]}" "$out/apt/"
(
    cd "$out/apt"
    apt-ftparchive packages . >Packages
    gzip -9kn Packages
    apt-ftparchive \
        -o APT::FTPArchive::Release::Origin=pdvim \
        -o APT::FTPArchive::Release::Label=pdvim \
        -o APT::FTPArchive::Release::Suite=stable \
        -o APT::FTPArchive::Release::Description="pdvim, a keyboard-driven PDF reader" \
        release . >Release
    sign --clearsign --output InRelease Release
    sign --armor --detach-sign --output Release.gpg Release
)

# dnf: each package signed, and the repository's index too.
cp "${rpms[@]}" "$out/rpm/"
# Ubuntu's rpm looks for gpg by another name.
rpmsign --addsign --define "_gpg_name $key" --define "__gpg $(command -v gpg)" \
    "$out"/rpm/*.rpm
createrepo_c --quiet "$out/rpm"
sign --armor --detach-sign --output "$out/rpm/repodata/repomd.xml.asc" \
    "$out/rpm/repodata/repomd.xml"

gpg --batch --yes --export "$key" >"$out/pdvim.gpg"
gpg --batch --yes --armor --export "$key" >"$out/pdvim.asc"

cat >"$out/pdvim.sources" <<SOURCES
Types: deb
URIs: $url/apt/
Suites: ./
Signed-By: /etc/apt/keyrings/pdvim.gpg
SOURCES

cat >"$out/pdvim.repo" <<REPO
[pdvim]
name=pdvim
baseurl=$url/rpm/
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=$url/pdvim.asc
REPO

cat >"$out/index.html" <<HTML
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>pdvim packages</title>
<style>
body { font: 16px/1.5 system-ui, sans-serif; max-width: 46rem; margin: 3rem auto; padding: 0 1rem; }
pre { background: #f4f4f4; padding: 0.75rem 1rem; overflow-x: auto; }
</style>
</head>
<body>
<h1>pdvim packages</h1>
<p>Repositories for <a href="https://github.com/djmbritt/pdvim">pdvim</a>, a
keyboard-driven PDF reader, so that it is updated with the rest of the
system.</p>

<h2>Debian and Ubuntu</h2>
<pre>sudo install -d /etc/apt/keyrings
sudo curl -fsSLo /etc/apt/keyrings/pdvim.gpg $url/pdvim.gpg
sudo curl -fsSLo /etc/apt/sources.list.d/pdvim.sources $url/pdvim.sources
sudo apt update
sudo apt install pdvim</pre>

<h2>Fedora</h2>
<pre>sudo curl -fsSLo /etc/yum.repos.d/pdvim.repo $url/pdvim.repo
sudo dnf install pdvim</pre>

<h2>openSUSE</h2>
<pre>sudo zypper addrepo $url/pdvim.repo
sudo zypper install pdvim</pre>
</body>
</html>
HTML

find "$out" -type f | sort
