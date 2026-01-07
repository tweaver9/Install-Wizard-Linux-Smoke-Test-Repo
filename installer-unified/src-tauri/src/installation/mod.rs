// Installation logic (Phase 5)
//
// This module contains the OS-specific deployment logic (Windows + Linux) and
// shared utilities for running external commands with timeouts/retries.
//
// IMPORTANT:
// - Never log secrets (connection strings, license keys, tokens).
// - All I/O should be async.

pub mod docker;
pub mod files;
pub mod service;

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "linux")]
pub mod linux;

use anyhow::{Context, Result};
use log::{debug, error, info, warn};
use std::process::Stdio;
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tokio_retry::strategy::{jitter, ExponentialBackoff};
use tokio_retry::RetryIf;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
}

fn mask_arg_for_log(arg: &str) -> String {
    // Heuristic masking: treat anything that looks like a secret as sensitive.
    let lower = arg.to_ascii_lowercase();
    if lower.contains("password=")
        || lower.contains("pwd=")
        || lower.contains("secret")
        || lower.contains("token")
        || lower.contains("license")
        || lower.contains("apikey")
        || lower.contains("api_key")
    {
        return "***".to_string();
    }

    // Connection-string-like values: delegate to existing masker.
    if arg.contains(';') && lower.contains('=') {
        return crate::utils::logging::mask_connection_string(arg);
    }

    // Generic long values: partially mask.
    crate::utils::logging::mask_sensitive(arg)
}

fn is_transient_exec_error(e: &anyhow::Error) -> bool {
    let msg = e.to_string().to_ascii_lowercase();
    msg.contains("timed out")
        || msg.contains("timeout")
        || msg.contains("temporarily")
        || msg.contains("temporary")
        || msg.contains("busy")
        || msg.contains("in use")
        || msg.contains("used by another process")
        || msg.contains("resource")
        || msg.contains("i/o")
        || msg.contains("io error")
        || msg.contains("connection")
        || msg.contains("network")
}

async fn run_cmd_with_timeout_once(
    program: &str,
    args: &[String],
    timeout_dur: Duration,
    operation: &str,
) -> Result<CommandOutput> {
    let started = Instant::now();

    debug!(
        "[PHASE: installation] [STEP: cmd] run_cmd_with_timeout_once entered (operation={}, program={}, args=[{}], timeout_ms={})",
        operation,
        program,
        args.iter().map(|a| mask_arg_for_log(a)).collect::<Vec<_>>().join(", "),
        timeout_dur.as_millis()
    );

    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().with_context(|| {
        format!(
            "Failed to spawn command '{}' (operation={})",
            program, operation
        )
    })?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout (operation={})", operation))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr (operation={})", operation))?;

    let stdout_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await?;
        Ok::<String, std::io::Error>(String::from_utf8_lossy(&buf).to_string())
    });
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        stderr.read_to_end(&mut buf).await?;
        Ok::<String, std::io::Error>(String::from_utf8_lossy(&buf).to_string())
    });

    let status = match timeout(timeout_dur, child.wait()).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return Err(anyhow::Error::new(e)).with_context(|| {
                format!(
                    "Command wait failed (operation={}, program={})",
                    operation, program
                )
            });
        }
        Err(_) => {
            warn!(
                "[PHASE: installation] [STEP: cmd] Timeout reached (operation={}, program={}, timeout_ms={}); attempting to kill process",
                operation,
                program,
                timeout_dur.as_millis()
            );

            if let Err(e) = child.kill().await {
                warn!(
                    "[PHASE: installation] [STEP: cmd] Failed to kill timed-out process (operation={}, program={}): {}",
                    operation, program, e
                );
            }

            // Best-effort reap (avoid zombies)
            let _ = timeout(Duration::from_secs(5), child.wait()).await;

            return Err(anyhow::anyhow!(
                "Command timed out after {}ms (operation={}, program={})",
                timeout_dur.as_millis(),
                operation,
                program
            ));
        }
    };

    let stdout_str = stdout_task
        .await
        .context("stdout join failed")?
        .context("stdout read failed")?;
    let stderr_str = stderr_task
        .await
        .context("stderr join failed")?
        .context("stderr read failed")?;

    let duration_ms = started.elapsed().as_millis();
    let out = CommandOutput {
        exit_code: status.code(),
        stdout: stdout_str,
        stderr: stderr_str,
        duration_ms,
    };

    debug!(
        "[PHASE: installation] [STEP: cmd] run_cmd_with_timeout_once exit (operation={}, program={}, exit_code={:?}, duration_ms={}, stdout_len={}, stderr_len={})",
        operation,
        program,
        out.exit_code,
        out.duration_ms,
        out.stdout.len(),
        out.stderr.len()
    );

    Ok(out)
}

