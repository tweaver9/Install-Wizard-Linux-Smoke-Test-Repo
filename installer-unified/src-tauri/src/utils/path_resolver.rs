use anyhow::Result;
use std::path::PathBuf;

/// Resolve deployment folder (absolute path)
pub fn resolve_deployment_folder() -> Result<PathBuf> {
    // Prefer the folder where the EXE is running from (works in dev and deployed)
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(dir) = exe_path.parent() {
            return Ok(dir.to_path_buf());
        }
    }

    // Fallback: current working directory
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    Ok(cwd)
}

/// Resolve log folder (absolute path)
pub fn resolve_log_folder() -> Result<PathBuf> {
    // Prefer a repo/workspace-level log folder. When running from nested dirs like
    // `.../installer-unified/src-tauri`, we MUST NOT create `Prod_Wizard_Log/` inside
    // those subdirectories.
    //
    // Strategy:
    // - Walk up from CWD looking for an existing `Prod_Wizard_Log/`
    // - Or a repo root marker `UNIFIED_CROSS_PLATFORM_INSTALLER_PLAN.md`, then use/create
    //   `<repo_root>/Prod_Wizard_Log/`
    if let Ok(mut dir) = std::env::current_dir() {
        for _ in 0..12 {
            let candidate = dir.join("Prod_Wizard_Log");
            if candidate.exists() {
                return Ok(candidate);
            }

            // Repo root marker (plan file lives at repo root in this project)
            if dir
                .join("UNIFIED_CROSS_PLATFORM_INSTALLER_PLAN.md")
                .exists()
            {
                std::fs::create_dir_all(&candidate)
                    .map_err(|e| anyhow::anyhow!("Failed to create log folder: {}", e))?;
                return Ok(candidate);
            }

            if let Some(parent) = dir.parent() {
                dir = parent.to_path_buf();
            } else {
                break;
            }
        }
    }

    // Fallback: base off the deployment folder (best-effort).
    let base = resolve_deployment_folder()?;
    let log_dir = base.join("Prod_Wizard_Log");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create log folder: {}", e))?;
    Ok(log_dir)
}

/// Resolve migration bundle path (absolute path)
#[allow(dead_code)]
pub fn resolve_migration_bundle(engine: &str, version: &str) -> Result<PathBuf> {
    let deployment = resolve_deployment_folder()?;
    let bundle_name = format!("migrations-{}-v{}.cadalytix-bundle", engine, version);
    let bundle_path = deployment
        .join("installer")
        .join("migrations")
        .join(bundle_name);

    // You can keep this strict if you want:
    bundle_path
        .canonicalize()
        .map_err(|_| anyhow::anyhow!("Migration bundle not found: {:?}", bundle_path))
}
