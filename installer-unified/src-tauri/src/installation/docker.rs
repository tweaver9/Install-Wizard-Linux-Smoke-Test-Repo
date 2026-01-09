// Docker setup (Linux deployment option)
// Ported from the plan's Phase 5 guidance.
// Phase 3: Full Docker integration with compose template, image loading, and orchestration.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::installation::{run_cmd_with_timeout, CommandOutput};

#[allow(dead_code)]
const DOCKER_CMD_TIMEOUT: Duration = Duration::from_secs(120);

/// Docker version information.
#[derive(Debug, Clone, Default)]
pub struct DockerVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Original version string for display/logging.
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ComposeInvocation {
    DockerComposeBinary,
    DockerSubcommand,
}

/// Parse docker version output into a DockerVersion struct.
///
/// Expected format: "Docker version 24.0.5, build abcdef"
/// Also handles: "Docker version 20.10.21, build baeda1f82a" and similar variants.
pub fn parse_docker_version(output: &str) -> Option<DockerVersion> {
    // Look for "Docker version X.Y.Z" pattern
    let output = output.trim();

    // Find version number after "Docker version " or at start
    let version_str = if let Some(pos) = output.to_lowercase().find("docker version ") {
        let start = pos + "docker version ".len();
        &output[start..]
    } else {
        output
    };

    // Extract version part (stop at comma, space, or end)
    let version_part = version_str
        .split(|c: char| c == ',' || c == ' ' || c == '-')
        .next()?;

    // Parse X.Y.Z
    let parts: Vec<&str> = version_part.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let major: u32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch: u32 = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);

    // If we couldn't parse any meaningful version, return None
    if major == 0 && minor == 0 && patch == 0 && !version_part.starts_with('0') {
        return None;
    }

    Some(DockerVersion {
        major,
        minor,
        patch,
        raw: output.to_string(),
    })
}

/// Check if the Docker daemon is running by executing `docker info`.
///
/// Returns true if daemon is accessible, false otherwise.
#[allow(dead_code)]
pub async fn is_docker_daemon_running() -> Result<bool> {
    debug!("[PHASE: preflight] [STEP: docker] is_docker_daemon_running entered");

    let args = vec!["info".to_string()];
    let result = run_cmd_with_timeout("docker", &args, Duration::from_secs(15), "docker_info").await;

    match result {
        Ok(out) => {
            let running = out.exit_code == Some(0);
            debug!(
                "[PHASE: preflight] [STEP: docker] is_docker_daemon_running exit (running={}, exit_code={:?})",
                running, out.exit_code
            );
            // Check for permission denied in stderr
            if !running && out.stderr.to_lowercase().contains("permission denied") {
                warn!(
                    "[PHASE: preflight] [STEP: docker] Docker daemon check failed due to permission denied"
                );
            }
            Ok(running)
        }
        Err(e) => {
            debug!(
                "[PHASE: preflight] [STEP: docker] is_docker_daemon_running exit (running=false, error={})",
                e
            );
            Ok(false)
        }
    }
}

/// Get Docker version information.
#[allow(dead_code)]
pub async fn get_docker_version() -> Result<DockerVersion> {
    debug!("[PHASE: preflight] [STEP: docker] get_docker_version entered");

    let args = vec!["--version".to_string()];
    let out = run_cmd_with_timeout("docker", &args, Duration::from_secs(15), "docker_version").await?;

    if out.exit_code != Some(0) {
        anyhow::bail!("docker --version returned non-zero exit code");
    }

    let version = parse_docker_version(&out.stdout)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse docker version from: {}", out.stdout))?;

    debug!(
        "[PHASE: preflight] [STEP: docker] get_docker_version exit (major={}, minor={}, patch={})",
        version.major, version.minor, version.patch
    );

    Ok(version)
}

#[allow(dead_code)]
pub async fn check_docker_installed() -> Result<()> {
    let args = vec!["--version".to_string()];
    let out =
        run_cmd_with_timeout("docker", &args, Duration::from_secs(15), "docker_version").await?;
    if out.exit_code == Some(0) {
        return Ok(());
    }
    anyhow::bail!("Docker is not installed or not available in PATH");
}

