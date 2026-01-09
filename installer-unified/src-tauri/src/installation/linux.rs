// Linux-specific installation
//
// Phase 5: Installation Logic (Linux)
//
// NOTE: This workspace is currently being validated on Windows; Linux deployment
// functions are behind `cfg(target_os = "linux")` in `installation/mod.rs`.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::Path;
use tokio::time::Duration;

use crate::installation::run_cmd_with_timeout;

// Re-export types from the cross-platform parsers module
pub use crate::installation::linux_parsers::{
    parse_meminfo_available_kb, parse_os_release, LinuxDistro,
};

// ============================================================================
// Root/sudo detection (P2-5)
// ============================================================================

/// Check if the current process is running as root (euid == 0).
pub fn is_running_as_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Check if passwordless sudo is available.
///
/// Runs `sudo -n true` and returns true if it succeeds.
pub async fn check_sudo_available() -> Result<bool> {
    debug!("[PHASE: preflight] [STEP: linux] check_sudo_available entered");

    let args = vec!["-n".to_string(), "true".to_string()];
    let result = run_cmd_with_timeout("sudo", &args, Duration::from_secs(10), "sudo_check").await;

    match result {
        Ok(out) => {
            let available = out.exit_code == Some(0);
            debug!(
                "[PHASE: preflight] [STEP: linux] check_sudo_available exit (available={}, exit_code={:?})",
                available, out.exit_code
            );
            Ok(available)
        }
        Err(e) => {
            debug!(
                "[PHASE: preflight] [STEP: linux] check_sudo_available exit (available=false, error={})",
                e
            );
            Ok(false)
        }
    }
}

/// Require root or passwordless sudo.
///
/// Returns Ok(()) if running as root or if passwordless sudo is available.
/// Returns an error with a clear message otherwise.
pub async fn require_root_or_passwordless_sudo() -> Result<()> {
    if is_running_as_root() {
        return Ok(());
    }

    if check_sudo_available().await? {
        return Ok(());
    }

    anyhow::bail!(
        "Requires root or passwordless sudo. Re-run installer with sudo or configure sudoers."
    )
}

// ============================================================================
// File permission helpers (P2-3)
// ============================================================================

/// Set executable permissions (chmod 755) on a file.
pub async fn set_executable_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    debug!(
        "[PHASE: installation] [STEP: linux] set_executable_permissions entered (path={:?})",
        path
    );

    let metadata = tokio::fs::metadata(path)
        .await
        .with_context(|| format!("File not found: {:?}", path))?;

    let mut perms = metadata.permissions();
    perms.set_mode(0o755);
    tokio::fs::set_permissions(path, perms)
        .await
        .with_context(|| format!("Failed to set permissions on {:?}", path))?;

    debug!(
        "[PHASE: installation] [STEP: linux] set_executable_permissions exit ok (path={:?})",
        path
    );

    Ok(())
}

/// Set ownership of a file or directory using chown.
///
/// If running as root, runs chown directly.
/// If not root, requires passwordless sudo.
pub async fn set_service_user_ownership(path: &Path, user: &str, group: &str) -> Result<()> {
    debug!(
        "[PHASE: installation] [STEP: linux] set_service_user_ownership entered (path={:?}, user={}, group={})",
        path, user, group
    );

    require_root_or_passwordless_sudo().await?;

    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid path for chown"))?;

    let owner_spec = format!("{}:{}", user, group);

    let (program, args) = if is_running_as_root() {
        ("chown", vec![owner_spec.clone(), path_str.to_string()])
    } else {
        (
            "sudo",
            vec![
                "-n".to_string(),
                "chown".to_string(),
                owner_spec.clone(),
                path_str.to_string(),
            ],
        )
    };

    let out = run_cmd_with_timeout(program, &args, Duration::from_secs(30), "chown").await?;

    if out.exit_code != Some(0) {
        anyhow::bail!(
            "chown failed (exit_code={:?}): {}",
            out.exit_code,
            out.stderr
        );
    }

    debug!(
        "[PHASE: installation] [STEP: linux] set_service_user_ownership exit ok (path={:?})",
        path
    );

    Ok(())
}

/// Detect the Linux distribution by reading /etc/os-release.
pub async fn detect_linux_distro() -> Result<LinuxDistro> {
    debug!("[PHASE: preflight] [STEP: linux] detect_linux_distro entered");

    let contents = tokio::fs::read_to_string("/etc/os-release")
        .await
        .context("Failed to read /etc/os-release")?;

    let distro = parse_os_release(&contents);

    debug!(
        "[PHASE: preflight] [STEP: linux] detect_linux_distro exit (id={}, version_id={}, pretty_name={})",
        distro.id, distro.version_id, distro.pretty_name
    );

    Ok(distro)
}

