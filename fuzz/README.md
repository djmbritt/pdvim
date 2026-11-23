# Fuzzing pdvim

These are [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets, run with libFuzzer and AddressSanitizer. Each target is a single line that hands the input to a function in [`src/fuzz.rs`](../src/fuzz.rs). That function can reach the parts of the crate that are not public. It fails on a panic, and also when a part breaks a promise the rest of pdvim relies on.

| Target    | What it drives | What it checks besides not crashing |
|-----------|----------------|-------------------------------------|
| `pdf`     | Opens the input as a PDF and, for the first four pages, reads the sizes, links, text, search hits and margins and renders them. Also reads the table of contents. This is the same code the document worker runs. | MuPDF and the `mupdf` crate do not crash on any file. |
| `formats` | Opens the input as an EPUB or FB2 book, a CBZ comic, an XPS document, an SVG drawing, an HTML page or an image, picked by the first byte, laid out on a small page where it has no pages of its own. Then reads its first four pages as `pdf` does. The seeds start with the byte that picks their format. | MuPDF does not crash on any file of these kinds. |
| `config`  | Parses the input as `config.toml`, applies the bindings and builds the cheat sheet. | Every problem has a message. A key bound is a key found. |
| `command` | Parses and completes a line typed after `:`. Path completion is skipped. | Completion starts on a character boundary. Each candidate starts with the common prefix. |
| `keys`    | Parses key names such as `ctrl+d` and `g g`. | Keys written back the way the cheat sheet shows them parse to the same keys. |
| `modes`   | Builds a link-label set, a recent list, a table of contents and some page text from the first bytes. Then types the remaining bytes at the bindings, the cheat sheet, the labels, the recent list, the sidebar and visual mode. | The sidebar shows its selected entry. The cheat sheet fills its rows. The visual cursor stays on the text. `y` copies the selection. |
| `reader`  | Scrolls, zooms, turns, trims, sets columns of pages and restores a view over pages of arbitrary size, including empty documents and zero-sized pages. | The current page and visible pages are in range. The zoom and position stay finite. |

## Running

You need a nightly toolchain and `cargo install cargo-fuzz`. The first build takes a few minutes, because it compiles MuPDF and iced with sanitizer flags.

```sh
fuzz/run.sh                 # every target for a minute each
fuzz/run.sh pdf             # one target until stopped
fuzz/run.sh pdf 600 -jobs=8 # ten minutes, eight processes
```

`run.sh` starts each target from the inputs in `seeds/<target>`. It adds the dictionary in `dicts/` for `pdf` and `config`, and keeps what it learns in `corpus/<target>`, which is not committed.

Among the PDF seeds, the `lang-*.pdf` ones hold a sample of text in each of sixteen languages and scripts, shaped, right to left and without spaces, as Chromium prints them. `languages/make-pdfs.sh` makes them from `languages/samples.tsv`, which the language tests in `src/languages.rs` read too.

A crash is written to `artifacts/<target>/`. To replay it, or to shrink it:

```sh
cd fuzz
cargo +nightly fuzz run pdf artifacts/pdf/crash-…
cargo +nightly fuzz tmin pdf artifacts/pdf/crash-…
```

Once a crash is fixed, add a unit test for it next to the code. Then copy the input into `seeds/<target>/` so later runs start from it.

## Coverage

`coverage.sh` runs every target over its corpus and seeds with coverage turned on, and prints how much of each file in `src/` they reach. It needs nightly's `llvm-tools` (`rustup component add --toolchain nightly llvm-tools-preview`), and leaves the full report for each target in `coverage/<target>/`.

```sh
fuzz/coverage.sh             # every target
fuzz/coverage.sh pdf formats # only these
```

Measured on 2026-11-21, over a corpus of about 41,000 inputs from earlier runs:

| File | Lines | Functions | Regions |
|------|------:|----------:|--------:|
| `src/app.rs` | 0% | 0% | 0% |
| `src/app/bars.rs` | 0% | 0% | 0% |
| `src/app/chapters.rs` | 0% | 0% | 0% |
| `src/app/copy.rs` | 0% | 0% | 0% |
| `src/app/flash.rs` | 0% | 0% | 0% |
| `src/app/forms.rs` | 0% | 0% | 0% |
| `src/app/incsearch.rs` | 0% | 0% | 0% |
| `src/app/info.rs` | 0% | 0% | 0% |
| `src/app/links.rs` | 0% | 0% | 0% |
| `src/app/markings.rs` | 0% | 0% | 0% |
| `src/app/mode.rs` | 0% | 0% | 0% |
| `src/app/reflow.rs` | 0% | 0% | 0% |
| `src/app/run.rs` | 0% | 0% | 0% |
| `src/app/tabs.rs` | 0% | 0% | 0% |
| `src/app/unlock.rs` | 0% | 0% | 0% |
| `src/backend.rs` | 27% | 22% | 26% |
| `src/bindings.rs` | 82% | 82% | 82% |
| `src/command.rs` | 68% | 56% | 66% |
| `src/complete.rs` | 59% | 55% | 63% |
| `src/config.rs` | 71% | 64% | 74% |
| `src/dirs.rs` | 0% | 0% | 0% |
| `src/flash.rs` | 0% | 0% | 0% |
| `src/fonts.rs` | 97% | 93% | 93% |
| `src/fuzz.rs` | 98% | 100% | 98% |
| `src/help.rs` | 59% | 45% | 59% |
| `src/hints.rs` | 96% | 100% | 94% |
| `src/history.rs` | 0% | 0% | 0% |
| `src/input.rs` | 18% | 33% | 40% |
| `src/jumps.rs` | 0% | 0% | 0% |
| `src/keymap.rs` | 83% | 78% | 84% |
| `src/layout.rs` | 88% | 89% | 90% |
| `src/markings.rs` | 0% | 0% | 0% |
| `src/num.rs` | 75% | 75% | 81% |
| `src/outline.rs` | 76% | 72% | 77% |
| `src/picker.rs` | 89% | 86% | 91% |
| `src/reader.rs` | 95% | 92% | 96% |
| `src/recall.rs` | 0% | 0% | 0% |
| `src/recolor.rs` | 100% | 100% | 100% |
| `src/theme.rs` | 8% | 15% | 6% |
| `src/visual.rs` | 93% | 92% | 94% |
| **Total** | **28%** | **29%** | **28%** |

The targets reach most of what they are meant to: parsing commands, keys and the configuration, laying out and moving through pages, the labels, visual mode and recolouring. `src/app.rs` and `src/app/` are at 0% because the window, iced's event loop and its drawing are not fuzzed; the targets drive the parts the app is built from instead. `backend.rs` is about a quarter covered because the fuzzers open documents and read their first pages, but don't highlight, fill in forms or save. `history.rs` and `recall.rs` are not fuzzed, as the history comes from pdvim's own database rather than from outside.

## In CI

[`.github/workflows/fuzz.yml`](../.github/workflows/fuzz.yml) runs every target through `run.sh` for three minutes after each push to `main` and on each pull request. Once a week it runs each one for half an hour, and it can be run by hand from the Actions tab for as long as you like. Each run starts from the corpus the last one left, kept in the Actions cache, so the corpus keeps growing. When a target crashes, hangs or leaks, the run fails, and the inputs that caused it are attached to the run as artifacts.

## Without the fuzzer

`cargo test` runs every target once over a few fixed inputs, so the harness keeps compiling and stays sound on stable Rust.

## What it has found

Each of these has a unit test, and its input is among the seeds.

- **Crash on an outline entry with no title.** An entry without `/Title`, or one that is not a dictionary because the file is damaged, made pdvim segfault when the sidebar opened. The `mupdf` crate reads each title without checking for null. pdvim now looks for such entries first and reads the outline from a separate copy of the document with those entries mended. It loads every object of that copy first, so that MuPDF's rebuild of a damaged file doesn't undo the mending.
- **A huge page rendered past the size cap.** A floor on the render scale, applied after the cap, undid the cap. A page claiming to be billions of points long came out 16,777,216 pixels long, far beyond what a graphics card accepts. Margin measurement for `trim` had no cap at all.
- **A huge count froze visual mode.** `999999999$` repeated a motion that had stopped moving a billion times. `2tx` with an `x` next to the cursor found the same `x` each time, so it never reached the second one.
- **A book took seconds a page.** The `mupdf` crate's system-font lookup has fontconfig read every installed font again for each font it looks for. MuPDF looks for one each time it meets a character its fonts lack, and remembers only the fonts it finds. A 184-byte HTML page of bytes that are not UTF-8 took 2.5 seconds, and longer under the sanitizer. pdvim now does the lookup itself ([`src/fonts.rs`](../src/fonts.rs)): it reads the installed fonts once per thread and keeps every answer, fonts not found included. It also finds fonts for Chinese, Japanese and Korean, which the crate's lookup didn't, so such text is no longer blank.
- **A page of HTML that lays out to millions of pages.** Damaged markup a few kilobytes long laid out to as many as 746 million pages, and measuring each one took minutes and would have taken gigabytes. A book's pages are now all the size it was laid out on, so they are not measured one by one, and a document of more than 100,000 pages is refused.
- **A leak in MuPDF on a damaged comic.** `cbz_load_page` keeps the page's image data in a variable not marked `fz_var`, so when the image fails to decode the buffer can be lost. It is 40 bytes a page and not pdvim's to fix, so `run.sh` turns leak detection off for `formats`.
- **The sidebar opened scrolled away from its selection.** The entry for the page in view was selected but out of sight until a key was pressed.
