# pdvim compared with Okular, zathura and Evince

Measured on 23 November 2026.

## Summary

pdvim opens documents within a tenth of a second of the quickest of the four, with the least work and the smallest install. Okular was ready first on every document, but on the 1,908-page file pdvim was only 0.02 s behind it (0.39 s against 0.37 s), and zathura and Evince were slower.

pdvim used the least processor time to open every document, about half of what the others used, and the least memory on the two longest documents (99 MB on the 1,908-page file, against 120–155 MB). Its install pulls in 19 system packages, against 225–447 for the others.

Its one clear weakness is what it maps at run time: 503 MiB, most of it graphics drivers that its renderer, wgpu, loads and never uses. That is the first thing to fix.

## What was compared, and how

Each reader opened each of four documents 7 times on one laptop (AMD Ryzen 7 7840U, 60 GiB, Arch Linux, Hyprland 0.56); every figure here is the median of those 7 runs.

| Reader  | Version               | Toolkit                    | PDF engine                      |
| ------- | --------------------- | -------------------------- | ------------------------------- |
| pdvim   | 0.2.13, release build | iced 0.14, drawn with wgpu | MuPDF, built in                 |
| zathura | 2026.10.4             | GTK 4 and girara           | MuPDF, through its plugin       |
| Okular  | 26.08.2               | Qt 6 and KDE Frameworks 6  | Poppler, and 13 other back-ends |
| Evince  | 48.4                  | GTK 3                      | Poppler                         |

The documents were public ones installed with the system:

- A one-page test document, from pdvim's own fixtures (`fuzz/seeds/pdf/text.pdf`).
- The Ghostscript manual: 477 pages of text.
- Ghostscript's colour-management guide: 42 pages with many pictures.
- Four copies of the manual joined into one file: 1,908 pages.

Each run started the reader in a 1000 × 750 window on a workspace not in view, with empty settings, data and cache folders. A script sampled the processor time of the reader and its processes every 20 ms, and read their memory once they had been idle for a second.

## Opening a document

Okular was ready first on every document. pdvim was second on the 1,908-page file and third on the others, behind Evince, 0.02–0.11 s after Okular; zathura was last on every document.

Seconds until the document is open and drawn:

| Document           | pdvim | zathura |   Okular | Evince |
| ------------------ | ----: | ------: | -------: | -----: |
| 1 page             |  0.36 |    0.47 | **0.25** |   0.29 |
| 477 pages          |  0.39 |    0.49 | **0.34** |   0.36 |
| 42 pages, pictures |  0.38 |    0.47 | **0.28** |   0.35 |
| 1,908 pages        |  0.39 |    0.63 | **0.37** |   0.48 |

"Open and drawn" is the moment the reader had done all its work of opening but the last 20 ms. pdvim's window appears later than Okular's and Evince's (0.36–0.39 s against 0.22–0.29 s), but pdvim is then ready almost at once, while they keep working after their windows are up.

Seconds of processor time used to open, over all threads:

| Document           |    pdvim | zathura | Okular | Evince |
| ------------------ | -------: | ------: | -----: | -----: |
| 1 page             | **0.16** |    0.30 |   0.28 |   0.32 |
| 477 pages          | **0.19** |    0.34 |   0.38 |   0.44 |
| 42 pages, pictures | **0.18** |    0.31 |   0.30 |   0.41 |
| 1,908 pages        | **0.26** |    0.46 |   0.40 |   0.55 |

The work grows least for pdvim as the document grows: from 0.16 s for one page to 0.26 s for 1,908 pages.

## Memory in use

pdvim used the least memory on the 477-page and 1,908-page documents; Evince used the least on one page, and Okular on the 42-page document with pictures.

Megabytes in use (PSS) once the document is open:

| Document           |  pdvim | zathura | Okular | Evince |
| ------------------ | -----: | ------: | -----: | -----: |
| 1 page             |     74 |      87 |     73 | **49** |
| 477 pages          | **90** |     105 |    111 |    116 |
| 42 pages, pictures |     88 |      96 | **77** |    109 |
| 1,908 pages        | **99** |     120 |    155 |    140 |

Memory is the proportional set size (PSS) of the reader and any processes it started: its own memory plus its share of libraries it shares with other programs. From one page to 1,908, pdvim grew by 25 MB, Okular by 82 MB and Evince by 91 MB.

## Going to the last page

pdvim and zathura went to the last page of the 1,908-page file with about 0.02 s of processor time; Evince used 0.39 s. That is the work done, not the wait: under 20 ms is too quick to time apart from noise, so both pdvim and zathura were in effect instant.

Okular was not measured: it ignores keys sent to a window without focus, and giving it focus would have taken the desktop's focus from the person at it.

## Install and load footprint

pdvim installs smallest by far, but maps the most at run time, because its renderer loads every graphics driver on the machine.

