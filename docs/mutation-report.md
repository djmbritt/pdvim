# What cargo-mutants found in pdvim's tests

Run on the night of 23–24 November 2026, on commit `f139664`.

## Summary

cargo-mutants made 4,128 small changes to pdvim's library, one at a time, and ran the 369 library tests after each. The run was stopped halfway, after 2,034 changes, when the machine ran low on memory.

Of those 2,034, the tests caught 900 and missed 651; 19 made a test hang, which counts as caught, and 464 did not compile. So about two in five of the changes that built went unnoticed.

Most misses are not bugs in pdvim. They show where the tests run code without checking what it does. Four patterns account for nearly all of them, and two cheap kinds of test would catch the most.

## What a miss means

A test suite that passes with `>` changed to `>=`, or with a whole match arm deleted, would also pass with that bug written by hand. A miss marks code where a future mistake would go unnoticed. Some misses are harmless, such as a `>=` that can never differ from `>` in practice, so each needs a look before writing a test for it.

## Results by file

The run reached the files below; `src/app/*`, `visual.rs`, `reader.rs` and the rest were not reached.

| File | Caught | Missed | Missed, share |
| --- | ---: | ---: | ---: |
| `src/app.rs` | 140 | 365 | 72% |
| `src/backend.rs` | 258 | 147 | 36% |
| `src/contents.rs` | 95 | 65 | 41% |
| `src/complete.rs` | 31 | 16 | 34% |
| `src/fonts.rs` | 5 | 15 | 75% |
| `src/config.rs` | 8 | 12 | 60% |
| `src/history.rs` | 30 | 6 | 17% |
| `src/dirs.rs` | 0 | 6 | 100% |
| `src/flash.rs` | 19 | 5 | 21% |
| `src/command.rs` | 168 | 9 | 5% |
| `src/hints.rs` | 19 | 2 | 10% |
| `src/help.rs` | 21 | 2 | 9% |
| `src/input.rs` | 1 | 1 | 50% |
| `src/layout.rs` | 45 | 0 | 0% |
| `src/keymap.rs` | 50 | 0 | 0% |
| `src/jumps.rs` | 10 | 0 | 0% |

An earlier run over `visual.rs`, `reader.rs` and `command.rs` missed 155 of 862 (18%), most of them in `reader.rs` (84) and `visual.rs` (62).

## What the misses show

### 1. Many tests check that nothing goes wrong, not that the result is right

This is the largest source of misses, and it is by design.

- **The drawing tests** (`src/app/drawing.rs`) draw every screen with the renderer that needs no window, to find panics and labels drawn off the page. They never compare what is drawn with a picture. So deleting a label's text, position, colour or size passes every test: 16 misses in `draw_text_label` alone, and about 70 of the 365 in `app.rs` are drawing code.
- **The every-key tests** (`src/app/every_key.rs`) press every default key in every mode and run every command, and check that nothing panics and the reader stays sound. They do not check that a key did what it is for. So 16 arms of `App::act` can be deleted outright, among them `ShowMatches`, `Write`, `SelectAgain` and following a link, and 19 arms of `translate`, which turns key presses into pdvim's keys (the arrow keys among them).

### 2. Some code has no test at all

- `App::on_wheel`, the mouse wheel's scrolling, has the most misses of any function (29), and no test calls it.
- `dirs.rs` (where configuration, data and history live) caught nothing. `fonts.rs` and `config.rs` mostly miss too: they read the system and the reader's own files, which the tests rightly keep away from.

### 3. Tests check that something happened, not how much

Changing `*` to `+` or `/`, or `-` to `+`, passes in the arithmetic of:

- `Marking::colour`, the colours of highlights (18 misses);
- `App::render_missing`, which pages are rendered and how sharply (14);
- `on_wheel` and `move_view`, how far the view moves.

Arithmetic changes are 172 of the 651 misses.

### 4. Edges and thresholds are not pinned

115 misses change a comparison, such as `<` to `<=` or `==`, and 56 swap `&&` and `||`. They cluster in the code that guesses:

- the table of contents built from headings and running heads (`from_headings`, 25 misses, and `running_heads`, 15, in `contents.rs`);
- the order lines are read in (`in_reading_order` in `backend.rs`, 21).

A line one point either side of a threshold changes what counts as a heading or which column comes first, so these are where wrong guesses would hide.

### 5. What is well tested

`layout.rs`, `keymap.rs` and `jumps.rs` had no misses, and `command.rs` only 5%. They are built from small pure functions, each tested directly. `show_line_hints` in `app.rs` shows the opposite: its check for what is on screen was a condition inside a closure, hard to reach exactly from a test, until it was moved into a function of its own (`shows_in`) and tested at each edge.

## Where tests would pay off most

1. **Make the every-key tests check outcomes** for the actions that have one: after `Write` the file is saved, after `ShowMatches` the list shows, after `SelectAgain` the selection is back. This catches dozens of misses at once in `App::act`, and the arrow keys in `translate`.
2. **Test the mouse wheel:** a line of wheel scrolls exactly one step, scaled by `wheel_speed`; pixels scroll by pixels.
3. **Pin the contents guesser's thresholds**, with lines just either side of each limit, and the reading order with two columns whose edges almost touch.
4. **Check a few drawn labels**, only where the drawing matters enough to be worth the upkeep of comparing it.

Done since:

- tests for the misses in `show_line_hints` and `lines_in_view`, with the on-screen check moved into `shows_in`;
- the every-key tests check what each key with a visible effect does (item 1);
- the mouse wheel is tested for its exact distances, and for zooming and changing a book's type with control held (item 2);
- the contents guesser's thresholds, with the margin check moved into `in_margin`, and `translate` for every named key (item 3).

23 of the missed mutants in the wheel, the contents guesser and `translate` were applied by hand, and each now fails a test; the every-key checks were not tried this way. One wheel mutant cannot be caught: `<` to `<=` in `down < 0.0` for books, where `down` is never 0.

## Running it

- **Time.** Half the run took about four hours at six jobs, so the whole of it takes about eight. Each test run takes 14 s alone and about 28 s with six at once.
- **Memory.** cargo-mutants builds each job's copy under `$TMPDIR`, 3 GB or more apiece. Here `/tmp` is held in memory, so six copies came to some 18 GB of it; that, not the tests, is why the run was stopped. A stopped run also leaves its copies behind in `/tmp/cargo-mutants-*`. Put the copies on disk:

  ```sh
  TMPDIR=~/.cache/mutants cargo mutants -j 6
  ```

- **Mutants that do not build.** 23% did not compile, as is usual in Rust. They cost only build time.
- **Results** of this run are in `target/mutants-full/mutants.out/`: `missed.txt` lists every miss with its file, line and change.
