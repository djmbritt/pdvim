# 003 — Rune against iced

28 November 2026 · iced: `main` (`eb9d922`) · Rune: branch `rune-port`
(see [002](002-port-to-rune.md)), Rune `dev` at `7e3d266`

> **Measured again in [004](004-where-rune-start-up-goes.md#003-measured-again).**
> 003's own Rune binary, run again later the same day interleaved with
> iced, opened the manual drawn in 0.23 s rather than 0.62 s, and scrolled
> for 0.03 processor seconds rather than 0.07; iced's figures held. What
> slowed Rune in these runs was not found. Every run here also compiled
> its shaders from scratch (empty cache folders), which a reader's pdvim
> does only the first time: with them cached, Rune opens faster than iced.

## Question

Does pdvim perform better or worse once iced is replaced with Rune?

## Method

The same harness and machine as [001](001-baseline-iced.md). Both builds
are `--release` from the same source apart from the port. Four
configurations of the Rune build were measured, because Rune lets an app
choose how the window is first drawn (`CpuFirst`, set here with the
`RUNE_CPU_FIRST` variable, no rebuild):

| Label | What it is | Runs per document |
| --- | --- | ---: |
| `iced` | `main`, iced 0.14 drawing with wgpu | 7 |
| `rune` | the port, Rune's default (`CpuFirst::Auto`): the window opens on Rune's CPU renderer, and the GPU takes over in the background | 7 |
| `rune-gpu` | `RUNE_CPU_FIRST=off`: straight to the GPU, as iced does | 5 |
| `rune-cpu` | `RUNE_CPU_FIRST=only`: the CPU renderer throughout | 5 |

Raw runs: [`data/gui-iced.jsonl`](data/gui-iced.jsonl),
[`data/gui-rune.jsonl`](data/gui-rune.jsonl),
[`data/gui-rune-gpu.jsonl`](data/gui-rune-gpu.jsonl),
[`data/gui-rune-cpu.jsonl`](data/gui-rune-cpu.jsonl). All figures are
medians. Every run ended with the document's title in the window, so every
run opened and read the document.

## Results

### Opening a document

Seconds until the window appears:

| Document | iced | rune | rune-gpu | rune-cpu |
| --- | ---: | ---: | ---: | ---: |
| 1 page | 0.34 | **0.04** | 0.55 | 0.07 |
| 477 pages | 0.35 | **0.08** | 0.55 | 0.10 |
| 42 pages, pictures | 0.33 | 0.09 | 0.58 | **0.08** |
| 1,908 pages | 0.37 | **0.08** | 0.55 | **0.08** |

Seconds until the document is open and drawn, and the processor stops:

| Document | iced | rune | rune-gpu | rune-cpu |
| --- | ---: | ---: | ---: | ---: |
| 1 page | 0.34 | **0.23** | 0.58 | 0.26 |
| 477 pages | 0.35 | 0.62 | 0.64 | **0.31** |
| 42 pages, pictures | 0.33 | 0.61 | 0.66 | **0.26** |
| 1,908 pages | 0.37 | 0.58 | 0.62 | **0.34** |

Processor seconds to open, all threads:

| Document | iced | rune | rune-gpu | rune-cpu |
| --- | ---: | ---: | ---: | ---: |
| 1 page | **0.16** | 0.40 | 0.56 | 0.35 |
| 477 pages | **0.19** | 0.89 | 0.66 | 0.44 |
| 42 pages, pictures | **0.17** | 0.84 | 0.60 | 0.39 |
| 1,908 pages | **0.25** | 0.89 | 0.77 | 0.44 |

Memory once open (PSS, MB), and threads:

| Document | iced | rune | rune-gpu | rune-cpu | threads: iced / rune |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 page | 68 | 93 | 89 | **50** | 42 / 19 |
| 477 pages | 89 | 109 | 113 | **79** | 42 / 20 |
| 42 pages, pictures | 73 | 102 | 100 | **60** | 42 / 20 |
| 1,908 pages | 98 | 103 | 120 | **65** | 42 / 20 |

### Reading

Processor seconds for 50 presses of `j`, and for 20 of `Ctrl-f`:

| Document | `j`: iced | rune | rune-gpu | rune-cpu | `Ctrl-f`: iced | rune | rune-gpu | rune-cpu |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 page | **0.01** | 0.02 | 0.06 | 0.64 | **0.00** | 0.01 | 0.02 | 0.08 |
| 477 pages | **0.02** | 0.07 | 0.07 | 1.57 | **0.05** | 0.11 | 0.10 | 0.67 |
| 42 pages, pictures | **0.02** | 0.07 | 0.08 | 1.60 | **0.06** | 0.14 | 0.13 | 0.66 |
| 1,908 pages | **0.02** | 0.08 | 0.07 | 1.42 | **0.04** | 0.12 | 0.09 | 0.52 |

Going to the last page (`G`) took 0.00–0.03 s and 0.00–0.03 processor
seconds in every configuration: too little to tell apart. At rest, no
configuration used any processor over 5 s.

Memory after all of that (PSS, MB):

| Document | iced | rune | rune-gpu | rune-cpu |
| --- | ---: | ---: | ---: | ---: |
| 1 page | 68 | 93 | 90 | **50** |
| 477 pages | 91 | 127 | 129 | **90** |
| 42 pages, pictures | 93 | 131 | 129 | 105 |
| 1,908 pages | 104 | 117 | 136 | **84** |

### Hot paths

Unchanged, as expected: the backend does not touch the toolkit. The
differences go both ways and are of the size criterion runs differ by
(`page/render/1` +13%, `page/render/2.5` −8%, the rest within ±10%).
`page/record` came out at 448 µs against 270 µs in the first run, but
measured again alone it was 261 µs on Rune and 267 µs on iced: noise.
Logs: [`data/bench-iced.log`](data/bench-iced.log),
[`data/bench-rune.log`](data/bench-rune.log).

```sh
python3 experiments/bench/criterion.py iced=experiments/data/bench-iced.log rune=experiments/data/bench-rune.log
```

### Build

| | iced | Rune |
| --- | ---: | ---: |
| Clean release build | **2 min 29 s** | 5 min 4 s |
| Crates compiled | 357 | **305** |
| Release binary | **41.4 MB** | 55.2 MB |
| Release binary, stripped | **32.2 MB** | 44.7 MB |
| Shared libraries linked at start (`ldd`) | 13 | **5** |
| Minimum Rust | **1.90** | 1.99 |
| Dependency source | crates.io | a pinned git revision plus 5 patched crates |

## What it means

- **The window comes up far sooner on Rune**: 0.04–0.09 s against
  0.33–0.37 s, because Rune's first frames are painted on the processor
  while the GPU starts. For someone opening a PDF, the window is there at
  once.
- **But the page is ready later, and opening costs 2.5–5 times the
  processor time.** With the default setting a multi-page document is done
  (drawn, and handed over to the GPU) only at about 0.6 s, against 0.35 s
  on iced: the start is paid for on the processor and then on the GPU. Most of the cost is the GPU
  itself: Rune's Vulkan start-up alone (`rune-gpu`) takes about 0.55 s,
  against 0.34 s for all of iced's opening.
- **Reading costs more on Rune's GPU path**, 2–4 times iced's processor
  time for scrolling and 2–3 times for paging, though still under a tenth of a
  second for 50 key presses. Each key re-runs the view and diffs it, where
  iced only redrew the canvas.
- **Rune's CPU renderer is the leanest way to open a document** (0.26–0.34
  s, 50–79 MB, the least memory of all four), **and the worst way to read
  one**: scrolling costs 60–80 times iced's processor time, because every frame
  of a full-window page is painted by the processor.
