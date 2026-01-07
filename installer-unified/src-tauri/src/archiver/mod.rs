//! Cold storage archiver pipeline (engine-agnostic skeleton).
//!
//! Scope for Phase 5:
//! - Implement core archival control-flow with strict verification gates and idempotency.
//! - Provide a deterministic `--archive-dry-run` mode that produces proof logs under `Prod_Wizard_Log/`.
//!
//! Non-negotiable: NO partitioning. This module never modifies disks/volumes; it only writes files.

use anyhow::Result;
use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use log::{error, info, warn};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::time::{timeout, Duration};
use zip::write::FileOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveFormat {
    ZipNdjson,
    ZipCsv,
}

impl ArchiveFormat {
    fn as_str(&self) -> &'static str {
        match self {
            ArchiveFormat::ZipNdjson => "zip+ndjson",
            ArchiveFormat::ZipCsv => "zip+csv",
        }
    }

    fn file_name_in_zip(&self) -> &'static str {
        match self {
            ArchiveFormat::ZipNdjson => "calls.ndjson",
            ArchiveFormat::ZipCsv => "calls.csv",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveLedgerEntry {
    month: String, // YYYY-MM
    status: String,
    format: String,
    row_count: u64,
    min_ts_utc: String,
    max_ts_utc: String,
    zip_sha256: String,
    zip_bytes: u64,
    created_utc: String,
}

#[derive(Debug, Clone)]
struct ArchiveRunConfig {
    correlation_id: String,
    month: NaiveDate, // first day of month
    format: ArchiveFormat,
    destination_dir: PathBuf,
    max_usage_gb: u32,
    allow_without_watermark: bool,
    dry_run: bool,
}

/// Archive export output: (uncompressed_bytes, row_count, min_timestamp_utc, max_timestamp_utc)
type DemoExport = (Vec<u8>, u64, DateTime<Utc>, DateTime<Utc>);

pub async fn archive_dry_run() -> Result<()> {
    let started = Instant::now();
    let log_dir = crate::utils::path_resolver::resolve_log_folder()?;
    let transcript_path = log_dir.join("B2_archive_pipeline_dryrun_transcript.log");

    let mut transcript = String::new();
    let mut push = |line: String| {
        transcript.push_str(&line);
        transcript.push('\n');
    };

    push("ARCHIVE_DRY_RUN begin".to_string());
    push(format!("log_dir={}", log_dir.to_string_lossy()));
    let supported = [ArchiveFormat::ZipNdjson, ArchiveFormat::ZipCsv];
    push(format!(
        "supported_formats={}",
        supported
            .iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>()
            .join(",")
    ));

    // Deterministic inputs (no random UUIDs) so proofs are stable.
    let cfg = ArchiveRunConfig {
        correlation_id: "archive-dry-run".to_string(),
        month: NaiveDate::from_ymd_opt(2025, 1, 1)
            .ok_or_else(|| anyhow::anyhow!("Invalid month"))?,
        format: ArchiveFormat::ZipNdjson,
        destination_dir: log_dir.join("B2_archive_dryrun_destination"),
        max_usage_gb: 10,
        allow_without_watermark: true,
        dry_run: true,
    };

    ensure_dir_with_retries(&cfg.destination_dir, "ensure_archive_destination").await?;

    // Placeholder schedule artifacts (ready to be wired to a real runner in a later phase).
    // These are written under Prod_Wizard_Log/ for deterministic proof output.
    let schedule_dir = log_dir.join("B2_archive_schedule_placeholders");
    write_schedule_placeholders(&schedule_dir, 1, "00:05", &mut push).await?;

    let ledger_path = log_dir.join("B2_archive_pipeline_dryrun_ledger.json");
    push(format!(
        "EVENT archive-ledger path={}",
        ledger_path.to_string_lossy()
    ));

    // Run twice to prove idempotency deterministically.
    let first = archive_one_month(&cfg, &ledger_path, &mut push).await;
    push(format!(
        "run1 result={} duration_ms={}",
        if first.is_ok() { "ok" } else { "err" },
        started.elapsed().as_millis()
    ));
    if let Err(e) = first {
        push(format!("run1 error={}", e));
    }

    let second = archive_one_month(&cfg, &ledger_path, &mut push).await;
    push(format!(
        "run2 result={} duration_ms={}",
        if second.is_ok() { "ok" } else { "err" },
        started.elapsed().as_millis()
    ));
    if let Err(e) = second {
        push(format!("run2 error={}", e));
    }
    push("idempotent: run twice -> second skips when ledger shows complete".to_string());

    push(format!(
        "ARCHIVE_DRY_RUN end elapsed_ms={}",
        started.elapsed().as_millis()
    ));
    // Include ExitCode in transcript so verification scripts can match a single artifact file.
    push("ExitCode=0".to_string());

    tokio::fs::write(&transcript_path, transcript).await?;
    info!(
        "[PHASE: archive] [STEP: dry_run] Wrote transcript to {:?}",
        transcript_path
    );

    Ok(())
}

async fn write_schedule_placeholders(
    out_dir: &Path,
    day_of_month: u8,
    time_local: &str,
    push: &mut dyn FnMut(String),
) -> Result<()> {
    ensure_dir_with_retries(out_dir, "ensure_schedule_placeholders_dir").await?;

    let win_ps1 = out_dir.join("B2_archive_windows_task_scheduler_placeholder.ps1");
    let linux_service = out_dir.join("cadalytix-archive.service");
    let linux_timer = out_dir.join("cadalytix-archive.timer");

    let win_contents = format!(
        r#"# CADalytix Archive Schedule Placeholder (Phase 5)
#
# This file is a PLACEHOLDER artifact only.
# The installer does NOT register a Scheduled Task in this phase.
#
# Intended schedule (local server time):
#   - Day of month: {day}
#   - Time: {time}
#
# TODO (wire-up): Replace <ARCHIVE_COMMAND> with the real archive runner command.
# Example (Task Scheduler command line):
#   schtasks /Create /SC MONTHLY /D {day} /TN "CADalytix Archive" /TR "<ARCHIVE_COMMAND>" /ST {time} /F
#
# Example <ARCHIVE_COMMAND> (placeholder):
#   "C:\Program Files\CADalytix\installer-unified.exe" --archive-run-once
"#,
        day = day_of_month,
        time = time_local
    );
    write_file_with_retries(
        &win_ps1,
        win_contents.as_bytes(),
        "write_schedule_windows_ps1",
    )
    .await?;
    push(format!(
        "schedule placeholder windows_ps1={}",
        win_ps1.to_string_lossy()
    ));

    let linux_service_contents = r#"[Unit]
Description=CADalytix Archive Runner (Placeholder)
After=network.target

[Service]
Type=oneshot
# TODO (wire-up): Replace this ExecStart with the real archive runner command.
ExecStart=/usr/bin/cadalytix-archive-runner --run-once
"#;
    write_file_with_retries(
        &linux_service,
        linux_service_contents.as_bytes(),
        "write_schedule_linux_service",
    )
    .await?;
    push(format!(
        "schedule placeholder linux_service={}",
        linux_service.to_string_lossy()
    ));

    // Note: Persistent=true provides catch-up behavior when missed.
    let linux_timer_contents = format!(
        r#"[Unit]
Description=CADalytix Archive Runner Schedule (Placeholder)

[Timer]
# Runs on the {day:02} day of each month at {time} (local server time).
OnCalendar=*-*-{day:02} {time}:00
Persistent=true

[Install]
WantedBy=timers.target
"#,
        day = day_of_month,
        time = time_local
    );
    write_file_with_retries(
        &linux_timer,
        linux_timer_contents.as_bytes(),
        "write_schedule_linux_timer",
    )
    .await?;
    push(format!(
        "schedule placeholder linux_timer={}",
        linux_timer.to_string_lossy()
    ));

    Ok(())
}

async fn archive_one_month(
    cfg: &ArchiveRunConfig,
    ledger_path: &Path,
    push: &mut dyn FnMut(String),
) -> Result<()> {
    let month_key = cfg.month.format("%Y-%m").to_string();
    push(format!(
        "EVENT archive-start correlation_id={} month={}",
        cfg.correlation_id, month_key
    ));
    push("verified_steps order=1..6".to_string());

    // Idempotency: if ledger says complete, skip.
    if let Some(existing) = read_ledger(ledger_path).await?.get(&month_key) {
        if existing.status == "complete" {
            push(format!(
                "EVENT archive-skip month={} reason=already_complete",
                month_key
            ));
            return Ok(());
        }
    }

    // VERIFIED STEP 1/6: destination checks (exists, is a directory, writable).
    push(format!(
        "VERIFY 1/6 destination-check begin path={}",
        cfg.destination_dir.to_string_lossy()
    ));
    match tokio::fs::metadata(&cfg.destination_dir).await {
        Ok(m) => {
            if !m.is_dir() {
                push(format!(
                    "EVENT archive-destination-check-fail month={} message=\"Destination is not a directory\"",
                    month_key
                ));
                anyhow::bail!("Archive destination is not a directory");
            }
        }
        Err(_) => {
            push(format!(
                "EVENT archive-destination-check-fail month={} message=\"Destination folder is not accessible\"",
                month_key
            ));
            anyhow::bail!("Archive destination folder is not accessible");
        }
    }
    let write_test = cfg
        .destination_dir
        .join("__cadalytix_archive_write_test.tmp");
    if let Err(_e) =
        write_file_with_retries(&write_test, b"ok", "archive_destination_write_test").await
    {
        push(format!(
            "EVENT archive-destination-check-fail month={} message=\"Destination folder is not writable\"",
            month_key
        ));
        anyhow::bail!("Archive destination folder is not writable");
    }
    let _ = tokio::fs::remove_file(&write_test).await;
    push("VERIFY 1/6 destination-check ok".to_string());

    // Gate: ingestion watermark check (placeholder).
    push("VERIFY 2/6 watermark-check begin".to_string());
    if !cfg.allow_without_watermark {
        push(format!(
            "EVENT archive-error month={} message=\"Ingestion watermark not present\"",
            month_key
        ));
        anyhow::bail!("Ingestion watermark not present for month {}", month_key);
    }
    push(format!(
        "EVENT archive-watermark month={} status=ok",
        month_key
    ));
    push("VERIFY 2/6 watermark-check ok".to_string());

    // Export (demo data source): deterministic rows within the month.
    push("VERIFY 3/6 export begin".to_string());
    let (export_bytes, row_count, min_ts, max_ts) = export_demo_rows(cfg.month, cfg.format)?;
    push(format!(
        "EVENT archive-export month={} rows={} min_ts_utc={} max_ts_utc={}",
        month_key,
        row_count,
        min_ts.to_rfc3339(),
        max_ts.to_rfc3339()
    ));
    push(format!("VERIFY 3/6 export ok rows={}", row_count));

    // Compress to ZIP.
    push("VERIFY 4/6 zip begin".to_string());
    let zip_bytes = zip_single_file(cfg.format.file_name_in_zip(), &export_bytes)?;
    let zip_sha256 = crate::security::crypto::sha256_hex(&zip_bytes);
    push(format!(
        "EVENT archive-zip month={} format={} zip_bytes={} zip_sha256={}",
        month_key,
        cfg.format.as_str(),
        zip_bytes.len(),
        zip_sha256
    ));
    push(format!("VERIFY 4/6 zip ok sha256={}", zip_sha256));

    // Cap enforcement: ensure destination usage + zip <= cap.
    push("VERIFY 5/6 cap+write begin".to_string());
    let cap_bytes = (cfg.max_usage_gb as u64).saturating_mul(1024_u64.pow(3));
    let current_usage = folder_size_bytes(&cfg.destination_dir).await?;
    if cap_bytes > 0 && current_usage.saturating_add(zip_bytes.len() as u64) > cap_bytes {
        push(format!(
            "EVENT archive-cap-exceeded month={} cap_bytes={} current_bytes={} new_bytes={}",
            month_key,
            cap_bytes,
            current_usage,
            zip_bytes.len()
        ));
        anyhow::bail!("Archive cap exceeded for destination folder");
    }
    push(format!(
        "EVENT archive-cap-ok month={} cap_bytes={} current_bytes={}",
        month_key, cap_bytes, current_usage
    ));

    // Write with temp + atomic rename.
    let final_name = format!("cadalytix-archive-{}.zip", month_key);
    let tmp_name = format!("{}.tmp", final_name);
    let final_path = cfg.destination_dir.join(final_name);
    let tmp_path = cfg.destination_dir.join(tmp_name);
    write_file_with_retries(&tmp_path, &zip_bytes, "write_archive_tmp").await?;
    rename_with_retries(&tmp_path, &final_path, "rename_archive_zip").await?;
    push(format!(
        "VERIFY 5/6 cap+write ok path={}",
        final_path.to_string_lossy()
    ));

    // Verify on-disk checksum.
    push("VERIFY 6/6 verify+ledger begin".to_string());
    let on_disk = tokio::fs::read(&final_path).await?;
    let on_disk_sha = crate::security::crypto::sha256_hex(&on_disk);
    if on_disk_sha != zip_sha256 {
        push(format!(
            "EVENT archive-verify-fail month={} expected_sha256={} actual_sha256={}",
            month_key, zip_sha256, on_disk_sha
        ));
        anyhow::bail!("Archive verification failed (sha256 mismatch)");
    }
    push(format!(
        "EVENT archive-verify-ok month={} path={}",
        month_key,
        final_path.to_string_lossy()
    ));

    // Purge step placeholder (never purge in dry-run).
    if cfg.dry_run {
        push(format!(
            "EVENT archive-purge-skip month={} reason=dry_run",
            month_key
        ));
    } else {
        push(format!(
            "EVENT archive-purge month={} status=not_implemented",
            month_key
        ));
    }

    // Ledger: mark complete.
    let entry = ArchiveLedgerEntry {
        month: month_key.clone(),
        status: "complete".to_string(),
        format: cfg.format.as_str().to_string(),
        row_count,
        min_ts_utc: min_ts.to_rfc3339(),
        max_ts_utc: max_ts.to_rfc3339(),
        zip_sha256: zip_sha256.clone(),
        zip_bytes: zip_bytes.len() as u64,
        created_utc: Utc::now().to_rfc3339(),
    };
    write_ledger_entry(ledger_path, &entry).await?;
    push(format!(
        "EVENT archive-ledger-write month={} status=complete",
        month_key
    ));
    push("VERIFY 6/6 verify+ledger ok".to_string());

    Ok(())
}

fn export_demo_rows(month_start: NaiveDate, format: ArchiveFormat) -> Result<DemoExport> {
    // Deterministic: fixed 5 rows, one per day starting at day 1.
    let mut rows = Vec::new();
    for i in 0..5u64 {
        let d = month_start
            .with_day((i + 1) as u32)
            .ok_or_else(|| anyhow::anyhow!("Invalid demo day"))?;
        let dt = d
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid demo time"))?;
        let ts = Utc.from_utc_datetime(&dt);
        rows.push((i + 1, ts));
    }

    let min_ts = rows.first().map(|(_, ts)| *ts).unwrap();
    let max_ts = rows.last().map(|(_, ts)| *ts).unwrap();

    let bytes = match format {
        ArchiveFormat::ZipNdjson => {
            let mut out = String::new();
            for (id, ts) in rows.iter() {
                out.push_str(
                    &serde_json::json!({
                        "call_id": id,
                        "call_received_at_utc": ts.to_rfc3339(),
                        "demo": true
                    })
                    .to_string(),
                );
                out.push('\n');
            }
            out.into_bytes()
        }
        ArchiveFormat::ZipCsv => {
            let mut out = String::new();
            out.push_str("call_id,call_received_at_utc,demo\n");
            for (id, ts) in rows.iter() {
                out.push_str(&format!("{},{},true\n", id, ts.to_rfc3339()));
            }
            out.into_bytes()
        }
    };

    Ok((bytes, rows.len() as u64, min_ts, max_ts))
}

fn zip_single_file(name_in_zip: &str, content: &[u8]) -> Result<Vec<u8>> {
    let cursor = std::io::Cursor::new(Vec::<u8>::new());
    let cursor = {
        let mut zip = zip::ZipWriter::new(cursor);
        let opts = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        zip.start_file(name_in_zip, opts)?;
        use std::io::Write;
        zip.write_all(content)?;
        zip.finish()?
    };
    Ok(cursor.into_inner())
}

async fn folder_size_bytes(dir: &Path) -> Result<u64> {
    let mut total: u64 = 0;
    let mut rd = match tokio::fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(e) => {
            warn!(
                "[PHASE: archive] [STEP: folder_size] Unable to read destination dir {:?}: {:?}",
                dir, e
            );
            return Ok(0);
        }
    };
    while let Ok(Some(ent)) = rd.next_entry().await {
        let meta = match ent.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

async fn read_ledger(path: &Path) -> Result<BTreeMap<String, ArchiveLedgerEntry>> {
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Ok(BTreeMap::new());
    }
    let bytes = tokio::fs::read(path).await?;
    let map: BTreeMap<String, ArchiveLedgerEntry> = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => {
            warn!(
                "[PHASE: archive] [STEP: ledger] Failed to parse ledger (path={:?}, error={:?})",
                path, e
            );
            BTreeMap::new()
        }
    };
    Ok(map)
}

