# pdvim's RPM repository (Fedora Copr)

`pdvim.spec` builds pdvim from source, offline, with the crates of
`Cargo.lock` vendored into a second tarball. Fedora Copr builds it for each
release and serves the result as a dnf/zypper repository, so users get new
versions with their normal updates instead of downloading an `.rpm` each
time.

| File | What it is |
| --- | --- |
| `packaging/rpm/pdvim.spec` | The spec. Source0 is the GitHub tarball of the tag, Source1 the vendored crates. |
| `packaging/rpm/make-srpm.sh` | Makes both tarballs and the `.src.rpm` from a git checkout. |
| `.copr/Makefile` | Copr's "make srpm" entry point; installs what the script needs and runs it. |

## Why Copr (and not OBS, for now)

- Copr builds for every Fedora release (and Rawhide), **openSUSE
  Tumbleweed and Leap 16.0**, EPEL 10 and others from one spec, which is
  what users of `.rpm` distributions want.
- It is little to maintain: one spec, the repository's own
  `.copr/Makefile`, and a webhook. No separate checkout of the packaging
  (OBS keeps its own copy of the sources in `osc`, and is a second account
  and tool to learn).
- Debian and Ubuntu, the reason to use OBS, cannot build pdvim from source
  with their own Rust today: pdvim needs Rust 1.90, Debian 13 has 1.85 and
  Ubuntu 24.04's default `rustc` is 1.75 (it does offer `rustc-1.91` and
  later as separate packages). A Debian source package would also need its
  own `debian/` packaging kept up to date next to the spec. The `.deb` from
  the GitHub release, built on Ubuntu 22.04, keeps serving them; if an apt
  repository is wanted later, OBS (with `rustc-1.9x` on Ubuntu) or an apt
  repository of that `.deb` on GitHub Pages are the ways to go.

## Building it yourself

From a checkout, on Fedora (`sudo dnf install cargo rpm-build git-core`):

    packaging/rpm/make-srpm.sh             # the latest v* tag, into dist/srpm/
    packaging/rpm/make-srpm.sh HEAD        # what is committed now
    sudo dnf builddep dist/srpm/pdvim-*.src.rpm
    rpmbuild --rebuild dist/srpm/pdvim-*.src.rpm

Only `make-srpm.sh` uses the network (to fetch the crates);
`rpmbuild --rebuild` does not. Or, in a clean chroot:
`mock -r fedora-43-x86_64 dist/srpm/pdvim-*.src.rpm`.

Without Fedora, in a container:

    podman run --rm -it -v "$PWD:/src:z" -w /src fedora:latest bash -c '
        dnf -y install make && make -f .copr/Makefile srpm outdir=/src/dist/srpm REF=HEAD'

## Setting up Copr (once)

The repository must be public first: Copr clones it. Until then, you can
still upload an SRPM made with `make-srpm.sh` by hand (step 5, second way).

