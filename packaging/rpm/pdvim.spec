# Builds pdvim from source, offline, for Fedora and openSUSE: the crates it
# needs come in Source1, which packaging/rpm/make-srpm.sh makes with
# `cargo vendor`. See packaging/rpm/README.md.
#
# The Version below is kept in step with Cargo.toml by
# packaging/bump-version.sh, and make-srpm.sh sets it again from the
# Cargo.toml of the tree it packages, so the two cannot disagree in an SRPM.

# MuPDF, which the mupdf crate compiles from C, is built with the
# distribution's CFLAGS. Objects compiled with -flto do not link into the
# Rust binary (the AUR PKGBUILD has options=('!lto') for the same reason).
%global _lto_cflags %{nil}

%if !(0%{?fedora} || 0%{?rhel})
# Outside Fedora, cargo's release profile is used, which has no debugging
# information, so there would be only an empty debugsource package.
%global debug_package %{nil}
%endif

Name:           pdvim
Version:        0.1.44
Release:        1%{?dist}
Summary:        A minimal, keyboard-driven PDF reader

# pdvim is AGPL-3.0-or-later. The rest is the crates linked into the binary
# (see cargo-vendor.txt and LICENSE.dependencies in the package), as
# %%cargo_license_summary prints them in the build log for 0.1.31 (the
# mupdf crates say AGPL-3.0), and, last, the C libraries bundled in
# mupdf-sys (FreeType, libjpeg) that are not under one of those already.
License:        AGPL-3.0-or-later AND AGPL-3.0-only AND ((MIT OR Apache-2.0) AND NCSA) AND ((MIT OR Apache-2.0) AND Unicode-3.0) AND (0BSD OR MIT OR Apache-2.0) AND Apache-2.0 AND (Apache-2.0 AND MIT) AND (Apache-2.0 OR GPL-2.0-only) AND (Apache-2.0 OR MIT) AND (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT) AND BSD-2-Clause AND (BSD-2-Clause OR Apache-2.0 OR MIT) AND BSD-3-Clause AND (BSD-3-Clause OR Apache-2.0) AND (BSD-3-Clause OR MIT OR Apache-2.0) AND BSL-1.0 AND CC0-1.0 AND (CC0-1.0 OR Apache-2.0) AND ISC AND MIT AND (MIT OR Apache-2.0) AND (MIT OR Apache-2.0 OR LGPL-2.1-or-later) AND (MIT OR Apache-2.0 OR Zlib) AND (MIT OR Zlib OR Apache-2.0) AND MPL-2.0 AND (Unlicense OR MIT) AND Zlib AND (Zlib OR Apache-2.0 OR MIT) AND (FTL OR GPL-2.0-or-later) AND IJG
URL:            https://github.com/djmbritt/pdvim
# The GitHub tarball of the tag vX.Y.Z. While the repository is private,
# make-srpm.sh makes the same tarball with `git archive`.
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
# The crates of Cargo.lock, made by make-srpm.sh (`cargo vendor`).
Source1:        %{name}-%{version}-vendor.tar.xz

# Rust 1.90 is Cargo.toml's rust-version.
BuildRequires:  cargo >= 1.90
BuildRequires:  rust >= 1.90
%if 0%{?fedora} || 0%{?rhel}
BuildRequires:  cargo-rpm-macros >= 26
%endif
# MuPDF is compiled from source (with make), and bindgen makes its bindings
# with libclang.
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  make
BuildRequires:  clang
BuildRequires:  pkgconfig(fontconfig)
BuildRequires:  pkgconfig(freetype2)
BuildRequires:  desktop-file-utils

# The binary links to fontconfig, which rpm finds by itself. The libraries
# for the window and the GPU are opened at run time, so rpm does not see
# them: keyboard input needs xkbcommon, and Wayland its client library.
%if 0%{?__isa_bits} == 64
%global lib64 ()(64bit)
%endif
Requires:       libxkbcommon.so.0%{?lib64}
Requires:       libwayland-client.so.0%{?lib64}
Requires:       hicolor-icon-theme
# Drawing with the GPU (wgpu); without them it draws with the CPU.
Recommends:     libvulkan.so.1%{?lib64}
Recommends:     libEGL.so.1%{?lib64}
# For X11 sessions.
Recommends:     libX11.so.6%{?lib64}
Recommends:     libXcursor.so.1%{?lib64}
Recommends:     libXrandr.so.2%{?lib64}
Recommends:     libXi.so.6%{?lib64}
Recommends:     libxkbcommon-x11.so.0%{?lib64}

# The MuPDF the mupdf-sys crate compiles in.
# (On Fedora, cargo-rpm-macros adds a bundled(crate(...)) Provides for each
# vendored crate, from cargo-vendor.txt.)
Provides:       bundled(mupdf) = 1.27.2

%description
pdvim is a minimal, keyboard-driven reader for PDF, EPUB, CBZ, XPS and
other documents, with vi keybindings, tabs, search, a visual mode and
highlights.

%prep
%autosetup -n %{name}-%{version} -a1
%if 0%{?fedora} || 0%{?rhel}
%cargo_prep -v vendor
%else
# openSUSE: point cargo at the vendored crates and keep it offline.
mkdir -p .cargo
cat > .cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"

[net]
offline = true
EOF
%endif

%build
%if 0%{?fedora} || 0%{?rhel}
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%{cargo_vendor_manifest}
%else
export CARGO_HOME="$PWD/.cargo"
cargo build --release --frozen --offline -j%{?_smp_build_ncpus}%{!?_smp_build_ncpus:1}
cargo tree --frozen --offline -e normal,build --prefix none --format '{p} {l}' | sort -u > LICENSE.dependencies
%endif

%install
# Not %%cargo_install: pdvim has a library target too (src/lib.rs), for
# which it would also install the crate's source in %%{_datadir}/cargo.
# %%cargo_build builds with cargo's "rpm" profile, into target/rpm.
%if 0%{?fedora} || 0%{?rhel}
install -Dpm 0755 target/rpm/%{name} %{buildroot}%{_bindir}/%{name}
%else
install -Dpm 0755 target/release/%{name} %{buildroot}%{_bindir}/%{name}
%endif
install -Dpm 0644 packaging/pdvim.desktop %{buildroot}%{_datadir}/applications/pdvim.desktop
install -Dpm 0644 packaging/pdvim.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/pdvim.svg
install -Dpm 0644 docs/man/pdvim.1 %{buildroot}%{_mandir}/man1/pdvim.1
install -Dpm 0644 docs/man/pdvim.5 %{buildroot}%{_mandir}/man5/pdvim.5

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/pdvim.desktop
%if 0%{?fedora} || 0%{?rhel}
%cargo_test
%else
export CARGO_HOME="$PWD/.cargo"
cargo test --release --frozen --offline -j%{?_smp_build_ncpus}%{!?_smp_build_ncpus:1}
%endif

%files
%license LICENSE LICENSE.dependencies
%if 0%{?fedora} || 0%{?rhel}
%license cargo-vendor.txt
%endif
%doc README.md defaults.toml
%{_bindir}/%{name}
%{_datadir}/applications/pdvim.desktop
%{_datadir}/icons/hicolor/scalable/apps/pdvim.svg
%{_mandir}/man1/pdvim.1*
%{_mandir}/man5/pdvim.5*

%changelog
* Sun Nov 22 2026 David Britt - 0.1.31-1
- Built from the v0.1.31 tag; see https://github.com/djmbritt/pdvim/releases
