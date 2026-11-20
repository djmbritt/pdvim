# pdvim

A minimal, keyboard-driven PDF reader with vi key bindings.

pdvim uses [MuPDF](https://mupdf.com/) to render pages and [iced](https://iced.rs/) to draw the window. Pages are laid out in one continuous column. It also remembers where you stopped reading each document.

## Features

- vi-style movement with counts (`12j`, `5J`, `42G`)
- Search forwards and backwards with `/` and `?`, then jump between matches with `n` and `N`
- Zoom, fit to width, fit to page, and rotate
- Recoloring for dark backgrounds, with colors you choose (like Zathura's `recolor`)
- Trimming of blank page margins
- Per-document memory of page, zoom, fit, and rotation, stored in SQLite
- A list of recently read documents to reopen from
- A configuration file for settings and key bindings
- Tabs, with vim's `gt` / `gT`, `Ctrl-Tab` / `Ctrl-Shift-Tab`, and `:tabedit`

## Building

pdvim needs Rust 2024 edition (Rust 1.85 or newer). The `mupdf` crate builds MuPDF from source, so you also need a C toolchain and `libclang`.

```sh
cargo build --release
```

The binary is written to `target/release/pdvim`.

## Usage

```sh
pdvim [FILE]...
```

Each file opens in a tab of its own. If you start pdvim without a file, it shows the documents you read most recently. Pick one, or open another file with `:open`.

`:open` and the recent list put each document in a new tab, unless the tab in view is empty. A document that is already open in a tab is brought into view instead.

## Keys

Put a count before a key to repeat it, as in vi. For example, `3j` scrolls three steps down and `12G` goes to page 12.

### Movement

| Key                                         | Action                                     |
| ------------------------------------------- | ------------------------------------------ |
| `j` / `k` / `h` / `l`, arrow keys           | Scroll down / up / left / right            |
| `Ctrl-d` / `Ctrl-u`                         | Scroll half a screen down / up             |
| `Ctrl-f`, `Space` / `Ctrl-b`, `Shift-Space` | Scroll a full screen down / up             |
| `J`, `PageDown` / `K`, `PageUp`             | Go to the next / previous page             |
| `gg`, `Home` / `G`, `End`                   | Go to the first / last page                |
| `{n}G`, `{n}gg`                             | Go to page _n_                             |
| `H` / `L`                                   | Go to the top / bottom of the current page |

The mouse wheel also scrolls.

### View

| Key                   | Action                           |
| --------------------- | -------------------------------- |
| `+`, `zi` / `-`, `zo` | Zoom in / out                    |
| `=`, `z0`             | Zoom to 100%                     |
| `{n}=`, `{n}zz`       | Zoom to _n_%                     |
| `s`                   | Fit the page width to the window |
| `a`                   | Fit the whole page in the window |
| `r`                   | Rotate                           |
| `Ctrl-r`              | Turn recoloring on or off        |
| `t`                   | Turn margin trimming on or off   |

### Tabs

| Key                                       | Action                        |
| ----------------------------------------- | ----------------------------- |
| `gt`, `Ctrl-Tab` / `gT`, `Ctrl-Shift-Tab` | Go to the next / previous tab |
| `{n}gt`, `{n}Ctrl-Tab`                    | Go to tab _n_                 |

### Search and prompt

| Key                       | Action                                                 |
| ------------------------- | ------------------------------------------------------ |
| `/` / `?`                 | Search forwards / backwards                            |
| `n` / `N`                 | Go to the next / previous match                        |
| `:`                       | Open the command prompt                                |
| `o`                       | Show recent documents                                  |
| `Esc`, `Ctrl-c`, `Ctrl-[` | Cancel a half-typed command and hide search highlights |
| `q`                       | Close the tab, and quit after the last one             |

## Commands

| Command                               | Action                                                               |
| ------------------------------------- | -------------------------------------------------------------------- |
| `:{n}`                                | Go to page _n_                                                       |
| `:o[pen] {file}`                      | Open a document in a new tab (`~` expands to your home directory)    |
| `:rec[ent]`                           | Show recent documents                                                |
| `:noh[lsearch]`                       | Hide search highlights                                               |
| `:set recolor [true\|false]`          | Turn recoloring on or off                                            |
| `:set trim [true\|false]`             | Turn margin trimming on or off                                       |
| `:set recolor-lightcolor #rrggbb`     | Set the color white paper becomes when recolored (default `#1a1a1a`) |
| `:set recolor-darkcolor #rrggbb`      | Set the color black ink becomes when recolored (default `#dddddd`)   |
| `:set scroll-step {pixels}`           | Set how far one `j` or `k` scrolls                                   |
| `:map {keys} {action}`               | Bind keys to an action (see [Keys](#keys-1))                         |
| `:unmap {keys}`                       | Remove a key binding                                                 |
| `:mapclear`                           | Remove every key binding                                             |
| `:tabe[dit] {file}`, `:tabnew [file]` | Open a document in a new tab, or open an empty tab                   |
| `:tabn[ext] [n]`                      | Go to the next tab, or to tab _n_                                    |
| `:tabp[revious]`                      | Go to the previous tab                                               |
| `:tabc[lose]`                         | Close the tab, unless it is the last                                 |
| `:q[uit]`                             | Close the tab, and quit after the last one                           |
| `:qa[ll]`                             | Quit                                                                 |

If you name a boolean setting without a value, it is turned on.

## Recent documents

`o` or `:recent` lists up to 20 documents you read before, the most recent first. The open document and files that no longer exist are left out.

| Key                                        | Action                                  |
| ------------------------------------------ | --------------------------------------- |
| `j` / `k`, arrow keys, `Ctrl-n` / `Ctrl-p` | Move the selection down / up            |
| `gg`, `Home` / `G`, `End`                  | Select the first / last entry           |
| `Enter`, `l`, `o`                          | Open the selected document in a new tab |
| `1`–`9`                                    | Open that entry in a new tab            |
| `Esc`, `Ctrl-c`, `q`                       | Close the list                          |

## Configuration

pdvim reads `$XDG_CONFIG_HOME/pdvim/config.toml` when it starts. If `XDG_CONFIG_HOME` is not set, it reads `~/.config/pdvim/config.toml`. You don't need the file.

[`config.example.toml`](config.example.toml) lists every setting and key binding at its default. To start from it, copy it into place and change what you like:

```sh
mkdir -p ~/.config/pdvim
cp config.example.toml ~/.config/pdvim/config.toml
```

A shorter file works too, since anything you leave out keeps its default:

```toml
[settings]
# Recolor and trim documents that have not been opened before.
recolor = true
trim = true
scroll-step = 60
recolor-lightcolor = "#1a1a1a"
recolor-darkcolor = "#dddddd"

[keys]
# Move between tabs with Ctrl-n and Ctrl-p.
"ctrl+n" = "next-tab"
"ctrl+p" = "previous-tab"
# Quit with Q instead of q.
"q" = "none"
"shift+q" = "quit"
```

`[settings]` takes the same names and values as `:set`. `recolor` and `trim` decide how a document starts the first time you open it. After that, the document is restored the way you left it.

`[keys]` binds keys to the [actions](#actions) below, on top of the default keys. Bind keys to `"none"` to remove a default.

If an entry has a mistake, pdvim skips it, shows the first mistake in the status bar, and prints every mistake to standard error. If the file is not valid TOML, pdvim ignores all of it.

### Keys

Write a key as a single character, such as `j`, `G`, or `?`, or by name: `space`, `escape`, `enter`, `backspace`, `up`, `down`, `left`, `right`, `pageup`, `pagedown`, `home`, or `end`. Put `ctrl+` or `shift+` in front for a key with modifiers: `ctrl+d`, `shift+space`, `ctrl+tab`, `ctrl+shift+tab`. `shift+q` is the same as `Q`.

Separate the keys of a sequence with spaces, as in `"g g"` or `"z i"`. A count typed before the keys is passed to the action, as with the built-in keys. If one binding is the start of a longer one, the shorter one wins: binding `g` hides `g g`.

The `:map {keys} {action}`, `:unmap {keys}`, and `:mapclear` commands change the bindings while pdvim runs. They write keys the same way, as in `:map ctrl+n next-tab`.

The keys used in the command prompt and in the recent-documents list can't be changed.

### Actions

| Action                                | Default keys                                |
| ------------------------------------- | ------------------------------------------- |
| `scroll-down` / `scroll-up`           | `j`, `down` / `k`, `up`                     |
| `scroll-left` / `scroll-right`        | `h`, `left` / `l`, `right`                  |
| `scroll-half-down` / `scroll-half-up` | `ctrl+d` / `ctrl+u`                         |
| `scroll-full-down` / `scroll-full-up` | `ctrl+f`, `space` / `ctrl+b`, `shift+space` |
| `next-page` / `previous-page`         | `J`, `pagedown` / `K`, `pageup`             |
| `first-page` / `last-page`            | `g g`, `home` / `G`, `end`                  |
| `page-top` / `page-bottom`            | `H` / `L`                                   |
| `zoom-in` / `zoom-out`                | `+`, `z i`, `z I` / `-`, `z o`, `z O`       |
| `zoom-to`                             | `=`, `z z`, `z Z`                           |
| `zoom-reset`                          | `z 0`                                       |
| `fit-width` / `fit-page`              | `s` / `a`                                   |
| `rotate`                              | `r`                                         |
| `toggle-recolor`                      | `ctrl+r`                                    |
| `toggle-trim`                         | `t`                                         |
| `search-forward` / `search-backward`  | `/` / `?`                                   |
| `search-next` / `search-previous`     | `n` / `N`                                   |
| `command-prompt`                      | `:`                                         |
| `recent`                              | `o`                                         |
| `next-tab` / `previous-tab`           | `g t`, `ctrl+tab` / `g T`, `ctrl+shift+tab` |
| `abort`                               | `escape`, `ctrl+c`, `ctrl+[`                |
| `quit`                                | `q`                                         |

With a count, `first-page` and `last-page` go to that page, `zoom-to` zooms to that percentage, and `next-tab` goes to that tab.

## Reading history

pdvim saves where you left each document in `$XDG_DATA_HOME/pdvim/history.sqlite`. If `XDG_DATA_HOME` is not set, it uses `~/.local/share/pdvim/history.sqlite`. Entries are keyed by file path.

## Limitations

Password-protected PDFs are not supported yet.

## License

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html)
