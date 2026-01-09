// Service installation/management helpers
//
// Phase 5: Installation Logic (Windows + Linux)

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::time::Duration;

use crate::installation::run_cmd_with_timeout;

/// Default service name for CADalytix.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub const SERVICE_NAME: &str = "cadalytix";

// ============================================================================
// Systemd unit file generation (pure function, testable on all platforms)
// ============================================================================

/// Build a systemd unit file text for a service.
///
/// This is a pure function for testability on any OS.
/// Paths are quoted to handle spaces correctly (systemd supports quoted arguments).
pub fn build_systemd_unit_text(
    service_name: &str,
    exec_path: &Path,
    working_dir: &Path,
    user: Option<&str>,
) -> String {
    let exec_str = exec_path.to_string_lossy();
    let work_str = working_dir.to_string_lossy();

    // Quote paths to handle spaces correctly (systemd supports quoted arguments)
    let exec_quoted = quote_systemd_path(&exec_str);
    let work_quoted = quote_systemd_path(&work_str);

    let user_line = match user {
        Some(u) => format!("User={}\n", u),
        None => String::new(),
    };

    format!(
        r#"[Unit]
Description=CADalytix Service ({service_name})
After=network.target

[Service]
Type=simple
WorkingDirectory={work_quoted}
ExecStart={exec_quoted}
Restart=always
RestartSec=5
{user_line}
[Install]
WantedBy=multi-user.target
"#,
        service_name = service_name,
        work_quoted = work_quoted,
        exec_quoted = exec_quoted,
        user_line = user_line.trim_end(),
    )
}

/// Quote a path for systemd unit files if it contains spaces or special characters.
/// Returns the path unquoted if no spaces, or quoted with double-quotes if spaces present.
fn quote_systemd_path(path: &str) -> String {
    if path.contains(' ') || path.contains('\t') || path.contains('"') {
        // Escape internal double quotes and wrap in double quotes
        format!("\"{}\"", path.replace('"', "\\\""))
    } else {
        path.to_string()
    }
}

// ============================================================================
// Linux systemd service management (cfg-gated)
// ============================================================================

/// Linux service status information.
#[derive(Debug, Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct ServiceStatus {
    /// Active state: "active", "inactive", "failed", etc.
    pub active_state: String,
    /// Sub-state: "running", "dead", "exited", etc.
    pub sub_state: Option<String>,
    /// Full `systemctl status` output for debugging.
    pub raw: String,
}

/// Check if a Linux systemd service is running.
///
/// Returns true if `systemctl is-active` returns "active".
#[cfg(target_os = "linux")]
pub async fn is_linux_service_running(service_name: &str) -> Result<bool> {
    debug!(
        "[PHASE: installation] [STEP: service] is_linux_service_running entered (service_name={})",
        service_name
    );

    // --no-pager prevents blocking on interactive pager
    let args = vec![
        "is-active".to_string(),
        "--no-pager".to_string(),
        service_name.to_string(),
    ];
    let result = run_cmd_with_timeout("systemctl", &args, Duration::from_secs(15), "systemctl_is_active").await;

    match result {
        Ok(out) => {
            let active = out.stdout.trim().eq_ignore_ascii_case("active");
            debug!(
                "[PHASE: installation] [STEP: service] is_linux_service_running exit (running={}, stdout={})",
                active,
                out.stdout.trim()
            );
            Ok(active)
        }
        Err(e) => {
            debug!(
                "[PHASE: installation] [STEP: service] is_linux_service_running exit (running=false, error={})",
                e
            );
            Ok(false)
        }
    }
}

