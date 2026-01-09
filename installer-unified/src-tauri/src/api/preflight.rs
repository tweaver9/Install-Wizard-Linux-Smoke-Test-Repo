// Preflight API endpoints
// Ported from C# InstallerPreflightEndpoints.cs

use crate::database::connection::DatabaseConnection;
use crate::models::requests::{
    PreflightDataSourceRequestDto, PreflightHostRequestDto, PreflightPermissionsRequestDto,
};
use crate::models::responses::{
    ApiResponse, DiscoveredColumnDto, PreflightCheckDto, PreflightDataSourceResponseDto,
    PreflightHostResponseDto, PreflightPermissionsResponseDto, SampleStatsDto,
};
use crate::utils::logging::mask_connection_string;
use crate::utils::validation::{validate_and_quote_sql_server_object, validate_connection_string};
use futures::TryStreamExt;
use log::{info, warn};
use tiberius::QueryItem;

fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn os_description() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

// Tauri command handlers for preflight API

#[tauri::command]
pub async fn preflight_host(
    payload: Option<PreflightHostRequestDto>,
) -> Result<ApiResponse<PreflightHostResponseDto>, String> {
    let strict_mode = payload.map(|p| p.strict_mode).unwrap_or(false);
    info!(
        "[PHASE: preflight] [STEP: host] Host preflight check requested (strict_mode={})",
        strict_mode
    );

    let machine = machine_name();
    let os_desc = os_description();
    let is_windows = cfg!(windows);

    let mut checks: Vec<PreflightCheckDto> = Vec::new();

    // Windows Server detection (best-effort)
    let is_windows_server = false;
    #[cfg(windows)]
    {
        // If we can detect product name, mark server vs workstation.
        // Best-effort: use env vars and treat unknown as workstation.
        if let Ok(product) = std::env::var("OS") {
            // "Windows_NT" doesn't indicate Server; keep false.
            let _ = product;
        }
    }

    if is_windows {
        // We can't reliably detect Server SKU without extra APIs; keep best-effort flag.
        checks.push(PreflightCheckDto {
            name: "Operating System".to_string(),
            status: if is_windows_server {
                "Pass".to_string()
            } else if strict_mode {
                "Fail".to_string()
            } else {
                "Warn".to_string()
            },
            detail: if is_windows_server {
                format!("Running on Windows Server: {}", os_desc)
            } else {
                format!("Running on Windows: {}", os_desc)
            },
        });
    } else {
        checks.push(PreflightCheckDto {
            name: "Operating System".to_string(),
            status: if strict_mode {
                "Fail".to_string()
            } else {
                "Warn".to_string()
            },
            detail: format!("Running on non-Windows OS: {}", os_desc),
        });
    }

    // Windows prerequisites (best-effort; strict_mode can elevate to Fail)
    #[cfg(windows)]
    {
        // .NET Runtime (plan calls out .NET 8.0)
        match crate::installation::windows::check_dotnet_runtime_8_installed().await {
            Ok(installed) => {
                checks.push(PreflightCheckDto {
                    name: ".NET 8 Runtime".to_string(),
                    status: if installed {
                        "Pass".to_string()
                    } else if strict_mode {
                        "Fail".to_string()
                    } else {
                        "Warn".to_string()
                    },
                    detail: if installed {
                        ".NET 8 runtime detected".to_string()
                    } else {
                        ".NET 8 runtime not detected (install from prerequisites if required)"
                            .to_string()
                    },
                });
            }
            Err(e) => {
                warn!(
                    "[PHASE: preflight] [STEP: host] .NET runtime check error: {}",
                    e
                );
                checks.push(PreflightCheckDto {
                    name: ".NET 8 Runtime".to_string(),
                    status: if strict_mode {
                        "Fail".to_string()
                    } else {
                        "Warn".to_string()
                    },
                    detail: "Unable to determine .NET runtime status. Please check logs."
                        .to_string(),
                });
            }
        }

        // Disk space check (plan: ~1 GB minimum on system drive)
        match crate::installation::windows::get_free_space_bytes_ps("C").await {
            Ok(bytes) => {
                let min_bytes: u64 = 1_000_000_000;
                let ok = bytes >= min_bytes;
                checks.push(PreflightCheckDto {
                    name: "Disk Space (C:)".to_string(),
                    status: if ok {
                        "Pass".to_string()
                    } else {
                        "Fail".to_string()
                    },
                    detail: format!(
                        "Free space: {} MB (minimum: {} MB)",
                        bytes / 1_000_000,
                        min_bytes / 1_000_000
                    ),
                });
            }
            Err(e) => {
                warn!(
                    "[PHASE: preflight] [STEP: host] Disk space check error: {}",
                    e
                );
                checks.push(PreflightCheckDto {
                    name: "Disk Space (C:)".to_string(),
                    status: if strict_mode {
                        "Fail".to_string()
                    } else {
                        "Warn".to_string()
                    },
                    detail: "Unable to determine free disk space. Please check logs.".to_string(),
                });
            }
        }

        // WebView2 runtime is required to run Tauri on Windows; if we are running, it's likely present.
        checks.push(PreflightCheckDto {
            name: "WebView2 Runtime".to_string(),
            status: "Pass".to_string(),
            detail: "WebView2 runtime assumed present (installer is running)".to_string(),
        });
    }

    // Linux-specific preflight checks
    #[cfg(target_os = "linux")]
    {
        use std::path::Path;

        // Linux distro detection
        match crate::installation::linux::detect_linux_distro().await {
            Ok(distro) => {
                checks.push(PreflightCheckDto {
                    name: "Linux Distribution".to_string(),
                    status: "Pass".to_string(),
                    detail: format!(
                        "{} (id={}, version={})",
                        distro.pretty_name, distro.id, distro.version_id
                    ),
                });
            }
            Err(e) => {
                warn!(
                    "[PHASE: preflight] [STEP: host] Linux distro detection error: {}",
                    e
                );
                checks.push(PreflightCheckDto {
                    name: "Linux Distribution".to_string(),
                    status: if strict_mode {
                        "Fail".to_string()
                    } else {
                        "Warn".to_string()
                    },
                    detail: "Unable to detect Linux distribution. Please check logs.".to_string(),
                });
            }
        }

        // Linux disk space check (minimum 1 GB on root filesystem)
        match crate::installation::linux::get_free_space_bytes_linux(Path::new("/")).await {
            Ok(bytes) => {
                let min_bytes: u64 = 1_000_000_000; // 1 GB
                let ok = bytes >= min_bytes;
                checks.push(PreflightCheckDto {
                    name: "Disk Space (/)".to_string(),
                    status: if ok {
                        "Pass".to_string()
                    } else {
                        "Fail".to_string()
                    },
                    detail: format!(
                        "Free space: {} MB (minimum: {} MB)",
                        bytes / 1_000_000,
                        min_bytes / 1_000_000
                    ),
                });
            }
            Err(e) => {
                warn!(
                    "[PHASE: preflight] [STEP: host] Linux disk space check error: {}",
                    e
                );
                checks.push(PreflightCheckDto {
                    name: "Disk Space (/)".to_string(),
                    status: if strict_mode {
                        "Fail".to_string()
                    } else {
                        "Warn".to_string()
                    },
                    detail: "Unable to determine free disk space. Please check logs.".to_string(),
                });
            }
        }

        // Linux memory check (minimum 512 MB available)
        match crate::installation::linux::get_available_memory_mb().await {
            Ok(mb) => {
                let min_mb: u64 = 512;
                let ok = mb >= min_mb;
                checks.push(PreflightCheckDto {
                    name: "Available Memory".to_string(),
                    status: if ok {
                        "Pass".to_string()
                    } else {
                        "Fail".to_string()
                    },
                    detail: format!("Available: {} MB (minimum: {} MB)", mb, min_mb),
                });
            }
            Err(e) => {
                warn!(
                    "[PHASE: preflight] [STEP: host] Linux memory check error: {}",
                    e
                );
                checks.push(PreflightCheckDto {
                    name: "Available Memory".to_string(),
                    status: if strict_mode {
                        "Fail".to_string()
                    } else {
                        "Warn".to_string()
                    },
                    detail: "Unable to determine available memory. Please check logs.".to_string(),
                });
            }
        }

        // Docker checks (informational - detect presence and daemon status)
        // Note: Docker mode determination happens elsewhere; this is informational.
        match crate::installation::docker::get_docker_version().await {
            Ok(version) => {
                checks.push(PreflightCheckDto {
                    name: "Docker".to_string(),
                    status: "Pass".to_string(),
                    detail: format!(
                        "Docker installed (v{}.{}.{})",
                        version.major, version.minor, version.patch
                    ),
                });

                // If Docker is installed, also check if daemon is running
                match crate::installation::docker::is_docker_daemon_running().await {
                    Ok(true) => {
                        checks.push(PreflightCheckDto {
                            name: "Docker Daemon".to_string(),
                            status: "Pass".to_string(),
                            detail: "Docker daemon is running".to_string(),
                        });
                    }
                    Ok(false) => {
                        checks.push(PreflightCheckDto {
                            name: "Docker Daemon".to_string(),
                            status: "Warn".to_string(),
                            detail: "Docker daemon is not running or not accessible".to_string(),
                        });
                    }
                    Err(e) => {
                        warn!(
                            "[PHASE: preflight] [STEP: host] Docker daemon check error: {}",
                            e
                        );
                        checks.push(PreflightCheckDto {
                            name: "Docker Daemon".to_string(),
                            status: "Warn".to_string(),
                            detail: "Unable to check Docker daemon status".to_string(),
                        });
                    }
                }
            }
            Err(_) => {
                // Docker not installed - informational only, not a failure
                checks.push(PreflightCheckDto {
                    name: "Docker".to_string(),
                    status: "Warn".to_string(),
                    detail: "Docker not detected (required for Docker deployment mode)".to_string(),
                });
            }
        }
    }

    // Domain membership (best-effort, Windows-only heuristic)
    let mut is_domain_joined = false;
    if is_windows {
        let user_domain = std::env::var("USERDOMAIN").unwrap_or_default();
        is_domain_joined = !user_domain.is_empty() && !user_domain.eq_ignore_ascii_case(&machine);
        checks.push(PreflightCheckDto {
            name: "Domain Membership".to_string(),
            status: if is_domain_joined {
                "Pass".to_string()
            } else {
                "Warn".to_string()
            },
            detail: if is_domain_joined {
                format!("Machine appears domain-joined: {}", user_domain)
            } else {
                "Machine does not appear to be domain-joined".to_string()
            },
        });
    }

    // IIS hosting (best-effort)
    let iis_vars = [
        "ASPNETCORE_IIS_HTTPAUTH",
        "ASPNETCORE_IIS_PHYSICAL_PATH",
        "APP_POOL_ID",
    ];
    let is_iis_hosting = iis_vars
        .iter()
        .any(|v| std::env::var(v).ok().filter(|s| !s.is_empty()).is_some());
    checks.push(PreflightCheckDto {
        name: "IIS Hosting".to_string(),
        status: if is_iis_hosting {
            "Pass".to_string()
        } else {
            "Warn".to_string()
        },
        detail: if is_iis_hosting {
            "Application appears hosted in IIS".to_string()
        } else {
            "Application does not appear hosted in IIS".to_string()
        },
    });

    // Container detection (best-effort)
    let is_container = std::env::var("DOTNET_RUNNING_IN_CONTAINER")
        .ok()
        .filter(|s| !s.is_empty())
        .is_some();
    checks.push(PreflightCheckDto {
        name: "Container Detection".to_string(),
        status: "Pass".to_string(),
        detail: if is_container {
            "Application appears to be running in a container".to_string()
        } else {
            "Application does not appear to be running in a container".to_string()
        },
    });

    let overall_status = if checks.iter().any(|c| c.status == "Fail") {
        "Fail"
    } else if checks.iter().any(|c| c.status == "Warn") {
        "Warn"
    } else {
        "Pass"
    };

    Ok(ApiResponse::ok(PreflightHostResponseDto {
        machine_name: machine,
        os_description: os_desc,
        is_windows,
        is_windows_server,
        is_domain_joined,
        is_iis_hosting,
        is_container,
        checks,
        overall_status: overall_status.to_string(),
    }))
}