- **Memory is higher** on the GPU path than on iced: 5–29 MB once open,
  13–38 MB after reading, and it grows more as pages are read (up to 136 MB
  on the 1,908-page file with `rune-gpu`, against 104).
- **The backend is untouched**, and so is everything the benchmarks time.
- **The build is twice as slow and the binary a third larger**, with fewer
  crates but larger ones, and Rune comes from git rather than crates.io.

On these measures pdvim performs **worse on Rune than on iced**, except for
how soon the window appears and how few threads it runs.

## What to try next

1. **Vulkan start-up.** `docs/comparison.md` found wgpu loading every
   installed Vulkan driver; try `VK_DRIVER_FILES` with the AMD driver alone
   on both builds, and Rune's `with_direct_drivers` path, to see how much
   of Rune's 0.55 s is that.
2. **Less work per key.** Most keys only move the view: keep the page
   layer's elements keyed and still, move them with a `Live` offset and
   `tx.live_only()`, so a scroll does not re-run the view, diff and layout.
3. **Memory.** Rune keeps each page's pixels on the processor as well as on
   the GPU (`Pixels`), and keeps unshown images up to 64 MiB; try freeing
   pages' CPU copies once uploaded, and `App::trim_heap_at_rest`.
4. **Hand over to the GPU only when needed.** `CpuFirst::Only` until the
   first scroll, then the GPU, might give the fast, lean open and the cheap
   scroll together.
