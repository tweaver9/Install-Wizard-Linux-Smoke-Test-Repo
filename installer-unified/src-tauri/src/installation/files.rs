//! File deployment helpers (Phase 5).
//!
//! Goals:
//! - Async I/O only (tokio)
//! - Retry transient file lock errors (Windows AV/indexers, etc.)
//! - Timeout all operations (plan: 60s file ops default)
//! - Preserve permissions on Unix best-effort
//! - Never fail silently (log with context)

use anyhow::{Context, Result};
use log::{debug, warn};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};

/// Recursively collect all regular files under `root`.
///
/// Returns absolute paths.
pub async fn collect_files_recursive(root: &Path) -> Result<Vec<PathBuf>> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: files] collect_files_recursive entered (root={:?})",
        root
    );

    let mut out: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&dir)
            .await
            .with_context(|| format!("read_dir failed: {:?}", dir))?;
        while let Some(ent) = rd.next_entry().await? {
            let p = ent.path();
            let meta = ent.metadata().await?;
            if meta.is_dir() {
                stack.push(p);
            } else if meta.is_file() {
                out.push(p);
            }
        }
    }

    debug!(
        "[PHASE: installation] [STEP: files] collect_files_recursive exit (files={}, duration_ms={})",
        out.len(),
        started.elapsed().as_millis()
    );
    Ok(out)
}

fn is_transient_fs_error(e: &anyhow::Error) -> bool {
    let msg = e.to_string().to_ascii_lowercase();
    msg.contains("used by another process")
        || msg.contains("in use")
        || msg.contains("access is denied")
        || msg.contains("permission denied")
        || msg.contains("resource busy")
        || msg.contains("temporarily")
        || msg.contains("temporary")
        || msg.contains("timed out")
        || msg.contains("timeout")
}

/// Copy one file with retries + timeout.
///
/// Caller must create parent directory.
pub async fn copy_file_with_retries(src: &Path, dst: &Path, label: &str) -> Result<u64> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: files] copy_file_with_retries entered (label={}, src={:?}, dst={:?})",
        label, src, dst
    );

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        let res = timeout(Duration::from_secs(60), tokio::fs::copy(src, dst)).await;
        match res {
            Ok(Ok(n)) => {
                // Best-effort permissions preservation.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = tokio::fs::metadata(src).await {
                        let mode = meta.permissions().mode();
                        let _ =
                            tokio::fs::set_permissions(dst, std::fs::Permissions::from_mode(mode))
                                .await;
                    }
                }

                debug!(
                    "[PHASE: installation] [STEP: files] copy_file_with_retries exit ok (label={}, bytes={}, attempt={}, duration_ms={})",
                    label,
                    n,
                    attempt,
                    started.elapsed().as_millis()
                );
                return Ok(n);
            }
            Ok(Err(e)) => {
                let err = anyhow::Error::new(e).context("copy failed");
                let transient = is_transient_fs_error(&err);
                warn!(
                    "[PHASE: installation] [STEP: files] copy failed (label={}, attempt={}, transient={}, src={:?}, dst={:?}, err={})",
                    label,
                    attempt,
                    transient,
                    src,
                    dst,
                    err
                );
                last_err = Some(err);
                if !transient {
                    break;
                }
            }
            Err(_) => {
                let err = anyhow::anyhow!("copy timed out after 60s");
                warn!(
                    "[PHASE: installation] [STEP: files] copy timeout (label={}, attempt={}, src={:?}, dst={:?})",
                    label, attempt, src, dst
                );
                last_err = Some(err);
            }
        }

        let backoff_ms = 200_u64.saturating_mul(1_u64 << ((attempt - 1) as u32));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("copy failed")))
}

/// Copy one file with retries + timeout, returning `(bytes_written, sha256_hex)`.
///
/// - Hash is computed over the bytes copied (source contents).
/// - Caller must create parent directory.
pub async fn copy_file_with_retries_and_sha256(
    src: &Path,
    dst: &Path,
    label: &str,
) -> Result<(u64, String)> {
    let started = Instant::now();
    debug!(
        "[PHASE: installation] [STEP: files] copy_file_with_retries_and_sha256 entered (label={}, src={:?}, dst={:?})",
        label, src, dst
    );

    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        let timeout_dur = match tokio::fs::metadata(src).await {
            Ok(m) => {
                // Dynamic timeout: base 60s + 1s per MiB, capped at 10 minutes.
                let mib = (m.len() / (1024 * 1024)).min(10_000);
                let secs = (60_u64).saturating_add(mib).min(600);
                Duration::from_secs(secs)
            }
            Err(_) => Duration::from_secs(60),
        };

        let res = timeout(timeout_dur, copy_file_once_and_sha256(src, dst)).await;
        match res {
            Ok(Ok((n, sha))) => {
                debug!(
                    "[PHASE: installation] [STEP: files] copy_file_with_retries_and_sha256 exit ok (label={}, bytes={}, sha256={}, attempt={}, duration_ms={})",
                    label,
                    n,
                    sha,
                    attempt,
                    started.elapsed().as_millis()
                );
                return Ok((n, sha));
            }
            Ok(Err(e)) => {
                let transient = is_transient_fs_error(&e);
                warn!(
                    "[PHASE: installation] [STEP: files] copy+sha failed (label={}, attempt={}, transient={}, src={:?}, dst={:?}, err={})",
                    label,
                    attempt,
                    transient,
                    src,
                    dst,
                    e
                );
                last_err = Some(e);
                if !transient {
                    break;
                }
            }
            Err(_) => {
                let err = anyhow::anyhow!(
                    "copy+sha timed out (timeout_ms={})",
                    timeout_dur.as_millis()
                );
                warn!(
                    "[PHASE: installation] [STEP: files] copy+sha timeout (label={}, attempt={}, src={:?}, dst={:?}, timeout_ms={})",
                    label,
                    attempt,
                    src,
                    dst,
                    timeout_dur.as_millis()
                );
                last_err = Some(err);
            }
        }

        let backoff_ms = 200_u64.saturating_mul(1_u64 << ((attempt - 1) as u32));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("copy+sha failed")))
}

async fn copy_file_once_and_sha256(src: &Path, dst: &Path) -> Result<(u64, String)> {
    let mut src_f = tokio::fs::File::open(src)
        .await
        .with_context(|| format!("open src failed: {:?}", src))?;
    let mut dst_f = tokio::fs::File::create(dst)
        .await
        .with_context(|| format!("create dst failed: {:?}", dst))?;

    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;

    loop {
        let n = src_f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        dst_f.write_all(&buf[..n]).await?;
        total = total.saturating_add(n as u64);
    }
    dst_f.flush().await?;

    // Best-effort permissions preservation.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = tokio::fs::metadata(src).await {
            let mode = meta.permissions().mode();
            let _ = tokio::fs::set_permissions(dst, std::fs::Permissions::from_mode(mode)).await;
        }
    }

    let digest = hasher.finalize();
    let sha256 = digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    Ok((total, sha256))
}