/// Detect which compose invocation method is available.
///
/// Priority order (V2 preferred):
/// 1. `docker compose` (Docker Compose V2 - plugin style)
/// 2. `docker-compose` (Docker Compose V1 - standalone binary)
#[allow(dead_code)]
pub async fn detect_compose_invocation() -> Result<ComposeInvocation> {
    debug!("[PHASE: preflight] [STEP: docker] detect_compose_invocation: checking V2 (docker compose)");

    // Prefer `docker compose` (V2) first - it's the modern approach
    let out = run_cmd_with_timeout(
        "docker",
        &["compose".to_string(), "version".to_string()],
        Duration::from_secs(10),
        "docker_compose_subcommand_version",
    )
    .await;
    if out.as_ref().ok().and_then(|o| o.exit_code) == Some(0) {
        debug!("[PHASE: preflight] [STEP: docker] detect_compose_invocation: using docker compose (V2)");
        return Ok(ComposeInvocation::DockerSubcommand);
    }

    debug!("[PHASE: preflight] [STEP: docker] detect_compose_invocation: V2 not available, checking V1 (docker-compose)");

    // Fall back to `docker-compose` (V1)
    let out = run_cmd_with_timeout(
        "docker-compose",
        &["--version".to_string()],
        Duration::from_secs(10),
        "docker_compose_version",
    )
    .await;
    if out.as_ref().ok().and_then(|o| o.exit_code) == Some(0) {
        debug!("[PHASE: preflight] [STEP: docker] detect_compose_invocation: using docker-compose (V1)");
        return Ok(ComposeInvocation::DockerComposeBinary);
    }

    anyhow::bail!("Neither 'docker compose' (V2) nor 'docker-compose' (V1) is available. Please install Docker Compose.");
}

/// Run a docker compose command using the appropriate invocation method.
///
/// This is the unified helper for all compose operations (up, ps, down, logs, config).
/// Uses detect_compose_invocation() internally if not provided.
#[allow(dead_code)]
pub async fn run_compose_cmd(
    inv: ComposeInvocation,
    compose_file: &Path,
    subcommand: &str,
    extra_args: &[&str],
    timeout: Duration,
    log_label: &str,
) -> Result<CommandOutput> {
    let f = compose_file
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid compose file path"))?;

    let (program, args) = match inv {
        ComposeInvocation::DockerComposeBinary => {
            let mut a = vec!["-f".to_string(), f.to_string(), subcommand.to_string()];
            for arg in extra_args {
                a.push(arg.to_string());
            }
            ("docker-compose", a)
        }
        ComposeInvocation::DockerSubcommand => {
            let mut a = vec![
                "compose".to_string(),
                "-f".to_string(),
                f.to_string(),
                subcommand.to_string(),
            ];
            for arg in extra_args {
                a.push(arg.to_string());
            }
            ("docker", a)
        }
    };

    run_cmd_with_timeout(program, &args, timeout, log_label).await
}

#[allow(dead_code)]
pub async fn docker_load_tar(tar_path: &Path) -> Result<()> {
    let p = tar_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid tar path"))?;
    let args = vec!["load".to_string(), "-i".to_string(), p.to_string()];
    let out = run_cmd_with_timeout("docker", &args, DOCKER_CMD_TIMEOUT, "docker_load").await?;
    if out.exit_code == Some(0) {
        return Ok(());
    }
    warn!(
        "[PHASE: installation] [STEP: docker] docker load failed: {}",
        out.stderr
    );
    anyhow::bail!("Docker image load failed");
}

#[allow(dead_code)]
pub async fn docker_pull(image: &str) -> Result<()> {
    let args = vec!["pull".to_string(), image.to_string()];
    let out = run_cmd_with_timeout("docker", &args, DOCKER_CMD_TIMEOUT, "docker_pull").await?;
    if out.exit_code == Some(0) {
        return Ok(());
    }
    warn!(
        "[PHASE: installation] [STEP: docker] docker pull failed: {}",
        out.stderr
    );
    anyhow::bail!("Docker image pull failed");
}

/// Run `docker compose up -d` to start containers.
#[allow(dead_code)]
pub async fn compose_up(inv: ComposeInvocation, compose_file: &Path) -> Result<()> {
    info!(
        "[PHASE: installation] [STEP: docker] Starting Docker/Linux containers via {:?}",
        inv
    );

    let out = run_compose_cmd(inv, compose_file, "up", &["-d"], DOCKER_CMD_TIMEOUT, "compose_up").await?;

    if out.exit_code == Some(0) {
        return Ok(());
    }
    warn!(
        "[PHASE: installation] [STEP: docker] Docker/Linux compose up failed: {}",
        out.stderr
    );
    anyhow::bail!("Docker/Linux compose up failed: {}", out.stderr.trim());
}

/// Run `docker compose ps` to get container status.
#[allow(dead_code)]
pub async fn compose_ps(inv: ComposeInvocation, compose_file: &Path) -> Result<CommandOutput> {
    run_compose_cmd(inv, compose_file, "ps", &[], DOCKER_CMD_TIMEOUT, "compose_ps").await
}

/// Run `docker compose down` to stop and remove containers.
#[allow(dead_code)]
pub async fn compose_down(inv: ComposeInvocation, compose_file: &Path) -> Result<()> {
    let out = run_compose_cmd(inv, compose_file, "down", &[], DOCKER_CMD_TIMEOUT, "compose_down").await?;

    if out.exit_code == Some(0) {
        return Ok(());
    }
    warn!(
        "[PHASE: installation] [STEP: docker] Docker/Linux compose down failed: {}",
        out.stderr
    );
    anyhow::bail!("Docker/Linux compose down failed: {}", out.stderr.trim());
}

