#!/bin/sh
# Fuzzes one target, or each in turn, starting from the seeds and with a
# dictionary where there is one.
#
#   fuzz/run.sh                 every target for 60 seconds each
#   fuzz/run.sh pdf             the pdf target until stopped
#   fuzz/run.sh pdf 600         the pdf target for ten minutes
#
# Anything after the time is passed to libFuzzer, as in -jobs=4.
set -eu
cd "$(dirname "$0")"

run() {
    target=$1
    seconds=$2
    shift 2
    mkdir -p "corpus/$target"
    # An input that takes ten seconds is a hang, not a slow input.
    set -- -max_total_time="$seconds" -timeout=10 "$@"
    case $target in
        pdf) set -- "$@" -dict=dicts/pdf.dict -rss_limit_mb=4096 ;;
        # MuPDF leaks a little reading a damaged comic page (cbz_load_page
        # keeps its buffer in a variable not marked fz_var), which would
        # stop every run.
        formats) set -- "$@" -rss_limit_mb=4096 -detect_leaks=0 ;;
        config) set -- "$@" -dict=dicts/config.dict ;;
    esac
    cargo +nightly fuzz run "$target" "corpus/$target" "seeds/$target" -- "$@"
}

if [ $# -eq 0 ]; then
    for target in $(cargo +nightly fuzz list); do
        run "$target" 60
    done
else
    target=$1
    seconds=${2:-0}
    shift $(($# < 2 ? $# : 2))
    run "$target" "$seconds" "$@"
fi