1. Log in at <https://copr.fedorainfracloud.org> with a Fedora account
   (create one at <https://accounts.fedoraproject.org>).
2. **New Project**, named `pdvim`:
   - Chroots: `fedora-43-x86_64`, `fedora-44-x86_64`, `fedora-45-x86_64`,
     `fedora-rawhide-x86_64`, `opensuse-tumbleweed-x86_64`,
     `opensuse-leap-16.0-x86_64`, and the `aarch64` ones of each if wanted.
     (Copr has no Leap 15 chroot; Leap 15.6 is at its end of life.)
   - Leave "Enable internet access during builds" **off**: the build needs
     none.
   - Description / instructions: `sudo dnf copr enable djmbritt/pdvim &&
     sudo dnf install pdvim`.
3. **Packages → New package → SCM**:
   - Package name: `pdvim`
   - Clone URL: `https://github.com/djmbritt/pdvim.git`
   - Committish: `main`
   - Spec file: `packaging/rpm/pdvim.spec`
   - Type: `git`; SRPM build method: **make srpm**
   - Tick **Auto-rebuild** ("Rebuild the package when the webhook is
     triggered").
4. **Settings → Integrations**: copy the GitHub webhook URL,
   `https://copr.fedorainfracloud.org/webhooks/github/<ID>/<UUID>/`, and
   add **`pdvim/`** at its end (the tags are `vX.Y.Z`, not
   `pdvim-X.Y.Z`, so Copr has to be told the package). In GitHub, **Settings
   → Webhooks → Add webhook**: that URL as the payload URL, content type
   `application/json`, and **Let me select individual events → Branch or tag
   creation** only (untick "Pushes", so commits to `main` do not rebuild the
   same version).
5. Build the current release once by hand, to check it:
   - **Packages → pdvim → Rebuild**; or
   - by uploading an SRPM: `packaging/rpm/make-srpm.sh`, then **Builds →
     New build → Upload** the `.src.rpm` (or
     `copr-cli build pdvim dist/srpm/pdvim-*.src.rpm`). This way also works
     while the repository is private.

A build takes a while: MuPDF is compiled from C each time.

## How a release reaches users

1. `packaging/bump-version.sh` (which also sets the spec's `Version:`),
   commit, `git tag vX.Y.Z && git push origin main vX.Y.Z`, as now.
2. GitHub tells Copr that a tag was created. Copr clones the repository and
   runs `make -f .copr/Makefile srpm`, which runs `make-srpm.sh`: it
   packages **the latest `v*` tag** (so whichever commit Copr checked out,
   the release is what gets built), sets the spec's `Version:` from that
   tag's `Cargo.toml` and adds a changelog entry for it.
3. Copr builds the SRPM in every chroot, without the network, running
   `cargo test` in `%check`, and publishes the RPMs. `dnf upgrade` /
   `zypper up` then offers them.

If the webhook is not wanted, `.github/workflows/copr.yml` can trigger the
build instead when a release is published: add the repository secret
`COPR_WEBHOOK_URL` with Copr's **custom** webhook URL for the package
(Settings → Integrations, `https://copr.fedorainfracloud.org/webhooks/custom/<ID>/<UUID>/pdvim/`)
and the variable `COPR_ENABLED` = `true`. Without both it does nothing. Use
one or the other, not both, or each release is built twice.

To build a snapshot of `main` (say, into a second Copr project for
testing), run `packaging/rpm/make-srpm.sh HEAD` and upload the SRPM; Copr's
"make srpm" method itself always gets the latest tag.

## Installing (for users)

Fedora:

    sudo dnf copr enable djmbritt/pdvim
    sudo dnf install pdvim

openSUSE Tumbleweed (Leap 16.0: put `opensuse-leap-16.0` for
`opensuse-tumbleweed`):

    sudo zypper addrepo https://copr.fedorainfracloud.org/coprs/djmbritt/pdvim/repo/opensuse-tumbleweed/djmbritt-pdvim-opensuse-tumbleweed.repo
    sudo zypper refresh && sudo zypper install pdvim

## What changes when the repository is public

- Copr can clone it: set up the SCM package and webhook above.
- `Source0` in the spec (the GitHub tarball of the tag) can be downloaded;
  `make-srpm.sh` makes the same tree with `git archive`, so nothing has to
  change, but `spectool -g packaging/rpm/pdvim.spec` then works.
- Arm runners: add the `aarch64` chroots in Copr (Copr builds them on its
  own machines, so this needs nothing from GitHub).

## Tested

With 0.1.31 (November 2026), on a 16-thread x86_64 machine, in containers:
`make -f .copr/Makefile srpm` in `fedora:latest` (Fedora 45), then
`rpmbuild --rebuild` with `podman run --network=none`, then the RPM
installed in a clean container and `pdvim --version` run.

| | Rust | Offline rebuild (with `%check`) | Result |
| --- | --- | --- | --- |
| Fedora 45 | 1.100 | 11m43s | 250 tests pass, installs, starts |
| openSUSE Tumbleweed | 1.99 | 7m01s | 250 tests pass, installs, starts |

Making the SRPM (fetching about 500 crates) took about a minute once the
tools were installed. The vendored crates are 85 MB compressed (most of it
is MuPDF's C source inside `mupdf-sys`, and the Windows crates cargo vendors
regardless of platform), so the SRPM is about 86 MB. Fedora's build is
slower because of its `rpm` cargo profile (one codegen unit, full debug
information).

Leap 16.0's Rust is 1.98, new enough. Leap 15.6 had 1.94 in its updates,
but its update repositories have stopped and Copr has no Leap 15 chroot.
Debian 13 (Rust 1.85) is too old.

## Notes on the spec

- MuPDF is built with the distribution's compiler flags but without LTO
  (`%global _lto_cflags %{nil}`): LTO objects from that C build do not link
  into the Rust binary.
- On Fedora it uses `cargo-rpm-macros` (`%cargo_prep -v vendor`,
  `%cargo_build`, `%cargo_test`, `%cargo_license`, `%cargo_vendor_manifest`,
  which also generates the `bundled(crate(...))` Provides). openSUSE has no
  such macros, so there it runs cargo directly with the same offline
  configuration.
- The window and GPU libraries (xkbcommon, Wayland, Vulkan, EGL, X11) are
  opened at run time, so rpm does not find them; the spec requires
  xkbcommon and the Wayland client library and recommends the rest, by
  soname, which both Fedora and openSUSE provide.
- The `License:` tag is AGPL-3.0-or-later plus the licenses of the crates
  linked in, as `%cargo_license_summary` prints them in the build log; check
  it there when dependencies change.