/// Get detailed status of a Linux systemd service.
#[cfg(target_os = "linux")]
pub async fn get_linux_service_status(service_name: &str) -> Result<ServiceStatus> {
    debug!(
        "[PHASE: installation] [STEP: service] get_linux_service_status entered (service_name={})",
        service_name
    );

    // Get ActiveState and SubState
    let show_args = vec![
        "show".to_string(),
        service_name.to_string(),
        "-p".to_string(),
        "ActiveState".to_string(),
        "-p".to_string(),
        "SubState".to_string(),
        "--no-pager".to_string(),
    ];
    let show_out = run_cmd_with_timeout("systemctl", &show_args, Duration::from_secs(15), "systemctl_show").await?;

    let mut active_state = String::new();
    let mut sub_state: Option<String> = None;

    for line in show_out.stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "ActiveState" => active_state = value.trim().to_string(),
                "SubState" => sub_state = Some(value.trim().to_string()),
                _ => {}
            }
        }
    }

    // Get full status for raw output (--lines=50 limits journal output, --no-pager prevents blocking)
    let status_args = vec![
        "status".to_string(),
        service_name.to_string(),
        "--no-pager".to_string(),
        "--lines=50".to_string(),
    ];
    let status_out = run_cmd_with_timeout("systemctl", &status_args, Duration::from_secs(15), "systemctl_status").await;
    let raw = status_out.map(|o| o.stdout).unwrap_or_default();

    debug!(
        "[PHASE: installation] [STEP: service] get_linux_service_status exit (active_state={}, sub_state={:?})",
        active_state, sub_state
    );

    Ok(ServiceStatus {
        active_state,
        sub_state,
        raw,
    })
}

/// Install and start a Linux systemd service.
///
/// This function:
/// 1. Generates a systemd unit file
/// 2. Writes it to /etc/systemd/system/{service_name}.service
/// 3. Runs systemctl daemon-reload, enable, and restart
/// 4. Verifies the service is running
///
/// Requires root or passwordless sudo.
#[cfg(target_os = "linux")]
pub async fn install_and_start_linux_service(
    service_name: &str,
    exec_path: &Path,
    working_dir: &Path,
    user: Option<&str>,
) -> Result<()> {
    use crate::installation::linux::{is_running_as_root, require_root_or_passwordless_sudo};

    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: service] install_and_start_linux_service entered (service_name={}, exec_path={:?}, working_dir={:?}, user={:?})",
        service_name, exec_path, working_dir, user
    );

    // Check privileges first
    require_root_or_passwordless_sudo().await?;

    // Generate unit file content
    let unit_content = build_systemd_unit_text(service_name, exec_path, working_dir, user);
    let unit_path = format!("/etc/systemd/system/{}.service", service_name);

    // Write unit file (as root or via sudo)
    if is_running_as_root() {
        tokio::fs::write(&unit_path, &unit_content)
            .await
            .with_context(|| format!("Failed to write systemd unit file: {}", unit_path))?;
    } else {
        // Write via sudo using tee
        write_file_via_sudo(&unit_path, &unit_content).await?;
    }

    info!(
        "[PHASE: installation] [STEP: service] Wrote systemd unit file: {}",
        unit_path
    );

    // Run systemctl commands
    run_systemctl_cmd(&["daemon-reload"], "daemon_reload").await?;
    run_systemctl_cmd(&["enable", service_name], "enable").await?;
    run_systemctl_cmd(&["restart", service_name], "restart").await?;

    // Verify service is running
    let running = is_linux_service_running(service_name).await?;
    if !running {
        // Get status for better error message
        let status = get_linux_service_status(service_name).await.ok();
        let status_info = status
            .map(|s| format!("active_state={}, sub_state={:?}", s.active_state, s.sub_state))
            .unwrap_or_else(|| "unknown".to_string());
        anyhow::bail!(
            "Service '{}' is not running after start. Status: {}",
            service_name,
            status_info
        );
    }

    info!(
        "[PHASE: installation] [STEP: service] install_and_start_linux_service exit ok (service_name={}, duration_ms={})",
        service_name,
        started.elapsed().as_millis()
    );

    Ok(())
}

/// Run a systemctl command, using sudo -n if not root.
/// Always includes --no-pager to prevent blocking on interactive pager.
#[cfg(target_os = "linux")]
async fn run_systemctl_cmd(args: &[&str], operation: &str) -> Result<()> {
    use crate::installation::linux::is_running_as_root;

    // Build args with --no-pager to prevent blocking
    let mut base_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    base_args.push("--no-pager".to_string());

    let (program, final_args) = if is_running_as_root() {
        ("systemctl", base_args)
    } else {
        // Prepend sudo -n systemctl
        let mut v = vec!["-n".to_string(), "systemctl".to_string()];
        v.extend(base_args);
        ("sudo", v)
    };

    let out = run_cmd_with_timeout(program, &final_args, Duration::from_secs(30), operation).await?;

    if out.exit_code != Some(0) {
        anyhow::bail!(
            "systemctl {} failed (exit_code={:?}): {}",
            operation,
            out.exit_code,
            out.stderr
        );
    }

    Ok(())
}

