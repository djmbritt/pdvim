# Submitting pdvim to Flathub

What is left to do to put pdvim on Flathub, in order. Flathub's own pages
are the authority, and they change; read them before starting:

- https://docs.flathub.org/docs/for-app-authors/requirements
- https://docs.flathub.org/docs/for-app-authors/submission
- https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines
- https://docs.flathub.org/docs/for-app-authors/maintenance

## Read this first: Flathub's rules on AI

Flathub's requirements have a generative AI policy. In short:

- **The Flathub manifest must not contain AI-generated or AI-assisted
  content.** Disclosing it does not make it acceptable.
- AI-generated code, documentation or packaging in the app itself must be
  disclosed in the pull request: which parts, and roughly how much.
  Reviewers may reject a submission for it.
- An AI tool must not open the pull request, nor write its commits,
  description, review comments or replies.

So there is no Flathub manifest in this directory, and none should be
written by an AI. `packaging/flatpak/io.github.djmbritt.pdvim.yml`, the
manifest the CI builds with, should not be copied into the submission
either, unless you wrote it yourself. Write the Flathub one by hand; the
notes below are what it has to do, not the manifest itself.

What has to be disclosed: much of pdvim's history has Claude as co-author
(`git log --format=%B | grep -c 'Co-Authored-By: Claude'`), and the
metainfo's screenshots, branding and releases were added with Claude.
Say so in the pull request, in your own words.

## Before submitting

1. **Make the repository public.** Flathub builds from public sources only,
   and the screenshots in the metainfo are linked at
   `raw.githubusercontent.com/djmbritt/pdvim/vX.Y.Z/...`, which only
   answers once it is public.
2. **Tag a release** that has the metainfo as it is now, with its
   `<releases>` entry and screenshots at that tag. Push the tag.
   `packaging/bump-version.sh` now adds the release entry and moves the
   screenshot links to the new version, so a release made after it does
   this by itself. Check that the screenshot links open in a browser.
3. **History.** Flathub asks for "sufficient development history and a
   clear commitment to ongoing maintenance". pdvim's first commit is from
   13 November 2026, and its releases came days apart. Expect a reviewer to
   ask about this; letting the project age a few weeks with regular
   releases helps.

## The manifest: what it has to do

Notes for writing `io.github.djmbritt.pdvim.yml` by hand. It sits at the
top of the submission, named after the app ID.

- The runtime is `org.freedesktop.Platform` and its SDK, at the latest
  version on Flathub when you submit (26.08 at the time of writing).
- The SDK extensions are `rust-stable` and an `llvm` one: the `mupdf`
  crate makes its bindings with bindgen, which needs libclang, so
  `LIBCLANG_PATH` must point at the extension's `lib`. For 26.08 Flathub has `llvm22`,
  which the CI builds with, and `llvm23`; check with
  `flatpak remote-ls flathub | grep Sdk.Extension.llvm`.
- pdvim's source is a `git` source with `url`, `tag` **and** `commit`
  (the commit the tag points at, `git rev-parse vX.Y.Z^{commit}`), or an
  `archive` of the tag with its `sha256`. Not `type: dir`: Flathub builds
  only from public URLs.
