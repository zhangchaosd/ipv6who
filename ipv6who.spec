# Draft spec for COPR, modeled on `rust2rpm --vendor` output.
# Dependencies are vendored (Source1, built by dist/vendor.sh), so the
# build does not need rust-*-devel packages from Fedora.
%bcond check 1

# prevent library files from being installed
%global cargo_install_lib 0

Name:           ipv6who
Version:        0.3.1
Release:        1%{?dist}
Summary:        HTTP service that reports the host's global IPv6 address

SourceLicense:  MIT
# Licenses of the statically linked vendored crates, from %%cargo_license_summary.
# Update when dependencies change.
License:        MIT AND Apache-2.0 AND BSD-3-Clause AND (Apache-2.0 OR BSL-1.0) AND (Apache-2.0 OR MIT) AND (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT) AND (BSD-2-Clause OR Apache-2.0 OR MIT) AND (MIT OR Apache-2.0) AND (Unlicense OR MIT)
URL:            https://github.com/zhangchaosd/ipv6who
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.xz

BuildRequires:  cargo-rpm-macros >= 26
BuildRequires:  systemd-rpm-macros

%global _description %{expand:
A small HTTP service that reports the global IPv6 address of the host.
Only clients from configured networks may connect; others are refused
at the connection level.}

%description %{_description}

%prep
%autosetup -n %{name}-%{version} -p1 -a1
%cargo_prep -v vendor

%build
%cargo_build
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies
%{cargo_vendor_manifest}

%install
%cargo_install
install -Dpm 0644 dist/%{name}.service %{buildroot}%{_unitdir}/%{name}.service
sed -i 's|/usr/local/bin/|%{_bindir}/|' %{buildroot}%{_unitdir}/%{name}.service

%if %{with check}
%check
%cargo_test
%endif

%post
%systemd_post %{name}.service

%preun
%systemd_preun %{name}.service

%postun
%systemd_postun_with_restart %{name}.service

%files
%license LICENSE
%license LICENSE.dependencies
%license cargo-vendor.txt
%doc README.md
%{_bindir}/%{name}
%{_unitdir}/%{name}.service

%changelog
* Sat Oct 17 2026 ipv6who contributors - 0.3.1-1
- Windows: link the C runtime statically so the service starts without
  the Visual C++ Redistributable
- Log to stderr

* Sat Oct 17 2026 ipv6who contributors - 0.3.0-1
- Configure everything on the command line; drop the config file

* Sat Oct 17 2026 ipv6who contributors - 0.2.0-1
- Support macOS and Windows
- Report hostname and OS name, version and architecture in /v1/ipv6
- Add --dump-addresses and --log-file

* Thu Oct 15 2026 ipv6who contributors - 0.1.0-1
- Initial package
