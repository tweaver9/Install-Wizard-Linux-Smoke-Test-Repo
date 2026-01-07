//! Disk / filesystem utilities (no partitioning).
//!
//! Non-negotiable: we only *detect* mounts/volumes and free space; we never
//! modify partitions or create volumes.

use anyhow::Result;
use log::{debug, info};
use std::path::Path;
use std::time::Instant;

/// Best-effort free-space check for a given filesystem path (returns bytes).
///
/// - Windows: resolves a drive letter and delegates to PowerShell `Get-PSDrive`.
/// - Linux: uses `df -Pk <path>` and parses available KB.
///
/// Retries transient failures via the shared command runner.
pub async fn get_free_space_bytes_for_path(path: &str) -> Result<u64> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: free_space] get_free_space_bytes_for_path entered (path={})",
        path
    );

    let p = Path::new(path);
    let bytes = if cfg!(windows) {
        get_free_space_bytes_windows(path).await?
    } else if cfg!(target_os = "linux") {
        get_free_space_bytes_linux(p).await?
    } else {
        anyhow::bail!("Unsupported OS for free space detection");
    };

    info!(
        "[PHASE: installation] [STEP: free_space] get_free_space_bytes_for_path exit (bytes={}, duration_ms={})",
        bytes,
        started.elapsed().as_millis()
    );
    Ok(bytes)
}

#[cfg(windows)]
async fn get_free_space_bytes_windows(path: &str) -> Result<u64> {
    let drive = extract_windows_drive_letter(path)
        .ok_or_else(|| anyhow::anyhow!("Unable to determine drive letter for path"))?;
    crate::installation::windows::get_free_space_bytes_ps(&drive).await
}

#[cfg(not(windows))]
async fn get_free_space_bytes_windows(_path: &str) -> Result<u64> {
    anyhow::bail!("Windows free space detection not available on this OS")
}

fn extract_windows_drive_letter(path: &str) -> Option<String> {
    // Accept forms like:
    // - C:\...
    // - C:
    // - c:\...
    let trimmed = path.trim();
    let mut chars = trimmed.chars();
    let first = chars.next()?;
    let second = chars.next()?;
    if second != ':' || !first.is_ascii_alphabetic() {
        return None;
    }
    Some(first.to_ascii_uppercase().to_string())
}

#[cfg(target_os = "linux")]
async fn get_free_space_bytes_linux(path: &Path) -> Result<u64> {
    use anyhow::Context;
    use tokio::time::Duration;

    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid path"))?
        .to_string();

    // `df -Pk` prints in KB, POSIX format. We parse the "Available" column.
    let out = crate::installation::run_cmd_with_timeout(
        "df",
        &vec!["-Pk".to_string(), path_str],
        Duration::from_secs(10),
        "get_free_space_linux_df",
    )
    .await?;

    if out.exit_code != Some(0) {
        anyhow::bail!("Failed to query free space (exit_code={:?})", out.exit_code);
    }

    // Expect:
    // Filesystem 1024-blocks Used Available Capacity Mounted on
    // ...
    let mut lines = out.stdout.lines();
    let _header = lines.next();
    let data = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("df output missing data row"))?;
    let cols: Vec<&str> = data.split_whitespace().collect();
    if cols.len() < 4 {
        anyhow::bail!("df output parse error");
    }
    let avail_kb: u64 = cols[3]
        .parse()
        .with_context(|| format!("Unable to parse df available KB '{}'", cols[3]))?;
    Ok(avail_kb.saturating_mul(1024))
}

#[cfg(not(target_os = "linux"))]
async fn get_free_space_bytes_linux(_path: &Path) -> Result<u64> {
    anyhow::bail!("Linux free space detection not available on this OS")
}