/// Run `docker compose logs` to get container logs.
#[allow(dead_code)]
pub async fn compose_logs(inv: ComposeInvocation, compose_file: &Path, tail_lines: u32) -> Result<String> {
    let tail_arg = format!("--tail={}", tail_lines);
    let out = run_compose_cmd(inv, compose_file, "logs", &[&tail_arg], Duration::from_secs(30), "compose_logs").await?;

    // Logs go to both stdout and stderr
    Ok(format!("{}{}", out.stdout, out.stderr))
}

// ============================================================================
// P3-2: Template substitution engine
// ============================================================================

/// Generate a docker-compose.yml from a template by replacing {{VAR}} placeholders.
///
/// # Arguments
/// * `template_path` - Path to the template file (e.g., docker-compose.template.yml)
/// * `output_path` - Path where the generated compose file will be written
/// * `variables` - Map of placeholder names to values (without the {{ }})
///
/// # Errors
/// Returns an error if:
/// - Template file cannot be read
/// - Any placeholders remain unresolved after substitution
/// - Output file cannot be written
pub async fn generate_compose_file(
    template_path: &Path,
    output_path: &Path,
    variables: &HashMap<String, String>,
) -> Result<()> {
    info!(
        "[PHASE: installation] [STEP: docker] generate_compose_file entered (template={:?}, output={:?}, var_count={})",
        template_path, output_path, variables.len()
    );

    // Read template
    let template_content = tokio::fs::read_to_string(template_path)
        .await
        .with_context(|| format!("Failed to read compose template: {:?}", template_path))?;

    // Perform substitution
    let output_content = substitute_placeholders(&template_content, variables);

    // Check for unresolved placeholders
    if let Some(unresolved) = find_unresolved_placeholder(&output_content) {
        anyhow::bail!(
            "Unresolved placeholder '{}' in compose template. Ensure all required variables are provided.",
            unresolved
        );
    }

    // Basic YAML sanity: must be non-empty and contain 'services:'
    if output_content.trim().is_empty() {
        anyhow::bail!("Generated compose file is empty");
    }
    if !output_content.contains("services:") {
        anyhow::bail!("Generated compose file does not contain 'services:' section");
    }

    // Write output
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create output directory: {:?}", parent))?;
    }

    tokio::fs::write(output_path, &output_content)
        .await
        .with_context(|| format!("Failed to write compose file: {:?}", output_path))?;

    info!(
        "[PHASE: installation] [STEP: docker] generate_compose_file exit ok (output={:?}, size={})",
        output_path,
        output_content.len()
    );

    Ok(())
}

/// Substitute {{VAR}} placeholders in a string with values from a map.
pub fn substitute_placeholders(template: &str, variables: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in variables {
        let placeholder = format!("{{{{{}}}}}", key); // {{KEY}}
        result = result.replace(&placeholder, value);
    }
    result
}

/// Find the first unresolved {{...}} placeholder in the content.
/// Returns the placeholder name (without braces) if found.
pub fn find_unresolved_placeholder(content: &str) -> Option<String> {
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            chars.next(); // consume second {
            let mut name = String::new();
            while let Some(&ch) = chars.peek() {
                if ch == '}' {
                    chars.next();
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        if !name.is_empty() {
                            return Some(name);
                        }
                    }
                    break;
                }
                name.push(ch);
                chars.next();
            }
        }
    }
    None
}

// ============================================================================
// P3-3: Docker image loading (.tar)
// ============================================================================

/// Load Docker images from .tar files in a directory.
///
/// # Arguments
/// * `images_dir` - Directory containing .tar image files
/// * `emit_progress` - Progress callback for UI updates
///
/// # Returns
/// List of loaded image names parsed from `docker load` output.
pub async fn load_docker_images(
    images_dir: &Path,
    emit_progress: &crate::api::installer::ProgressEmitter,
) -> Result<Vec<String>> {
    info!(
        "[PHASE: installation] [STEP: docker] load_docker_images entered (dir={:?})",
        images_dir
    );

    if !images_dir.exists() {
        anyhow::bail!(
            "Docker images directory not found: {:?}. Populate this folder for Docker mode.",
            images_dir
        );
    }

    // Find all .tar files
    let mut tar_files: Vec<std::path::PathBuf> = Vec::new();
    let mut entries = tokio::fs::read_dir(images_dir)
        .await
        .with_context(|| format!("Failed to read images directory: {:?}", images_dir))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().map(|e| e == "tar").unwrap_or(false) {
            tar_files.push(path);
        }
    }

    if tar_files.is_empty() {
        anyhow::bail!(
            "No Docker images found in {:?}. Populate this folder with .tar image files for Docker mode.",
            images_dir
        );
    }

    tar_files.sort();
    let total = tar_files.len();
    let mut loaded_images: Vec<String> = Vec::new();

    for (idx, tar_path) in tar_files.iter().enumerate() {
        let filename = tar_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown.tar".to_string());

        emit_progress(crate::api::installer::ProgressPayload {
            correlation_id: String::new(),
            step: "docker_load".to_string(),
            severity: "info".to_string(),
            phase: "install".to_string(),
            percent: 50 + ((idx * 20) / total) as i32,
            message: format!("Loading Docker image {}/{}: {}", idx + 1, total, filename),
            elapsed_ms: None,
            eta_ms: None,
        });

        info!(
            "[PHASE: installation] [STEP: docker] Loading image {}/{}: {:?}",
            idx + 1,
            total,
            tar_path
        );

        let tar_str = tar_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid tar path: {:?}", tar_path))?;

        let args = vec!["load".to_string(), "-i".to_string(), tar_str.to_string()];
        let out = run_cmd_with_timeout("docker", &args, Duration::from_secs(300), "docker_load").await?;

        if out.exit_code != Some(0) {
            anyhow::bail!(
                "Failed to load Docker image '{}': {}",
                filename,
                out.stderr.trim()
            );
        }

        // Parse loaded image names from output
        let names = parse_docker_load_output(&out.stdout);
        loaded_images.extend(names);
    }

    info!(
        "[PHASE: installation] [STEP: docker] load_docker_images exit ok (loaded={} images from {} tar files)",
        loaded_images.len(),
        total
    );

    Ok(loaded_images)
}