- The crates come from `cargo-sources.json`, made from the tag's
  `Cargo.lock` with flatpak-builder-tools' `flatpak-cargo-generator.py`
  (the CI's "List the crates" step shows how). It is put in the
  submission beside the manifest. The build has no network, so cargo runs
  `--offline`.
- What is installed: the binary, `packaging/pdvim.desktop` as
  `io.github.djmbritt.pdvim.desktop` (`rename-desktop-file`),
  `packaging/pdvim.svg` as the scalable icon (`rename-icon`), and
  `packaging/io.github.djmbritt.pdvim.metainfo.xml` into
  `/app/share/metainfo`. The licence goes to
  `/app/share/licenses/io.github.djmbritt.pdvim/`; flatpak-builder does
  that by itself for a `LICENSE` at the top of the source.
- Flathub builds for x86_64 and aarch64. The CI only builds x86_64. If
  aarch64 does not build, a `flathub.json` with
  `{"only-arches": ["x86_64"]}` goes beside the manifest.
- An `x-checker-data` on the git source, of type `git` with a
  `tag-pattern` of `^v([\d.]+)$`, lets Flathub's checker open update pull
  requests (see Updates, below).
- YAML style: two spaces, no aligned values, a blank line between
  modules, and keys in the order Flathub's requirements list.

### Permissions, and why

Explain these in the pull request; reviewers look at them on their own.

- `--share=ipc`, `--socket=wayland`, `--socket=fallback-x11`,
  `--device=dri`: a GPU-drawn window.
- **`--filesystem=home`**, and `--filesystem=/run/media` and
  `--filesystem=/media` for removable drives. Reviewers push back on
  `home`, and Flathub says that where a portal covers the use, it must be
  used instead. Here no portal does, for these reasons:
  - pdvim has no file chooser. Documents are opened by path: on the
    command line, at the `:open` prompt (which completes paths by listing
    directories), and from the list of recent documents. The document
    portal only hands over files picked in a chooser or passed by the
    desktop to the app, so typed paths, completion and reopening recent
    documents would all stop working.
  - Highlights are saved into the PDF. When MuPDF cannot add to a file
    incrementally, pdvim writes a new file beside it and renames it over
    the old one, so the file is never left half written. That needs the
    directory, not just the file, which the document portal does not
    give.
  - `:w` to another name writes a new file anywhere.
  - The Flatpak's wrapper reads the configuration in `~/.config/pdvim`,
    as everywhere else.

  `xdg-documents` and `xdg-download` alone would cover where many people
  keep PDFs, but not papers kept in `~/src`, `~/notes` or the like, and
  pdvim would fail with "permission denied" on them with no way to ask.
  `/mnt` is dropped: it is rarely used for documents and
  is easy for a reviewer to question.

  To narrow this later, pdvim would need a file chooser through the
  portal (`ashpd`, or `rfd` with its portal backend) and saving through
  the portal's file. Even then typed paths and recent documents would
  need static access.

  **`--filesystem=home` needs a linter exception.** Flathub's linter
  fails a manifest with it (`finish-args-home-filesystem-access`), and the
  build is not published until Flathub grants an exception, in
  `flatpak_builder_lint/staticfiles/exceptions.json` of
  https://github.com/flathub-infra/flatpak-builder-lint. They are granted
  case by case, and have been for apps in the same position: AbiWord
  ("Doesn't use portals"), a comic book reader with its own file
  browser, and a renaming tool that writes beside the files. Ask for it
  in the submission pull request, or as the linter docs say, with the
  reason in your own words: Flathub says not to use an LLM for exception
  requests at all, and may refuse one for good if you do.

  If it is refused, the fallback is `--filesystem=xdg-documents`,
  `--filesystem=xdg-download`, `--filesystem=xdg-desktop` and the two
  media paths. Documents kept elsewhere then fail to open with
  "permission denied", and the wrapper that points pdvim at
  `~/.config/pdvim` should go, so it reads its configuration from
  `~/.var/app/io.github.djmbritt.pdvim/config/pdvim` instead.
- No `--share=network`: pdvim does not use the network. `xdg-open` and
  `:!` commands run inside the sandbox; `xdg-open` goes through the
  OpenURI portal, so following web links still works.

## Validating

On a machine with Flatpak (or in the
`ghcr.io/flathub-infra/flatpak-github-actions:freedesktop-26.08`
container, as the CI does):

    flatpak install -y flathub org.flatpak.Builder
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
        appstream packaging/io.github.djmbritt.pdvim.metainfo.xml
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
        manifest io.github.djmbritt.pdvim.yml
    flatpak run --command=flathub-build org.flatpak.Builder \
        --install io.github.djmbritt.pdvim.yml
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo
    flatpak run io.github.djmbritt.pdvim

The metainfo passes `appstreamcli validate --no-net` now. With the network,
it and the linter's `appstream` check warn that the homepage, bug tracker
and screenshot links cannot be reached, which is only because the
repository is private and has no `v0.1.31` tag on GitHub yet. Flathub
counts warnings as failures, so check again once it is public.

## Submitting

From Flathub's submission page:

1. Fork https://github.com/flathub/flathub with "Copy the master branch
   only" **unchecked**.
2. `git clone --branch=new-pr git@github.com:djmbritt/flathub.git && cd flathub`
3. `git checkout -b io.github.djmbritt.pdvim new-pr`
4. Add, at the top: `io.github.djmbritt.pdvim.yml`, `cargo-sources.json`,
   and `flathub.json` if needed. No source code, no binaries, no copy of
   the metainfo: it comes from pdvim's own repository.
5. Commit, push, and open a pull request against the **`new-pr`** branch,
   titled `Add io.github.djmbritt.pdvim`. Fill in the template, with the
   permissions and the AI disclosure. Write it yourself.
6. Answer the reviewers. When they are done, comment `bot, build` to have
   Flathub build it. Screenshots do not show in these test builds.
7. Once merged, Flathub makes `flathub/io.github.djmbritt.pdvim` and
   invites you to it. Turn on two-factor authentication on GitHub and
   accept within a week. The app appears on Flathub an hour or two after.
   Verifying the app (the tick on its page) is done by logging in to
   Flathub with GitHub, which shows you own `djmbritt`.

## Updates

Updates never go through `flathub/flathub` again. Each one is a pull
request to `flathub/io.github.djmbritt.pdvim`:

1. Release as now: `packaging/bump-version.sh`, commit, tag `vX.Y.Z`, push.
2. In the Flathub repository, change the source's `tag` and `commit`, and
   remake `cargo-sources.json` from the new tag's `Cargo.lock`.
3. Open the pull request. Flathub builds it and posts a test build; merge
   it when it passes, and the update is published.

With `x-checker-data` on the source, Flathub's external data checker
notices new tags and opens the pull request for step 2 by itself. It does
not remake `cargo-sources.json`, though, so when `Cargo.lock` changed,
push a new one to its branch before merging, or the build fails offline.
