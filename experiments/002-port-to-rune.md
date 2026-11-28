# 002 — Porting from iced to Rune

28 November 2026 · branch `rune-port` · Rune `dev` at
[`7e3d266`](https://github.com/anomalyco/rune/commit/7e3d2662b4b2ded3d350cbb6cb46db79d2fc2dd3)

## Question

What does it take to take iced out of pdvim and draw the window with
[Rune](https://github.com/anomalyco/rune) instead, and what changes for the
reader?

## Approach

pdvim is written in iced's Elm style: `update(Message) -> Task<Message>` and
`view() -> Element`. Rune is built differently: handlers on elements change
the state in place (`Fn(&mut S, &mut Tx)`), a `poll` hook takes in work done
on other threads, and `view(&S, &mut Cx<S>) -> El` is re-run after each
change and diffed.

Rather than rewrite pdvim's 32,000 lines around Rune's handlers, the port
keeps `update`, `Message` and `Task`, and changes only what touches the
toolkit:

| Piece | iced | Rune |
| --- | --- | --- |
| `Task<Message>` | iced's task | `src/task.rs`: a list of steps (a future, a clipboard write or read, full screen, quit) |
| Running tasks | iced's runtime | `src/app/window.rs`: futures run on a 2-thread `futures` pool; answers come back over a channel, the pool wakes the window, `poll` hands them to `update` |
| Keys | `keyboard::listen()` subscription | `AppSpec::on_key`, translated to pdvim's own `Key` |
| Mouse over pages | `canvas::Program::update` | `on_drag` (left and middle buttons), `on_wheel`, `on_pointer_move_local` for the cursor shape |
| Size of the page area | the canvas comparing its bounds each event | `on_layout_tx` on the page layer, which also reports the screen's scale |
| System light/dark | `system::theme_changes()` | `Cx::system_dark()` in the view; a marker element keyed by it reports each change through `on_mount` |
| Closing | `window::close_requests()` | `App::on_close_request`, answering `Defer`; pdvim quits with `Tx::quit` when it is ready |
| Title, full screen, clipboard | `title`, `window::set_mode`, `clipboard::write/read` | `Tx::set_title`, `Tx::set_full_screen`, `Tx::copy`, `Tx::read_clipboard` |
| Pages and highlights | two `Canvas` layers | `src/ui.rs`'s `Frame`: each rectangle, line of text and page is an absolutely placed element; pages are `rune::Pixels` |
| Lists, panels, bars | `container`/`row`/`column`/`text`, `mouse_area` | `el`/`row`/`col`/`text` with `on_press_tx`; shared pieces in `src/app/widgets.rs` |
| Colours, points, sizes | `iced::Color` and friends | `src/ui.rs`: small types of the same shape, sRGB converted to Rune's linear colour |
| Drawing in tests | `iced_test::Simulator` with tiny-skia | `rune::Runtime::new` headless: lays the window out and builds it, no GPU |

### What it came to

- 31 files changed, plus 4 new ones (`task.rs`, `ui.rs`, `app/window.rs`,
  `app/widgets.rs`, 606 lines): about 1,480 lines in and 1,365 out.
- `Cargo.toml`: `iced`, and the dev-dependencies `iced_runtime` and
  `iced_test`, out; `rune` (git, pinned) and `futures` in. Rune needs its
  own patched `winit`, `sctk-adwaita`, `parley_engine`, `wgpu-hal` and
  `naga`, so pdvim's manifest carries a `[patch.crates-io]` for them at the
  same revision. (Its AccessKit patches are left out: pdvim builds Rune
  without its `accessibility` feature, as it had none on iced.)
- Rune is built with `default-features = false` and only `dbus` (the
  desktop's light/dark preference) and `embedded-fonts`.
- `font-kit` now loads fontconfig at run time (`source-fontconfig-dlopen`):
  Rune does, and the two must agree or `font-kit` does not compile.
- `.cargo/config.toml` (which told `iced_test` to draw with tiny-skia) is
  gone.
- The minimum Rust goes from 1.90 to **1.99**, Rune's own toolchain. Raising
  it turned on clippy's `map_unwrap_or` suggestions for `map_or_default`,
  which is stable from then; those 26 places were changed as clippy said.

### Things learned on the way

1. **Rune animates by default.** Layout moves on springs and elements fade
   in (0.3 s) and out (0.24 s). Right for an app, wrong for a reader: the
   first build showed new pages as faint ghosts after paging, with the old
   page left on top. Every message is now handled with `Tx::instant()`, the
   root is `instant_changes()`, and everything drawn over the page area has
   `Transition::none()`. After that, pages, highlights and labels land in
   the same frame, as on iced.
2. **The wheel.** Rune turns wheel notches into points; pdvim scrolls by
   its own steps per notch. The window is given `wheel_line(1.0)` so that a
   notch is one point and can be turned back into lines.
3. **Fontconfig.** See above: a feature clash between Rune's text stack and
   `font-kit`, found at the first build.
4. **Clippy's `struct_excessive_bools`.** Adding a `bool` to `App` went over
   the limit; the wheel's Control flag lives in `pointer::Mouse` instead.

### Checked by hand

On Hyprland, the release build opened the Ghostscript manual and drew it
the same as iced did, pixel for pixel within 5/255. Scrolling, paging, `G`,
the command line with its completions, `:help`, search highlights, link
labels (`f`), the contents sidebar (Tab) and the overview (`go`) all work.
The bars and panels look as they did, in Rune's own monospaced type rather
than iced's.

Not checked by hand: the mouse (nothing on this machine can send clicks or
the wheel to a window without taking the person's focus), and the system
theme following.

### Tests

All 402 tests pass (`cargo test`, 2 min 8 s on this machine). The drawing
tests and the every-key tests now build the window with Rune's headless
runtime rather than iced's simulator.

## Not done

- **`fuzz/`** builds pdvim as a dependency in a workspace of its own, which
  needs the same `[patch.crates-io]` lines to build against Rune. It was
  left alone, since changing it changes `fuzz/Cargo.lock`.
- **Packaging.** A git dependency changes how the Flatpak's
  `cargo-sources.json` and the AUR and Copr builds fetch sources, and the
  minimum Rust rises to 1.99. None of the packaging was touched.
- **Accessibility.** Rune can expose the window to screen readers
  (AccessKit); pdvim on iced had none, and the port keeps it off.
