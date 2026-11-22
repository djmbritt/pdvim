#!/usr/bin/env bash
# Makes pdvim's source RPM from this git checkout: the source tarball
# (Source0), the tarball of the crates it needs (Source1, so that the RPM
# builds without the network) and the .src.rpm, in OUTDIR.
#
# usage: packaging/rpm/make-srpm.sh [-o OUTDIR] [REF]
#
#   REF     the commit to package, a tag such as v0.1.31 by default the
#           latest v* tag reachable from HEAD; say HEAD for what is
#           committed now (uncommitted changes are never packaged)
#   OUTDIR  where to put the files, dist/srpm by default
#
# Needs git, cargo (with the network, to fetch the crates), tar, xz and
# rpmbuild (rpm-build on Fedora). Copr runs it through .copr/Makefile.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
outdir=$root/dist/srpm
while getopts o: opt; do
    case $opt in
    o) outdir=$OPTARG ;;
    *)
        sed -n '2,13s/^# \{0,1\}//p' "$0" >&2
        exit 2
        ;;
    esac
done
shift $((OPTIND - 1))
cd "$root"

ref=${1:-$(git describe --tags --abbrev=0 --match 'v[0-9]*' HEAD)}
commit=$(git rev-parse --verify "$ref^{commit}")
version=$(git show "$commit:Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -n1)
if [ -z "$version" ]; then
    echo "make-srpm.sh: no version in Cargo.toml at $ref" >&2
    exit 1
fi
case $ref in
v[0-9]*)
    if [ "$ref" != "v$version" ]; then
        echo "make-srpm.sh: tag $ref does not match Cargo.toml version $version" >&2
        exit 1
    fi
    ;;
esac
name=pdvim
echo "make-srpm.sh: packaging $name $version from $ref ($commit)"

mkdir -p "$outdir"
outdir=$(cd "$outdir" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Source0: the same tree GitHub's tarball of the tag holds.
git archive --format=tar.gz --prefix="$name-$version/" -o "$work/$name-$version.tar.gz" "$commit"

# Source1: vendor/, with every crate of Cargo.lock.
tar -xzf "$work/$name-$version.tar.gz" -C "$work"
(
    cd "$work/$name-$version"
    # Its own CARGO_HOME, so the crates come from crates.io and not from a
    # source replacement in the user's cargo configuration.
    CARGO_HOME="$work/cargo-home" cargo vendor --locked --versioned-dirs vendor >/dev/null
)
# Without times, owners or order that change from one run to the next.
tar --sort=name --mtime="@$(git show -s --format=%ct "$commit")" \
    --owner=0 --group=0 --numeric-owner \
    -C "$work/$name-$version" -cf - vendor |
    xz -T0 -6 >"$work/$name-$version-vendor.tar.xz"

# The spec, with the version of the tree packaged and a changelog entry for
# it. The spec comes from this checkout, so that tags made before it
# existed can be packaged too.
date=$(LC_ALL=C git show -s --format=%cd --date=format:'%a %b %d %Y' "$commit")
sed -e "s/^Version:.*/Version:        $version/" \
    -e '/^%changelog/,$d' \
    "$root/packaging/rpm/$name.spec" >"$work/$name.spec"
cat >>"$work/$name.spec" <<EOF
%changelog
* $date David Britt - $version-1
- Built from $ref ($(git rev-parse --short "$commit")); see https://github.com/djmbritt/pdvim/releases
EOF

cp "$work/$name-$version.tar.gz" "$work/$name-$version-vendor.tar.xz" "$outdir/"
rpmbuild -bs \
    --define "_sourcedir $work" \
    --define "_srcrpmdir $outdir" \
    "$work/$name.spec"
ls -l "$outdir"
