// Docker setup (Linux deployment option)
// Ported from the plan's Phase 5 guidance.

use anyhow::Result;
use log::{info, warn};
use std::path::Path;
use std::time::Duration;

use crate::installation::{run_cmd_with_timeout, CommandOutput};

#[allow(dead_code)]
const DOCKER_CMD_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ComposeInvocation {
    DockerComposeBinary,
    DockerSubcommand,
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

#[allow(dead_code)]
pub async fn detect_compose_invocation() -> Result<ComposeInvocation> {
    // Prefer docker-compose if available (matches plan), otherwise fall back to `docker compose`.
    let out = run_cmd_with_timeout(
        "docker-compose",
        &["--version".to_string()],
        Duration::from_secs(10),
        "docker_compose_version",
    )
    .await;
    if out.as_ref().ok().and_then(|o| o.exit_code) == Some(0) {
        return Ok(ComposeInvocation::DockerComposeBinary);
    }

    let out = run_cmd_with_timeout(
        "docker",
        &["compose".to_string(), "version".to_string()],
        Duration::from_secs(10),
        "docker_compose_subcommand_version",
    )
    .await;
    if out.as_ref().ok().and_then(|o| o.exit_code) == Some(0) {
        return Ok(ComposeInvocation::DockerSubcommand);
    }

    anyhow::bail!("Neither 'docker-compose' nor 'docker compose' is available");
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

#[allow(dead_code)]
pub async fn compose_up(inv: ComposeInvocation, compose_file: &Path) -> Result<()> {
    let f = compose_file
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid compose file path"))?;

    let (program, args) = match inv {
        ComposeInvocation::DockerComposeBinary => (
            "docker-compose",
            vec![
                "-f".to_string(),
                f.to_string(),
                "up".to_string(),
                "-d".to_string(),
            ],
        ),
        ComposeInvocation::DockerSubcommand => (
            "docker",
            vec![
                "compose".to_string(),
                "-f".to_string(),
                f.to_string(),
                "up".to_string(),
                "-d".to_string(),
            ],
        ),
    };

    info!(
        "[PHASE: installation] [STEP: docker] Starting containers via {:?}",
        inv
    );
    let out = run_cmd_with_timeout(program, &args, DOCKER_CMD_TIMEOUT, "compose_up").await?;
    if out.exit_code == Some(0) {
        return Ok(());
    }
    warn!(
        "[PHASE: installation] [STEP: docker] compose up failed: {}",
        out.stderr
    );
    anyhow::bail!("Docker compose up failed");
}

#[allow(dead_code)]
pub async fn compose_ps(inv: ComposeInvocation, compose_file: &Path) -> Result<CommandOutput> {
    let f = compose_file
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid compose file path"))?;

    let (program, args) = match inv {
        ComposeInvocation::DockerComposeBinary => (
            "docker-compose",
            vec!["-f".to_string(), f.to_string(), "ps".to_string()],
        ),
        ComposeInvocation::DockerSubcommand => (
            "docker",
            vec![
                "compose".to_string(),
                "-f".to_string(),
                f.to_string(),
                "ps".to_string(),
            ],
        ),
    };

    run_cmd_with_timeout(program, &args, DOCKER_CMD_TIMEOUT, "compose_ps").await
}

#[allow(dead_code)]
pub async fn compose_down(inv: ComposeInvocation, compose_file: &Path) -> Result<()> {
    let f = compose_file
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid compose file path"))?;

    let (program, args) = match inv {
        ComposeInvocation::DockerComposeBinary => (
            "docker-compose",
            vec!["-f".to_string(), f.to_string(), "down".to_string()],
        ),
        ComposeInvocation::DockerSubcommand => (
            "docker",
            vec![
                "compose".to_string(),
                "-f".to_string(),
                f.to_string(),
                "down".to_string(),
            ],
        ),
    };

    let out = run_cmd_with_timeout(program, &args, DOCKER_CMD_TIMEOUT, "compose_down").await?;
    if out.exit_code == Some(0) {
        return Ok(());
    }
    warn!(
        "[PHASE: installation] [STEP: docker] compose down failed: {}",
        out.stderr
    );
    anyhow::bail!("Docker compose down failed");
}
