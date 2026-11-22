# pdvim

A keyboard-driven PDF reader with vi keys. pdvim also reads EPUB and FB2 books, CBZ comics, XPS documents, SVG drawings and images.

pdvim renders pages with [MuPDF](https://mupdf.com/) and draws its window with [iced](https://iced.rs/). Pages run in one continuous column, or up to four side by side. pdvim remembers where you stopped in every document.

![pdvim showing the first page of Alice's Adventures in Wonderland, with two tabs open](docs/screenshots/reading.png)

## What it does

- **Moves like vi.** `j` / `k`, `gg` / `G`, counts such as `12G`, marks (`ma`, `` `a ``), a jump list (`Ctrl-o` / `Ctrl-i`), search with `/` and `?`, and `.` to repeat a command.
- **Needs no mouse.** `f` puts a label on every link and form field in view. Type a label to follow the link or fill in the field. `yi` does the same for pictures and figures, to copy one as an image.
- **Selects text like vim.** `v` and `V` start a selection at a labelled line, and `w`, `e`, `f`, `ip` and the other motions grow it. Copy the selection, or highlight, underline, strike out or squiggle it.
- **Saves standard annotations.** Highlights and notes go into the PDF when you save with `:w`, so other readers show them too. `u` undoes them.
- **Reads well at night.** Recolors pages to suit a dark theme, and follows the system's light or dark setting. The four [Catppuccin](https://catppuccin.com) flavors are built in. It can also trim blank margins.
- **Finds its way around.** A table of contents sidebar, `]]` / `[[` from chapter to chapter, a grid of every page with `go`, and printed page numbers such as `xii`.
- **Also has** tabs, a list of recent documents, password-protected PDFs, fillable forms, `:!` shell commands, and `:export` to PNG or SVG.
- **Is configured in one file.** A TOML file holds settings and key bindings for every mode. Settings can differ by path, and keys can run command lines.

## Screenshots

The table of contents, in the Catppuccin Mocha theme, with the page recolored to match:

![The table of contents sidebar over a recolored page of the Ghostscript manual](docs/screenshots/contents.png)

Visual mode starts from a line picked by its label:

![Two-letter labels on each line of text, to start selecting from](docs/screenshots/labels.png)

A highlight, an underline, a squiggly underline, and a selection in progress:

![Lines highlighted, underlined and squiggled, with a selection in progress](docs/screenshots/visual.png)

`:help` lists every key binding in force:

![The cheat sheet of key bindings](docs/screenshots/help.png)

## Installing

Each [release](https://github.com/djmbritt/pdvim/releases) has packages for:

| Platform             | Download                                                                                        |
| -------------------- | ----------------------------------------------------------------------------------------------- |
| Linux, x86-64        | `pdvim_VERSION-1_amd64.deb`, or `pdvim-VERSION-x86_64-unknown-linux-gnu.tar.gz`                 |
| Linux, x86-64        | `pdvim-VERSION-x86_64.AppImage`, or `pdvim-VERSION-x86_64.flatpak` for `flatpak install --user` |
| Fedora, openSUSE     | `pdvim-VERSION-1.x86_64.rpm`, for `sudo dnf install ./pdvim-VERSION-1.x86_64.rpm`               |
| Arch Linux           | `yay -S pdvim` or `paru -S pdvim` from the AUR, or `makepkg -si` in [`packaging/`](packaging/)  |
| Nix                  | `nix run github:djmbritt/pdvim`, or `nix profile add github:djmbritt/pdvim`                     |
| macOS, Apple silicon | `pdvim-VERSION-aarch64-apple-darwin.dmg`, or the `.tar.gz` for the terminal                     |
| macOS, Intel         | `pdvim-VERSION-x86_64-apple-darwin.dmg`, or the `.tar.gz`                                       |
| Windows, x86-64      | `pdvim-VERSION-x86_64-pc-windows-msvc.zip`                                                      |

The packages install the manual pages too: `man pdvim` covers the keys and commands, and `man 5 pdvim` covers the configuration file.

On Debian, Ubuntu, Fedora and openSUSE, the [package repository](https://djmbritt.github.io/pdvim/) keeps pdvim up to date with the rest of the system. On Debian and Ubuntu:

```sh
sudo install -d /etc/apt/keyrings
sudo curl -fsSLo /etc/apt/keyrings/pdvim.gpg https://djmbritt.github.io/pdvim/pdvim.gpg
sudo curl -fsSLo /etc/apt/sources.list.d/pdvim.sources https://djmbritt.github.io/pdvim/pdvim.sources
sudo apt update && sudo apt install pdvim
```

On Fedora:

```sh
sudo curl -fsSLo /etc/yum.repos.d/pdvim.repo https://djmbritt.github.io/pdvim/pdvim.repo
sudo dnf install pdvim
```

### Building from source

pdvim needs Rust 1.90 or newer. The `mupdf` crate builds MuPDF from source, so you also need a C toolchain and `libclang`. With Nix, `nix develop` opens a shell that has them, and `nix build` builds the package.

```sh
cargo build --release
```

The binary is written to `target/release/pdvim`.

## Getting started

```sh
pdvim paper.pdf book.epub   # each file opens in a tab of its own
pdvim                       # pick from the documents you read last
```

Some keys to try first:

- `j` / `k` scroll, `J` / `K` turn the page, and `gg` / `G` go to the first or last page.
- `Tab` opens the table of contents, and `go` shows every page small.
- `/` searches, and `n` / `N` go to the next or previous match.
- `f` follows a link by its label.
- `v` selects text. Then `y` copies it, or `H` highlights it. `:w` saves the highlights into the file.
- `:help` lists every key, and `q` closes the tab.

Put a count before a key to repeat it, as in vi: `3j` scrolls three steps, and `12G` goes to page 12. `Esc` cancels a key sequence you have half typed.

## Keys

These are the default keys. `:help` shows the keys bound right now, and `man pdvim` describes each one in full.

### Moving

| Key                                         | Action                                                |
| ------------------------------------------- | ----------------------------------------------------- |
| `j` / `k` / `h` / `l`, arrow keys           | Scroll down / up / left / right                       |
| `Ctrl-d` / `Ctrl-u`                         | Scroll half a screen down / up                        |
| `Ctrl-f`, `Space` / `Ctrl-b`, `Shift-Space` | Scroll a full screen down / up                        |
| `J`, `PageDown` / `K`, `PageUp`             | Go to the next / previous page                        |
| `gg`, `Home` / `G`, `End`                   | Go to the first / last page, or page _n_ with a count |
| `H` / `L`                                   | Go to the top / bottom of the page                    |
| `]]` / `[[`                                 | Go to the next / previous chapter                     |
| `Tab`                                       | Show the table of contents                            |
| `go`                                        | Show every page small, to pick one                    |
| `m{a-z}` / `m{A-Z}`                         | Mark this place in this document / for any document   |
| `` `{letter} ``, `'{letter}`                | Go to a mark                                          |
| `Ctrl-o` / `Ctrl-i`                         | Go back / forward through the jumps                   |

A jump is `gg`, `G`, a search match, a chapter, a followed link or a mark. `:marks` lists the marks, which are kept after pdvim closes.

The table of contents moves with `j` / `k`, and `l` / `h` open and close a chapter's sections. The page grid moves with `h` `j` `k` `l`. In both, `Enter` goes to the item selected, and `Esc` or `q` goes back. Going somewhere this way counts as a jump, so `Ctrl-o` brings you back.

### Viewing

| Key                   | Action                                     |
| --------------------- | ------------------------------------------ |
| `+`, `zi` / `-`, `zo` | Zoom in / out                              |
| `=`, `z0`             | Zoom to 100%, or to _n_% with a count      |
| `zw` / `zp`           | Fit the page's width / the whole page      |
| `r`                   | Rotate                                     |
| `R`                   | Turn recoloring on or off                  |
| `t`                   | Turn margin trimming on or off             |
| `zc`                  | Show two pages side by side, or one        |
| `zb`                  | Hide the tab and status bars, or show them |
| `F11`                 | Fill the screen, or stop                   |

### Searching and commands

| Key       | Action                                            |
| --------- | ------------------------------------------------- |
| `/` / `?` | Search forwards / backwards                       |
| `n` / `N` | Go to the next / previous match                   |
| `:`       | Open the command prompt                           |
| `.`       | Run the last command line again                   |
| `o`       | Show the documents you read recently              |
| `Ctrl-g`  | Show the document's title, author, dates and more |
| `Esc`     | Cancel, and hide the search highlights            |
| `q`       | Close the tab, and quit after the last one        |

Matches are highlighted while you type a search. At the prompt, `Tab` completes commands, settings and file paths, and `Up` / `Down` bring back earlier commands and searches. That history is kept after pdvim closes.

### Links and forms

| Key  | Action                                                  |
| ---- | ------------------------------------------------------- |
| `f`  | Label every link, form field and marking in view        |
| `F`  | Label the links, and open the one you pick in a new tab |
| `;y` | Label the links, and copy where the one you pick goes   |
| `gi` | Fill in the first form field in view                    |
| `g;` | Go back to the form field you were in last              |

Type a label to pick its link or field. Web and mail links open in your default program. Picking a text field lets you type into it on the page, and `Tab` goes on to the next field. Checkboxes tick and lists drop down. After any key that shows labels, `/` and some text narrows the labels to the items that hold it.

### Selecting text

`v` selects characters and `V` selects whole lines. Each line in view gets a label, and typing one starts the selection there. `s` starts it at the text you type instead, as [flash.nvim](https://github.com/folke/flash.nvim) does.

| Key in visual mode         | Action                                                     |
| -------------------------- | ---------------------------------------------------------- |
| `h` `j` `k` `l`            | Move the cursor a character or a line                      |
| `w` / `b` / `e`            | Move to the next word / the word before / the word's end   |
| `0` / `^` / `$`            | Move to the start / the first letter / the end of the line |
| `f` `F` `t` `T`, `;` / `,` | Move to a character on the line, and repeat that           |
| `ip` / `ap`                | Select the paragraph                                       |
| `o`                        | Move to the other end of the selection                     |
| `y`                        | Copy the selection                                         |
| `H` / `U` / `S` / `W`      | Highlight / underline / strike out / squiggle it           |
| `n`                        | Put a note beside the line                                 |
| `c` / `d`                  | Change the note on a marking / take markings away          |
| `Esc`                      | Leave without copying                                      |

The status bar always lists what you can do next.

### Highlights and notes

| Key              | Action                                                   |
| ---------------- | -------------------------------------------------------- |
| `]h` / `[h`      | Go to the next / previous marking                        |
| `gh`             | Label the markings in view, to pick one                  |
| `d` / `c` / `yh` | Take away / change the note on / copy the marking picked |
| `Shift-Tab`      | List every marking in a sidebar                          |
| `u` / `Ctrl-r`   | Undo / redo                                              |
| `Ctrl-s`, `:w`   | Save the markings into the file                          |

Nothing is written until you save. Until then the tab shows `[+]`, and pdvim asks before closing the document. `:w copy.pdf` saves a copy instead, and the `autosave` setting saves each change as you make it. `:note {text}` puts a sticky note at the top of the window.

### Copying

| Key  | Action                                                  |
| ---- | ------------------------------------------------------- |
| `yy` | Copy the path of the document's file                    |
| `yt` | Copy the document's title                               |
| `yp` | Copy the text of the page in view                       |
| `yi` | Label the pictures and figures, to copy one as an image |

### Tabs

| Key                                       | Action                                    |
| ----------------------------------------- | ----------------------------------------- |
| `gt`, `Ctrl-Tab` / `gT`, `Ctrl-Shift-Tab` | Go to the next / previous tab, or tab _n_ |
| `b`                                       | List the tabs, and type to find one       |
| `U`                                       | Open the tab you closed last again        |

### Modes

The box at the left of the status bar always names the mode. Modes of the same kind share a color:

| Color  | Modes                                                                                 |
| ------ | ------------------------------------------------------------------------------------- |
| Blue   | `NORMAL`: reading                                                                     |
| Mauve  | `VISUAL`, `V-LINE`, `SELECT`, `JUMP`: selecting text                                  |
| Green  | `INSERT`: filling in a form field                                                     |
| Peach  | `COMMAND`, `SEARCH`, `PASSWORD`, `CONFIRM`: typing or answering at the bottom         |
| Yellow | `FOLLOW`, `PICK`: labels on links, fields, markings and figures                       |
| Teal   | `CONTENTS`, `OVERVIEW`, `MARKINGS`, `HELP`, `RECENT`, `TABS`, `INFO`: a list or panel |

## Commands

| Command                                | Action                                                       |
| -------------------------------------- | ------------------------------------------------------------ |
| `:{n}`, `:goto {label}`                | Go to page _n_, or to the page numbered `xii` or `A-3`       |
| `:o[pen] {file}`, `:tabe[dit] {file}`  | Open a document in a new tab                                 |
| `:rec[ent]`                            | Show the documents you read recently                         |
| `:w[rite]`, `:w {file}`                | Save the markings, or save a copy with them                  |
| `:q[uit]`, `:qa[ll]`, `:wq`, `:q!`     | Close the tab or quit, saving first or throwing changes away |
| `:tabn[ext]`, `:tabp[revious]`         | Go to the next or previous tab                               |
| `:tabc[lose]`, `:tabo[nly]`            | Close this tab, or every other one                           |
| `:tabm[ove] [n\|+n\|-n]`               | Move the tab                                                 |
| `:note {text}`                         | Put a note in the margin at the top of the window            |
| `:marks`, `:delm[arks] {letters}`      | List marks, or take them away                                |
| `:info`                                | Show the title, author, dates, pages and encryption          |
| `:export[!] {file}`                    | Save the page in view as a `.png` image or an `.svg` drawing |
| `:!{command}`                          | Run a shell command; `%` stands for the document's file      |
| `:set {setting} {value}`               | Change a [setting](#settings)                                |
| `:theme {name}`                        | Change the theme, or show it without a name                  |
| `:map`, `:unmap`, `:mapclear`          | Change the [key bindings](#key-bindings)                     |
| `:so[urce] [file]`                     | Read the configuration file again                            |
| `:h[elp] [topic]`, `:apr[opos] {word}` | Show the keys: all of them, or those about a topic or word   |
| `:ve[rsion]`                           | Show pdvim's version                                         |

Any action can also be run by name, such as `:next-tab` or `:zoom-to 150`. Separate several commands with `;`. For example, `:!xdg-open %` opens the document in your default program, and `:!cp % ~/papers/` copies it.

## Books, comics and images

Besides PDF, pdvim opens EPUB and FB2 books, CBZ and CBT comics, XPS documents, SVG drawings, and PNG, JPEG, GIF, TIFF, BMP and other images.

A book has no pages of its own, so pdvim lays it out on pages the size of the window, in the type size `fontsize` sets. In a book, the zoom keys make the type bigger or smaller. When the window changes, the book is laid out again, and you stay at the same passage.

Search, copying, links, the table of contents and visual mode work in every format that has text. Highlights, notes, forms and `:w` work in PDFs only.

## Configuration

pdvim reads `~/.config/pdvim/config.toml` when it starts, or the same path under `$XDG_CONFIG_HOME` when that is set. You don't need the file. After you edit it, `:source` reads it again.

[`defaults.toml`](defaults.toml) lists every setting and every key binding of every mode, with comments. pdvim reads it before your file. Copy it to start from:

```sh
mkdir -p ~/.config/pdvim
cp defaults.toml ~/.config/pdvim/config.toml
```

A short file works too, since anything you leave out keeps its default:

```toml
[settings]
recolor = true
theme = "catppuccin-macchiato"
scroll-step = 60

[keys.normal]
"ctrl+n" = "next-tab"
"ctrl+p" = "previous-tab"
"ctrl+t" = ":theme catppuccin-latte"

[keys.visual]
"Y" = "yank"
```

If an entry has a mistake, pdvim skips it and shows the mistake in the status bar.

### Settings

`:set` takes the same names and values while pdvim runs. A boolean setting named without a value is turned on.

| Setting                                    | Default                                 | What it does                                                               |
| ------------------------------------------ | --------------------------------------- | -------------------------------------------------------------------------- |
| `recolor`                                  | `"auto"`                                | Recolor pages for a dark background: `true`, `false`, or follow the system |
| `trim`                                     | `false`                                 | Hide the blank margins of pages                                            |
| `theme`                                    | `"auto"`                                | The theme, or `"auto"` to follow the system                                |
| `theme-dark` / `theme-light`               | `catppuccin-mocha` / `catppuccin-latte` | The themes `"auto"` uses                                                   |
| `recolor-lightcolor` / `recolor-darkcolor` | `"theme"`                               | The colors paper and ink become when recolored, as `#rrggbb`               |
| `scroll-step`                              | `40`                                    | How far `j` and `k` scroll, in pixels                                      |
| `fontsize`                                 | `12`                                    | The type size of books, in points                                          |
| `columns`                                  | `1`                                     | How many pages sit side by side, 1 to 4                                    |
| `autosave`                                 | `false`                                 | Save each marking into the file as soon as it is made                      |
| `confirm-quit` / `confirm-tab-close`       | `true`                                  | Ask before quitting / before closing a tab                                 |
| `hintchars`                                | `"asdfghjkl"`                           | The letters labels are made of                                             |
| `incsearch`                                | `true`                                  | Highlight matches while a search is typed                                  |
| `tabbar` / `statusbar`                     | `true`                                  | Show the tab bar / the status bar                                          |
| `highlight-color`, `underline-color`       | `#ffff00`, `#1e88e5`                    | The colors of highlights and underlines                                    |
| `strikeout-color`, `squiggly-color`        | `#e53935`, `#fb8c00`                    | The colors of strike-outs and squiggly underlines                          |
| `note-color`                               | `#fdd835`                               | The color of a note's icon                                                 |

The themes are `catppuccin-latte`, `catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`, `pdvim-dark` and `pdvim-light`. On Linux, pdvim reads the system's light or dark preference from the desktop portal.

### Settings by path

`[[paths]]` tables give some documents settings of their own. Each one has a pattern under `match`, and any of `recolor`, `trim` and `columns`:

```toml
# Scans stay as printed, and trimmed.
[[paths]]
match = "~/scans/**"
recolor = false
trim = true

# Comics two pages at a time, wherever they are.
[[paths]]
match = "*.cbz"
columns = 2
```

`*` matches any characters except `/`, `?` matches one character, and `**` matches anything, however deep. A pattern without a `/` is matched against the file's name alone. When several rules match, the last one wins.

### Key bindings

Each `[keys.<mode>]` table binds keys to that mode's actions, on top of the defaults. Bind keys to `"none"` to remove a default.

- **Keys** are written as a character (`j`, `G`, `?`) or a name (`space`, `tab`, `escape`, `enter`, `pageup`, `f1`, and so on), with `ctrl+` or `shift+` in front for modifiers: `ctrl+d`, `shift+tab`.
- **Sequences** are keys separated by spaces, as in `"g g"`.
- **Command lines**, written after a `:`, can be bound to keys in normal mode, as in `":set recolor false; zoom-to 150"`.

The modes are `normal`, `visual`, `hints`, `overview`, `outline`, `markings`, `recent`, `help`, `prompt` and `confirm`. `defaults.toml` and `:help` list the actions of each. While pdvim runs, `:map visual Y yank`, `:unmap outline q` and `:mapclear prompt` do the same.

## Files

| What            | Linux                                 | macOS                  | Windows                |
| --------------- | ------------------------------------- | ---------------------- | ---------------------- |
| Configuration   | `~/.config/pdvim/config.toml`         | `~/.config/pdvim`      | `%APPDATA%\pdvim`      |
| Reading history | `~/.local/share/pdvim/history.sqlite` | `~/.local/share/pdvim` | `%LOCALAPPDATA%\pdvim` |

The reading history keeps each document's page, zoom, marks and other state, and your command history. On Linux, `$XDG_CONFIG_HOME` and `$XDG_DATA_HOME` move these files when they are set.

The Flatpak reads the same configuration, but keeps its history in `~/.var/app/io.github.djmbritt.pdvim`. It can open files in your home folder, `/media`, `/run/media` and `/mnt`.

## Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets for every part of pdvim that reads input from outside: documents, the configuration file, commands and keys. GitHub Actions fuzzes each target after every push to `main`, and for longer once a week. See [fuzz/README.md](fuzz/README.md) for how to run them and the coverage they reach.

```sh
cargo install cargo-fuzz
fuzz/run.sh pdf 600     # fuzz PDF loading for ten minutes
```

## License

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html)
