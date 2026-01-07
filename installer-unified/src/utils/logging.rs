use std::path::PathBuf;
use chrono::Local;
use std::fs;

pub fn initialize(log_folder: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    // Create log folder if it doesn't exist
    fs::create_dir_all(log_folder)?;
    
    // Create log file with timestamp
    let timestamp = Local::now().format("%Y-%m-%d-%H%M%S");
    let log_file = log_folder.join(format!("installer-{}.log", timestamp));
    
    // Initialize env_logger
    env_logger::Builder::from_default_env()
        .target(env_logger::Target::File(log_file.clone()))
        .filter_level(log::LevelFilter::Debug)
        .format(|buf, record| {
            use std::io::Write;
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            writeln!(
                buf,
                "[{}] [{}] [{}] {}",
                timestamp,
                record.level(),
                record.module_path().unwrap_or("unknown"),
                record.args()
            )
        })
        .init();
    
    log::info!("[PHASE: initialization] Logging initialized: {}", log_file.display());
    
    Ok(())
}

