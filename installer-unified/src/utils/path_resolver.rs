use std::path::{Path, PathBuf};
use log::{info, debug};

/// Resolve deployment folder (absolute path)
pub fn resolve_deployment_folder() -> PathBuf {
    // Try current directory first (if running from deployment folder)
    if let Ok(current) = std::env::current_dir() {
        if current.to_string_lossy().contains("Prod_Install_Wizard_Deployment") {
            if let Ok(canonical) = current.canonicalize() {
                debug!("[PATH] Deployment folder (from current dir): {}", canonical.display());
                return canonical;
            }
        }
    }
    
    // Fallback to repo location (absolute path)
    let path = PathBuf::from(r"F:\Prod_Install_Wizard_Deployment");
    if let Ok(canonical) = path.canonicalize() {
        debug!("[PATH] Deployment folder (from absolute): {}", canonical.display());
        return canonical;
    }
    
    // If canonicalize fails, return the path as-is
    info!("[PATH] Deployment folder (fallback): {}", path.display());
    path
}

/// Resolve log folder (absolute path)
pub fn resolve_log_folder() -> PathBuf {
    // Try relative to deployment folder first
    let deployment = resolve_deployment_folder();
    if let Some(parent) = deployment.parent() {
        let relative_log = parent.join("Prod_Wizard_Log");
        if relative_log.exists() {
            if let Ok(canonical) = relative_log.canonicalize() {
                debug!("[PATH] Log folder (relative to deployment): {}", canonical.display());
                return canonical;
            }
        }
    }
    
    // Fallback to repo location (absolute path)
    let path = PathBuf::from(r"F:\Prod_Wizard_Log");
    if let Ok(canonical) = path.canonicalize() {
        debug!("[PATH] Log folder (from absolute): {}", canonical.display());
        return canonical;
    }
    
    // Create if doesn't exist
    if !path.exists() {
        if let Err(e) = std::fs::create_dir_all(&path) {
            eprintln!("[ERROR] Failed to create log folder: {}", e);
        }
    }
    
    info!("[PATH] Log folder (fallback): {}", path.display());
    path
}

/// Resolve migration bundle path (absolute path)
pub fn resolve_migration_bundle(engine: &str, version: &str) -> Result<PathBuf, String> {
    let deployment = resolve_deployment_folder();
    let bundle_name = format!("migrations-{}-v{}.cadalytix-bundle", engine, version);
    let bundle_path = deployment
        .join("installer")
        .join("migrations")
        .join(bundle_name);
    
    if let Ok(canonical) = bundle_path.canonicalize() {
        debug!("[PATH] Migration bundle: {}", canonical.display());
        Ok(canonical)
    } else {
        Err(format!("Migration bundle not found: {}", bundle_path.display()))
    }
}

/// Resolve UI path (absolute path)
pub fn resolve_ui_path() -> PathBuf {
    let deployment = resolve_deployment_folder();
    let ui_path = deployment
        .join("installer-unified")
        .join("frontend")
        .join("dist")
        .join("index.html");
    
    debug!("[PATH] UI path: {}", ui_path.display());
    ui_path
}

