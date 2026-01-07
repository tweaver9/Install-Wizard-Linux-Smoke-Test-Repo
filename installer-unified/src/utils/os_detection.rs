use log::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingSystem {
    Windows,
    Linux,
    Unknown,
}

pub fn detect_os() -> OperatingSystem {
    #[cfg(target_os = "windows")]
    {
        info!("[OS] Detected: Windows");
        return OperatingSystem::Windows;
    }
    
    #[cfg(target_os = "linux")]
    {
        info!("[OS] Detected: Linux");
        return OperatingSystem::Linux;
    }
    
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        info!("[OS] Detected: Unknown");
        return OperatingSystem::Unknown;
    }
}

pub fn get_os_name() -> String {
    match detect_os() {
        OperatingSystem::Windows => "Windows".to_string(),
        OperatingSystem::Linux => "Linux".to_string(),
        OperatingSystem::Unknown => "Unknown".to_string(),
    }
}

