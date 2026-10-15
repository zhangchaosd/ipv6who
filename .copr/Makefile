# Builds the SRPM for COPR's "SCM" source type with the make_srpm method.
# COPR runs, as root in a Fedora chroot with network access:
#   make -f <clone>/.copr/Makefile srpm outdir=<dir> spec=<clone>/ipv6who.spec
# Source0 is downloaded from the GitHub tag named by the spec's Version, and
# Source1 (vendored crates) is built from that same tarball with dist/vendor.sh.

topdir := $(abspath $(dir $(lastword $(MAKEFILE_LIST)))/..)
spec ?= $(topdir)/ipv6who.spec
outdir ?= $(CURDIR)
workdir := $(abspath $(outdir))/.copr-work

.PHONY: srpm
srpm:
	dnf -y install cargo rpm-build rpmdevtools git
	rm -rf $(workdir)
	mkdir -p $(workdir)/sources $(workdir)/tree
	spectool -g -s 0 -C $(workdir)/sources $(spec)
	tar -xzf $(workdir)/sources/*.tar.gz -C $(workdir)/tree --strip-components=1
	cd $(workdir)/tree && CARGO_HOME=$(workdir)/cargo sh $(topdir)/dist/vendor.sh
	mv $(workdir)/tree/*-vendor.tar.xz $(workdir)/sources/
	rpmbuild -bs $(spec) \
		--define "_topdir $(workdir)/rpmbuild" \
		--define "_sourcedir $(workdir)/sources" \
		--define "_srcrpmdir $(abspath $(outdir))"
	rm -rf $(workdir)
