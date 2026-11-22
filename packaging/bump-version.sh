#!/usr/bin/env bash
# Raises pdvim's version in Cargo.toml, both lockfiles and the RPM spec,
# and prints the new version.
#
# usage: packaging/bump-version.sh [patch|minor|major]   (patch by default)
#
#   patch  0.1.0 -> 0.1.1
#   minor  0.1.3 -> 0.2.0
#   major  0.4.2 -> 1.0.0
set -euo pipefail

part=${1:-patch}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
IFS=. read -r major minor patch <<<"$current"
case "$part" in
patch) patch=$((patch + 1)) ;;
minor) minor=$((minor + 1)) patch=0 ;;
major) major=$((major + 1)) minor=0 patch=0 ;;
*)
    echo "bump-version.sh: say patch, minor or major, not $part" >&2
    exit 1
    ;;
esac
next="$major.$minor.$patch"

# The package's own version is the first `version =` line of Cargo.toml.
sed -i.bak "0,/^version = \".*\"/s//version = \"$next\"/" Cargo.toml
# In a lockfile, the line after pdvim's name.
for lock in Cargo.lock fuzz/Cargo.lock; do
    sed -i.bak "/^name = \"pdvim\"\$/{n;s/^version = \".*\"/version = \"$next\"/;}" "$lock"
done
# The RPM spec (packaging/rpm/make-srpm.sh sets it again from Cargo.toml).
sed -i.bak "s/^Version:\( *\).*/Version:\1$next/" packaging/rpm/pdvim.spec
rm -f Cargo.toml.bak Cargo.lock.bak fuzz/Cargo.lock.bak packaging/rpm/pdvim.spec.bak

echo "$next"
