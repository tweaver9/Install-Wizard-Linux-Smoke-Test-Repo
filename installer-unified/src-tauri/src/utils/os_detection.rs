use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum OperatingSystem {
    Windows,
    Linux,
    Unknown,
}

/// Detect the current operating system
#[allow(dead_code)]
pub fn detect_os() -> OperatingSystem {
    #[cfg(target_os = "windows")]
    return OperatingSystem::Windows;

    #[cfg(target_os = "linux")]
    return OperatingSystem::Linux;

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    return OperatingSystem::Unknown;
}

/// Get OS name as string
#[allow(dead_code)]
pub fn get_os_name() -> String {
    match detect_os() {
        OperatingSystem::Windows => "Windows".to_string(),
        OperatingSystem::Linux => "Linux".to_string(),
        OperatingSystem::Unknown => "Unknown".to_string(),
    }
}