#[tauri::command]
pub async fn preflight_permissions(
    payload: Option<PreflightPermissionsRequestDto>,
) -> Result<ApiResponse<PreflightPermissionsResponseDto>, String> {
    info!("[PHASE: preflight] [STEP: permissions] Permissions preflight check requested");

    let Some(req) = payload else {
        return Ok(ApiResponse::fail("Invalid request: body is required"));
    };

    if let Err(e) = validate_connection_string(&req.config_db_connection_string) {
        return Ok(ApiResponse::fail(format!(
            "Invalid ConfigDbConnectionString: {}",
            e
        )));
    }
    if let Err(e) = validate_connection_string(&req.call_data_connection_string) {
        return Ok(ApiResponse::fail(format!(
            "Invalid CallDataConnectionString: {}",
            e
        )));
    }
    if req.source_object_name.trim().is_empty() {
        return Ok(ApiResponse::fail("SourceObjectName is required"));
    }

    // NOTE: For now we implement SQL Server checks (matches current C# host).
    let mut checks: Vec<PreflightCheckDto> = Vec::new();
    let mut overall_pass = true;
    let mut remediation = "All permissions are valid.".to_string();

    // Config DB: connectivity + membership checks
    match DatabaseConnection::sql_server(&req.config_db_connection_string).await {
        Ok(conn) => {
            if let Some(client_arc) = conn.as_sql_server() {
                let mut client = client_arc.lock().await;

                // Connectivity
                let ok = client.simple_query("SELECT 1").await.is_ok();
                checks.push(PreflightCheckDto {
                    name: "Config DB connectivity".to_string(),
                    status: if ok {
                        "Pass".to_string()
                    } else {
                        "Fail".to_string()
                    },
                    detail: if ok {
                        "Connected to config DB".to_string()
                    } else {
                        "Unable to connect to config DB".to_string()
                    },
                });
                if !ok {
                    overall_pass = false;
                    remediation =
                        "Verify config DB connection string and network access.".to_string();
                }

                // DDL/DML permission heuristics (db_owner / db_ddladmin / db_datawriter)
                let db_owner = scalar_int(&mut client, "SELECT IS_MEMBER('db_owner')")
                    .await
                    .unwrap_or(0)
                    == 1;
                let db_ddladmin = scalar_int(&mut client, "SELECT IS_MEMBER('db_ddladmin')")
                    .await
                    .unwrap_or(0)
                    == 1;
                let db_datawriter = scalar_int(&mut client, "SELECT IS_MEMBER('db_datawriter')")
                    .await
                    .unwrap_or(0)
                    == 1;

                if req.require_config_db_ddl {
                    let ok = db_owner || db_ddladmin;
                    checks.push(PreflightCheckDto {
                        name: "Config DB DDL permissions".to_string(),
                        status: if ok {
                            "Pass".to_string()
                        } else {
                            "Fail".to_string()
                        },
                        detail: if ok {
                            "DDL permissions appear sufficient".to_string()
                        } else {
                            "Missing DDL permissions (need db_owner or db_ddladmin)".to_string()
                        },
                    });
                    if !ok {
                        overall_pass = false;
                        remediation = "Grant db_owner (preferred) or db_ddladmin on the config database user.".to_string();
                    }
                }

                if req.require_config_db_dml {
                    let ok = db_owner || db_datawriter;
                    checks.push(PreflightCheckDto {
                        name: "Config DB DML permissions".to_string(),
                        status: if ok {
                            "Pass".to_string()
                        } else {
                            "Fail".to_string()
                        },
                        detail: if ok {
                            "DML permissions appear sufficient".to_string()
                        } else {
                            "Missing DML permissions (need db_owner or db_datawriter)".to_string()
                        },
                    });
                    if !ok {
                        overall_pass = false;
                        remediation = "Grant db_owner (preferred) or db_datawriter on the config database user.".to_string();
                    }
                }
            } else {
                warn!(
                    "[PHASE: preflight] [STEP: permissions] Internal error: missing SQL Server client for config DB (masked={})",
                    mask_connection_string(&req.config_db_connection_string)
                );
                overall_pass = false;
                remediation =
                    "Internal error: SQL Server client unavailable. Please check logs.".to_string();
                checks.push(PreflightCheckDto {
                    name: "Config DB connectivity".to_string(),
                    status: "Fail".to_string(),
                    detail: "Internal error: SQL Server client unavailable".to_string(),
                });
            }
        }
        Err(e) => {
            warn!(
                "[PHASE: preflight] [STEP: permissions] Failed to connect to config DB: {} (masked={})",
                e,
                mask_connection_string(&req.config_db_connection_string)
            );
            overall_pass = false;
            remediation =
                "Unable to connect to config DB. Verify connection string and network access."
                    .to_string();
            checks.push(PreflightCheckDto {
                name: "Config DB connectivity".to_string(),
                status: "Fail".to_string(),
                detail: "Unable to connect to config DB".to_string(),
            });
        }
    }

    // Call data read check (SQL Server only for now)
    if req.require_call_data_read {
        match DatabaseConnection::sql_server(&req.call_data_connection_string).await {
            Ok(conn) => {
                if let Some(client_arc) = conn.as_sql_server() {
                    let mut client = client_arc.lock().await;

                    match validate_and_quote_sql_server_object(&req.source_object_name) {
                        Ok(quoted) => {
                            let sql = format!("SELECT TOP 1 * FROM {}", quoted);
                            let ok = client.simple_query(sql).await.is_ok();
                            checks.push(PreflightCheckDto {
                                name: "Call data read access".to_string(),
                                status: if ok {
                                    "Pass".to_string()
                                } else {
                                    "Fail".to_string()
                                },
                                detail: if ok {
                                    "Able to read from call data source object".to_string()
                                } else {
                                    "Unable to read from call data source object".to_string()
                                },
                            });
                            if !ok {
                                overall_pass = false;
                                remediation = "Grant SELECT on the call data source object to the call data user.".to_string();
                            }
                        }
                        Err(e) => {
                            overall_pass = false;
                            remediation = format!("Invalid SourceObjectName: {}", e);
                            checks.push(PreflightCheckDto {
                                name: "Call data source object name".to_string(),
                                status: "Fail".to_string(),
                                detail: format!("Invalid source object name: {}", e),
                            });
                        }
                    }
                } else {
                    warn!(
                        "[PHASE: preflight] [STEP: permissions] Internal error: missing SQL Server client for call data DB (masked={})",
                        mask_connection_string(&req.call_data_connection_string)
                    );
                    overall_pass = false;
                    remediation =
                        "Internal error: SQL Server client unavailable. Please check logs."
                            .to_string();
                    checks.push(PreflightCheckDto {
                        name: "Call data DB connectivity".to_string(),
                        status: "Fail".to_string(),
                        detail: "Internal error: SQL Server client unavailable".to_string(),
                    });
                }
            }
            Err(_) => {
                overall_pass = false;
                remediation = "Unable to connect to call data DB. Verify connection string and network access.".to_string();
                checks.push(PreflightCheckDto {
                    name: "Call data DB connectivity".to_string(),
                    status: "Fail".to_string(),
                    detail: "Unable to connect to call data DB".to_string(),
                });
            }
        }
    }

    Ok(ApiResponse::ok(PreflightPermissionsResponseDto {
        checks,
        overall_status: if overall_pass {
            "Pass".to_string()
        } else {
            "Fail".to_string()
        },
        recommended_remediation: remediation,
    }))
}