/// Run an external command with a timeout and up to 3 retries for transient failures.
///
/// Returns captured stdout/stderr even when exit code is non-zero (caller decides success).
pub async fn run_cmd_with_timeout(
    program: &str,
    args: &[String],
    timeout_dur: Duration,
    operation: &str,
) -> Result<CommandOutput> {
    let started = Instant::now();
    info!(
        "[PHASE: installation] [STEP: cmd] run_cmd_with_timeout entered (operation={}, program={}, args_count={}, timeout_ms={})",
        operation,
        program,
        args.len(),
        timeout_dur.as_millis()
    );

    let program_owned = program.to_string();
    let args_owned = args.to_vec();
    let operation_owned = operation.to_string();

    let attempt = move || {
        let program = program_owned.clone();
        let args = args_owned.clone();
        let op = operation_owned.clone();
        async move { run_cmd_with_timeout_once(&program, &args, timeout_dur, &op).await }
    };

    let retry_strategy = ExponentialBackoff::from_millis(200)
        .factor(2)
        .max_delay(Duration::from_secs(2))
        .take(3)
        .map(jitter);

    let result = RetryIf::spawn(retry_strategy, attempt, |e: &anyhow::Error| {
        let transient = is_transient_exec_error(e);
        if transient {
            warn!(
                "[PHASE: installation] [STEP: cmd] Transient command failure detected; will retry (operation={}, program={}, err={})",
                operation,
                program,
                e
            );
        }
        transient
    })
    .await;

    match &result {
        Ok(out) => {
            info!(
                "[PHASE: installation] [STEP: cmd] run_cmd_with_timeout exit (operation={}, program={}, exit_code={:?}, duration_ms={})",
                operation,
                program,
                out.exit_code,
                started.elapsed().as_millis()
            );
        }
        Err(e) => {
            error!(
                "[PHASE: installation] [STEP: cmd] run_cmd_with_timeout error (operation={}, program={}, duration_ms={}, err={:?})",
                operation,
                program,
                started.elapsed().as_millis(),
                e
            );
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_arg_for_log_redacts_passwordish_values() {
        let masked = mask_arg_for_log("Password=PASSWORD_SHOULD_BE_REDACTED");
        assert_eq!(masked, "***");
        let masked = mask_arg_for_log("pwd=PASSWORD_SHOULD_BE_REDACTED");
        assert_eq!(masked, "***");
    }

    #[test]
    fn mask_arg_for_log_partially_masks_long_values() {
        let masked = mask_arg_for_log("abcdefghijklmnopqrstuvwxyz");
        assert!(masked.contains("..."));
    }

    #[tokio::test]
    async fn run_cmd_with_timeout_basic_smoke() {
        let timeout_dur = Duration::from_secs(5);

        #[cfg(windows)]
        let (program, args) = (
            "cmd",
            vec!["/C".to_string(), "echo".to_string(), "hello".to_string()],
        );

        #[cfg(not(windows))]
        let (program, args) = ("sh", vec!["-c".to_string(), "echo hello".to_string()]);

        let out = run_cmd_with_timeout(program, &args, timeout_dur, "test_echo")
            .await
            .expect("command should run");
        assert_eq!(out.exit_code, Some(0));
        assert!(out.stdout.to_ascii_lowercase().contains("hello"));
    }
}
