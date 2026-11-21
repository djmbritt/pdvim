# pdvim

A minimal, keyboard-driven PDF reader with vi key bindings.

pdvim uses [MuPDF](https://mupdf.com/) to render pages and [iced](https://iced.rs/) to draw the window. Pages are laid out in one continuous column. It also remembers where you stopped reading each document.

![pdvim showing the first page of Alice's Adventures in Wonderland, with two tabs open](docs/screenshots/reading.png)

## Features

- vi-style movement with counts (`12j`, `5J`, `42G`)
- Search forwards and backwards with `/` and `?`, then jump between matches with `n` and `N`
- Zoom, fit to width, fit to page, and rotate
- Recoloring for dark backgrounds, with colors you choose (like Zathura's `recolor`), following the system's light or dark theme by default
- Themes, including the four [Catppuccin](https://catppuccin.com) flavors, for the bars, lists and recolored pages
- Trimming of blank page margins
- Per-document memory of page, zoom, fit, and rotation, stored in SQLite
- A list of recently read documents to reopen from
- A configuration file for settings and key bindings
- Selecting and copying text from the keyboard, with a visual mode like vim's
- A sidebar with the table of contents, to go to a chapter
- Marks (`ma`, `` `a ``, and `A`–`Z` across documents) and a jump list (`Ctrl-o`, `Ctrl-i`), as in vim
- Following links from the keyboard: `f` labels every link, form field, highlight and note in view, and typing a label follows the link, fills in the field, or picks the marking
- Filling in PDF forms: text fields and lists at the prompt, and boxes ticked with a label
- A cheat sheet of the key bindings in force, with `:help`
- `Tab` completion of commands, settings, and file paths in the command prompt, and `Up` / `Down` to recall earlier commands and searches
- Tabs, with vim's `gt` / `gT`, `Ctrl-Tab` / `Ctrl-Shift-Tab`, and `:tabedit`
- Highlighting, underlining, striking out and squiggling text into the PDF, saved with `:w` or `Ctrl-s`, with undo
- Going through the markings with `]h` / `[h`, and listing them in a sidebar
- Chapter by chapter with `]]` / `[[`, and the printed page numbers (`xii`, `A-3`) in the status bar and `:goto`
- Password-protected PDFs, and `:info` for the title, author and dates

## Screenshots

The table of contents, in the Catppuccin Mocha theme, with the page recolored to match:

![The table of contents sidebar over a recolored page of the Ghostscript manual](docs/screenshots/contents.png)

Visual mode starts from a line picked by its label, then moves like vim:

![Two-letter labels on each line of text, to start selecting from](docs/screenshots/labels.png)

A highlight, an underline, a squiggly underline, and a selection in progress:

![Lines highlighted, underlined and squiggled, with a selection in progress](docs/screenshots/visual.png)

`:help` lists every key binding in force:

![The cheat sheet of key bindings](docs/screenshots/help.png)

## Installing

Each [release](https://github.com/djmbritt/pdvim/releases) has packages for:

| Platform             | Download                                                                                                                    |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Linux, x86-64        | `pdvim_VERSION-1_amd64.deb`, or `pdvim-VERSION-x86_64-unknown-linux-gnu.tar.gz`                                             |
| macOS, Apple silicon | `pdvim-VERSION-aarch64-apple-darwin.dmg`, or the `.tar.gz` for the terminal                                                 |
| macOS, Intel         | `pdvim-VERSION-x86_64-apple-darwin.dmg`, or the `.tar.gz`                                                                   |
| Windows, x86-64      | `pdvim-VERSION-x86_64-pc-windows-msvc.zip`                                                                                  |
| Arch Linux           | `yay -S pdvim` or `paru -S pdvim` from the AUR, or `makepkg -si` in [`packaging/`](packaging/) to build what is checked out |

On macOS and Windows pdvim keeps its configuration in `~/.config/pdvim` and `%APPDATA%\pdvim`, and its reading history in `~/.local/share/pdvim` and `%LOCALAPPDATA%\pdvim`.

## Building

pdvim needs Rust 2024 edition (Rust 1.85 or newer). The `mupdf` crate builds MuPDF from source, so you also need a C toolchain and `libclang`.

```sh
cargo build --release
```

The binary is written to `target/release/pdvim`.

## Usage

```sh
pdvim [FILE]...
pdvim --version
```

Each file opens in a tab of its own. If you start pdvim without a file, it shows the documents you read most recently. Pick one, or open another file with `:open`.

`:open` and the recent list put each document in a new tab, unless the tab in view is empty. A document that is already open in a tab is brought into view instead.

## Keys

Put a count before a key to repeat it, as in vi. For example, `3j` scrolls three steps down and `12G` goes to page 12.

### Movement

| Key                                         | Action                                                          |
| ------------------------------------------- | --------------------------------------------------------------- |
| `j` / `k` / `h` / `l`, arrow keys           | Scroll down / up / left / right                                 |
| `Ctrl-d` / `Ctrl-u`                         | Scroll half a screen down / up                                  |
| `Ctrl-f`, `Space` / `Ctrl-b`, `Shift-Space` | Scroll a full screen down / up                                  |
| `J`, `PageDown` / `K`, `PageUp`             | Go to the next / previous page                                  |
| `gg`, `Home` / `G`, `End`                   | Go to the first / last page                                     |
| `{n}G`, `{n}gg`                             | Go to page _n_                                                  |
| `H` / `L`                                   | Go to the top / bottom of the current page                      |
| `Tab`                                       | Show the table of contents                                      |
| `]]` / `[[`                                 | Go to the next / previous chapter or section; `{n}]]` goes n on |

The mouse wheel also scrolls.

`Tab` opens the table of contents in a sidebar, with the chapter you're reading selected. Move with `j` / `k`, `gg` / `G`, and `Ctrl-d` / `Ctrl-u`. `l` opens a chapter to show its sections, or steps into them, and `h` closes it or goes up to the chapter it's in. `Space` opens and closes. `Enter` goes to the chapter and closes the sidebar, and `Esc`, `q`, or `Tab` close it without going anywhere.

### View

| Key                   | Action                           |
| --------------------- | -------------------------------- |
| `+`, `zi` / `-`, `zo` | Zoom in / out                    |
| `=`, `z0`             | Zoom to 100%                     |
| `{n}=`, `{n}zz`       | Zoom to _n_%                     |
| `zw`                  | Fit the page width to the window |
| `zp`                  | Fit the whole page in the window |
| `r`                   | Rotate                           |
| `R`                   | Turn recoloring on or off        |
| `t`                   | Turn margin trimming on or off   |

### Links

| Key | Action                   |
| --- | ------------------------ |
| `f` | Label every link, form field, highlight and note in view |

After `f`, each link in view is highlighted and gets a label of two letters. Type a label to follow its link. A link to another place in the document goes there. A web or mail link (`http`, `https`, `mailto`) opens in your default program; links to anything else, such as files, are not followed. Highlights, notes and the other markings get labels too, just left of them; typing one picks that marking as `]h` would, so its note shows and `d` and `c` work on it. `Backspace` takes back the first letter, and `Esc` puts the labels away.

The fields of a PDF form get labels too, so `f` is also how you fill one in. Picking a checkbox ticks or unticks it, and picking a radio button picks it. Picking a text field opens the prompt after the field's name, with what the field holds, to change and `Enter`; `Ctrl-u` clears it. For a list or a drop-down, `Tab` offers the values it takes that start as what you've typed, or that contain it, and goes through them. Read-only fields, buttons and signatures get no label. The form's scripts are not run. Filling in a field is a change like a highlight: `u` takes it back, `:w` saves it into the file, and pdvim asks before quitting with it unsaved.

### Marks and jumps

| Key                          | Action                                                               |
| ---------------------------- | -------------------------------------------------------------------- |
| `m{a-z}`                     | Mark this place in the document                                      |
| `m{A-Z}`                     | Mark this place, to come back to from any document                   |
| `` `{letter} ``, `'{letter}` | Go to a mark, opening its document for `A`–`Z`                       |
| ` `` `, `''`                 | Go back to where the last jump left from                             |
| `Ctrl-o` / `Ctrl-i`          | Go back through the jumps, or forward again; `{n}Ctrl-o` goes n back |

A jump is `gg`, `G`, `{n}G`, `:{n}`, a search match, a chapter from the table of contents, a followed link, or a mark. Marks are kept in the reading history, so they last after pdvim closes; the jump list lasts while the document is open. `:marks` lists the marks, and `:delm[arks] {letters}` or `:delmarks !` takes them away.

### Selecting text

| Key       | Action                                  |
| --------- | --------------------------------------- |
| `v` / `V` | Select characters / whole lines of text |
| `s`       | Jump to text you type, and select from there |

After `v` or `V`, each line of text in view gets a label of two letters, and so does each sentence that starts partway along a line. Type a label to put the cursor there, and start selecting from there. The status bar then lists what you can do next, each with its key; `d` and `c` show there when there is a marking to take away or change.

`s` jumps by what you type, as [flash.nvim](https://github.com/folke/flash.nvim) does. Type some of the text in view: each match is highlighted and gets a label just after it, the nearest first. Keep typing to narrow the matches, or type a label to start selecting there; `Enter` goes to the nearest match, `Backspace` takes back a letter, and `Esc` gives up. Labels never start with a letter that could carry on what you typed, so a letter is always one or the other. While there are few matches the labels are one letter, and two when there are more. Lowercase matches either case; a capital makes the case count. In visual mode, `s` moves the cursor the same way, and the selection stretches to it.

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
| `s`                       | Jump the cursor to text you type, by its label             |
| `o`                       | Move the cursor to the other end of the selection          |
| `v` / `V`                 | Switch to selecting characters / lines, or leave           |
| `y`                       | Copy the selection to the clipboard, and leave             |
| `H`                       | Highlight the selection in the PDF file, and leave         |
| `U`                       | Underline the selection, and leave                         |
| `S`                       | Strike out the selection, and leave                        |
| `W`                       | Underline the selection with a squiggle, and leave         |
| `n`                       | Put a note beside the cursor's line, typed at the prompt   |
| `c`                       | Change the note on the marking at the cursor, and leave    |
| `d`                       | Take away the markings under the selection, and leave      |
| `Esc`, `Ctrl-c`           | Leave without copying                                      |

`H`, `U`, `S` and `W` add the marking to the document as a standard PDF annotation (a highlight, underline, strike-out or squiggly underline), which other PDF readers show too once it is saved. Each has its color setting: `highlight-color`, `underline-color`, `strikeout-color` and `squiggly-color`. Nothing is written until you save with `:w`; until then the tab, the status bar and the window title show `[+]`, and closing the document asks first. `:w` saves into the file incrementally: everything that was in it when pdvim opened it stays byte for byte as it was, and the changes are written after it. `:w copy.pdf` writes the whole document with its highlights to a new file instead, and leaves the original alone. Set `autosave` to save every change into the file as it is made. Many downloaded PDFs are slightly damaged, and MuPDF repairs them as it opens them; such a file can't be added to, so `:w` writes it whole, beside the original, and then puts it in the original's place. That repairs the file too.

`n` opens the prompt as `:note `; type the note and press `Enter`. The note is a sticky-note annotation, which shows as an icon over the first letter of the cursor's sentence, and which other PDF readers open to show its text. The icon lets the text under it show through, and stays inside the text, so trimming the margins with `t` keeps it in view; notes and highlights never change how much `t` trims. `:note {text}` in normal mode puts one in the margin at the top of the window. Notes count as markings: `]h` / `[h` go to them and show their text, the markings sidebar lists them, `d` takes one away, `u` takes one back, and `:w` saves them. To change a note, go to it with `]h` or `[h`, or pick it with `gh`, and press `c`, or press `c` on it in the sidebar: the prompt opens with the note as it is, to change and `Enter`. `u` changes it back. In visual mode, a band over the bottom of the pages shows the note on the marking at the cursor, or on a note beside its line, and `c` changes it. `c` on a highlight or another marking sets the comment on it, which other PDF readers show with it.

Press `u` to take back the last marking, or the last taking away of one, and `Ctrl-r` to put back what `u` took back. Taking back everything since the last save leaves nothing to save. pdvim remembers what to undo for each open document until you close it.

A count before a motion repeats it, as in `3w` or `2fe`. As in vim, `f`, `F`, `t`, and `T` look along the cursor's line only, and the cursor stays put if the character isn't there. These are the default keys; the `[keys.visual]` table of the [configuration](#configuration) changes them.

### Highlights and other markings

| Key         | Action                                                     |
| ----------- | ---------------------------------------------------------- |
| `]h` / `[h` | Go to the next / previous marking; `{n}]h` goes n on       |
| `gh`        | Label the markings in view; type a label to pick one       |
| `d`         | Take away the marking `]h`, `[h` or `gh` picked            |
| `c`         | Change the note on the marking `]h`, `[h` or `gh` picked   |
| `y`         | Copy the picked marking's text, or a note's text           |
| `Shift-Tab` | List the markings in a sidebar, with their text and page   |
| `u`         | Take back the last marking, or the last taking away of one |
| `Ctrl-r`    | Put back what `u` took back                                |
| `Ctrl-s`    | Save the markings into the file, like `:w`                 |

These go through every highlight, underline, strike-out, squiggly underline and note in the document, whether pdvim or another program made it. `]h` starts from the marking it last went to while that is in view, and from the top of the view otherwise; the marking it goes to gets a frame, which `Esc` hides. `gh` puts a label of two letters beside each marking in view, as `f` does for links; typing a label picks that marking, so that `d`, `c` and `y` work on it. In the sidebar, `j` and `k` move, `Enter` goes to the marking selected, `d` takes it away, `c` changes its note, and `q`, `Esc` or `Shift-Tab` close the list. In visual mode, `d` takes away the markings under the selection. Taking a marking away is a change like any other: `:w` saves it, and `u` puts the marking back.

### Tabs

| Key                                       | Action                        |
| ----------------------------------------- | ----------------------------- |
| `gt`, `Ctrl-Tab` / `gT`, `Ctrl-Shift-Tab` | Go to the next / previous tab |
| `{n}gt`, `{n}Ctrl-Tab`                    | Go to tab _n_                 |

A password-protected PDF asks for its password in the status bar when it opens, showing a dot for each character typed. A wrong password asks again; `Esc` gives up and closes its tab. pdvim keeps the password only while the document is open, and a locked PDF stays locked with the same password when you save markings into it or save a copy with `:w copy.pdf`.

When a book numbers its pages differently from their place in the file, as with roman numerals for the front matter, the status bar shows the printed number first: `xii (14/300)`. `:{n}` goes to the page at place _n_, and `:goto {label}` to the page with that number printed on it; a number printed on no page is taken as a place.

A tab, and the window while its tab is in view, goes by the document's title when the PDF gives one, and by the file name otherwise. Titles that are only the name of the file the PDF was made from, such as `Microsoft Word - report.docx` or `untitled`, are passed over.

### Search and prompt

| Key                       | Action                                                 |
| ------------------------- | ------------------------------------------------------ |
| `/` / `?`                 | Search forwards / backwards                            |
| `n` / `N`                 | Go to the next / previous match                        |
| `:`                       | Open the command prompt                                |
| `o`                       | Show recent documents                                  |
| `Esc`, `Ctrl-c`, `Ctrl-[` | Cancel a half-typed command and hide search highlights |
| `q`                       | Close the tab, and quit after the last one             |
| `Ctrl-g`                  | Show the document's title, author, dates and more      |

## Commands

| Command                                | Action                                                               |
| -------------------------------------- | -------------------------------------------------------------------- |
| `:{n}`                                 | Go to page _n_                                                       |
| `:goto {label}`                        | Go to the page with that number printed on it, as `xii` or `A-3`     |
| `:o[pen] {file}`                       | Open a document in a new tab (`~` expands to your home directory)    |
| `:rec[ent]`                            | Show recent documents                                                |
| `:h[elp]`                              | Show every action and the keys bound to it                           |
| `:marks`                               | List the marks of the document in view, and `A`–`Z`                  |
| `:delm[arks] {letters}` or `!`         | Take marks away: those letters, or with `!` all of this document's   |
| `:ve[rsion]`                           | Show pdvim's version and where to find it on GitHub                  |
| `:info`, `:f[ile]`                     | Show the document's title, author, dates, pages, format, encryption  |
| `:note {text}`                         | Put a note with that text in the margin at the top of the window     |
| `:noh[lsearch]`                        | Hide search highlights                                               |
| `:set recolor [true\|false\|auto]`     | Turn recoloring on or off, or follow the system's theme (default)    |
| `:set trim [true\|false]`              | Turn margin trimming on or off                                       |
| `:set recolor-lightcolor #rrggbb`      | Set the color white paper becomes when recolored (default: theme's)  |
| `:set recolor-darkcolor #rrggbb`       | Set the color black ink becomes when recolored (default: theme's)    |
| `:theme {name}\|auto`                  | Set the theme, or follow the system's (default); or `:colo[rscheme]` |
| `:theme`                               | Show the theme in force                                              |
| `:set theme {name}\|auto`              | The same as `:theme`                                                 |
| `:set theme-dark {name}`               | Set the theme followed while the system is dark                      |
| `:set theme-light {name}`              | Set the theme followed while the system is light                     |
| `:set scroll-step {pixels}`            | Set how far one `j` or `k` scrolls                                   |
| `:set confirm-quit [true\|false]`      | Ask before quitting (default on)                                     |
| `:set confirm-tab-close [true\|false]` | Ask before closing a tab while others are open (default on)          |
| `:set highlight-color #rrggbb`         | Set the color `H` in visual mode highlights in (default `#ffff00`)   |
| `:set underline-color #rrggbb`         | Set the color of `U`'s underline (default `#1e88e5`)                 |
| `:set strikeout-color #rrggbb`         | Set the color of `S`'s line through the text (default `#e53935`)     |
| `:set squiggly-color #rrggbb`          | Set the color of `W`'s squiggly underline (default `#fb8c00`)        |
| `:set note-color #rrggbb`              | Set the color of a note's icon (default `#fdd835`)                   |
| `:map [mode] {keys} {action}`          | Bind keys to an action (see [Keys](#keys-1))                         |
| `:unmap [mode] {keys}`                 | Remove a key binding                                                 |
| `:mapclear [mode]`                     | Remove every key binding of a mode                                   |
| `:tabe[dit] {file}`, `:tabnew [file]`  | Open a document in a new tab, or open an empty tab                   |
| `:tabn[ext] [n]`                       | Go to the next tab, or to tab _n_                                    |
| `:tabp[revious]`                       | Go to the previous tab                                               |
| `:tabc[lose]`                          | Close the tab, unless it is the last                                 |
| `:q[uit]`                              | Close the tab, and quit after the last one                           |
| `:qa[ll]`                              | Quit                                                                 |
| `:q!`, `:qa!`                          | Close the tab, or quit, throwing away highlights not saved           |
| `:w[rite]`                             | Save the highlights into the file                                    |
| `:w {file}`, `:w! {file}`              | Save a copy with the highlights; `!` overwrites a file that exists   |
| `:wq`, `:x`                            | Save, then close the tab                                             |
| `:wa[ll]`                              | Save every document with highlights not saved                        |
| `:wqa[ll]`, `:xa[ll]`                  | Save every document, then quit                                       |

If you name a boolean setting without a value, it is turned on.

`:help` shows a cheat sheet of every action in every mode, the keys bound to it, and what it does. It shows your own bindings, including changes from the configuration file and `:map`. Scroll it with `j` / `k`, `Ctrl-d` / `Ctrl-u`, and `g` / `G`, and close it with `q` or `Esc`. To open it with a key, bind the `help` action, as in `"ctrl+h" = "help"`.

You can also run any [action](#actions) by name, as in `:scroll-down` or `:next-tab`. A number after it works like a count typed before a key: `:zoom-to 150` zooms to 150%, and `:first-page 12` goes to page 12.

Press `Tab` in the command prompt to complete what you're typing: command and action names, setting names and their values, mode and action names after `:map`, and file paths after `:open`, `:tabedit`, and `:tabnew`. Paths offer folders and PDF files. If more than one completion fits, `Tab` fills in as much as they share and lists them above the prompt. Press `Tab` again to go through the list, and `Shift-Tab` to go back. While the list is open, it narrows as you type, and it closes when nothing fits.

Press `Up` or `Ctrl-p` at `:`, `/` or `?` to bring back an earlier command or search, and again to go further back; `Down` or `Ctrl-n` comes forward again, back to what you were typing. Whatever you have typed already narrows them to the lines that start the same way, so `:zo` then `Up` finds your last `:zoom`. Commands and searches are kept apart, `/` and `?` share theirs, and the last 500 of each are kept in the reading history, so they last after pdvim closes. Passwords are never kept.

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
# Always Catppuccin Macchiato, whatever the system's theme.
theme = "catppuccin-macchiato"

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

`[settings]` takes the same names and values as `:set`. With `confirm-quit` on, as it is by default, pdvim asks `quit pdvim? (y/n)` before quitting: when you close the last tab, use `:qa`, or close the window. Press `y` or `Enter` to quit, or `Esc` or any other key to stay. If the window is closed again while pdvim is asking, for example by a window manager that insists, it quits without asking. With `confirm-tab-close` on, also the default, pdvim asks `close tab {name}? (y/n)` before closing a tab with `q`, `:q`, or `:tabclose` while other tabs are open. Closing the last tab quits pdvim, so `confirm-quit` decides whether that asks. Whatever these say, pdvim asks before closing a document with highlights not saved, unless `autosave` is on. `recolor` is `"auto"` by default: pdvim recolors every document while the system prefers a dark theme, shows them as printed while it prefers a light one, and changes as soon as the system does. pdvim reads the preference from the desktop's color-scheme setting (`org.freedesktop.appearance` through the desktop portal on Linux). `R` still turns recoloring on or off for the document in view, until the system's theme changes again. With `recolor` set to `true` or `false`, it and `trim` decide how a document starts the first time you open it. After that, the document is restored the way you left it.

`theme` sets the colors of the tab bar, the status bar, the lists and the backdrop behind the pages, and the colors pages are recolored in unless `recolor-lightcolor` and `recolor-darkcolor` set their own. The themes are `catppuccin-latte` (light), `catppuccin-frappe`, `catppuccin-macchiato`, `catppuccin-mocha`, `pdvim-dark` and `pdvim-light`. With `theme = "auto"`, the default, pdvim uses `theme-dark` (`catppuccin-mocha`) while the system prefers a dark theme and `theme-light` (`catppuccin-latte`) while it prefers a light one, whatever `recolor` is set to.

`[keys.<mode>]` binds keys to that mode's actions, on top of the default keys. Bind keys to `"none"` to remove a default. Keys put straight in `[keys]` are bound in normal mode, as before modes could be configured. The modes are:

| Mode       | When                                                          |
| ---------- | ------------------------------------------------------------- |
| `normal`   | Reading: the [actions](#actions) below                        |
| `visual`   | Selecting text after `v` or `V`                               |
| `hints`    | Labels on links or lines; letters not bound type a label      |
| `outline`  | The table of contents                                         |
| `markings` | The list of highlights and other markings                     |
| `recent`   | The recent-documents list; digits not bound open that entry   |
| `help`     | The cheat sheet                                               |
| `prompt`   | Typing after `:`, `/` or `?`; keys not bound type themselves  |
| `confirm`  | Questions such as `quit pdvim? (y/n)`; keys not bound mean no |

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
| `fit-width` / `fit-page`              | `z w` / `z p`                               |
| `rotate`                              | `r`                                         |
| `toggle-recolor`                      | `R`                                         |
| `toggle-trim`                         | `t`                                         |
| `search-forward` / `search-backward`  | `/` / `?`                                   |
| `search-next` / `search-previous`     | `n` / `N`                                   |
| `command-prompt`                      | `:`                                         |
| `recent`                              | `o`                                         |
| `next-tab` / `previous-tab`           | `g t`, `ctrl+tab` / `g T`, `ctrl+shift+tab` |
| `abort`                               | `escape`, `ctrl+c`, `ctrl+[`                |
| `info`                                | `ctrl+g`                                    |
| `quit`                                | `q`                                         |
| `help`                                | none                                        |
| `follow-link`                         | `f`                                         |
| `visual` / `visual-line`              | `v` / `V`                                   |
| `flash`                               | `s`                                         |
| `undo` / `redo`                       | `u` / `ctrl+r`                              |
| `write`                               | `ctrl+s`                                    |
| `next-marking` / `previous-marking`   | `] h` / `[ h`                               |
| `delete-marking`                      | `d h`                                       |
| `markings`                            | `shift+tab`                                 |
| `outline`                             | `tab`                                       |
| `next-chapter` / `previous-chapter`   | `] ]` / `[ [`                               |
| `set-mark` / `go-to-mark`             | `m` / `` ` ``, `'`                          |
| `jump-back` / `jump-forward`          | `ctrl+o` / `ctrl+i`                         |

With a count, `first-page` and `last-page` go to that page, `zoom-to` zooms to that percentage, and `next-tab` goes to that tab.

## Reading history

pdvim saves where you left each document in `$XDG_DATA_HOME/pdvim/history.sqlite`. If `XDG_DATA_HOME` is not set, it uses `~/.local/share/pdvim/history.sqlite`. Entries are keyed by file path.

## Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets for the parts of pdvim that read input it does not control: PDF files, the configuration file, `:` commands, key names and key presses. See [fuzz/README.md](fuzz/README.md).

```sh
cargo install cargo-fuzz
fuzz/run.sh pdf 600     # fuzz PDF loading for ten minutes
```

## License

[AGPL-3.0-or-later](https://www.gnu.org/licenses/agpl-3.0.html)
