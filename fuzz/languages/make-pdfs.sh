#!/usr/bin/env bash
# Makes a PDF of each sample in samples.tsv, as fuzz/seeds/pdf/lang-CODE.pdf:
# a small page with the sample set in a narrow column, so that it runs over
# several lines, in a Noto font of its script, right to left where it is
# written so. Chromium lays the text out and prints it, shaping it as a
# browser or word processor would and embedding the fonts, which is how
# most PDFs in these scripts are made.
#
# The PDFs are kept in the repository; run this again only when the
# samples change. It needs Chromium and the Noto fonts the samples name.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
out="$here/../seeds/pdf"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

chromium=$(command -v chromium || command -v chromium-browser || command -v google-chrome-stable)

grep -v '^#' "$here/samples.tsv" | while IFS=$'\t' read -r code language spaced direction font sentences; do
    # The sentences, tabs between them, as one paragraph.
    text=${sentences//$'\t'/ }
    if [[ $spaced == unspaced ]]; then
        text=${sentences//$'\t'/}
    fi
    # Korean is spaced between words, and broken between them too, as its
    # word processors do; broken within a word, the space lost there
    # could not be told from none.
    breaking=normal
    if [[ $code == ko ]]; then
        breaking=keep-all
    fi
    cat >"$work/$code.html" <<HTML
<!DOCTYPE html>
<html lang="${code%%-*}" dir="$direction">
<head>
<meta charset="utf-8">
<title>$language</title>
<style>
  @page { size: 105mm 148mm; margin: 12mm; }
  body { font-family: "$font"; font-size: 12pt; line-height: 1.5; margin: 0; }
  p { width: 70mm; margin: 0; word-break: $breaking; }
</style>
</head>
<body><p>$text</p></body>
</html>
HTML
    "$chromium" --headless --disable-gpu --no-sandbox --no-pdf-header-footer \
        --print-to-pdf="$out/lang-$code.pdf" "file://$work/$code.html" 2>/dev/null
    echo "lang-$code.pdf: $language"
done