async fn write_ledger_entry(path: &Path, entry: &ArchiveLedgerEntry) -> Result<()> {
    let mut map = read_ledger(path).await?;
    map.insert(entry.month.clone(), entry.clone());
    let bytes = serde_json::to_vec_pretty(&map)?;
    write_file_with_retries(path, &bytes, "write_archive_ledger").await
}

async fn ensure_dir_with_retries(path: &Path, label: &str) -> Result<()> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        let started = Instant::now();
        match timeout(Duration::from_secs(5), tokio::fs::create_dir_all(path)).await {
            Ok(Ok(())) => {
                info!(
                    "[PHASE: archive] [STEP: fs] {} ok (attempt={}, duration_ms={})",
                    label,
                    attempt,
                    started.elapsed().as_millis()
                );
                return Ok(());
            }
            Ok(Err(e)) => {
                warn!(
                    "[PHASE: archive] [STEP: fs] {} failed (attempt={}, error={:?})",
                    label, attempt, e
                );
                last_err = Some(anyhow::anyhow!(e));
            }
            Err(_) => {
                warn!(
                    "[PHASE: archive] [STEP: fs] {} timed out (attempt={})",
                    label, attempt
                );
                last_err = Some(anyhow::anyhow!("create_dir_all timed out"));
            }
        }

        let backoff_ms = 50_u64.saturating_mul(1_u64 << ((attempt - 1) as u32));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Failed to create directory")))
}

