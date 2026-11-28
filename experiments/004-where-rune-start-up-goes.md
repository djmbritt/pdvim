# 004 — Where Rune's start-up goes

28 November 2026 · branch `rune-port` (with [`src/app/launch.rs`](../src/app/launch.rs))
· Rune `dev` at `7e3d266`

## Question

[003](003-rune-vs-iced.md) found Rune's window up at once but the document
ready later than on iced, and opening costing 2.5–5 times the processor
time. Its guess: most of that is the GPU starting, perhaps shaders being
compiled at every launch, as Rune ships compiled shaders only for macOS and
Windows. Is that so, and would a better GPU help?

## Method

**Launch marks.** Rune records when the process reaches each step of a
launch: the event loop, the window, the GPU's adapter, device and surface,
the renderer's pipelines, the first frame, the handover from the CPU
renderer to the GPU (`rune::marks`). pdvim now writes them out when asked:
with `PDVIM_LAUNCH_MARKS=FILE`, one JSON line of every mark, in
microseconds from the process's start, 4 s after the start. It adds its
own: `pdvim.run` (`run` entered), `pdvim.init` and `pdvim.init.done` (the
app made), `pdvim.page` (the first page back from MuPDF) and
`pdvim.page.drawn` (the first frame drawn after it). Without the variable
nothing is recorded.

[`bench/launch.py`](bench/launch.py) opens the 477-page manual 7 times
with it set, and `RUNE_GPU_LOG=1`, in four ways:

| | `RUNE_CPU_FIRST` | Shader cache |
| --- | --- | --- |
| `auto` | unset: Rune's default, the CPU renderer first, the GPU taking over in the background | |
| `gpu` | `off`: straight to the GPU, as iced does | |
| `cold` | | an empty cache folder each run, as `measure.py` gives |
| `warm` | | one cache folder kept across runs, filled by a run beforehand |

The cache matters because Mesa, the AMD driver here, keeps the shaders it
compiles in `$XDG_CACHE_HOME/mesa_shader_cache` (4.5 MB for pdvim).
`measure.py` gives every run empty folders, so every run so far, iced's
and Rune's, compiled its shaders from scratch. Anyone who has opened pdvim
before has them cached. `measure.py` now takes `CACHE=DIR` to keep one.

Linux records a process's start to the nearest clock tick (10 ms), so the
marks are ±10 ms from the start (`pdvim.run` reads 0 or 10 ms). The time
between two marks is exact.

**The window, cold and warm.** `measure.py` as before, 5 runs of the
1-page and 477-page documents, cold and warm, for iced, Rune's default and
Rune straight to the GPU, interleaved in one sitting
([`data/gui-cache.jsonl`](data/gui-cache.jsonl)). Idle was measured over
1 s rather than 5.

## Results

### The steps

