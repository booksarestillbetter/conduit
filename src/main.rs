// src/main.rs
mod api;
mod auth;
mod config;
mod db;
mod downloader;
mod engines;
mod events;
mod ip_asn;
mod notify;
mod plex_client;
mod trakt_client;
mod fetcher;
mod web;

use auth::AppState;
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use clap::Parser;
use config::ConfigManager;
use db::Database;
use notify::NotificationManager;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use fetcher::FetcherPool;

#[derive(Parser, Debug)]
#[command(author, version, about = "Conduit - Unified Media Fetching Commander & Automation")]
struct Args {
    #[arg(short, long, env = "CONDUIT_CONFIG", default_value = "data/config.json")]
    config: String,

    #[arg(short, long, env = "CONDUIT_DB", default_value = "data/db.sqlite")]
    db: String,

    #[arg(long, env = "CONDUIT_DB_KEY")]
    db_key: Option<String>,

    #[arg(short, long, env = "CONDUIT_BIND")]
    bind: Option<String>,

    #[arg(short, long, env = "CONDUIT_PORT")]
    port: Option<u16>,

    #[arg(short, long, env = "CONDUIT_LOG", default_value = "info")]
    log_level: String,

    #[arg(long, env = "CONDUIT_SSL_CERT")]
    ssl_cert: Option<String>,

    #[arg(long, env = "CONDUIT_SSL_KEY")]
    ssl_key: Option<String>,

    #[arg(long, env = "CONDUIT_SSL_ENABLED")]
    ssl_enabled: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config_path = args.config;
    let db_path = args.db;
    let bind_opt = args.bind;
    let port_opt = args.port;
    let ssl_cert_opt = args.ssl_cert;
    let ssl_key_opt = args.ssl_key;
    let ssl_enabled_opt = args.ssl_enabled;