/// Parse `docker load` output to extract loaded image names.
///
/// Typical output formats:
/// - "Loaded image: myimage:tag"
/// - "Loaded image ID: sha256:abc123..."
pub fn parse_docker_load_output(stdout: &str) -> Vec<String> {
    let mut images = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        // "Loaded image: name:tag"
        if let Some(rest) = line.strip_prefix("Loaded image:") {
            let name = rest.trim();
            if !name.is_empty() && !name.starts_with("sha256:") {
                images.push(name.to_string());
            }
        }
        // "Loaded image ID: sha256:..." - we skip these as they're not useful names
    }
    images
}

// ============================================================================
// P3-5: Container readiness and logs helpers
// ============================================================================

/// Wait for Docker containers to become healthy/running.
///
/// Polls `docker compose ps` every few seconds until containers are running
/// or the timeout is reached.
pub async fn wait_for_containers_healthy(
    compose_path: &Path,
    timeout_secs: u64,
) -> Result<()> {
    info!(
        "[PHASE: installation] [STEP: docker] wait_for_containers_healthy entered (compose={:?}, timeout={}s)",
        compose_path, timeout_secs
    );

    let started = Instant::now();
    let timeout_dur = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_secs(3);

    let inv = detect_compose_invocation().await?;

    loop {
        if started.elapsed() >= timeout_dur {
            // Get final status snapshot for error message
            let ps_output = compose_ps(inv, compose_path).await.ok();
            let snapshot = ps_output
                .map(|o| o.stdout)
                .unwrap_or_else(|| "(unable to get status)".to_string());

            anyhow::bail!(
                "Docker/Linux installation timeout: containers not ready after {}s.\n\n\
                 Container Status:\n{}\n\n\
                 Troubleshooting:\n\
                 1. Check container logs: docker compose -f {:?} logs\n\
                 2. Check container status: docker compose -f {:?} ps\n\
                 3. Verify Docker daemon is running: docker info",
                timeout_secs,
                snapshot.trim(),
                compose_path,
                compose_path
            );
        }

        let ps_result = compose_ps(inv, compose_path).await?;

        if ps_result.exit_code == Some(0) {
            let status = parse_compose_ps_output(&ps_result.stdout);

            if status.all_running && status.container_count > 0 {
                info!(
                    "[PHASE: installation] [STEP: docker] wait_for_containers_healthy exit ok (containers={}, elapsed={}ms)",
                    status.container_count,
                    started.elapsed().as_millis()
                );
                return Ok(());
            }

            debug!(
                "[PHASE: installation] [STEP: docker] Containers not ready yet (running={}, count={})",
                status.all_running, status.container_count
            );
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Parsed status from `docker compose ps` output.
#[derive(Debug, Default)]
pub struct ComposePsStatus {
    pub container_count: usize,
    pub all_running: bool,
    pub containers: Vec<ContainerStatus>,
}

/// Individual container status.
#[derive(Debug, Clone)]
pub struct ContainerStatus {
    pub name: String,
    pub state: String,
}

/// Parse `docker compose ps` output to determine container status.
///
/// Handles both table format and JSON format (if available).
pub fn parse_compose_ps_output(stdout: &str) -> ComposePsStatus {
    let lines: Vec<&str> = stdout.lines().collect();

    // Skip header line(s) and parse container lines
    let mut containers = Vec::new();
    let mut in_body = false;

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Detect header (NAME, STATUS, etc.)
        if line.to_uppercase().contains("NAME") && line.to_uppercase().contains("STATUS") {
            in_body = true;
            continue;
        }

        // Parse body lines
        if in_body || !line.contains("NAME") {
            // Typical format: "container-name   service   running   Up 2 minutes"
            // or: "NAME   SERVICE   STATUS   PORTS"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0].to_string();
                // Look for status keywords
                let line_lower = line.to_lowercase();
                let state = if line_lower.contains("running") || line_lower.contains("up") {
                    "running".to_string()
                } else if line_lower.contains("exited") {
                    "exited".to_string()
                } else if line_lower.contains("created") {
                    "created".to_string()
                } else {
                    "unknown".to_string()
                };

                // Skip header-like lines
                if name.to_uppercase() != "NAME" && name.to_uppercase() != "CONTAINER" {
                    containers.push(ContainerStatus { name, state });
                }
            }
        }
    }

    let all_running = !containers.is_empty() && containers.iter().all(|c| c.state == "running");

    ComposePsStatus {
        container_count: containers.len(),
        all_running,
        containers,
    }
}

/// Get logs from a specific container.
///
/// # Arguments
/// * `container_name` - Name of the container
/// * `lines` - Number of log lines to retrieve
pub async fn get_container_logs(container_name: &str, lines: u32) -> Result<String> {
    debug!(
        "[PHASE: installation] [STEP: docker] get_container_logs entered (container={}, lines={})",
        container_name, lines
    );

    let args = vec![
        "logs".to_string(),
        "--tail".to_string(),
        lines.to_string(),
        container_name.to_string(),
    ];

    let out = run_cmd_with_timeout("docker", &args, Duration::from_secs(30), "docker_logs").await?;

    // Docker logs go to both stdout and stderr depending on the log stream
    let combined = format!("{}{}", out.stdout, out.stderr);

    debug!(
        "[PHASE: installation] [STEP: docker] get_container_logs exit (container={}, len={})",
        container_name,
        combined.len()
    );

    Ok(combined)
}

// ============================================================================
// P3-4: Docker compose up flow (install_docker_mode)
// ============================================================================

use crate::api::installer::{InstallArtifacts, ProgressEmitter, ProgressPayload, StartInstallRequest};

/// Full Docker installation flow.
///
/// Steps:
/// 1. Verify Docker installed + daemon running
/// 2. Ensure runtime docker folders exist
/// 3. Create data directories
/// 4. Generate docker-compose.yml from template
/// 5. Load Docker images from .tar files (if present)
/// 6. Run compose up -d
/// 7. Wait for containers ready
/// 8. Return InstallArtifacts
pub async fn install_docker_mode(
    req: &StartInstallRequest,
    emit_progress: &ProgressEmitter,
    correlation_id: &str,
) -> Result<InstallArtifacts> {
    let started = Instant::now();
    info!(
        "[PHASE: installation] [STEP: docker] install_docker_mode entered (dest={}, correlation={})",
        req.destination_folder, correlation_id
    );

    let dest_root = std::path::Path::new(&req.destination_folder);

    // Step 1: Verify Docker is available and running
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "docker_check".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 40,
        message: "Docker/Linux: Checking Docker installation...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    check_docker_installed().await.map_err(|e| {
        anyhow::anyhow!(
            "Docker/Linux installation requires Docker. Please install Docker first. Error: {}",
            e
        )
    })?;

    let daemon_running = is_docker_daemon_running().await?;
    if !daemon_running {
        anyhow::bail!(
            "Docker/Linux installation requires the Docker daemon to be running. Please start Docker Desktop or the Docker service."
        );
    }

    // Step 2: Locate runtime docker folders
    // These are relative to the executable or a known runtime location
    let runtime_base = locate_docker_runtime_dir()?;
    let template_path = runtime_base.join("compose").join("docker-compose.template.yml");
    let images_dir = runtime_base.join("images");

    if !template_path.exists() {
        anyhow::bail!(
            "Docker compose template not found: {:?}. Ensure runtime assets are present.",
            template_path
        );
    }

    // Step 3: Create data directories
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "docker_dirs".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 45,
        message: "Docker/Linux: Creating data directories...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    let data_path = dest_root.join("data");
    let logs_path = dest_root.join("logs");

    tokio::fs::create_dir_all(&data_path)
        .await
        .with_context(|| format!("Failed to create data directory: {:?}", data_path))?;
    tokio::fs::create_dir_all(&logs_path)
        .await
        .with_context(|| format!("Failed to create logs directory: {:?}", logs_path))?;

    // Step 4: Generate docker-compose.yml from template
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "docker_compose_gen".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 50,
        message: "Docker/Linux: Generating docker-compose.yml...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    let compose_output = dest_root.join("docker-compose.yml");
    let install_id = uuid::Uuid::new_v4().to_string();

    let mut variables = HashMap::new();
    variables.insert("DB_CONNECTION_STRING".to_string(), req.config_db_connection_string.clone());
    variables.insert("DATA_PATH".to_string(), data_path.to_string_lossy().to_string());
    variables.insert("LOG_PATH".to_string(), logs_path.to_string_lossy().to_string());
    variables.insert("WEB_PORT".to_string(), "8080".to_string());
    variables.insert("INSTALL_ID".to_string(), install_id.clone());

    generate_compose_file(&template_path, &compose_output, &variables).await?;

    // Step 5: Load Docker images from .tar files (if present)
    if images_dir.exists() {
        let mut has_tar_files = false;
        if let Ok(mut entries) = tokio::fs::read_dir(&images_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if entry.path().extension().map(|e| e == "tar").unwrap_or(false) {
                    has_tar_files = true;
                    break;
                }
            }
        }

        if has_tar_files {
            emit_progress(ProgressPayload {
                correlation_id: correlation_id.to_string(),
                step: "docker_load".to_string(),
                severity: "info".to_string(),
                phase: "install".to_string(),
                percent: 55,
                message: "Docker/Linux: Loading Docker images...".to_string(),
                elapsed_ms: Some(started.elapsed().as_millis()),
                eta_ms: None,
            });

            load_docker_images(&images_dir, emit_progress).await?;
        }
    }

    // Step 6: Run compose up -d
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "docker_start".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 70,
        message: "Docker/Linux: Starting containers...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    let inv = detect_compose_invocation().await?;
    compose_up(inv, &compose_output).await?;

    // Step 7: Wait for containers ready
    emit_progress(ProgressPayload {
        correlation_id: correlation_id.to_string(),
        step: "docker_ready".to_string(),
        severity: "info".to_string(),
        phase: "install".to_string(),
        percent: 85,
        message: "Docker/Linux: Waiting for containers to be ready...".to_string(),
        elapsed_ms: Some(started.elapsed().as_millis()),
        eta_ms: None,
    });

    wait_for_containers_healthy(&compose_output, 120).await?;

    // Step 8: Return artifacts
    info!(
        "[PHASE: installation] [STEP: docker] install_docker_mode exit ok (duration={}ms)",
        started.elapsed().as_millis()
    );

    Ok(InstallArtifacts {
        log_folder: Some(logs_path.to_string_lossy().to_string()),
        artifacts_dir: Some(dest_root.to_string_lossy().to_string()),
        manifest_path: None,
        mapping_path: None,
        config_path: Some(compose_output.to_string_lossy().to_string()),
    })
}