/// Write a file via sudo tee (for non-root users with passwordless sudo).
///
/// Uses `sudo -n tee -- <path>` with stdin write (no shell string building).
/// After writing, sets permissions to 0644 for systemd unit files.
/// Paths with spaces are handled correctly (passed as separate args, not concatenated).
#[cfg(target_os = "linux")]
async fn write_file_via_sudo(path: &str, content: &str) -> Result<()> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    debug!(
        "[PHASE: installation] [STEP: service] write_file_via_sudo entered (path={})",
        path
    );

    // Use -- to prevent path from being interpreted as an option (handles paths starting with -)
    // Path is passed as a separate argument, not concatenated, so spaces are handled correctly
    let mut child = Command::new("sudo")
        .arg("-n")
        .arg("tee")
        .arg("--")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn sudo tee. Ensure passwordless sudo is configured.")?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes()).await?;
        stdin.flush().await?;
        // Explicitly drop to close stdin and signal EOF
        drop(stdin);
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "sudo tee failed (path={}). Ensure passwordless sudo is configured. Error: {}",
            path,
            stderr.trim()
        );
    }

    // Set proper permissions for systemd unit file (0644)
    let chmod_output = Command::new("sudo")
        .arg("-n")
        .arg("chmod")
        .arg("0644")
        .arg("--")
        .arg(path)
        .output()
        .await
        .context("Failed to run sudo chmod")?;

    if !chmod_output.status.success() {
        let stderr = String::from_utf8_lossy(&chmod_output.stderr);
        warn!(
            "[PHASE: installation] [STEP: service] chmod 0644 failed (path={}): {}",
            path,
            stderr.trim()
        );
        // Non-fatal: systemd may still work with different permissions
    }

    debug!(
        "[PHASE: installation] [STEP: service] write_file_via_sudo exit ok (path={})",
        path
    );

    Ok(())
}

/// Write a Windows service install/start placeholder script.
///
/// This is used when runtime/service wiring is not yet available at build-time,
/// but we still want deterministic artifacts for support and validation.
pub async fn write_windows_service_install_script(
    artifacts_dir: &Path,
    service_name: &str,
    exe_path: &Path,
) -> Result<PathBuf> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: service] write_windows_service_install_script entered (service_name={}, exe_path={:?})",
        service_name, exe_path
    );

    tokio::fs::create_dir_all(artifacts_dir).await?;
    let path = artifacts_dir.join("install_windows_service.ps1");
    let exe_str = exe_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid exe path"))?;

    let content = format!(
        r#"# CADalytix Windows Service Install Placeholder (Phase 5)
# This script is generated by the unified installer for support/verification.
# It is NOT executed automatically unless service wiring is enabled.

$ErrorActionPreference = "Stop"

$serviceName = "{service_name}"
$exePath = "{exe_str}"

Write-Host "Installing service $serviceName from $exePath"

# Create / update service
sc.exe stop $serviceName | Out-Null
sc.exe delete $serviceName | Out-Null

sc.exe create $serviceName binPath= "`"$exePath`"" start= auto DisplayName= "CADalytix" | Out-Null

# Start service
sc.exe start $serviceName | Out-Null

Write-Host "Service started."
"#,
        service_name = service_name,
        exe_str = exe_str.replace('"', "`\"")
    );

    tokio::fs::write(&path, content)
        .await
        .with_context(|| format!("Failed to write {:?}", path))?;

    debug!(
        "[PHASE: installation] [STEP: service] write_windows_service_install_script exit (path={:?}, duration_ms={})",
        path,
        started.elapsed().as_millis()
    );
    Ok(path)
}

