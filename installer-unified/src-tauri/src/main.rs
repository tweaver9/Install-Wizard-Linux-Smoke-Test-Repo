// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Phase 8: Release E2E smoke - runs all proof modes in sequence.
    // Writes `P8_release_e2e_smoke_<os>.log` under `Prod_Wizard_Log/` and exits 0/1.
    if args.iter().any(|a| a == "--release-e2e-smoke") {
        installer_unified::run_release_e2e_smoke();
        return;
    }

    // Phase 8: Performance smoke - measures startup time and progress metrics.
    // Writes `P8_perf_<os>.log` under `Prod_Wizard_Log/` and exits 0/1.
    if args.iter().any(|a| a == "--perf-smoke") {
        installer_unified::run_perf_smoke();
        return;
    }

    // Non-interactive archive pipeline dry-run (deterministic proof runner).
    // Writes `B2_archive_pipeline_dryrun_transcript.log` under `Prod_Wizard_Log/` and exits.
    if args.iter().any(|a| a == "--archive-dry-run") {
        installer_unified::run_archive_dry_run();
        return;
    }

    // Non-interactive mapping contract + persistence proof mode (deterministic).
    // Writes `B3_mapping_persist_smoke_transcript.log` under `Prod_Wizard_Log/` and exits.
    if args.iter().any(|a| a == "--mapping-persist-smoke") {
        installer_unified::run_mapping_persist_smoke();
        return;
    }

    // Non-interactive install contract proof mode (for automated checks / log capture).
    // Prints a short event transcript and exits 0.
    if args.iter().any(|a| a == "--install-contract-smoke") {
        installer_unified::run_install_contract_smoke();
        return;
    }

    // D2 Database Setup proof mode (deterministic).
    // Writes `D2_db_setup_smoke_transcript.log` under `Prod_Wizard_Log/` and exits.
    if args.iter().any(|a| a == "--db-setup-smoke") {
        installer_unified::run_db_setup_smoke();
        return;
    }

    // Non-interactive TUI smoke test mode (for automated checks).
    // Renders a single frame for a specific page and exits 0.
    // Usage: --tui-smoke or --tui-smoke=welcome|license|destination|db|storage|retention|archive|consent|mapping|ready|progress
    if let Some(arg) = args
        .iter()
        .find(|a| a.as_str() == "--tui-smoke" || a.starts_with("--tui-smoke="))
    {
        let target = arg
            .split_once('=')
            .map(|(_, v)| v.to_string())
            .filter(|v| !v.trim().is_empty());
        installer_unified::run_tui_smoke(target);
        return;
    }

    // Linux launcher behavior:
    // - If GUI display available -> run GUI wizard
    // - Otherwise -> run headless TUI wizard
    // Overrides:
    // - CLI flag --tui or --cli forces TUI
    // - CLI flag --gui forces GUI
    // - Env var CADALYTIX_INSTALLER_UI=gui|tui|auto
    #[cfg(target_os = "linux")]
    {
        let force_gui = args.iter().any(|a| a == "--gui");
        let force_tui = args.iter().any(|a| a == "--tui" || a == "--cli");
        let env_pref = std::env::var("CADALYTIX_INSTALLER_UI")
            .ok()
            .unwrap_or_else(|| "auto".to_string());
        let env_pref = env_pref.trim().to_ascii_lowercase();

        let has_display = std::env::var_os("WAYLAND_DISPLAY")
            .filter(|v| !v.is_empty())
            .is_some()
            || std::env::var_os("DISPLAY")
                .filter(|v| !v.is_empty())
                .is_some();

        // If GUI is explicitly requested but no display is available, fail fast with a clean message.
        if (force_gui || env_pref == "gui") && !has_display {
            eprintln!(
                "CADalytix Setup: No GUI display detected (DISPLAY/WAYLAND_DISPLAY not set)."
            );
            eprintln!(
                "Use --tui/--cli or set CADALYTIX_INSTALLER_UI=tui to run the headless installer."
            );
            std::process::exit(2);
        }

        let run_tui = if force_gui {
            false
        } else if force_tui {
            true
        } else if env_pref == "gui" {
            false
        } else if env_pref == "tui" {
            true
        } else {
            // auto
            !has_display
        };

        if run_tui {
            installer_unified::run_tui();
        } else {
            installer_unified::run_gui();
        }
        return;
    }

    // Windows (and other platforms): always run GUI wizard.
    installer_unified::run_gui();
}
