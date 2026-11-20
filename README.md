# pdvim

A minimal, keyboard-driven PDF reader with vi key bindings.

pdvim uses [MuPDF](https://mupdf.com/) to render pages and [iced](https://iced.rs/) to draw the window. Pages are laid out in one continuous column. It also remembers where you stopped reading each document.

## Features

- vi-style movement with counts (`12j`, `5J`, `42G`)
- Search forwards and backwards with `/` and `?`, then jump between matches with `n` and `N`
- Zoom, fit to width, fit to page, and rotate
- Recoloring for dark backgrounds, with colors you choose (like Zathura's `recolor`), following the system's light or dark theme by default
- Trimming of blank page margins
- Per-document memory of page, zoom, fit, and rotation, stored in SQLite
- A list of recently read documents to reopen from
- A configuration file for settings and key bindings
- Selecting and copying text from the keyboard, with a visual mode like vim's
- A sidebar with the table of contents, to go to a chapter
- Following links from the keyboard: `f` labels every link in view, and typing a label follows it
- A cheat sheet of the key bindings in force, with `:help`
- `Tab` completion of commands, settings, and file paths in the command prompt
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
| `Tab`                                       | Show the table of contents                 |

The mouse wheel also scrolls.

`Tab` opens the table of contents in a sidebar, with the chapter you're reading selected. Move with `j` / `k`, `gg` / `G`, and `Ctrl-d` / `Ctrl-u`. `l` opens a chapter to show its sections, or steps into them, and `h` closes it or goes up to the chapter it's in. `Space` opens and closes. `Enter` goes to the chapter and closes the sidebar, and `Esc`, `q`, or `Tab` close it without going anywhere.

### View

| Key                   | Action                           |
| --------------------- | -------------------------------- |
| `+`, `zi` / `-`, `zo` | Zoom in / out                    |
| `=`, `z0`             | Zoom to 100%                     |
| `{n}=`, `{n}zz`       | Zoom to _n_%                     |
| `s`, `zw`             | Fit the page width to the window |
| `zp`                  | Fit the whole page in the window |
| `r`                   | Rotate                           |
| `R`                   | Turn recoloring on or off        |
| `t`                   | Turn margin trimming on or off   |

### Links

| Key | Action                   |
| --- | ------------------------ |
| `f` | Label every link in view |

After `f`, each link in view is highlighted and gets a label of two letters. Type a label to follow its link. A link to another place in the document goes there, and any other link, such as a web page, opens with `xdg-open` in your default program. `Backspace` takes back the first letter, and `Esc` puts the labels away.

### Selecting text

| Key       | Action                                  |
| --------- | --------------------------------------- |
| `v` / `V` | Select characters / whole lines of text |

After `v` or `V`, each line of text in view gets a label of two letters. Type a label to put the cursor at the start of that line, and start selecting from there.

| Key in visual mode        | Action                                                     |
| ------------------------- | ---------------------------------------------------------- |
| `h` / `l`, arrow keys     | Move the cursor a character left / right                   |
| `j` / `k`, arrow keys     | Move the cursor a line down / up, onto the next pages      |
| `w` / `b` / `e`           | Move to the next word / the word before / word's end       |
| `0` / `$`, `Home` / `End` | Move to the start / end of the line                        |
| `^`                       | Move to the first character on the line that isn't a space |
| `f` / `F` + a character   | Move to the next / previous such character on the line     |
| `t` / `T` + a character   | Move to just before / just after it                        |
| `;` / `,`                 | Repeat the last `f`, `F`, `t`, or `T` / the other way      |
| `o`                       | Move the cursor to the other end of the selection          |
| `v` / `V`                 | Switch to selecting characters / lines, or leave           |
| `y`                       | Copy the selection to the clipboard, and leave             |
| `H`                       | Highlight the selection in the PDF file, and leave         |
| `Esc`, `Ctrl-c`           | Leave without copying                                      |

`H` saves the highlight into the PDF file as a standard highlight annotation, so other PDF readers show it too. pdvim saves the file incrementally: everything that was in it when pdvim opened it stays byte for byte as it was, and the changes are written after it. It can't highlight a PDF that MuPDF had to repair to open, or one it can't write to, and says so instead.

Press `u` to take back the last highlight, and `Ctrl-r` to put back the last one taken back. Each saves the file the same way, incrementally. pdvim remembers what to undo for each open document until you close it.

A count before a motion repeats it, as in `3w` or `2fe`. As in vim, `f`, `F`, `t`, and `T` look along the cursor's line only, and the cursor stays put if the character isn't there. These are the default keys; the `[keys.visual]` table of the [configuration](#configuration) changes them.

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

| Command                                | Action                                                               |
| -------------------------------------- | -------------------------------------------------------------------- |
| `:{n}`                                 | Go to page _n_                                                       |
| `:o[pen] {file}`                       | Open a document in a new tab (`~` expands to your home directory)    |
| `:rec[ent]`                            | Show recent documents                                                |
| `:h[elp]`                              | Show every action and the keys bound to it                           |
| `:noh[lsearch]`                        | Hide search highlights                                               |
| `:set recolor [true\|false\|auto]`     | Turn recoloring on or off, or follow the system's theme (default)    |
| `:set trim [true\|false]`              | Turn margin trimming on or off                                       |
| `:set recolor-lightcolor #rrggbb`      | Set the color white paper becomes when recolored (default `#1a1a1a`) |
| `:set recolor-darkcolor #rrggbb`       | Set the color black ink becomes when recolored (default `#dddddd`)   |
| `:set scroll-step {pixels}`            | Set how far one `j` or `k` scrolls                                   |
| `:set confirm-quit [true\|false]`      | Ask before quitting (default on)                                     |
| `:set confirm-tab-close [true\|false]` | Ask before closing a tab while others are open (default on)          |
| `:set highlight-color #rrggbb`         | Set the color `H` in visual mode highlights in (default `#ffff00`)   |
| `:map [mode] {keys} {action}`          | Bind keys to an action (see [Keys](#keys-1))                         |
| `:unmap [mode] {keys}`                 | Remove a key binding                                                 |
| `:mapclear [mode]`                     | Remove every key binding of a mode                                   |
| `:tabe[dit] {file}`, `:tabnew [file]`  | Open a document in a new tab, or open an empty tab                   |
| `:tabn[ext] [n]`                       | Go to the next tab, or to tab _n_                                    |
| `:tabp[revious]`                       | Go to the previous tab                                               |
| `:tabc[lose]`                          | Close the tab, unless it is the last                                 |
| `:q[uit]`                              | Close the tab, and quit after the last one                           |
| `:qa[ll]`                              | Quit                                                                 |

If you name a boolean setting without a value, it is turned on.

`:help` shows a cheat sheet of every action in every mode, the keys bound to it, and what it does. It shows your own bindings, including changes from the configuration file and `:map`. Scroll it with `j` / `k`, `Ctrl-d` / `Ctrl-u`, and `g` / `G`, and close it with `q` or `Esc`. To open it with a key, bind the `help` action, as in `"ctrl+h" = "help"`.

You can also run any [action](#actions) by name, as in `:scroll-down` or `:next-tab`. A number after it works like a count typed before a key: `:zoom-to 150` zooms to 150%, and `:first-page 12` goes to page 12.

Press `Tab` in the command prompt to complete what you're typing: command and action names, setting names and their values, mode and action names after `:map`, and file paths after `:open`, `:tabedit`, and `:tabnew`. Paths offer folders and PDF files. If more than one completion fits, `Tab` fills in as much as they share and lists them above the prompt. Press `Tab` again to go through the list, and `Shift-Tab` to go back. While the list is open, it narrows as you type, and it closes when nothing fits.

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

pdvim's own defaults are a configuration file too: [`defaults.toml`](defaults.toml), built into pdvim and read before yours. It lists every setting and every key binding of every mode. To start from it, copy it into place and change what you like:

```sh
mkdir -p ~/.config/pdvim
cp defaults.toml ~/.config/pdvim/config.toml
```

A shorter file works too, since anything you leave out keeps its default:

```toml
[settings]
# Recolor and trim documents that have not been opened before.
recolor = true
trim = true
scroll-step = 60
confirm-quit = true
recolor-lightcolor = "#1a1a1a"
recolor-darkcolor = "#dddddd"

[keys.normal]
# Move between tabs with Ctrl-n and Ctrl-p.
"ctrl+n" = "next-tab"
"ctrl+p" = "previous-tab"
# Quit with Q instead of q.
"q" = "none"
"shift+q" = "quit"

[keys.visual]
# Copy with Y as well as y.
"Y" = "yank"
```

`[settings]` takes the same names and values as `:set`. With `confirm-quit` on, as it is by default, pdvim asks `quit pdvim? (y/n)` before quitting: when you close the last tab, use `:qa`, or close the window. Press `y` to quit, or any other key to stay. If the window is closed again while pdvim is asking, for example by a window manager that insists, it quits without asking. With `confirm-tab-close` on, also the default, pdvim asks `close tab {name}? (y/n)` before closing a tab with `q`, `:q`, or `:tabclose` while other tabs are open. Closing the last tab quits pdvim, so `confirm-quit` decides whether that asks. `recolor` is `"auto"` by default: pdvim recolors every document while the system prefers a dark theme, shows them as printed while it prefers a light one, and changes as soon as the system does. It reads the preference from the desktop's color-scheme setting (`org.freedesktop.appearance` through the desktop portal on Linux). `R` still turns recoloring on or off for the document in view, until the system's theme changes again. With `recolor` set to `true` or `false`, it and `trim` decide how a document starts the first time you open it. After that, the document is restored the way you left it.

`[keys.<mode>]` binds keys to that mode's actions, on top of the default keys. Bind keys to `"none"` to remove a default. Keys put straight in `[keys]` are bound in normal mode, as before modes could be configured. The modes are:

| Mode      | When                                                          |
| --------- | ------------------------------------------------------------- |
| `normal`  | Reading: the [actions](#actions) below                        |
| `visual`  | Selecting text after `v` or `V`                               |
| `hints`   | Labels on links or lines; letters not bound type a label      |
| `outline` | The table of contents                                         |
| `recent`  | The recent-documents list; digits not bound open that entry   |
| `help`    | The cheat sheet                                               |
| `prompt`  | Typing after `:`, `/` or `?`; keys not bound type themselves  |
| `confirm` | Questions such as `quit pdvim? (y/n)`; keys not bound mean no |

`defaults.toml` and `:help` list every mode's actions.

If an entry has a mistake, pdvim skips it, shows the first mistake in the status bar, and prints every mistake to standard error. If the file is not valid TOML, pdvim ignores all of it.

### Keys

Write a key as a single character, such as `j`, `G`, or `?`, or by name: `space`, `tab`, `escape`, `enter`, `backspace`, `up`, `down`, `left`, `right`, `pageup`, `pagedown`, `home`, or `end`. Put `ctrl+` or `shift+` in front for a key with modifiers: `ctrl+d`, `shift+space`, `shift+tab`, `ctrl+tab`, `ctrl+shift+tab`. `shift+q` is the same as `Q`.

Separate the keys of a sequence with spaces, as in `"g g"` or `"z i"`. A count typed before the keys is passed to the action, as with the built-in keys. If one binding is the start of a longer one, the shorter one wins: binding `g` hides `g g`.

The `:map`, `:unmap`, and `:mapclear` commands change the bindings while pdvim runs. They write keys the same way, with the mode first for any mode but normal: `:map ctrl+n next-tab`, `:map visual Y yank`, `:unmap outline q`, `:mapclear prompt`.

Two things are typed rather than bound: the character after `f`, `F`, `t` or `T` in visual mode, and the letters of a label.

### Actions

The actions of normal mode, and their default keys:

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
| `fit-width` / `fit-page`              | `s`, `z w` / `z p`                          |
| `rotate`                              | `r`                                         |
| `toggle-recolor`                      | `R`                                         |
| `toggle-trim`                         | `t`                                         |
| `search-forward` / `search-backward`  | `/` / `?`                                   |
| `search-next` / `search-previous`     | `n` / `N`                                   |
| `command-prompt`                      | `:`                                         |
| `recent`                              | `o`                                         |
| `next-tab` / `previous-tab`           | `g t`, `ctrl+tab` / `g T`, `ctrl+shift+tab` |
| `abort`                               | `escape`, `ctrl+c`, `ctrl+[`                |
| `quit`                                | `q`                                         |
| `help`                                | none                                        |
| `follow-link`                         | `f`                                         |
| `visual` / `visual-line`              | `v` / `V`                                   |
| `undo` / `redo`                       | `u` / `ctrl+r`                              |
| `outline`                             | `tab`                                       |

With a count, `first-page` and `last-page` go to that page, `zoom-to` zooms to that percentage, and `next-tab` goes to that tab.

## Reading history

pdvim saves where you left each document in `$XDG_DATA_HOME/pdvim/history.sqlite`. If `XDG_DATA_HOME` is not set, it uses `~/.local/share/pdvim/history.sqlite`. Entries are keyed by file path.

## Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets for the parts of pdvim that read input it does not control: PDF files, the configuration file, `:` commands, key names and key presses. See [fuzz/README.md](fuzz/README.md).

```sh
cargo install cargo-fuzz
fuzz/run.sh pdf 600     # fuzz PDF loading for ten minutes
```

## Limitations

Password-protected PDFs are not supported yet.

## License

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html)
