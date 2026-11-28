# 005 — Scrolling with a live offset

28 November 2026 · branch `rune-port` (with [`src/app/sliding.rs`](../src/app/sliding.rs))
· Rune `dev` at `7e3d266`

## Question

The port keeps pdvim's iced shape: every key runs `update`, then Rune runs
the whole view again, compares it with the last, lays it out and paints it.
Rune has a cheaper way for a change that only moves things: bind an
element's offset to a `Live` value, set the value, and say `Tx::live_only`,
and Rune neither runs the view nor lays it out. Its documentation shows a
pinch frame going from 13.7 ms to 2.0 ms that way.

Was the port leaving that on the table? How much does scrolling cost once
pdvim is built around it?

## The change

[`src/app/sliding.rs`](../src/app/sliding.rs), about 200 lines with its
documentation and 80 of tests, and small changes to the page layer.

- **Pages are laid out from an origin.** The page layer and the layer over
  it (search hits, selection, labels) are drawn as if the view were where
  it was when they were last laid out (the origin), with one more page on
  either side of those in view (as many as pdvim renders ahead). Both sit in
  one element, `sliding`, whose offset is bound to a `Live<[f32; 2]>`: how
  far the view has moved since. The reader gives screen positions from the
  origin while they are drawn (`Reader::draw_from`), so what is drawn
  depends on the origin alone.
