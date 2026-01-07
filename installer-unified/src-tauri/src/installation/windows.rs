// Windows-specific installation
//
// Phase 5: Installation Logic (Windows)
// This is the beginning of the Windows deployment/preflight logic, following the plan.

use anyhow::Result;
use log::{debug, info, warn};
use std::time::Instant;
use tokio::time::Duration;

use crate::installation::run_cmd_with_timeout;

/// Best-effort check for .NET runtime presence (per plan: .NET 8.0 runtime required for legacy components).
///
/// Note: The unified installer itself is native (Tauri/Rust); this check is retained for
/// prerequisites parity and future hybrid scenarios.
pub async fn check_dotnet_runtime_8_installed() -> Result<bool> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: preflight_windows] check_dotnet_runtime_8_installed entered"
    );

    let out = match run_cmd_with_timeout(
        "dotnet",
        &["--list-runtimes".to_string()],
        Duration::from_secs(10),
        "dotnet_list_runtimes",
    )
    .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!(
                "[PHASE: installation] [STEP: preflight_windows] dotnet runtime check failed to execute: {}",
                e
            );
            return Ok(false);
        }
    };

    if out.exit_code != Some(0) {
        warn!(
            "[PHASE: installation] [STEP: preflight_windows] dotnet runtime check returned non-zero exit_code={:?}",
            out.exit_code
        );
        return Ok(false);
    }

    // Look for "Microsoft.NETCore.App 8."
    let installed = out
        .stdout
        .lines()
        .any(|l| l.to_ascii_lowercase().contains("microsoft.netcore.app 8."));

    info!(
        "[PHASE: installation] [STEP: preflight_windows] check_dotnet_runtime_8_installed exit (installed={}, duration_ms={})",
        installed,
        started.elapsed().as_millis()
    );
    Ok(installed)
}

/// Best-effort free-space check for a Windows drive using PowerShell (returns bytes).
pub async fn get_free_space_bytes_ps(drive_letter: &str) -> Result<u64> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: preflight_windows] get_free_space_bytes_ps entered (drive={})",
        drive_letter
    );

    let drive = drive_letter
        .trim()
        .trim_end_matches(':')
        .to_ascii_uppercase();
    if drive.len() != 1 || !drive.chars().all(|c| c.is_ascii_alphabetic()) {
        anyhow::bail!("Invalid drive letter");
    }

    // Use Get-PSDrive for a simple numeric output (no formatting).
    let script = format!("(Get-PSDrive -Name {}).Free", drive);
    let out = run_cmd_with_timeout(
        "powershell",
        &["-NoProfile".to_string(), "-Command".to_string(), script],
        Duration::from_secs(10),
        "get_free_space",
    )
    .await?;

    if out.exit_code != Some(0) {
        anyhow::bail!("Failed to query free space (exit_code={:?})", out.exit_code);
    }

    let trimmed = out.stdout.trim();
    let bytes = trimmed
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("Unable to parse free space bytes '{}': {}", trimmed, e))?;

    info!(
        "[PHASE: installation] [STEP: preflight_windows] get_free_space_bytes_ps exit (drive={}, bytes={}, duration_ms={})",
        drive,
        bytes,
        started.elapsed().as_millis()
    );
    Ok(bytes)
}