/// Write Linux systemd service placeholder unit.
pub async fn write_linux_systemd_service_unit(
    artifacts_dir: &Path,
    service_name: &str,
    exec_path: &Path,
) -> Result<PathBuf> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: service] write_linux_systemd_service_unit entered (service_name={}, exec_path={:?})",
        service_name, exec_path
    );

    tokio::fs::create_dir_all(artifacts_dir).await?;
    let path = artifacts_dir.join("cadalytix.service");
    let exec_str = exec_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid exec path"))?;

    let content = format!(
        r#"# CADalytix systemd service placeholder (Phase 5)
[Unit]
Description=CADalytix
After=network.target

[Service]
Type=simple
ExecStart={exec_str}
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#,
        exec_str = exec_str
    );

    tokio::fs::write(&path, content)
        .await
        .with_context(|| format!("Failed to write {:?}", path))?;

    debug!(
        "[PHASE: installation] [STEP: service] write_linux_systemd_service_unit exit (path={:?}, duration_ms={})",
        path,
        started.elapsed().as_millis()
    );
    Ok(path)
}

/// Install/start and verify a Windows service using `sc.exe`.
///
/// This requires elevated permissions. Caller should handle/report failures cleanly.
#[cfg(windows)]
pub async fn install_and_start_windows_service(service_name: &str, exe_path: &Path) -> Result<()> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: service] install_and_start_windows_service entered (service_name={}, exe_path={:?})",
        service_name, exe_path
    );

    let exe_str = exe_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid exe path"))?;

    // Best-effort stop/delete (ignore failures if service doesn't exist).
    let _ = run_cmd_with_timeout(
        "sc.exe",
        &["stop".to_string(), service_name.to_string()],
        Duration::from_secs(20),
        "sc_stop",
    )
    .await;
    let _ = run_cmd_with_timeout(
        "sc.exe",
        &["delete".to_string(), service_name.to_string()],
        Duration::from_secs(20),
        "sc_delete",
    )
    .await;

    // Create service (note sc.exe requires a space after '=' so we pass `binPath=` then a value arg).
    let create_args = vec![
        "create".to_string(),
        service_name.to_string(),
        "binPath=".to_string(),
        format!("\"{}\"", exe_str),
        "start=".to_string(),
        "auto".to_string(),
        "DisplayName=".to_string(),
        "\"CADalytix\"".to_string(),
    ];
    let out =
        run_cmd_with_timeout("sc.exe", &create_args, Duration::from_secs(30), "sc_create").await?;
    if out.exit_code != Some(0) {
        warn!(
            "[PHASE: installation] [STEP: service] sc.exe create failed (exit_code={:?}) stderr={}",
            out.exit_code, out.stderr
        );
        anyhow::bail!(
            "Windows service creation failed (exit_code={:?})",
            out.exit_code
        );
    }

    let out = run_cmd_with_timeout(
        "sc.exe",
        &["start".to_string(), service_name.to_string()],
        Duration::from_secs(30),
        "sc_start",
    )
    .await?;
    if out.exit_code != Some(0) {
        warn!(
            "[PHASE: installation] [STEP: service] sc.exe start failed (exit_code={:?}) stderr={}",
            out.exit_code, out.stderr
        );
        anyhow::bail!(
            "Windows service start failed (exit_code={:?})",
            out.exit_code
        );
    }

    let running = is_windows_service_running(service_name).await?;
    if !running {
        anyhow::bail!("Windows service is not running after start");
    }

    info!(
        "[PHASE: installation] [STEP: service] install_and_start_windows_service exit ok (service_name={}, duration_ms={})",
        service_name,
        started.elapsed().as_millis()
    );
    Ok(())
}

#[cfg(windows)]
pub async fn is_windows_service_running(service_name: &str) -> Result<bool> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: service] is_windows_service_running entered (service_name={})",
        service_name
    );

    let out = run_cmd_with_timeout(
        "sc.exe",
        &["query".to_string(), service_name.to_string()],
        Duration::from_secs(20),
        "sc_query",
    )
    .await?;

    // `sc query` returns non-zero if service doesn't exist.
    if out.exit_code != Some(0) {
        warn!(
            "[PHASE: installation] [STEP: service] sc.exe query failed (exit_code={:?}) stderr={}",
            out.exit_code, out.stderr
        );
        return Ok(false);
    }

    let running = out.stdout.to_ascii_uppercase().contains("RUNNING");
    debug!(
        "[PHASE: installation] [STEP: service] is_windows_service_running exit (running={}, duration_ms={})",
        running,
        started.elapsed().as_millis()
    );
    Ok(running)
}

