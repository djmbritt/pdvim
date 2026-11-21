#!/bin/sh
# Measures how much of pdvim's own code the fuzz targets reach, running
# each over its corpus and seeds, and prints a Markdown table of it: line,
# function and region coverage for every file in src/, and the total.
#
#   fuzz/coverage.sh              every target
#   fuzz/coverage.sh pdf formats  only these
#
# Needs a nightly toolchain with llvm-tools:
#   rustup component add --toolchain nightly llvm-tools-preview
# The detailed report per target is left in coverage/<target>/.
set -eu
cd "$(dirname "$0")"

if [ $# -eq 0 ]; then
    set -- $(cargo +nightly fuzz list)
fi

bin=$(rustc +nightly --print sysroot)/lib/rustlib/$(rustc +nightly -vV | sed -n 's/^host: //p')/bin
triple=$(rustc +nightly -vV | sed -n 's/^host: //p')
# Leaks are no matter here, and MuPDF's on a damaged comic page would stop
# the formats target (see run.sh).
ASAN_OPTIONS="${ASAN_OPTIONS:+$ASAN_OPTIONS:}detect_leaks=0"
export ASAN_OPTIONS
objects=""
profiles=""
for target in "$@"; do
    mkdir -p "corpus/$target"
    # Each target over what it has learnt and what it starts from.
    cargo +nightly fuzz coverage "$target" "corpus/$target" "seeds/$target" \
        -- -detect_leaks=0 >&2
    objects="$objects -object target/$triple/coverage/$triple/release/$target"
    profiles="$profiles coverage/$target/coverage.profdata"
done

# shellcheck disable=SC2086
"$bin/llvm-profdata" merge -sparse $profiles -o coverage/all.profdata
# Only pdvim's own code: not the standard library, the crates it uses, or
# the fuzz targets themselves.
# shellcheck disable=SC2086
"$bin/llvm-cov" export -summary-only -instr-profile coverage/all.profdata \
    -ignore-filename-regex '(/\.cargo/|/rustc/|/fuzz/|/library/)' $objects |
    python3 -c '
import json, os, sys

data = json.load(sys.stdin)["data"][0]
rows = []
for entry in data["files"]:
    name = entry["filename"]
    if "/src/" not in name:
        continue
    rows.append(("src/" + name.split("/src/", 1)[1], entry["summary"]))
rows.sort()

def share(part):
    count = part["count"]
    return "–" if count == 0 else "%.0f%%" % (100 * part["covered"] / count)

print("| File | Lines | Functions | Regions |")
print("|------|------:|----------:|--------:|")
for name, summary in rows:
    print("| `%s` | %s | %s | %s |" % (
        name, share(summary["lines"]), share(summary["functions"]), share(summary["regions"])))
totals = data["totals"]
print("| **Total** | **%s** | **%s** | **%s** |" % (
    share(totals["lines"]), share(totals["functions"]), share(totals["regions"])))
'