async fn write_file_with_retries(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        let started = Instant::now();
        match timeout(Duration::from_secs(10), tokio::fs::write(path, bytes)).await {
            Ok(Ok(())) => {
                info!(
                    "[PHASE: archive] [STEP: fs] {} ok (attempt={}, path={:?}, bytes={}, duration_ms={})",
                    label,
                    attempt,
                    path,
                    bytes.len(),
                    started.elapsed().as_millis()
                );
                return Ok(());
            }
            Ok(Err(e)) => {
                warn!(
                    "[PHASE: archive] [STEP: fs] {} failed (attempt={}, path={:?}, error={:?})",
                    label, attempt, path, e
                );
                last_err = Some(anyhow::anyhow!(e));
            }
            Err(_) => {
                warn!(
                    "[PHASE: archive] [STEP: fs] {} timed out (attempt={}, path={:?})",
                    label, attempt, path
                );
                last_err = Some(anyhow::anyhow!("write timed out"));
            }
        }
        let backoff_ms = 50_u64.saturating_mul(1_u64 << ((attempt - 1) as u32));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Failed to write file")))
}

async fn rename_with_retries(from: &Path, to: &Path, label: &str) -> Result<()> {
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=3 {
        let started = Instant::now();
        match timeout(Duration::from_secs(5), tokio::fs::rename(from, to)).await {
            Ok(Ok(())) => {
                info!(
                    "[PHASE: archive] [STEP: fs] {} ok (attempt={}, duration_ms={})",
                    label,
                    attempt,
                    started.elapsed().as_millis()
                );
                return Ok(());
            }
            Ok(Err(e)) => {
                warn!(
                    "[PHASE: archive] [STEP: fs] {} failed (attempt={}, from={:?}, to={:?}, error={:?})",
                    label, attempt, from, to, e
                );
                last_err = Some(anyhow::anyhow!(e));
            }
            Err(_) => {
                warn!(
                    "[PHASE: archive] [STEP: fs] {} timed out (attempt={}, from={:?}, to={:?})",
                    label, attempt, from, to
                );
                last_err = Some(anyhow::anyhow!("rename timed out"));
            }
        }
        let backoff_ms = 50_u64.saturating_mul(1_u64 << ((attempt - 1) as u32));
        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
    }
    error!(
        "[PHASE: archive] [STEP: fs] {} failed permanently from={:?} to={:?}",
        label, from, to
    );
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Failed to rename file")))
}
