# Experiments

Each experiment has a numbered report here: what was tried, how it was
measured, what came out, and what was decided. Raw measurements go in
`data/`, and the scripts that took them in `bench/`, so that any figure in a
report can be measured again.

| # | Report | Question |
| --- | --- | --- |
| 001 | [Baseline: pdvim on iced](001-baseline-iced.md) | How does pdvim perform before the GUI toolkit is changed? |
| 002 | [Porting from iced to Rune](002-port-to-rune.md) | What does it take to replace iced with Rune, and what changed? |
| 003 | [Rune against iced](003-rune-vs-iced.md) | Does pdvim perform better or worse on Rune? |
| 004 | [Where Rune's start-up goes](004-where-rune-start-up-goes.md) | Is the slower start the GPU, the shaders, or something else, and would a better GPU help? |
| 005 | [Scrolling with a live offset](005-live-scrolling.md) | How much does scrolling cost once pdvim uses Rune's live values rather than rebuilding the view? |

## Measuring again

The machine used so far: AMD Ryzen 7 7840U (Radeon 780M), 60 GiB, Arch Linux,
Hyprland 0.56.2, Rust nightly 1.102 (2026-11-23).

**The window** (`bench/measure.py`): opens a document in a floating
1000 × 750 window on workspace 1 without taking focus, with empty config,
data, cache and state folders, and times opening, idling, scrolling and
jumping by sampling the processor time of the process every 20 ms. Keys go
to the test window alone, found by its pid. The documents are the four used
in `docs/comparison.md` (one page; the 477-page Ghostscript manual; its
42-page colour guide; and the manual four times over, 1,908 pages).

```sh
for doc in small manual pictures long; do
  python3 experiments/bench/measure.py LABEL path/to/pdvim target/compare/docs/$doc.pdf 7
done > experiments/data/gui-LABEL.jsonl
python3 experiments/bench/summarise.py experiments/data/gui-*.jsonl
```

`CACHE=DIR` keeps one cache folder across runs, so that the GPU driver's
compiled shaders are kept as they are for anyone who has run pdvim before
(see [004](004-where-rune-start-up-goes.md)).

**Launch steps** (`bench/launch.py`, Rune builds): opens a document with
`PDVIM_LAUNCH_MARKS` and `RUNE_GPU_LOG` set and records when each step of
the start was reached; `bench/marks.py` tabulates them.

```sh
python3 experiments/bench/launch.py LABEL path/to/pdvim target/compare/docs/manual.pdf 7 > experiments/data/launch-LABEL.jsonl
python3 experiments/bench/marks.py experiments/data/launch-*.jsonl
```

**Frames** (`bench/frames.py`): presses keys at a steady pace, a frame
each, and reads the processor time of the whole process; for a build made
with `--features frame-profile`, also Rune's log of each frame's stages.
`bench/framestats.py` tabulates them.

```sh
cargo build --release --features frame-profile
python3 experiments/bench/frames.py LABEL target/release/pdvim target/compare/docs/manual.pdf 5 > experiments/data/frames-LABEL.jsonl
python3 experiments/bench/framestats.py experiments/data/frames-*.jsonl
```

**The hot paths** (`cargo bench`): criterion's timings of rendering,
recolouring, reading and searching a page, and laying out long documents.
These do not touch the GUI toolkit; they are run to see that nothing else
moved.

**The build**: a clean `cargo build --release --locked` into an empty target
directory, timed; the binary's size; the crates compiled.
