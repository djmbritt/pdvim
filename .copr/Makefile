# Copr's "make srpm" source method runs, as root in a Fedora mock chroot
# with the network,
#
#     make -f .copr/Makefile srpm outdir=OUTDIR spec=SPEC
#
# from the top of its clone of the repository. This makes the SRPM of the
# latest v* tag there (or of REF=..., e.g. REF=HEAD for a snapshot) with
# packaging/rpm/make-srpm.sh; Copr then builds it, offline, for each chroot
# of the project. See packaging/rpm/README.md.

outdir ?= $(CURDIR)/dist/srpm
REF ?=
here := $(dir $(abspath $(lastword $(MAKEFILE_LIST))))

.PHONY: srpm
srpm:
	dnf -y install --setopt=install_weak_deps=False git-core cargo rpm-build tar xz
	# Copr's clone may be owned by another user than the one running this.
	git config --global --add safe.directory '*'
	$(here)../packaging/rpm/make-srpm.sh -o "$(outdir)" $(REF)