| Reader  | Packages, with dependencies | Installed, MiB | Files mapped when running | Mapped, MiB |
| ------- | --------------------------: | -------------: | ------------------------: | ----------: |
| pdvim   |                          19 |            187 |                        75 |         503 |
| zathura |                         225 |          1,102 |                       193 |         398 |
| Okular  |                         447 |          2,101 |                       238 |         192 |
| Evince  |                         258 |          1,072 |                       167 |         118 |

Installed sizes are pacman's, for the reader and everything it depends on; many of those packages are shared with the rest of the desktop. pdvim's is its 39.8 MiB binary, with MuPDF built in, plus the 19 system packages it links to; drivers it loads at run time are not counted. Mapped is every file in the running reader's memory map with the 477-page manual open, at its size on disk.

Of pdvim's 503 MiB, most is graphics drivers. wgpu loads every Vulkan driver installed while it picks a graphics card: here LLVM (165 MiB), NVIDIA's compiler and core libraries (154 MiB, on an AMD laptop), Mesa's Gallium (53 MiB), and the AMD and software Vulkan drivers. Little of that becomes resident, which is why pdvim's memory in use stays lowest.

## Features

pdvim is the only one of the four with vi keys, link labels, keyboard selection, saved annotations, forms and tabs together; it cannot print.

| Feature                                 | pdvim                                      | zathura                                 | Okular                                                                                      | Evince                                        |
| --------------------------------------- | ------------------------------------------ | --------------------------------------- | ------------------------------------------------------------------------------------------- | --------------------------------------------- |
| Formats, as installed here              | PDF, EPUB, FB2, CBZ, CBT, XPS, SVG, images | PDF, EPUB, FB2, MOBI, OXPS, SVG, images | PDF, PostScript, DjVu, DVI, EPUB, FB2, MOBI, comics, XPS, TIFF, fax, Markdown, text, images | PDF, PostScript, DjVu, DVI, comics, XPS, TIFF |
| Keys like vi, with counts and marks     | Yes                                        | Yes                                     | No                                                                                          | No                                            |
| Follow a link by typing its label       | Yes                                        | Yes                                     | No                                                                                          | No                                            |
| Select text from the keyboard           | Yes, visual mode                           | No                                      | No                                                                                          | Yes, caret mode (F7)                          |
| Highlights and notes saved into the PDF | Yes                                        | No                                      | Yes                                                                                         | Yes                                           |
| Fill in forms                           | Yes                                        | No                                      | Yes                                                                                         | Yes                                           |
| Tabs                                    | Yes                                        | No                                      | Yes                                                                                         | No                                            |
| Pages recoloured for a dark theme       | Yes                                        | Yes                                     | Yes                                                                                         | Yes                                           |
| Table of contents                       | Yes, guessed when a PDF has none           | Yes                                     | Yes                                                                                         | Yes                                           |
| Printing                                | No                                         | Yes                                     | Yes                                                                                         | Yes                                           |
| Configured by                           | A TOML file                                | A zathurarc file                        | Settings dialogs                                                                            | Menus and GSettings                           |
| Licence                                 | AGPL-3.0-or-later                          | Zlib                                    | GPL-2.0-or-later                                                                            | GPL-2.0-or-later                              |

Formats come from what is installed on this machine: zathura's MuPDF plugin, Evince's back-ends and Okular's generators; licences from pacman.

## What this means for pdvim

pdvim's engine side is already the most efficient of the four; what costs it is the start-up of its window, and the drivers that come with it.

- [ ] Tell wgpu which driver to use, rather than letting it load every Vulkan driver installed: this should cut most of the 503 MiB mapped, and some start-up time with it.
- [ ] Find what fills the 0.13 s before pdvim's window appears: Okular's and Evince's are on screen in 0.24–0.27 s, pdvim's in 0.36–0.39 s, though pdvim is then ready almost at once.
- [ ] Consider printing: it is the one common feature the other three have and pdvim lacks.

What to keep: pdvim's opening work stays between 0.16 and 0.26 s from one page to 1,908, its memory grows least with document size, and going to the last page costs it almost nothing.

## Caveats

These are warm starts on one machine, with each reader in its own default view, so small differences are not meaningful.

- **Default views differ.** pdvim fits the page's width at 163% and recolours it for its dark theme; zathura fits the whole page; Okular and Evince open a sidebar of contents or thumbnails. Each does a different amount of work for its first screen.
- **Warm starts.** The documents and libraries were already in the operating system's cache. From a cold disk all four would be slower, those that load more libraries more so.
- **Noise.** Other programs were running. Differences of a few hundredths of a second, and anything under the 20 ms the samples were taken at, are within the noise.
- **The timing method was changed once.** A first run timed each reader until it was completely idle, and put all four at about 1.5 s: it was measuring stray ticks from timers and the window's opening animation. Those 112 runs were thrown away. "Open and drawn" now means the moment all but the last 20 ms of opening work was done.
- **Okular's jump to the last page was not measured**, as above.