/// Get available memory in MB by reading /proc/meminfo.
pub async fn get_available_memory_mb() -> Result<u64> {
    debug!("[PHASE: preflight] [STEP: linux] get_available_memory_mb entered");

    let contents = tokio::fs::read_to_string("/proc/meminfo")
        .await
        .context("Failed to read /proc/meminfo")?;

    let kb = parse_meminfo_available_kb(&contents)
        .ok_or_else(|| anyhow::anyhow!("Could not parse memory info from /proc/meminfo"))?;

    let mb = kb / 1024;

    debug!(
        "[PHASE: preflight] [STEP: linux] get_available_memory_mb exit (kb={}, mb={})",
        kb, mb
    );

    Ok(mb)
}

/// Get free disk space in bytes for the given path using statvfs.
pub async fn get_free_space_bytes_linux(path: &Path) -> Result<u64> {
    use std::ffi::CString;

    debug!(
        "[PHASE: preflight] [STEP: linux] get_free_space_bytes_linux entered (path={:?})",
        path
    );

    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid path for statvfs"))?;

    let c_path = CString::new(path_str).context("Path contains null byte")?;

    // Use libc directly to avoid adding nix dependency
    let result = unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        let ret = libc::statvfs(c_path.as_ptr(), &mut stat);
        if ret == 0 {
            // Use f_frsize (fragment size), fallback to f_bsize if f_frsize is 0
            let block_size = if stat.f_frsize > 0 {
                stat.f_frsize
            } else {
                stat.f_bsize
            };
            // Available blocks * block size
            Ok((stat.f_bavail as u64) * (block_size as u64))
        } else {
            Err(std::io::Error::last_os_error())
        }
    };

    let bytes = result.context("statvfs failed")?;

    debug!(
        "[PHASE: preflight] [STEP: linux] get_free_space_bytes_linux exit (path={:?}, bytes={})",
        path, bytes
    );

    Ok(bytes)
}

// ============================================================================
// Linux native installation (P2-4)
// ============================================================================

use crate::api::installer::{InstallArtifacts, ProgressEmitter, ProgressPayload, StartInstallRequest};
use crate::installation::files::{collect_files_recursive, copy_file_with_retries_and_sha256};
use crate::installation::service::{install_and_start_linux_service, is_linux_service_running, SERVICE_NAME};
use crate::utils::path_resolver::resolve_deployment_folder;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Known executable names to look for in the runtime directory.
const KNOWN_EXECUTABLE_NAMES: &[&str] = &[
    "cadalytix-server",
    "cadalytix",
    "CADalytix.Server",
    "server",
];