- **A key that only moves the view only moves the offset.** `dispatch`
  takes a snapshot of everything the window shows apart from where the
  pages are (the title, the page and zoom in the status bar, what has been
  typed of a command, the marking's note, the tabs). It takes one only when
  nothing is shown over the pages: no prompt, list, panel, labels,
  selection or message. If, after the key:
  - the snapshot is the same,
  - the key asked for no work (no page to render, say),
  - and every page now in view was laid out,

  the offset is set to the distance moved and the handler says
  `live_only`. Otherwise the layers are laid out again from where the view
  is, and the offset goes back to zero. Answers from MuPDF, the mouse,
  resizing and everything else always lay out again.
- The backdrop moved from the page layer to the page area, which stays put,
  so that the slid layer never uncovers bare window.

### Is it right?

- **Tests.** Three new ones: pressing `j` slides the pages laid out before
  to exactly where the view puts them now, and some presses only slide;
  the prompt opening and a count being typed lay out again; `G` lays out
  again at the last page. All 405 pass.
- **Rune's own check.** A debug build of Rune runs the view again after
  each `live_only` event and warns if it built anything different. The
  first version drew the layers where the view is now and moved the
  container back by the offset: right on screen, but a different tree each
  time, and Rune said so (`layout rect of "pages/sliding" went from [0.0,
  0.0, …] to [0.0, 40.0, …]`). Drawing from the origin fixed it. With
  `j`, `k`, `l` and `h` pressed 44 times, 0.35 s apart so that each is
  checked, it warned of nothing.
- **On screen.** The manual after 45 × `j`, 5 × `k` and 3 × `l`, on the
  live build, then on the same window built again (`:` and Escape), then
  on the build before: the three screenshots are the same to the pixel.
  Compared with iced, only the page's edge differs (by at most 8/255), as
  in [002](002-port-to-rune.md).

## Method

- **Per frame**: builds of before and after with Rune's `frame-profile`
  feature (`cargo build --release --features frame-profile`, which pdvim
  now passes on), so that `RUNE_FRAME_LOG=FILE` writes each frame's stages
  as a JSON line. "Before" is the same code with the slide turned off.
- **The whole process**: the release builds without it, and iced.
- [`bench/frames.py`](bench/frames.py) opens a document, presses `j` 50
  times, then `k` 50 times, then `Ctrl-f` 20 times, a key every 50 ms, so
  that every press is a frame of its own, as a held key's repeats are. It
  quits with `:q` and `y`, and reads the processor time of the whole
  process over each run of keys.
- 5 runs of each build on the manual (477 pages) and its four-times copy
  (1,908 pages), interleaved, warm shader cache (see
  [004](004-where-rune-start-up-goes.md)). After each run, the script checked
  that no test window was left open.
  Raw: [`data/frames-profiled.jsonl`](data/frames-profiled.jsonl),
  [`data/frames-paced.jsonl`](data/frames-paced.jsonl).
  Tables: `python3 experiments/bench/framestats.py data/frames-profiled.jsonl`.

`measure.py`'s scroll figure, 50 × `j` sent as fast as `hyprctl` can, is
too coarse for this: its keys land several to a frame, and it counts
processor time in 10 ms ticks.

## Results

### A frame

The manual, medians of 5 runs; each stage is a frame's mean, in
microseconds of the window's thread (the 1,908-page copy is within 5%):

| | `j`, before | `j`, live | `k`, before | `k`, live | `Ctrl-f`, before | `Ctrl-f`, live |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Frames | 52 | 52 | 50 | 50 | 29 | 29 |
| Of them, the view run again | 52 | **5** | 50 | **1** | 29 | 28 |
| View | 46 | 5 | 46 | 1 | 52 | 47 |
| Compare with the last (commit) | 172 | 20 | 170 | 5 | 223 | 212 |
| Layout | 30 | 4 | 29 | 2 | 63 | 62 |
| Paint | 71 | 54 | 69 | 53 | 102 | 100 |
| Apply | 48 | 5 | 47 | 1 | 53 | 51 |
| **Building the frame, all** | 311 | **150** | 313 | **139** | 397 | 386 |
| Uploading | 36 | 33 | 25 | 23 | 208 | 207 |
| Encoding for the GPU | 69 | 74 | 68 | 75 | 79 | 80 |
| Submitting to the GPU | 182 | 189 | 179 | 187 | 201 | 208 |
| Presenting | 70 | 72 | 72 | 75 | 69 | 74 |
| **The frame** | 751 | **596** | 735 | **574** | 1,033 | 1,034 |

- **The slide does what Rune says**: 47 of 52 `j` frames and 49 of 50 `k`
  frames skip the view, the comparison, the layout and applying it, and
  building a frame takes half as long (311 → 150 µs).
- The frames that still build are those where a page comes into view (its
  rendering is asked for, then comes back) or the page number in the status
  bar changes.
- **But a frame only gets a fifth cheaper** (751 → 596 µs), because most of
  it was never the view. Painting (54 µs) still runs, and encoding,
  submitting and presenting to the GPU take about 340 µs of every frame,
  whatever changed. That cost is Rune's, the same for any app.
- `Ctrl-f` moves a whole screen, bringing new pages each time, so it lays
  out again and is unchanged.

### The whole process

Processor time of the whole process, every thread, for each run of keys:
median of 5 runs, in milliseconds (10 ms ticks):

| | iced | Rune, before | Rune, live |
| --- | ---: | ---: | ---: |
| 477 pages: 50 × `j` | 70 | 60 | **50** |
| 477 pages: 50 × `k` | 50 | 50 | **40** |
| 477 pages: 20 × `Ctrl-f` | 100 | **70** | **70** |
| 1,908 pages: 50 × `j` | 70 | **60** | **60** |
| 1,908 pages: 50 × `k` | 60 | 50 | **40** |
| 1,908 pages: 20 × `Ctrl-f` | 100 | **70** | 80 |

At this pace, **Rune already scrolled and paged for less than iced before
the change**: 50 `j` frames cost about 60 ms against iced's 70 ms, and
20 `Ctrl-f` 70 ms against 100. The slide saves about another 10 ms per 50
frames, at the edge of what 10 ms ticks can show.

## What it means

- **The port was not built around Rune's cheap path, and it cost
  something**: running the view, comparing it, laying it out and applying
  it was 40% of a scroll frame (296 of 751 µs). With live offsets it is
  2–6%.
- **That was not why Rune looked slower than iced.** The view's share was
  about 0.16 ms a frame, and Rune at a key per frame already cost less than
  iced. 003's 2–4 times was not reproduced later that day (see
  [004](004-where-rune-start-up-goes.md#003-measured-again)).
- **What is left of a frame is Rune's**: about 0.34 ms to encode, submit
  and present to the GPU, and 0.05 ms to paint. None of it is the GPU
  working: the times are the window thread's, nearly all of them
  processor time. A faster GPU would not change them; a faster processor,
  or fewer frames (a held key repeating faster than the screen refreshes),
  would.
- **Whether to keep it.** It halves the window's work per scroll frame for
  about 200 lines. The cost: everything shown over the pages has to be in
  `Still` or turn the slide off. Something added later that changes with
  the view, and is in neither, would be left behind on the screen, and
  only a test or Rune's debug check would catch it. Keep it, and keep
  Rune's check on in debug runs.

## What to try next

1. **Memoise the rest of the view** (`rune::memo`): the frames that do
   build (paging, a page coming into view) still compare the bars, lists and
   panels that did not change.
2. **Slide on the wheel and the touchpad** too. They take the same path
   already (`Message::Wheel`), but a touchpad sends many small moves per
   frame: worth measuring on its own.
3. **Ask Rune about the fixed cost per frame.** About 0.19 ms of every
   frame goes to submitting to the GPU, even for a frame that only moves
   one layer.
