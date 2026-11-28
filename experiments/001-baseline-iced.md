# 001 — Baseline: pdvim on iced

28 November 2026 · pdvim 0.2.27 (`eb9d922`, branch `main`) · iced 0.14

## Question

How does pdvim perform before its GUI toolkit, iced, is replaced with Rune?
Everything later is compared with these figures.

## Method

See [README.md](README.md#measuring-again) for the machine and the scripts.

- **Window**: `bench/measure.py`, 7 runs of each of the four documents, a
  release build of `main` (`cargo build --release --locked`). Raw runs:
  [`data/gui-iced.jsonl`](data/gui-iced.jsonl).
- **Hot paths**: `cargo bench --bench hot_paths`, run from a clean worktree
  of `main`. Log: [`data/bench-iced.log`](data/bench-iced.log).
- **Build**: a clean release build into an empty target directory.

## Results

### Opening a document

| Document | Window appears (s) | Open and drawn (s) | Processor (s) | Memory (PSS, MB) | Threads |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 page | 0.34 | 0.34 | 0.16 | 68 | 42 |
| 477 pages | 0.35 | 0.35 | 0.19 | 89 | 42 |
| 42 pages, pictures | 0.33 | 0.33 | 0.17 | 73 | 42 |
| 1,908 pages | 0.37 | 0.37 | 0.25 | 98 | 42 |

Medians of 7 runs. Open times ranged 0.32–0.40 s. The window appears and the
page is drawn within the same 20 ms sample, as `docs/comparison.md` found.

These agree with `docs/comparison.md` (0.36–0.39 s, 0.16–0.26 s of processor,
74–99 MB), measured five days earlier on pdvim 0.2.13: the harness is the
same, so the two can be read together.

### Reading

| Document | Idle 5 s (cpu s) | 50 × `j` (cpu s) | 20 × `Ctrl-f` (cpu s) | `G` (cpu s) | Memory after (MB) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 page | 0.00 | 0.01 | 0.00 | 0.00 | 68 |
| 477 pages | 0.00 | 0.02 | 0.05 | 0.01 | 91 |
| 42 pages, pictures | 0.00 | 0.02 | 0.06 | 0.02 | 93 |
| 1,908 pages | 0.00 | 0.02 | 0.04 | 0.02 | 104 |

The kernel counts processor time in 10 ms ticks, so 0.01–0.02 s is one or
two ticks: scrolling and jumping cost almost nothing. Paging (`Ctrl-f`)
costs more because each new page has to be rendered by MuPDF. At rest pdvim
uses no processor at all.

### Hot paths

| Benchmark | Median |
| --- | ---: |
| page/record | 269.56 µs |
| page/render/1 | 795.70 µs |
| page/render recoloured/1 | 941.86 µs |
| page/render/2.5 | 3.2152 ms |
| page/render recoloured/2.5 | 3.8671 ms |
| page/read text | 515.10 µs |
| text/search/e | 62.944 µs |
| text/flash/the cu | 348.94 µs |
| text/sentences | 14.212 µs |
| layout/lay out/10000 | 66.656 µs |
| layout/pages in view/10000 | 2.2160 µs |

(The full table is in the log.) None of these touch iced.

### Build

| | iced build |
| --- | ---: |
| Clean release build | 2 min 29 s |
| Crates compiled | 357 |
| Release binary | 41,413,152 B (39.5 MiB, not stripped) |
| Minimum Rust | 1.90 |

## Notes

- Clippy on `main` with this nightly (1.102) already fails on a handful of
  lints that are new to the toolchain (multiply-and-add in `forms.rs`,
  `reader.rs` and `fuzz.rs`; `assert!` on emptiness in tests). Not the
  port's doing; left alone.