Milliseconds from the process's start, median (fastest–slowest) of 7
runs, the main steps of [the full table](#every-mark):

| Step | auto, cold | auto, warm | gpu, cold | gpu, warm |
| --- | ---: | ---: | ---: | ---: |
| App made (`pdvim.init.done`) | 15 | 15 | 14 | 14 |
| Window made (`window.created`) | 17 | 17 | 204 | 50 |
| First frame shown (`frame.first.presented`) | **21** | **19** | 215 | 55 |
| GPU device ready (`gpu.device`) | 52 | 47 | 40 | 40 |
| Renderer's pipelines begun (`renderer.begin`) | | | 41 | 41 |
| Renderer's pipelines ready (`renderer.ready`, `gpu.window.warmed`) | 235 | 58 | 204 | 50 |
| First page back from MuPDF (`pdvim.page`) | 115 | 122 | 297 | 135 |
| First page drawn (`pdvim.page.drawn`) | **120** | **124** | 301 | 139 |
| GPU took over (`gpu.window.handed`) | 244 | 109 | | |

- **Building the GPU's pipelines is the cost: 163 ms with no shader cache,
  9 ms with one** (`renderer.begin` to `renderer.ready`). That is the
  driver compiling Rune's shaders, on the processor. Everything else on the
  GPU side, from loading the driver to having a device, takes about 25 ms
  either way.
- **Rune's default hides it.** The window and its first frame are up at
  20 ms, and the first page is drawn by the CPU renderer at 120 ms, while
  the pipelines are built on another thread. The GPU takes over at 244 ms
  cold, 109 ms warm. The reader sees nothing of it.
- **Straight to the GPU, nothing shows until the pipelines are built**:
  the window comes up at 204 ms cold, 50 ms warm.
- **The page waits for the window.** pdvim asks MuPDF for the document at
  its first `poll`, which Rune runs only once the window is made. Straight
  to the GPU, MuPDF starts only at 204 ms; the page is back 93 ms later. On
  the default path the window is made at 17 ms and the page is back at
  115 ms. Starting MuPDF in `setup`, before the window, would let the two
  overlap on the GPU path.

`RUNE_GPU_LOG` adds only which GPU was chosen: the compositor's, the Radeon
780M, on Vulkan through wgpu, with no retries or fallbacks.

### The window, cold and warm

| | iced, cold | iced, warm | Rune, cold | Rune, warm | Rune GPU, cold | Rune GPU, warm |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Window appears, 1 page (s) | 0.32 | 0.30 | **0.04** | **0.04** | 0.24 | 0.07 |
| Window appears, 477 pages (s) | 0.32 | 0.30 | **0.04** | 0.05 | 0.24 | 0.07 |
| Open and drawn, 1 page (s) | 0.32 | 0.30 | 0.23 | **0.06** | 0.24 | 0.07 |
| Open and drawn, 477 pages (s) | 0.34 | 0.31 | 0.23 | 0.13 | 0.31 | **0.09** |
| Processor to open, 1 page (s) | 0.16 | 0.13 | 0.40 | 0.11 | 0.25 | **0.09** |
| Processor to open, 477 pages (s) | 0.19 | 0.16 | 0.43 | 0.14 | 0.29 | **0.12** |
| Memory once open, 1 page (MB) | 69 | 63 | 92 | 60 | 90 | **58** |
| Memory once open, 477 pages (MB) | 96 | 89 | 101 | 82 | 113 | **81** |
| Threads | 42 | 36 | 19 | 14 | 19 | 14 |

With a warm cache, **Rune opens a document 2–5 times sooner than iced, on
less processor time and less memory**. iced gains little from the cache
(0.32 → 0.30 s): its time goes elsewhere (below). Scrolling, paging and
jumping cost the same cold or warm, and the same on both toolkits (50 × `j`:
0.02–0.04 s; 20 × `Ctrl-f`: 0.05–0.06 s).

### Where iced's 0.3 s goes

The machine has the AMD, NVIDIA and software (lavapipe) Vulkan drivers
installed. With `VK_DRIVER_FILES` set to the AMD driver alone, 5 runs of the
manual, warm cache ([`data/gui-drivers.jsonl`](data/gui-drivers.jsonl)):

| | iced | iced, AMD driver only | Rune GPU | Rune GPU, AMD driver only |
| --- | ---: | ---: | ---: | ---: |
| Window appears (s) | 0.29 | **0.12** | 0.07 | 0.07 |

iced (wgpu, asking every installed driver for its GPUs) spends about 0.17 s
loading the NVIDIA driver. Rune opens the compositor's GPU and nothing else,
and is not affected. With a cold cache and the AMD driver alone, iced's
window comes up at 0.14 s.

### 003 measured again

The figures above are better for Rune than 003's. So 003's own binaries
were measured again, interleaved with iced, 5 runs of the manual, cold, as
003 ran them ([`data/gui-recheck.jsonl`](data/gui-recheck.jsonl)):

| | 003 | Today |
| --- | ---: | ---: |
| Rune: open and drawn (s) | 0.62 | 0.23 |
| Rune GPU: window appears (s) | 0.55 | 0.22 |
| Rune: 50 × `j` (processor s) | 0.07 | 0.03 |
| iced: open and drawn (s) | 0.35 | 0.34 |
| iced: 50 × `j` (processor s) | 0.02 | 0.03 |

The binary (the same file) behaves differently from 003's runs. 003's iced
figures hold, and its Rune figures were consistent from run to run, so not
noise. But I could not reproduce them nor find out what made them slower.
No other test window was open then, and a benchmark build had finished
before. **003's Rune start-up and scrolling figures should be read as
taken under conditions that made Rune slower, and these as the fairer
comparison**, measured with both toolkits interleaved in one sitting.

## What it means

- **Shaders are compiled at the first launch only** (and again after the
  driver is updated, which empties Mesa's cache). It costs Rune 163 ms of
  the driver's time, then 9 ms once cached. Rune's CPU-first start keeps
  even the first launch from showing it: the window is up in 20 ms and the
  page drawn in 120 ms either way.
- **A better GPU would not change the start-up.** Every step of it runs on
  the processor: loading drivers, compiling shaders, the CPU renderer's
  first frames, MuPDF. A faster processor would, and so would fewer
  drivers. Reading is the same: what a frame costs is processor time
  ([005](005-live-scrolling.md)).
- **Warm, Rune is the faster start**: the page drawn in 0.06–0.13 s against
  iced's 0.30–0.31 s, on 0.11–0.14 processor seconds against 0.13–0.16.
- **The reverse of 003's verdict on starting, then**, for anyone but a
  first-time user. Cold, Rune's default still shows the page sooner (0.23 s
  against 0.34 s) but costs more than twice the processor time (0.40–0.43 s
  against 0.16–0.19), because it draws the first frames on the processor
  and builds the GPU's pipelines beside them.

## What to try next

1. **Start MuPDF before the window.** Carry out the start's futures in
   `setup` rather than at the first `poll`: on the GPU path the page would
   be back about 90 ms sooner.
2. **Make the benchmark's cache warm by default** and report cold starts
   apart, since a reader opens pdvim warm almost every time.

## Every mark

Milliseconds from the process's start, median (fastest–slowest) of 7
runs. `python3 experiments/bench/marks.py data/launch-*.jsonl` prints it
again.

| Mark | auto, cold | auto, warm | gpu, cold | gpu, warm |
| --- | ---: | ---: | ---: | ---: |
| `pdvim.run` | 10 (0–10) | 10 (0–10) | 10 (0–10) | 10 (0–10) |
| `loop.built` | 13 (2–17) | 13 (3–13) | 12 (3–14) | 13 (3–13) |
| `loop.run` | 13 (2–17) | 13 (3–13) | 12 (3–14) | 13 (3–13) |
| `loop.resumed` | 13 (2–17) | 13 (3–13) | 12 (3–14) | 13 (3–13) |
| `pdvim.init` | 13 (3–17) | 13 (3–13) | 13 (3–15) | 13 (3–13) |
| `pdvim.init.done` | 15 (4–19) | 15 (4–15) | 14 (5–16) | 14 (4–15) |
| `text.begin` | 15 (4–19) | 15 (4–15) | 14 (5–16) | 14 (4–15) |
| `text.fonts` | 15 (4–19) | 15 (5–15) | 14 (5–17) | 15 (5–15) |
| `app.setup` | 15 (4–19) | 15 (5–15) | 15 (5–17) | 15 (5–15) |
| `app.setup.done` | 15 (5–19) | 15 (5–15) | 15 (5–17) | 15 (5–15) |
| `window.create` | 15 (5–19) | 15 (5–15) | 15 (5–17) | 15 (5–15) |
| `gpu.begin` | – | – | 16 (6–20) | 16 (6–17) |
| `window.native` | 17 (6–20) | 16 (6–17) | 16 (6–20) | 16 (6–17) |
| `window.frame_installed` | 17 (6–20) | 16 (6–17) | 16 (6–20) | 16 (6–17) |
| `window.framed` | 17 (6–20) | 16 (6–17) | 16 (6–20) | 16 (6–17) |
| `cpu.side` | 17 (7–21) | 17 (7–18) | – | – |
| `frame.first.layout` | 21 (9–24) | 19 (9–20) | – | – |
| `frame.first.masks` | 21 (9–24) | 19 (9–21) | – | – |
| `frame.first.painted` | 21 (9–24) | 19 (10–21) | – | – |
| `frame.first.cpu` | 21 (9–24) | 19 (10–21) | – | – |
| `gpu.window.start` | 25 (16–31) | 26 (16–28) | – | – |
| `window.created` | 17 (7–21) | 17 (7–18) | 204 (197–214) | 50 (36–54) |
| `frame.first.presented` | 21 (9–24) | 19 (10–21) | 215 (204–222) | 55 (40–58) |
| `text.complete` | 21 (9–24) | 19 (10–21) | 215 (204–222) | 55 (40–58) |
| `input.ready` | 21 (9–24) | 19 (10–21) | 215 (204–222) | 55 (40–58) |
| `gpu.adapter` | 47 (39–62) | 46 (36–48) | 34 (26–40) | 37 (24–39) |
| `gpu.surface` | – | – | 41 (34–47) | 41 (28–44) |
| `renderer.begin` | – | – | 41 (34–47) | 41 (28–44) |
| `gpu.device` | 52 (44–68) | 47 (38–50) | 40 (31–46) | 40 (26–42) |
| `renderer.ready` | – | – | 204 (197–214) | 50 (36–54) |
| `pdvim.page` | 115 (57–121) | 122 (114–129) | 297 (286–305) | 135 (117–144) |
| `frame.first.gpu_done` | – | – | 215 (204–222) | 55 (40–58) |
| `pdvim.page.drawn` | 120 (62–133) | 124 (116–134) | 301 (291–309) | 139 (122–149) |
| `gpu.window.warmed` | 235 (222–280) | 58 (48–60) | – | – |
| `gpu.window.ready` | 235 (222–280) | 58 (48–60) | – | – |
| `gpu.window.switched` | 235 (222–280) | 58 (48–60) | – | – |
| `gpu.window.attached` | 239 (225–284) | 61 (50–63) | – | – |
| `frame.first.build` | 239 (225–284) | 105 (97–112) | 204 (197–214) | 50 (36–54) |
| `frame.first.encoded` | 243 (229–288) | 108 (99–116) | 214 (204–222) | 55 (40–57) |
| `gpu.window.handed` | 244 (229–289) | 109 (100–117) | – | – |