// ============================================================================
// Unit tests (cross-platform)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn build_systemd_unit_text_basic() {
        let exec_path = PathBuf::from("/opt/cadalytix/bin/cadalytix-server");
        let working_dir = PathBuf::from("/opt/cadalytix");
        let unit = build_systemd_unit_text("cadalytix", &exec_path, &working_dir, None);

        assert!(unit.contains("[Unit]"));
        assert!(unit.contains("Description=CADalytix Service (cadalytix)"));
        assert!(unit.contains("After=network.target"));
        assert!(unit.contains("[Service]"));
        assert!(unit.contains("WorkingDirectory=/opt/cadalytix"));
        assert!(unit.contains("ExecStart=/opt/cadalytix/bin/cadalytix-server"));
        assert!(unit.contains("Restart=always"));
        assert!(unit.contains("RestartSec=5"));
        assert!(unit.contains("[Install]"));
        assert!(unit.contains("WantedBy=multi-user.target"));
        // No User= line when user is None
        assert!(!unit.contains("User="));
    }

    #[test]
    fn build_systemd_unit_text_with_user() {
        let exec_path = PathBuf::from("/usr/local/bin/myservice");
        let working_dir = PathBuf::from("/var/lib/myservice");
        let unit = build_systemd_unit_text("myservice", &exec_path, &working_dir, Some("appuser"));

        assert!(unit.contains("Description=CADalytix Service (myservice)"));
        assert!(unit.contains("WorkingDirectory=/var/lib/myservice"));
        assert!(unit.contains("ExecStart=/usr/local/bin/myservice"));
        assert!(unit.contains("User=appuser"));
    }

    #[test]
    fn build_systemd_unit_text_paths_with_spaces_are_quoted() {
        let exec_path = PathBuf::from("/opt/my app/bin/server");
        let working_dir = PathBuf::from("/opt/my app");
        let unit = build_systemd_unit_text("test", &exec_path, &working_dir, None);

        // Paths with spaces should be quoted for systemd
        assert!(
            unit.contains("ExecStart=\"/opt/my app/bin/server\""),
            "ExecStart should be quoted. Got:\n{}",
            unit
        );
        assert!(
            unit.contains("WorkingDirectory=\"/opt/my app\""),
            "WorkingDirectory should be quoted. Got:\n{}",
            unit
        );
    }

    #[test]
    fn build_systemd_unit_text_paths_without_spaces_not_quoted() {
        let exec_path = PathBuf::from("/opt/cadalytix/bin/server");
        let working_dir = PathBuf::from("/opt/cadalytix");
        let unit = build_systemd_unit_text("test", &exec_path, &working_dir, None);

        // Paths without spaces should NOT be quoted
        assert!(
            unit.contains("ExecStart=/opt/cadalytix/bin/server"),
            "ExecStart should not be quoted. Got:\n{}",
            unit
        );
        assert!(
            !unit.contains("ExecStart=\"/opt/cadalytix/bin/server\""),
            "ExecStart should not have quotes. Got:\n{}",
            unit
        );
    }

    #[test]
    fn quote_systemd_path_handles_embedded_quotes() {
        // Test the quoting helper directly
        let path_with_quote = "/opt/my\"app/bin";
        let quoted = super::quote_systemd_path(path_with_quote);
        // Should escape the internal quote and wrap in quotes
        assert_eq!(quoted, "\"/opt/my\\\"app/bin\"");
    }

    #[test]
    fn build_systemd_unit_text_has_required_sections() {
        let exec_path = PathBuf::from("/bin/test");
        let working_dir = PathBuf::from("/tmp");
        let unit = build_systemd_unit_text("test", &exec_path, &working_dir, None);

        // Verify all required sections are present
        let sections: Vec<&str> = unit
            .lines()
            .filter(|l| l.starts_with('[') && l.ends_with(']'))
            .collect();
        assert_eq!(sections, vec!["[Unit]", "[Service]", "[Install]"]);
    }
}
