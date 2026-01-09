// Linux parsing utilities (cross-platform for testability)
//
// This module contains pure parsing functions that can be tested on any OS.
// The actual file I/O happens in linux.rs which is cfg-gated.

/// Linux distribution information parsed from /etc/os-release.
#[derive(Debug, Clone, Default)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct LinuxDistro {
    /// Distribution ID (e.g., "ubuntu", "rhel", "debian").
    pub id: String,
    /// Version ID (e.g., "22.04", "9").
    pub version_id: String,
    /// Human-readable name (e.g., "Ubuntu 22.04.3 LTS").
    pub pretty_name: String,
    /// Related distributions (e.g., ["debian"], ["fedora"]).
    pub id_like: Vec<String>,
}

/// Parse /etc/os-release content into a LinuxDistro struct.
///
/// This is a pure function for testability on any OS.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_os_release(contents: &str) -> LinuxDistro {
    let mut id = String::new();
    let mut version_id = String::new();
    let mut pretty_name = String::new();
    let mut id_like = Vec::new();

    for line in contents.lines() {
        let line = line.trim();
        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Parse KEY=VALUE or KEY="VALUE" or KEY='VALUE'
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            // Remove surrounding quotes if present
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);

            match key {
                "ID" => id = value.to_string(),
                "VERSION_ID" => version_id = value.to_string(),
                "PRETTY_NAME" => pretty_name = value.to_string(),
                "ID_LIKE" => {
                    // ID_LIKE may contain space-separated values
                    id_like = value.split_whitespace().map(|s| s.to_string()).collect();
                }
                _ => {}
            }
        }
    }

    // Apply defaults
    if id.is_empty() {
        id = "linux".to_string();
    }
    if pretty_name.is_empty() {
        pretty_name = if version_id.is_empty() {
            id.clone()
        } else {
            format!("{} {}", id, version_id)
        };
    }

    LinuxDistro {
        id,
        version_id,
        pretty_name,
        id_like,
    }
}

/// Parse /proc/meminfo content to extract available memory in kB.
///
/// Prefers MemAvailable, falls back to MemFree + Buffers + Cached.
/// Returns None if no usable memory info found.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_meminfo_available_kb(contents: &str) -> Option<u64> {
    let mut mem_available: Option<u64> = None;
    let mut mem_free: Option<u64> = None;
    let mut buffers: Option<u64> = None;
    let mut cached: Option<u64> = None;

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Format: "MemAvailable:    12345678 kB"
        if let Some((key, rest)) = line.split_once(':') {
            let key = key.trim();
            // Extract the numeric value (ignore "kB" suffix)
            let value_str = rest.trim().split_whitespace().next().unwrap_or("");
            let value: Option<u64> = value_str.parse().ok();

            match key {
                "MemAvailable" => mem_available = value,
                "MemFree" => mem_free = value,
                "Buffers" => buffers = value,
                "Cached" => cached = value,
                _ => {}
            }
        }
    }

    // Prefer MemAvailable (modern kernels)
    if let Some(avail) = mem_available {
        return Some(avail);
    }

    // Fallback: MemFree + Buffers + Cached
    match (mem_free, buffers, cached) {
        (Some(f), Some(b), Some(c)) => Some(f + b + c),
        (Some(f), Some(b), None) => Some(f + b),
        (Some(f), None, Some(c)) => Some(f + c),
        (Some(f), None, None) => Some(f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_os_release_ubuntu() {
        let contents = r#"
NAME="Ubuntu"
VERSION="22.04.3 LTS (Jammy Jellyfish)"
ID=ubuntu
ID_LIKE=debian
PRETTY_NAME="Ubuntu 22.04.3 LTS"
VERSION_ID="22.04"
HOME_URL="https://www.ubuntu.com/"
"#;
        let distro = parse_os_release(contents);
        assert_eq!(distro.id, "ubuntu");
        assert_eq!(distro.version_id, "22.04");
        assert_eq!(distro.pretty_name, "Ubuntu 22.04.3 LTS");
        assert_eq!(distro.id_like, vec!["debian"]);
    }

    #[test]
    fn parse_os_release_rhel() {
        let contents = r#"
NAME="Red Hat Enterprise Linux"
VERSION="9.2 (Plow)"
ID="rhel"
ID_LIKE="fedora"
VERSION_ID="9.2"
PRETTY_NAME="Red Hat Enterprise Linux 9.2 (Plow)"
"#;
        let distro = parse_os_release(contents);
        assert_eq!(distro.id, "rhel");
        assert_eq!(distro.version_id, "9.2");
        assert_eq!(distro.id_like, vec!["fedora"]);
    }

    #[test]
    fn parse_os_release_with_multiple_id_like() {
        let contents = r#"
ID=linuxmint
ID_LIKE="ubuntu debian"
VERSION_ID="21.2"
"#;
        let distro = parse_os_release(contents);
        assert_eq!(distro.id, "linuxmint");
        assert_eq!(distro.id_like, vec!["ubuntu", "debian"]);
    }

    #[test]
    fn parse_os_release_missing_fields_uses_defaults() {
        let contents = r#"
# minimal os-release
ID=alpine
"#;
        let distro = parse_os_release(contents);
        assert_eq!(distro.id, "alpine");
        assert_eq!(distro.version_id, "");
        assert_eq!(distro.pretty_name, "alpine");
        assert!(distro.id_like.is_empty());
    }

    #[test]
    fn parse_os_release_empty_uses_defaults() {
        let distro = parse_os_release("");
        assert_eq!(distro.id, "linux");
        assert_eq!(distro.version_id, "");
        assert_eq!(distro.pretty_name, "linux");
        assert!(distro.id_like.is_empty());
    }

    #[test]
    fn parse_os_release_single_quotes() {
        let contents = "ID='debian'\nVERSION_ID='12'";
        let distro = parse_os_release(contents);
        assert_eq!(distro.id, "debian");
        assert_eq!(distro.version_id, "12");
    }

    #[test]
    fn parse_meminfo_with_mem_available() {
        let contents = r#"
MemTotal:       16384000 kB
MemFree:         2000000 kB
MemAvailable:    8000000 kB
Buffers:          500000 kB
Cached:          3000000 kB
"#;
        let kb = parse_meminfo_available_kb(contents);
        assert_eq!(kb, Some(8000000));
    }

    #[test]
    fn parse_meminfo_fallback_without_mem_available() {
        let contents = r#"
MemTotal:       16384000 kB
MemFree:         2000000 kB
Buffers:          500000 kB
Cached:          3000000 kB
"#;
        let kb = parse_meminfo_available_kb(contents);
        assert_eq!(kb, Some(5500000));
    }

    #[test]
    fn parse_meminfo_only_memfree() {
        let contents = "MemFree:         4000000 kB\n";
        let kb = parse_meminfo_available_kb(contents);
        assert_eq!(kb, Some(4000000));
    }

    #[test]
    fn parse_meminfo_empty_returns_none() {
        let kb = parse_meminfo_available_kb("");
        assert_eq!(kb, None);
    }
}

