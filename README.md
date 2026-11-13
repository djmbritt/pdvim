# pdvim

A minimal, keyboard-driven PDF reader with vi keybindings.

pdvim uses [MuPDF](https://mupdf.com/) to render pages and [iced](https://iced.rs/) to draw the window. Pages are laid out in one continuous column. It also remembers where you stopped reading each document.

## Features

- vi-style movement with counts (`12j`, `5J`, `42G`)
- Search forwards and backwards with `/` and `?`, then jump between matches with `n` and `N`
- Zoom, fit to width, fit to page, and rotate
- Recolouring for dark backgrounds, with colours you choose (like Zathura's `recolor`)
- Trimming of blank page margins
- Per-document memory of page, zoom, fit and rotation, stored in SQLite

## Building

pdvim needs Rust 2024 edition (Rust 1.85 or newer). The `mupdf` crate builds MuPDF from source, so you also need a C toolchain and `libclang`.

```sh
cargo build --release
```

The binary is written to `target/release/pdvim`.

## Usage

```sh
pdvim [FILE]
```

If you start pdvim without a file, open one later with `:open`.

## Keys

Put a count before a key to repeat it, as in vi. For example, `3j` scrolls three steps down and `12G` goes to page 12.

### Movement

| Key | Action |
| --- | --- |
| `j` / `k` / `h` / `l`, arrow keys | Scroll down / up / left / right |
| `Ctrl-d` / `Ctrl-u` | Scroll half a screen down / up |
| `Ctrl-f`, `Space` / `Ctrl-b`, `Shift-Space` | Scroll a full screen down / up |
| `J`, `PageDown` / `K`, `PageUp` | Go to the next / previous page |
| `gg`, `Home` / `G`, `End` | Go to the first / last page |
| `{n}G`, `{n}gg` | Go to page *n* |
| `H` / `L` | Go to the top / bottom of the current page |

The mouse wheel also scrolls.

### View

| Key | Action |
| --- | --- |
| `+`, `zi` / `-`, `zo` | Zoom in / out |
| `=`, `z0` | Zoom to 100% |
| `{n}=`, `{n}zz` | Zoom to *n*% |
| `s` | Fit the page width to the window |
| `a` | Fit the whole page in the window |
| `r` | Rotate |
| `Ctrl-r` | Turn recolouring on or off |
| `t` | Turn margin trimming on or off |

### Search and prompt

| Key | Action |
| --- | --- |
| `/` / `?` | Search forwards / backwards |
| `n` / `N` | Go to the next / previous match |
| `:` | Open the command prompt |
| `Esc`, `Ctrl-c`, `Ctrl-[` | Cancel a half-typed command and hide search highlights |
| `q` | Quit |

## Commands

| Command | Action |
| --- | --- |
| `:{n}` | Go to page *n* |
| `:o[pen] {file}` | Open a document (`~` expands to your home directory) |
| `:noh[lsearch]` | Hide search highlights |
| `:set recolor [true\|false]` | Turn recolouring on or off |
| `:set trim [true\|false]` | Turn margin trimming on or off |
| `:set recolor-lightcolor #rrggbb` | Set the colour white paper becomes when recoloured (default `#1a1a1a`) |
| `:set recolor-darkcolor #rrggbb` | Set the colour black ink becomes when recoloured (default `#dddddd`) |
| `:set scroll-step {pixels}` | Set how far one `j` or `k` scrolls |
| `:q[uit]` | Quit |

If you name a boolean setting without a value, it is turned on.

## Reading history

pdvim saves where you left each document in `$XDG_DATA_HOME/pdvim/history.sqlite`. If `XDG_DATA_HOME` is not set, it uses `~/.local/share/pdvim/history.sqlite`. Entries are keyed by file path.

## Limitations

Password-protected PDFs are not supported yet.

## License

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html)
