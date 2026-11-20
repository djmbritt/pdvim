# Fuzzing pdvim

These are [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets, run with libFuzzer and AddressSanitizer. Each target is a single line that hands the input to a function in [`src/fuzz.rs`](../src/fuzz.rs). That function can reach the parts of the crate that are not public. It fails on a panic, and also when a part breaks a promise the rest of pdvim relies on.

| Target    | What it drives | What it checks besides not crashing |
|-----------|----------------|-------------------------------------|
| `pdf`     | Opens the input as a PDF and, for the first four pages, reads the sizes, links, text, search hits and margins and renders them. Also reads the table of contents. This is the same code the document worker runs. | MuPDF and the `mupdf` crate do not crash on any file. |
| `config`  | Parses the input as `config.toml`, applies the bindings and builds the cheat sheet. | Every problem has a message. A key bound is a key found. |
| `command` | Parses and completes a line typed after `:`. Path completion is skipped. | Completion starts on a character boundary. Each candidate starts with the common prefix. |
| `keys`    | Parses key names such as `ctrl+d` and `g g`. | Keys written back the way the cheat sheet shows them parse to the same keys. |
| `modes`   | Builds a link-label set, a recent list, a table of contents and some page text from the first bytes. Then types the remaining bytes at the bindings, the cheat sheet, the labels, the recent list, the sidebar and visual mode. | The sidebar shows its selected entry. The cheat sheet fills its rows. The visual cursor stays on the text. `y` copies the selection. |
| `reader`  | Scrolls, zooms, turns, trims and restores a view over pages of arbitrary size, including empty documents and zero-sized pages. | The current page and visible pages are in range. The zoom and position stay finite. |

## Running

You need a nightly toolchain and `cargo install cargo-fuzz`. The first build takes a few minutes, because it compiles MuPDF and iced with sanitizer flags.

```sh
fuzz/run.sh                 # every target for a minute each
fuzz/run.sh pdf             # one target until stopped
fuzz/run.sh pdf 600 -jobs=8 # ten minutes, eight processes
```

`run.sh` starts each target from the inputs in `seeds/<target>`. It adds the dictionary in `dicts/` for `pdf` and `config`, and keeps what it learns in `corpus/<target>`, which is not committed.

A crash is written to `artifacts/<target>/`. To replay it, or to shrink it:

```sh
cd fuzz
cargo +nightly fuzz run pdf artifacts/pdf/crash-…
cargo +nightly fuzz tmin pdf artifacts/pdf/crash-…
```

Once a crash is fixed, add a unit test for it next to the code. Then copy the input into `seeds/<target>/` so later runs start from it.

## Without the fuzzer

`cargo test` runs every target once over a few fixed inputs, so the harness keeps compiling and stays sound on stable Rust.

## What it has found

Each of these has a unit test, and its input is among the seeds.

- **Crash on an outline entry with no title.** An entry without `/Title`, or one that is not a dictionary because the file is damaged, made pdvim segfault when the sidebar opened. The `mupdf` crate reads each title without checking for null. pdvim now looks for such entries first and reads the outline from a separate copy of the document with those entries mended. It loads every object of that copy first, so that MuPDF's rebuild of a damaged file doesn't undo the mending.
- **A huge page rendered past the size cap.** A floor on the render scale, applied after the cap, undid the cap. A page claiming to be billions of points long came out 16,777,216 pixels long, far beyond what a graphics card accepts. Margin measurement for `trim` had no cap at all.
- **A huge count froze visual mode.** `999999999$` repeated a motion that had stopped moving a billion times. `2tx` with an `x` next to the cursor found the same `x` each time, so it never reached the second one.
- **The sidebar opened scrolled away from its selection.** The entry for the page in view was selected but out of sight until a key was pressed.