/// Locate the Docker runtime directory.
///
/// Searches in order:
/// 1. CADALYTIX_RUNTIME_DIR environment variable
/// 2. runtime/linux/docker relative to executable
/// 3. ../runtime/linux/docker relative to executable (for dev builds)
pub fn locate_docker_runtime_dir() -> Result<std::path::PathBuf> {
    // Check environment variable first
    if let Ok(runtime_dir) = std::env::var("CADALYTIX_RUNTIME_DIR") {
        let docker_dir = std::path::PathBuf::from(&runtime_dir).join("linux").join("docker");
        if docker_dir.exists() {
            return Ok(docker_dir);
        }
    }

    // Try relative to executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // Check runtime/linux/docker
            let candidate = exe_dir.join("runtime").join("linux").join("docker");
            if candidate.exists() {
                return Ok(candidate);
            }

            // Check ../runtime/linux/docker (for dev builds where exe is in target/debug)
            let candidate = exe_dir.join("..").join("..").join("..").join("runtime").join("linux").join("docker");
            if candidate.exists() {
                return Ok(candidate.canonicalize().unwrap_or(candidate));
            }
        }
    }

    // Fallback: current working directory
    let cwd_candidate = std::path::PathBuf::from("runtime").join("linux").join("docker");
    if cwd_candidate.exists() {
        return Ok(cwd_candidate);
    }

    anyhow::bail!(
        "Docker runtime directory not found. Set CADALYTIX_RUNTIME_DIR or ensure runtime/linux/docker exists."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_docker_version_standard_format() {
        let output = "Docker version 24.0.5, build ced0996";
        let v = parse_docker_version(output).unwrap();
        assert_eq!(v.major, 24);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 5);
        assert!(v.raw.contains("24.0.5"));
    }

    #[test]
    fn parse_docker_version_older_format() {
        let output = "Docker version 20.10.21, build baeda1f82a";
        let v = parse_docker_version(output).unwrap();
        assert_eq!(v.major, 20);
        assert_eq!(v.minor, 10);
        assert_eq!(v.patch, 21);
    }

    #[test]
    fn parse_docker_version_with_newline() {
        let output = "Docker version 23.0.1, build a5ee5b1\n";
        let v = parse_docker_version(output).unwrap();
        assert_eq!(v.major, 23);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 1);
    }

    #[test]
    fn parse_docker_version_two_part() {
        // Some docker versions might only have major.minor
        let output = "Docker version 19.03, build abc123";
        let v = parse_docker_version(output).unwrap();
        assert_eq!(v.major, 19);
        assert_eq!(v.minor, 3);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn parse_docker_version_invalid_returns_none() {
        let output = "not a docker version";
        let v = parse_docker_version(output);
        assert!(v.is_none());
    }

    #[test]
    fn parse_docker_version_empty_returns_none() {
        let v = parse_docker_version("");
        assert!(v.is_none());
    }

    #[test]
    fn parse_docker_version_desktop_format() {
        // Docker Desktop on Windows/Mac may have different formatting
        let output = "Docker version 25.0.3, build 4debf41";
        let v = parse_docker_version(output).unwrap();
        assert_eq!(v.major, 25);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 3);
    }

    // ========================================================================
    // P3-2: Template substitution tests
    // ========================================================================

    #[test]
    fn substitute_placeholders_replaces_all() {
        let template = "Host={{HOST}}, Port={{PORT}}, DB={{DB}}";
        let mut vars = HashMap::new();
        vars.insert("HOST".to_string(), "localhost".to_string());
        vars.insert("PORT".to_string(), "5432".to_string());
        vars.insert("DB".to_string(), "cadalytix".to_string());

        let result = substitute_placeholders(template, &vars);
        assert_eq!(result, "Host=localhost, Port=5432, DB=cadalytix");
    }

    #[test]
    fn substitute_placeholders_handles_repeated() {
        let template = "{{X}} and {{X}} again";
        let mut vars = HashMap::new();
        vars.insert("X".to_string(), "value".to_string());

        let result = substitute_placeholders(template, &vars);
        assert_eq!(result, "value and value again");
    }

    #[test]
    fn substitute_placeholders_leaves_unknown() {
        let template = "Known={{KNOWN}}, Unknown={{UNKNOWN}}";
        let mut vars = HashMap::new();
        vars.insert("KNOWN".to_string(), "yes".to_string());

        let result = substitute_placeholders(template, &vars);
        assert_eq!(result, "Known=yes, Unknown={{UNKNOWN}}");
    }

    #[test]
    fn find_unresolved_placeholder_detects_unresolved() {
        let content = "services:\n  image: {{IMAGE}}\n  port: 8080";
        let unresolved = find_unresolved_placeholder(content);
        assert_eq!(unresolved, Some("IMAGE".to_string()));
    }

    #[test]
    fn find_unresolved_placeholder_returns_none_when_all_resolved() {
        let content = "services:\n  image: myimage:latest\n  port: 8080";
        let unresolved = find_unresolved_placeholder(content);
        assert!(unresolved.is_none());
    }

    #[test]
    fn find_unresolved_placeholder_handles_empty() {
        let unresolved = find_unresolved_placeholder("");
        assert!(unresolved.is_none());
    }

    #[test]
    fn find_unresolved_placeholder_handles_partial_braces() {
        // Single braces should not match
        let content = "value = {not_placeholder}";
        let unresolved = find_unresolved_placeholder(content);
        assert!(unresolved.is_none());
    }

    // ========================================================================
    // P3-3: Docker load output parsing tests
    // ========================================================================

    #[test]
    fn parse_docker_load_output_extracts_image_name() {
        let stdout = "Loaded image: cadalytix/web:latest\n";
        let images = parse_docker_load_output(stdout);
        assert_eq!(images, vec!["cadalytix/web:latest"]);
    }

    #[test]
    fn parse_docker_load_output_handles_multiple_images() {
        let stdout = "Loaded image: image1:v1\nLoaded image: image2:v2\n";
        let images = parse_docker_load_output(stdout);
        assert_eq!(images, vec!["image1:v1", "image2:v2"]);
    }

    #[test]
    fn parse_docker_load_output_skips_sha256() {
        let stdout = "Loaded image ID: sha256:abc123def456\n";
        let images = parse_docker_load_output(stdout);
        assert!(images.is_empty());
    }

    #[test]
    fn parse_docker_load_output_handles_mixed() {
        let stdout = "Some loading text\nLoaded image: myapp:1.0\nLoaded image ID: sha256:xxx\n";
        let images = parse_docker_load_output(stdout);
        assert_eq!(images, vec!["myapp:1.0"]);
    }

    #[test]
    fn parse_docker_load_output_handles_empty() {
        let images = parse_docker_load_output("");
        assert!(images.is_empty());
    }

    // ========================================================================
    // P3-5: Compose ps parsing tests
    // ========================================================================

    #[test]
    fn parse_compose_ps_output_detects_running() {
        let stdout = "NAME              SERVICE    STATUS   PORTS\n\
                      cadalytix-web     web        running  0.0.0.0:8080->8080/tcp\n\
                      cadalytix-worker  worker     running  \n";
        let status = parse_compose_ps_output(stdout);
        assert_eq!(status.container_count, 2);
        assert!(status.all_running);
    }

    #[test]
    fn parse_compose_ps_output_detects_not_all_running() {
        let stdout = "NAME              SERVICE    STATUS   PORTS\n\
                      cadalytix-web     web        running  0.0.0.0:8080->8080/tcp\n\
                      cadalytix-worker  worker     exited   \n";
        let status = parse_compose_ps_output(stdout);
        assert_eq!(status.container_count, 2);
        assert!(!status.all_running);
    }

    #[test]
    fn parse_compose_ps_output_handles_empty() {
        let status = parse_compose_ps_output("");
        assert_eq!(status.container_count, 0);
        assert!(!status.all_running);
    }

    #[test]
    fn parse_compose_ps_output_handles_up_keyword() {
        // Some docker-compose versions use "Up" instead of "running"
        let stdout = "CONTAINER ID   NAME          COMMAND   STATUS   PORTS\n\
                      abc123         mycontainer   ...       Up 5 minutes   8080/tcp\n";
        let status = parse_compose_ps_output(stdout);
        assert_eq!(status.container_count, 1);
        assert!(status.all_running);
    }

    // ========================================================================
    // Phase 3 Finish: Additional edge case tests
    // ========================================================================

    #[test]
    fn substitute_placeholders_handles_paths_with_spaces() {
        // TASK C: Paths with spaces must work correctly
        let template = r#"volumes:
  - "{{DATA_PATH}}/logs:/app/logs"
  - "{{DATA_PATH}}/data:/app/data"
services:
  web:
    environment:
      - DB_CONN={{DB_CONNECTION_STRING}}"#;

        let mut vars = HashMap::new();
        vars.insert("DATA_PATH".to_string(), "/home/user/My Data Folder".to_string());
        vars.insert("DB_CONNECTION_STRING".to_string(), "Host=localhost;Database=test".to_string());

        let result = substitute_placeholders(template, &vars);

        // Verify no unresolved placeholders remain
        assert!(find_unresolved_placeholder(&result).is_none());

        // Verify paths with spaces are in the output
        assert!(result.contains("/home/user/My Data Folder/logs:/app/logs"));
        assert!(result.contains("/home/user/My Data Folder/data:/app/data"));
    }

    #[test]
    fn substitute_placeholders_driver_opts_device_preserves_quotes() {
        // Phase 3 Patch: Ensure driver_opts device values with spaces remain quoted
        // This is the actual pattern from docker-compose.template.yml
        let template = r#"volumes:
  cadalytix-data:
    driver: local
    driver_opts:
      type: "none"
      o: "bind"
      device: "{{DATA_PATH}}"
  cadalytix-logs:
    driver: local
    driver_opts:
      type: "none"
      o: "bind"
      device: "{{LOG_PATH}}""#;

        let mut vars = HashMap::new();
        vars.insert("DATA_PATH".to_string(), "/opt/My Company/cadalytix/data".to_string());
        vars.insert("LOG_PATH".to_string(), "/opt/My Company/cadalytix/logs".to_string());

        let result = substitute_placeholders(template, &vars);

        // Verify no unresolved placeholders remain
        assert!(find_unresolved_placeholder(&result).is_none());

        // Verify device paths with spaces are correctly quoted in output
        // The quotes must be preserved around the path
        assert!(
            result.contains(r#"device: "/opt/My Company/cadalytix/data""#),
            "DATA_PATH device value must be quoted in output"
        );
        assert!(
            result.contains(r#"device: "/opt/My Company/cadalytix/logs""#),
            "LOG_PATH device value must be quoted in output"
        );
    }

    #[test]
    fn parse_docker_load_output_ignores_loaded_image_id_lines() {
        // TASK D: "Loaded image ID:" lines should be ignored
        let stdout = "Loading layer...\n\
                      Loaded image ID: sha256:abc123def456789\n\
                      Loaded image: myapp:latest\n\
                      Loaded image ID: sha256:xyz987654321\n";
        let images = parse_docker_load_output(stdout);

        // Only "Loaded image:" (not "Loaded image ID:") should be collected
        assert_eq!(images.len(), 1);
        assert_eq!(images[0], "myapp:latest");
    }

    #[test]
    fn parse_docker_load_output_handles_multiple_loaded_image_lines() {
        // TASK D: Multiple "Loaded image:" lines
        let stdout = "Loaded image: app1:v1.0\n\
                      Loaded image: app2:v2.0\n\
                      Loaded image: app3:latest\n";
        let images = parse_docker_load_output(stdout);

        assert_eq!(images.len(), 3);
        assert_eq!(images[0], "app1:v1.0");
        assert_eq!(images[1], "app2:v2.0");
        assert_eq!(images[2], "app3:latest");
    }

    #[test]
    fn parse_docker_load_output_mixed_image_and_image_id_lines() {
        // TASK D: Mixed "Loaded image:" and "Loaded image ID:" lines
        let stdout = "Loaded image ID: sha256:111\n\
                      Loaded image: first:1.0\n\
                      Loaded image ID: sha256:222\n\
                      Loaded image: second:2.0\n\
                      Loaded image ID: sha256:333\n";
        let images = parse_docker_load_output(stdout);

        assert_eq!(images.len(), 2);
        assert_eq!(images[0], "first:1.0");
        assert_eq!(images[1], "second:2.0");
    }
}