mod client;
mod config;

use clap::{Parser, Subcommand, ValueEnum};
use collector_core::access::{AccessService, Clock};
use collector_domain::ApiError;
use collector_storage::Storage;
use collector_storage::credentials::{SecureTokens, SystemClock};
use serde::Serialize;
use std::{net::SocketAddr, process::ExitCode, sync::Arc};

#[derive(Parser)]
#[command(name = "collector", version, about = "Local NeoDB Collector Core")]
struct Cli {
    #[command(flatten)]
    overrides: config::Overrides,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve Core API and built WebUI on the same loopback port; no media writes.
    Serve,
    /// Diagnose local setup. Creates the data directory and migrates collector.db.
    Doctor {
        #[arg(long, value_enum, default_value = "human")]
        format: Format,
    },
    /// Pair using a one-time WebUI link piped to standard input.
    Pair {
        #[arg(long, default_value = "Collector CLI")]
        name: String,
        #[arg(long, value_enum, default_value = "human")]
        format: Format,
    },
    /// Verify the stored client credential against the running Core.
    Status {
        #[arg(long, default_value = "http://127.0.0.1:19423")]
        core_url: String,
        #[arg(long, value_enum, default_value = "human")]
        format: Format,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Human,
    Json,
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    status: &'static str,
    code: String,
    message: String,
}

impl Check {
    fn result(name: &'static str, result: Result<(), ApiError>, success: &str) -> Self {
        match result {
            Ok(()) => Self {
                name,
                status: "ok",
                code: "ok".into(),
                message: success.into(),
            },
            Err(e) => Self {
                name,
                status: "error",
                code: e.code,
                message: e.message,
            },
        }
    }
}

#[derive(Serialize)]
struct Report {
    ok: bool,
    checks: Vec<Check>,
}

fn resolve(overrides: &config::Overrides) -> Result<config::Config, ApiError> {
    let cwd = std::env::current_dir()
        .map_err(|_| ApiError::new("config_invalid", "Cannot resolve current directory."))?;
    config::resolve(
        overrides,
        &config::environment(),
        &cwd,
        config::default_data_dir().as_deref(),
    )
}

async fn doctor(config: Result<config::Config, ApiError>) -> Report {
    let config = match config {
        Ok(config) => config,
        Err(error) => {
            return Report {
                ok: false,
                checks: vec![Check::result("config", Err(error), "")],
            };
        }
    };
    let mut checks = vec![Check::result("config", Ok(()), "Configuration is valid.")];
    let directory = tokio::fs::create_dir_all(&config.data_dir)
        .await
        .map_err(|_| {
            ApiError::new(
                "data_directory_unavailable",
                "Cannot create or access the data directory.",
            )
        });
    checks.push(Check::result(
        "data_directory",
        directory,
        "Data directory is accessible; database check verifies write access.",
    ));
    let database = match Storage::open(&config.data_dir).await {
        Ok(storage) => {
            storage.close().await;
            Ok(())
        }
        Err(error) => Err(error),
    };
    checks.push(Check::result(
        "database",
        database,
        "Database is writable and migrations are current.",
    ));
    let port = tokio::net::TcpListener::bind(SocketAddr::new(config.bind, config.port))
        .await
        .map(drop)
        .map_err(|_| {
            ApiError::new(
                "port_unavailable",
                "Cannot bind the configured port; stop the running Core or choose another port.",
            )
        });
    checks.push(Check::result(
        "port",
        port,
        "Configured address can be bound.",
    ));
    let assets = tokio::fs::read(config.assets_dir.join("index.html"))
        .await
        .map(|_| ())
        .map_err(|_| {
            ApiError::new(
                "webui_assets_missing",
                "Build the WebUI with pnpm build, or configure webui.assets_dir.",
            )
        });
    checks.push(Check::result("assets", assets, "WebUI index is readable."));
    Report {
        ok: checks.iter().all(|c| c.status == "ok"),
        checks,
    }
}

async fn shutdown() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn serve(config: config::Config) -> Result<(), ApiError> {
    // Bind before migrations: a second Core must fail without changing local state.
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(config.bind, config.port))
        .await
        .map_err(|_| {
            ApiError::new(
                "port_unavailable",
                "Cannot bind the configured loopback port.",
            )
        })?;
    let address = listener
        .local_addr()
        .map_err(|_| ApiError::new("port_unavailable", "Cannot inspect listener address."))?;
    let storage = Storage::open(&config.data_dir).await?;
    storage.clean_expired_access(SystemClock.now()).await?;
    let access = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(SystemClock),
        audience: format!("http://{address}"),
    });
    let bootstrap = access.bootstrap().await?;
    let oauth = Arc::new(collector_core::oauth::OAuthService::new(
        Arc::new(storage.clone()),
        Arc::new(collector_storage::credentials::OsSecretStore),
        Arc::new(SecureTokens),
        Arc::new(SystemClock),
        Arc::new(collector_connectors::neodb::NeoDbClient::new()?),
        access.audience.clone(),
    ));
    let mut access_state = collector_server::access::AccessState::new(access);
    access_state.oauth = Some(oauth);
    eprintln!("Core listening on http://{address}");
    // Intentional local delivery of a single-use key; never an HTTP request log.
    eprintln!("Open WebUI (one use, 10 minutes): http://{address}/#key={bootstrap}");
    let result = collector_server::serve(
        listener,
        Arc::new(storage.clone()),
        config.assets_dir,
        access_state,
        shutdown(),
    )
    .await;
    storage.close().await;
    result.map_err(|_| ApiError::new("server_failed", "Core HTTP server stopped unexpectedly."))
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Pair { name, format } => {
            let result = match client::read_pairing() {
                Ok(link) => {
                    client::pair(&link, &name, &collector_storage::credentials::OsSecretStore).await
                }
                Err(error) => Err(error),
            };
            print_client(result, format)
        }
        Command::Status { core_url, format } => print_client(
            client::status(&core_url, &collector_storage::credentials::OsSecretStore).await,
            format,
        ),
        Command::Serve => {
            let result = match resolve(&cli.overrides) {
                Ok(config) => serve(config).await,
                Err(error) => Err(error),
            };
            match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{}: {}", error.code, error.message);
                    ExitCode::FAILURE
                }
            }
        }
        Command::Doctor { format } => {
            let report = doctor(resolve(&cli.overrides)).await;
            match format {
                Format::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("serializable report")
                ),
                Format::Human => {
                    for check in &report.checks {
                        println!(
                            "[{}] {}: {} ({})",
                            check.status, check.name, check.message, check.code
                        );
                    }
                }
            }
            if report.ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
    }
}

fn print_client<T: Serialize + std::fmt::Display>(
    result: Result<T, ApiError>,
    format: Format,
) -> ExitCode {
    match result {
        Ok(report) => {
            match format {
                Format::Json => println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("serializable report")
                ),
                Format::Human => println!("{report}"),
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            match format {
                Format::Json => println!(
                    "{}",
                    serde_json::to_string(&error).expect("serializable error")
                ),
                Format::Human => eprintln!("{error}"),
            }
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn doctor_reports_assets_port_and_database() {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = config::Config {
            bind: listener.local_addr().unwrap().ip(),
            port: listener.local_addr().unwrap().port(),
            data_dir: dir.path().join("data"),
            assets_dir: dir.path().join("missing"),
        };
        let report = doctor(Ok(config)).await;
        assert!(!report.ok);
        assert_eq!(report.checks.len(), 5);
        assert!(report.checks.iter().any(|c| c.code == "port_unavailable"));
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.code == "webui_assets_missing")
        );
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "database" && c.status == "ok")
        );
        assert!(dir.path().join("data/collector.db").exists());
    }
}
