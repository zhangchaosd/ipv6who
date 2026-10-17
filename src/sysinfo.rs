//! Basic description of the host OS, e.g. "Fedora Linux 44 (Cloud Edition)",
//! "macOS 26.0.1" or "Windows 11 Pro 24H2".

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    /// OS family: "linux", "macos" or "windows".
    pub os: &'static str,
    /// Human-readable OS name and version.
    pub name: String,
    /// Machine-readable OS version, when known.
    pub version: Option<String>,
    pub arch: &'static str,
}

pub fn detect() -> SystemInfo {
    let (name, version) = os::detect();
    SystemInfo {
        os: std::env::consts::OS,
        name,
        version,
        arch: std::env::consts::ARCH,
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod os {
    use std::{collections::HashMap, fs};

    pub fn detect() -> (String, Option<String>) {
        let text = fs::read_to_string("/etc/os-release")
            .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();
        parse_os_release(&text)
    }

    /// Parses os-release(5): PRETTY_NAME for the name, VERSION_ID for the version.
    pub fn parse_os_release(text: &str) -> (String, Option<String>) {
        let fields: HashMap<&str, String> = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim(), unquote(value.trim())))
            .collect();
        let version = fields.get("VERSION_ID").cloned();
        let name = fields.get("PRETTY_NAME").cloned().unwrap_or_else(|| {
            let base = fields.get("NAME").map_or("Linux", String::as_str);
            match &version {
                Some(v) => format!("{base} {v}"),
                None => base.to_owned(),
            }
        });
        (name, version)
    }

    fn unquote(value: &str) -> String {
        let inner = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    }
}

#[cfg(target_os = "macos")]
mod os {
    use std::{ffi::CString, ptr};

    pub fn detect() -> (String, Option<String>) {
        match sysctl_string("kern.osproductversion") {
            Some(v) => (format!("macOS {v}"), Some(v)),
            None => ("macOS".to_owned(), None),
        }
    }

    fn sysctl_string(name: &str) -> Option<String> {
        let name = CString::new(name).ok()?;
        let mut len = 0;
        // SAFETY: a null buffer asks sysctl for the required length.
        let rc = unsafe {
            libc::sysctlbyname(name.as_ptr(), ptr::null_mut(), &mut len, ptr::null_mut(), 0)
        };
        if rc != 0 {
            return None;
        }
        let mut buf = vec![0u8; len];
        // SAFETY: buf has `len` writable bytes.
        let rc = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                buf.as_mut_ptr().cast(),
                &mut len,
                ptr::null_mut(),
                0,
            )
        };
        if rc != 0 {
            return None;
        }
        buf.truncate(len);
        let s = String::from_utf8(buf).ok()?;
        Some(s.trim_end_matches('\0').to_owned())
    }
}

#[cfg(windows)]
mod os {
    use std::ptr;

    use windows_sys::Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW},
    };

    const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    /// First Windows 11 build.
    const WINDOWS_11_BUILD: u32 = 22000;

    pub fn detect() -> (String, Option<String>) {
        let build = reg_string("CurrentBuildNumber");
        let product = reg_string("ProductName").unwrap_or_else(|| "Windows".to_owned());
        let build_number = build.as_deref().and_then(|b| b.parse().ok()).unwrap_or(0);
        let mut name = fix_product_name(&product, build_number);
        if let Some(display) = reg_string("DisplayVersion") {
            name = format!("{name} {display}");
        }
        (name, build.map(|b| format!("10.0.{b}")))
    }

    /// The registry's ProductName still says "Windows 10" on Windows 11.
    pub fn fix_product_name(product: &str, build: u32) -> String {
        match product.strip_prefix("Windows 10") {
            Some(rest) if build >= WINDOWS_11_BUILD => format!("Windows 11{rest}"),
            _ => product.to_owned(),
        }
    }

    fn reg_string(value: &str) -> Option<String> {
        let key: Vec<u16> = CURRENT_VERSION.encode_utf16().chain([0]).collect();
        let value: Vec<u16> = value.encode_utf16().chain([0]).collect();
        let mut size = 0u32;
        // SAFETY: null data pointer asks for the size in bytes.
        let rc = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut size,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: buf holds `size` writable bytes.
        let rc = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(size as usize / 2);
        let s = String::from_utf16_lossy(&buf);
        let s = s.trim_end_matches('\0').trim();
        (!s.is_empty()).then(|| s.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_a_name() {
        let info = detect();
        assert!(!info.name.is_empty());
        assert_eq!(info.os, std::env::consts::OS);
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    #[test]
    fn parses_os_release() {
        let fedora = "NAME=\"Fedora Linux\"\nVERSION_ID=44\nPRETTY_NAME=\"Fedora Linux 44 (Cloud Edition)\"\n";
        assert_eq!(
            os::parse_os_release(fedora),
            ("Fedora Linux 44 (Cloud Edition)".into(), Some("44".into()))
        );
        let minimal = "NAME='Alpine Linux'\nVERSION_ID=3.22.1\n";
        assert_eq!(
            os::parse_os_release(minimal),
            ("Alpine Linux 3.22.1".into(), Some("3.22.1".into()))
        );
        assert_eq!(os::parse_os_release(""), ("Linux".into(), None));
    }

    #[cfg(windows)]
    #[test]
    fn windows_11_product_name() {
        assert_eq!(
            os::fix_product_name("Windows 10 Pro", 26100),
            "Windows 11 Pro"
        );
        assert_eq!(
            os::fix_product_name("Windows 10 Pro", 19045),
            "Windows 10 Pro"
        );
        assert_eq!(
            os::fix_product_name("Windows Server 2025 Datacenter", 26100),
            "Windows Server 2025 Datacenter"
        );
    }
}
