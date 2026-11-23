#!/bin/sh
# Measures how much of pdvim's own code the tests reach, and prints a
# Markdown table of it: line, function and region coverage for every file
# in src/, and the total. A report to browse, line by line, is left in
# target/coverage/html/index.html.
#
#   ./coverage.sh
#
# Needs llvm-tools for the toolchain in use:
#   rustup component add llvm-tools-preview
# The tests are built apart from the usual ones, in target/coverage, so
# that the instrumented build does not replace them.
set -eu
cd "$(dirname "$0")"

triple=$(rustc -vV | sed -n 's/^host: //p')
bin=$(rustc --print sysroot)/lib/rustlib/$triple/bin
if [ ! -x "$bin/llvm-profdata" ]; then
    echo "coverage.sh: llvm-tools is missing; rustup component add llvm-tools-preview" >&2
    exit 1
fi

out=target/coverage
rm -rf "$out/profiles" "$out/html"
mkdir -p "$out/profiles"
export CARGO_TARGET_DIR="$out/build"
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C instrument-coverage"
# Each test process writes a profile of its own.
export LLVM_PROFILE_FILE="$PWD/$out/profiles/%p-%m.profraw"

cargo test --all-targets >&2
# The test programs just run, to read the coverage out of.
objects=$(cargo test --all-targets --no-run --message-format=json 2>/dev/null |
    python3 -c '
import json, sys
for line in sys.stdin:
    message = json.loads(line)
    if message.get("reason") == "compiler-artifact" and message.get("executable") \
            and message["profile"]["test"]:
        print("-object", message["executable"])
')

"$bin/llvm-profdata" merge -sparse "$out"/profiles/*.profraw -o "$out/tests.profdata"
# Only pdvim's own code: not the standard library, the crates it uses, or
# the fuzz targets and the tests' own helpers. Nor what no test can run:
# main.rs opens the window, and clipboard.rs hands pictures to the
# system's clipboard, which a machine running the tests may not have.
ignore='(/\.cargo/|/rustc/|/fuzz/|/library/|/target/|/src/app/testing\.rs'
ignore="$ignore"'|/src/main\.rs|/src/clipboard\.rs)'
# shellcheck disable=SC2086
"$bin/llvm-cov" show -format=html -output-dir "$out/html" -show-line-counts-or-regions \
    -instr-profile "$out/tests.profdata" -ignore-filename-regex "$ignore" $objects >/dev/null
# The objects are split into words on purpose, and the Python in quotes
# is not the shell's to expand.
# shellcheck disable=SC2086,SC2016
"$bin/llvm-cov" export -summary-only -instr-profile "$out/tests.profdata" \
    -ignore-filename-regex "$ignore" $objects |
    python3 -c '
import json, sys

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