#[tauri::command]
pub async fn preflight_datasource(
    payload: PreflightDataSourceRequestDto,
) -> Result<ApiResponse<PreflightDataSourceResponseDto>, String> {
    info!("[PHASE: preflight] [STEP: datasource] Data source preflight check requested");

    // Explicit demo mode for schema mapping UX: no DB required.
    if payload.demo_mode {
        let demo = vec![
            DiscoveredColumnDto {
                name: "CallReceivedAt".to_string(),
                data_type: "datetime".to_string(),
                is_nullable: false,
            },
            DiscoveredColumnDto {
                name: "IncidentNumber".to_string(),
                data_type: "nvarchar".to_string(),
                is_nullable: false,
            },
            // Duplicates to validate disambiguation: City (1) / City (2)
            DiscoveredColumnDto {
                name: "City".to_string(),
                data_type: "nvarchar".to_string(),
                is_nullable: true,
            },
            DiscoveredColumnDto {
                name: "City".to_string(),
                data_type: "nvarchar".to_string(),
                is_nullable: true,
            },
            DiscoveredColumnDto {
                name: "State".to_string(),
                data_type: "nvarchar".to_string(),
                is_nullable: true,
            },
            DiscoveredColumnDto {
                name: "Zip".to_string(),
                data_type: "nvarchar".to_string(),
                is_nullable: true,
            },
        ];
        let checks = vec![PreflightCheckDto {
            name: "Demo mode".to_string(),
            status: "Pass".to_string(),
            detail: "Using built-in demo headers (no database connection).".to_string(),
        }];
        return Ok(ApiResponse::ok(PreflightDataSourceResponseDto {
            checks,
            overall_status: "Pass".to_string(),
            discovered_columns: demo,
            sample_stats: SampleStatsDto {
                sample_count: 0,
                min_call_received_at: None,
                max_call_received_at: None,
            },
        }));
    }

    if let Err(e) = validate_connection_string(&payload.call_data_connection_string) {
        return Ok(ApiResponse::fail(format!(
            "Invalid CallDataConnectionString: {}",
            e
        )));
    }
    if payload.source_object_name.trim().is_empty() {
        return Ok(ApiResponse::fail("SourceObjectName is required"));
    }

    let mut checks: Vec<PreflightCheckDto> = Vec::new();
    let mut discovered: Vec<DiscoveredColumnDto> = Vec::new();

    match DatabaseConnection::sql_server(&payload.call_data_connection_string).await {
        Ok(conn) => {
            let Some(client_arc) = conn.as_sql_server() else {
                checks.push(PreflightCheckDto {
                    name: "Call data DB connectivity".to_string(),
                    status: "Fail".to_string(),
                    detail: "Internal error: SQL Server client unavailable".to_string(),
                });
                return Ok(ApiResponse::ok(PreflightDataSourceResponseDto {
                    checks,
                    overall_status: "Fail".to_string(),
                    discovered_columns: vec![],
                    sample_stats: SampleStatsDto {
                        sample_count: 0,
                        min_call_received_at: None,
                        max_call_received_at: None,
                    },
                }));
            };
            let mut client = client_arc.lock().await;

            // Validate + quote source object
            let quoted = match validate_and_quote_sql_server_object(&payload.source_object_name) {
                Ok(q) => q,
                Err(e) => {
                    checks.push(PreflightCheckDto {
                        name: "Source object name".to_string(),
                        status: "Fail".to_string(),
                        detail: format!("Invalid SourceObjectName: {}", e),
                    });
                    return Ok(ApiResponse::ok(PreflightDataSourceResponseDto {
                        checks,
                        overall_status: "Fail".to_string(),
                        discovered_columns: vec![],
                        sample_stats: SampleStatsDto {
                            sample_count: 0,
                            min_call_received_at: None,
                            max_call_received_at: None,
                        },
                    }));
                }
            };

            // Connectivity check + sample query (no data returned to UI)
            let sample_sql = format!(
                "SELECT TOP ({}) * FROM {}",
                payload.sample_limit.max(1),
                quoted
            );
            let ok = client.simple_query(sample_sql).await.is_ok();
            checks.push(PreflightCheckDto {
                name: "Sample query".to_string(),
                status: if ok {
                    "Pass".to_string()
                } else {
                    "Fail".to_string()
                },
                detail: if ok {
                    "Sample query succeeded".to_string()
                } else {
                    "Sample query failed".to_string()
                },
            });

            // Best-effort column discovery via INFORMATION_SCHEMA (requires schema + table)
            if let Some((schema, table)) = split_schema_table(&payload.source_object_name) {
                let mut query = tiberius::Query::new(
                    r#"
                    SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE
                    FROM INFORMATION_SCHEMA.COLUMNS
                    WHERE TABLE_SCHEMA = @P1 AND TABLE_NAME = @P2
                    ORDER BY ORDINAL_POSITION
                    "#,
                );
                query.bind(schema.as_str());
                query.bind(table.as_str());

                if let Ok(mut stream) = query.query(&mut *client).await {
                    while let Ok(Some(item)) = stream.try_next().await {
                        if let QueryItem::Row(row) = item {
                            let name = row.get::<&str, _>(0).unwrap_or("").to_string();
                            let data_type = row.get::<&str, _>(1).unwrap_or("").to_string();
                            let is_nullable_str = row.get::<&str, _>(2).unwrap_or("NO");
                            discovered.push(DiscoveredColumnDto {
                                name,
                                data_type,
                                is_nullable: is_nullable_str.eq_ignore_ascii_case("YES"),
                            });
                        }
                    }
                }
            }
        }
        Err(e) => {
            checks.push(PreflightCheckDto {
                name: "Call data DB connectivity".to_string(),
                status: "Fail".to_string(),
                detail: format!("Unable to connect to call data DB: {}", e),
            });
        }
    }

    // Mapping requires headers; fail cleanly if none were discovered.
    if discovered.is_empty() {
        return Ok(ApiResponse::fail(
            "No headers could be detected for the selected source. Verify Source object name and permissions.".to_string(),
        ));
    }

    let overall_status = if checks.iter().any(|c| c.status == "Fail") {
        "Fail".to_string()
    } else {
        "Pass".to_string()
    };

    Ok(ApiResponse::ok(PreflightDataSourceResponseDto {
        checks,
        overall_status,
        discovered_columns: discovered,
        sample_stats: SampleStatsDto {
            sample_count: 0,
            min_call_received_at: None,
            max_call_received_at: None,
        },
    }))
}

fn split_schema_table(source_object_name: &str) -> Option<(String, String)> {
    // Accept schema-qualified (schema.table). If not provided, default schema is "dbo".
    let trimmed = source_object_name.trim().trim_matches(['[', ']']);
    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.len() {
        1 => Some((
            "dbo".to_string(),
            parts[0].trim_matches(['[', ']']).to_string(),
        )),
        2 => Some((
            parts[0].trim_matches(['[', ']']).to_string(),
            parts[1].trim_matches(['[', ']']).to_string(),
        )),
        _ => None,
    }
}

async fn scalar_int(
    client: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    sql: &str,
) -> Option<i32> {
    let mut stream = client.simple_query(sql).await.ok()?;
    while let Ok(Some(item)) = stream.try_next().await {
        if let QueryItem::Row(row) = item {
            return row.get::<i32, _>(0);
        }
    }
    None
}