/// Install CADalytix natively on Linux (systemd service).
///
/// This function:
/// 1. Validates prerequisites (runtime payloads exist, destination valid)
/// 2. Copies runtime files from runtime/linux/ and runtime/shared/
/// 3. Sets executable permissions
/// 4. Installs and starts the systemd service
/// 5. Verifies service is running
pub async fn install_linux_native(
    req: &StartInstallRequest,
    emit_progress: &ProgressEmitter,
    correlation_id: &str,
) -> Result<InstallArtifacts> {
    let started = Instant::now();
    info!(
        "[PHASE: install] [STEP: linux_native] install_linux_native entered (destination={})",
        req.destination_folder
    );

    // Step 1: Validate prerequisites
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "linux_validate".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 5,
        message: "Validating Linux prerequisites...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    // Check root/sudo access
    require_root_or_passwordless_sudo().await?;

    // Resolve runtime source directories
    let deployment_folder = resolve_deployment_folder()?;
    let runtime_dir = deployment_folder.join("runtime");
    let runtime_linux = runtime_dir.join("linux");
    let runtime_shared = runtime_dir.join("shared");

    // Validate runtime/linux exists and has files
    if !tokio::fs::try_exists(&runtime_linux).await.unwrap_or(false) {
        anyhow::bail!(
            "Linux runtime payloads are missing. Ensure runtime/linux is populated in the installer bundle. Expected path: {:?}",
            runtime_linux
        );
    }

    let linux_files = collect_files_recursive(&runtime_linux).await?;
    if linux_files.is_empty() {
        anyhow::bail!(
            "Linux runtime payloads are empty. Ensure runtime/linux contains the required files."
        );
    }

    // Step 2: Prepare destination directory
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "linux_prepare".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 10,
        message: "Preparing destination directory...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    let dest_root = PathBuf::from(&req.destination_folder);
    tokio::fs::create_dir_all(&dest_root)
        .await
        .with_context(|| format!("Failed to create destination directory: {:?}", dest_root))?;

    // Step 3: Copy runtime files
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "linux_copy".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 15,
        message: "Copying runtime files...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    let mut sources: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut manifest_files: HashMap<String, String> = HashMap::new();

    // Collect from shared directory first
    if tokio::fs::try_exists(&runtime_shared).await.unwrap_or(false) {
        let shared_files = collect_files_recursive(&runtime_shared).await?;
        for f in shared_files {
            let rel = f.strip_prefix(&runtime_shared).unwrap_or(&f);
            let dst = dest_root.join(rel);
            sources.push((f, dst));
        }
    }

    // Collect from linux directory
    for f in linux_files {
        let rel = f.strip_prefix(&runtime_linux).unwrap_or(&f);
        let dst = dest_root.join(rel);
        sources.push((f, dst));
    }

    // Copy files
    let total_files = sources.len().max(1);
    for (i, (src, dst)) in sources.iter().enumerate() {
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let (_bytes, sha256) = copy_file_with_retries_and_sha256(src, dst, "linux_deploy_copy").await?;

        let rel_path = dst
            .strip_prefix(&dest_root)
            .unwrap_or(dst)
            .to_string_lossy()
            .replace('\\', "/");
        manifest_files.insert(rel_path, sha256);

        // Emit progress every 10 files or at start/end
        if i == 0 || i == total_files - 1 || i % 10 == 0 {
            let pct = 15 + ((i * 50) / total_files) as i32;
            emit_progress(ProgressPayload {
                correlation_id: correlation_id.to_string(),
                step: "linux_copy".to_string(),
                severity: "info".to_string(),
                phase: "install".to_string(),
                percent: pct.min(65),
                message: format!("Copying files... ({}/{})", i + 1, total_files),
                elapsed_ms: Some(started.elapsed().as_millis()),
                eta_ms: None,
            });
        }
    }

    // Step 4: Find and set executable permissions
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "linux_permissions".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 70,
        message: "Setting executable permissions...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    let exec_path = find_main_executable(&dest_root).await?;
    set_executable_permissions(&exec_path).await?;

    info!(
        "[PHASE: install] [STEP: linux_native] Found main executable: {:?}",
        exec_path
    );

    // Step 5: Install and start systemd service
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "linux_service".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 80,
        message: "Installing and starting systemd service...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    install_and_start_linux_service(SERVICE_NAME, &exec_path, &dest_root, None).await?;

    // Step 6: Verify service is running
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "linux_verify".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 95,
        message: "Verifying service status...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    let running = is_linux_service_running(SERVICE_NAME).await?;
    if !running {
        anyhow::bail!(
            "Service '{}' is not running after installation. Check logs with: journalctl -u {}",
            SERVICE_NAME,
            SERVICE_NAME
        );
    }

    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "linux_complete".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 100,
        message: "Linux native installation complete.".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    info!(
        "[PHASE: install] [STEP: linux_native] install_linux_native exit ok (duration_ms={})",
        started.elapsed().as_millis()
    );

    Ok(InstallArtifacts {
        log_folder: None,
        artifacts_dir: Some(dest_root.to_string_lossy().to_string()),
        manifest_path: None,
        mapping_path: None,
        config_path: None,
    })
}

/// Find the main executable in the install directory.
///
/// Looks for known executable names or falls back to finding any executable file.
async fn find_main_executable(install_dir: &Path) -> Result<PathBuf> {
    // First, check for known executable names in common locations
    let search_dirs = [
        install_dir.to_path_buf(),
        install_dir.join("bin"),
    ];

    for dir in &search_dirs {
        for name in KNOWN_EXECUTABLE_NAMES {
            let candidate = dir.join(name);
            if tokio::fs::try_exists(&candidate).await.unwrap_or(false) {
                return Ok(candidate);
            }
        }
    }

    // Fallback: find any executable file
    let files = collect_files_recursive(install_dir).await?;
    let mut candidates: Vec<PathBuf> = Vec::new();

    for f in files {
        // Check if file is executable (has execute bit set or no extension on Unix)
        if let Ok(meta) = tokio::fs::metadata(&f).await {
            use std::os::unix::fs::PermissionsExt;
            let mode = meta.permissions().mode();
            // Check for any execute bit (owner, group, or other)
            if mode & 0o111 != 0 {
                candidates.push(f);
            }
        }
    }

    // Also include files without extensions that might be binaries
    let binary_candidates: Vec<_> = candidates
        .iter()
        .filter(|p| p.extension().is_none())
        .cloned()
        .collect();

    if binary_candidates.len() == 1 {
        return Ok(binary_candidates[0].clone());
    }

    if candidates.len() == 1 {
        return Ok(candidates[0].clone());
    }

    if candidates.is_empty() {
        anyhow::bail!(
            "No executable files found in {:?}. Ensure the runtime payloads include the server binary.",
            install_dir
        );
    }

    // Multiple candidates found - list them in error
    let candidate_list: Vec<String> = candidates
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    anyhow::bail!(
        "Multiple executable files found. Cannot determine which is the main server binary. Candidates: {:?}",
        candidate_list
    );
}