    // Setup logging
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("conduit={0},tower_http=info", args.log_level).into());

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting Conduit v{}", env!("CARGO_PKG_VERSION"));

    // Ensure data dir exists
    if let Some(parent) = std::path::Path::new(&config_path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // 1. Initialize Configuration Manager
    let config_mgr = ConfigManager::load_or_init(&config_path).await?;
    let mut config = config_mgr.get().await;

    // Apply CLI overrides
    if let Some(bind) = bind_opt {
        config.system.bind_addr = bind;
    }
    if let Some(port) = port_opt {
        config.system.port = port;
    }
    if let Some(cert) = ssl_cert_opt.clone() {
        config.system.ssl_cert = Some(cert);
    }
    if let Some(key) = ssl_key_opt.clone() {
        config.system.ssl_key = Some(key);
    }
    let ssl_enabled_cli = ssl_enabled_opt.as_deref().and_then(|val| {
        match val.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Some(true),
            "false" | "0" | "no" | "off" => Some(false),
            _ => None, // "auto" or unrecognized strings defer to config
        }
    });
    if let Some(enabled) = ssl_enabled_cli {
        config.system.ssl_enabled = enabled;
    }

    // Install Fatal Panic Hook with Backtraces & Post-Mortem Crash Log
    let crash_dir = config.system.data_dir.clone();
    std::panic::set_hook(Box::new(move |panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            *s
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.as_str()
        } else {
            "unknown panic payload"
        };

        let bt = std::backtrace::Backtrace::capture();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let crash_entry = format!(
            "[{}] 💥 FATAL PANIC at {}: {}\nBacktrace:\n{}\n\n",
            timestamp, location, message, bt
        );

        eprintln!("{}", crash_entry);
        error!("{}", crash_entry);

        let crash_file = std::path::Path::new(&crash_dir).join("crash.log");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_file)
        {
            use std::io::Write;
            let _ = file.write_all(crash_entry.as_bytes());
        }
    }));

    // 2. Initialize Encrypted SQLite Database (SQLCipher)
    let db_passphrase = if let Some(ref key) = args.db_key {
        key.clone()
    } else {
        let key_path = std::path::Path::new(&db_path).with_extension("key");
        if key_path.exists() {
            std::fs::read_to_string(&key_path)?.trim().to_string()
        } else if std::path::Path::new(&db_path).exists() {
            // Existing DB created prior to dedicated key generation
            config.system.jwt_secret.clone()
        } else {
            let new_key = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
            if let Err(e) = std::fs::write(&key_path, &new_key) {
                warn!("Could not write dedicated database key file to {}: {}", key_path.display(), e);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }
            info!("Generated dedicated SQLCipher database encryption key at {}", key_path.display());
            new_key
        }
    };
    let db = Database::init(&db_path, Some(&db_passphrase))?;
    info!("Encrypted SQLite database (SQLCipher) initialized at {}", db_path);

    // Process-wide WebSocket channel bus — wired into Database now so log_event/save_arr_grab
    // can push straight to subscribed clients; wired into AppState/engines below.
    let event_bus = events::new_bus();
    db.set_event_bus(event_bus.clone());

    // 3. Initialize Multi-Daemon Fetcher Pool & Restore Persisted State
    let fetcher_pool = Arc::new(FetcherPool::new());
    fetcher_pool.sync_nodes_from_config(&config);
    if let Ok(persisted_breakers) = db.list_circuit_breakers() {
        for b in persisted_breakers {
            info!(
                "⚡ Restored active tracker circuit breaker for '{}' (Canary: '{}', {} paused torrents)",
                b.tracker_host, b.canary_name, b.paused_torrents.len()
            );
            fetcher_pool.set_active_breaker(b.tracker_host.clone(), b);
        }
    }

    // 4. Initialize Notification Dispatcher
    let notifiers = Arc::new(NotificationManager);
    let arr_stats_cache = engines::arr_stats_poller::new_cache();
    let engine_registry = engines::registry::new_registry();
    let ip_asn_db = ip_asn::new_cache();

    // 5. Build Shared AppState
    let state = AppState {
        config: config_mgr.clone(),
        db: db.clone(),
        fetcher_pool: fetcher_pool.clone(),
        notifiers: notifiers.clone(),
        rate_limiter: auth::RateLimiter::new(),
        ws_tickets: auth::WsTicketStore::new(),
        mobile_pairs: auth::MobilePairingStore::new(),
        arr_stats_cache: arr_stats_cache.clone(),
        engine_registry: engine_registry.clone(),
        event_bus: event_bus.clone(),
        ip_asn_db: ip_asn_db.clone(),
    };

    // 6. Launch Background Engine Supervisor
    engines::start_all_engines(config_mgr.clone(), fetcher_pool.clone(), db.clone(), notifiers, arr_stats_cache, ip_asn_db, engine_registry, event_bus);
    info!("Background worker engines started successfully");

    // 7. Build API & Web Routers
    let app = api::build_api_router(state.clone())
        .fallback(web::static_handler);

    // 8. Start HTTP / HTTPS Server
    let addr: SocketAddr = format!("{}:{}", config.system.bind_addr, config.system.port).parse()?;

    let (auto_cert, auto_key) = auto_discover_ssl();
    let ssl_cert = ssl_cert_opt.or(config.system.ssl_cert.clone()).or(auto_cert);
    let ssl_key = ssl_key_opt.or(config.system.ssl_key.clone()).or(auto_key);
    let ssl_enabled = ssl_enabled_cli.unwrap_or(config.system.ssl_enabled)
        || (ssl_cert.is_some() && ssl_key.is_some() && (config.system.ssl_enabled || ssl_enabled_cli.is_none()));

    let handle = Handle::new();
    let handle_clone = handle.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        handle_clone.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
    });

    if ssl_enabled {
        if let (Some(cert_path), Some(key_path)) = (ssl_cert, ssl_key) {
            info!("🔒 Loading TLS certificates (Cert: {}, Key: {})...", cert_path, key_path);
            let rustls_config = RustlsConfig::from_pem_file(&cert_path, &key_path).await?;
            info!("🔒 Conduit HTTPS Server listening on https://{}", addr);
            info!("Swagger API documentation available at https://{}/swagger-ui", addr);

            axum_server::bind_rustls(addr, rustls_config)
                .handle(handle)
                .serve(app.into_make_service())
                .await?;
        } else {
            warn!("⚠️ SSL enabled in configuration but missing certificate or private key paths! Falling back to standard HTTP.");
            info!("Conduit HTTP Server listening on http://{}", addr);
            info!("Swagger API documentation available at http://{}/swagger-ui", addr);

            axum_server::bind(addr)
                .handle(handle)
                .serve(app.into_make_service())
                .await?;
        }
    } else {
        info!("Conduit HTTP Server listening on http://{}", addr);
        info!("Swagger API documentation available at http://{}/swagger-ui", addr);

        axum_server::bind(addr)
            .handle(handle)
            .serve(app.into_make_service())
            .await?;
    }

    info!("Conduit daemon stopped cleanly.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C, initiating graceful shutdown...");
        },
        _ = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown...");
        },
    }
}

fn auto_discover_ssl() -> (Option<String>, Option<String>) {
    let candidate_dirs = ["/data/ssl", "/data/certs", "/data", "ssl", "./ssl", "data/ssl"];
    let cert_names = [
        "conduit.crt", "conduit.pem", "conduit-fullchain.pem",
        "fullchain.pem", "cert.pem", "tls.crt",
    ];
    let key_names = [
        "conduit.key", "conduit-key.pem",
        "privkey.pem", "key.pem", "tls.key",
    ];

    let mut found_cert = None;
    let mut found_key = None;

    for dir in &candidate_dirs {
        let dir_path = std::path::Path::new(dir);
        if dir_path.is_dir() {
            if found_cert.is_none() {
                for c in &cert_names {
                    let p = dir_path.join(c);
                    if p.is_file() {
                        found_cert = Some(p.to_string_lossy().to_string());
                        break;
                    }
                }
            }
            if found_key.is_none() {
                for k in &key_names {
                    let p = dir_path.join(k);
                    if p.is_file() {
                        found_key = Some(p.to_string_lossy().to_string());
                        break;
                    }
                }
            }
        }
        if found_cert.is_some() && found_key.is_some() {
            break;
        }
    }

    (found_cert, found_key)
}
