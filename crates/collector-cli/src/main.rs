mod client;
mod config;
mod serve_lease;

use clap::{Parser, Subcommand, ValueEnum};
use collector_core::access::{AccessService, Clock};
use collector_core::{
    preview::PreviewSnapshotService, preview_records::PreviewRecordService,
    preview_runner::PreviewAttemptRunner,
};
use collector_domain::ApiError;
use collector_storage::Storage;
use collector_storage::credentials::{SecureTokens, SystemClock};
use serde::Serialize;
use std::{future::Future, net::SocketAddr, process::ExitCode, sync::Arc};

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
    serve_until(config, shutdown()).await
}

async fn serve_until(
    config: config::Config,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<(), ApiError> {
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
    let _lease = serve_lease::ServeLease::acquire(&config.data_dir)?;
    let storage = Storage::open(&config.data_dir).await?;
    if let Err(error) = storage.clean_expired_access(SystemClock.now()).await {
        storage.close().await;
        return Err(error);
    }
    let clock = Arc::new(SystemClock);
    let secrets = Arc::new(collector_storage::credentials::OsSecretStore);
    let neodb = match collector_connectors::neodb::NeoDbClient::new() {
        Ok(client) => Arc::new(client),
        Err(error) => {
            storage.close().await;
            return Err(error);
        }
    };
    let snapshots = Arc::new(PreviewSnapshotService::new(
        Arc::new(storage.clone()),
        secrets.clone(),
        clock.clone(),
        neodb.clone(),
        neodb.clone(),
    ));
    let producer = Arc::new(PreviewRecordService::new(snapshots));
    let runner = Arc::new(PreviewAttemptRunner::new(
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        Arc::new(storage.clone()),
        clock.clone(),
        producer,
    ));
    let (preview_runtime, maintenance) =
        match collector_server::preview_runtime::PreviewRuntime::start(
            runner,
            Arc::new(storage.clone()),
            clock.clone(),
            neodb.clone(),
            collector_connectors::imdb::catalog_identity,
            config.preview_retention,
        )
        .await
        {
            Ok(runtime) => runtime,
            Err(error) => {
                storage.close().await;
                return Err(error);
            }
        };
    let preview_runtime = Arc::new(preview_runtime);
    let access = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: clock.clone(),
        audience: format!("http://{address}"),
    });
    let bootstrap = match access.bootstrap().await {
        Ok(bootstrap) => bootstrap,
        Err(error) => {
            preview_runtime.shutdown().await;
            storage.close().await;
            return Err(error);
        }
    };
    let oauth = Arc::new(collector_core::oauth::OAuthService::new(
        Arc::new(storage.clone()),
        secrets,
        Arc::new(SecureTokens),
        clock,
        neodb,
        access.audience.clone(),
    ));
    let mut access_state = collector_server::access::AccessState::new(access);
    access_state.oauth = Some(oauth);
    access_state.mappings = Some(Arc::new(storage.clone()));
    eprintln!("Core listening on http://{address}");
    // Intentional local delivery of a single-use key; never an HTTP request log.
    eprintln!("Open WebUI (one use, 10 minutes): http://{address}/#key={bootstrap}");
    let (reason_sender, reason_receiver) = tokio::sync::oneshot::channel();
    let shutdown_runtime = preview_runtime.clone();
    let graceful_shutdown = async move {
        let reason = tokio::select! {
            biased;
            result = maintenance.wait() => result,
            _ = shutdown_signal => Ok(()),
        };
        shutdown_runtime.shutdown().await;
        let _ = reason_sender.send(reason);
    };
    let server_result = collector_server::serve(
        listener,
        Arc::new(storage.clone()),
        config.assets_dir,
        access_state,
        graceful_shutdown,
    )
    .await;
    let shutdown_result = if server_result.is_ok() {
        reason_receiver.await.unwrap_or_else(|_| {
            Err(ApiError::new(
                "preview_maintenance_failed",
                "Preview retention maintenance stopped unexpectedly.",
            ))
        })
    } else {
        Ok(())
    };
    preview_runtime.shutdown().await;
    storage.close().await;
    server_result
        .map_err(|_| ApiError::new("server_failed", "Core HTTP server stopped unexpectedly."))?;
    shutdown_result
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
    use collector_core::{
        access::{AccessRepository, Endpoint},
        mapping::{MappingRepository, imdb_default},
        preview_jobs::{PreviewJobRepository, PreviewJobStatus, PreviewSubmission},
    };
    #[tokio::test]
    async fn doctor_reports_assets_port_and_database() {
        let dir = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let data_dir = dir.path().join("data");
        let storage = Storage::open(&data_dir).await.unwrap();
        let endpoint = Endpoint {
            base_url: "https://neodb.example".into(),
            account: "alice".into(),
            credential_ref: "credential-generation".into(),
        };
        storage.set_endpoint(Some(endpoint.clone())).await.unwrap();
        let profile = imdb_default();
        storage.save(profile.clone(), 0).await.unwrap();
        storage
            .submit(
                PreviewSubmission::new(
                    "job:doctor-preserves".into(),
                    endpoint,
                    profile,
                    vec![],
                    100,
                    1_000,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        storage.close().await;
        let config = config::Config {
            bind: listener.local_addr().unwrap().ip(),
            port: listener.local_addr().unwrap().port(),
            data_dir: data_dir.clone(),
            assets_dir: dir.path().join("missing"),
            preview_retention: Default::default(),
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
        let storage = Storage::open(&data_dir).await.unwrap();
        assert_eq!(
            PreviewJobRepository::load(&storage, "job:doctor-preserves".into())
                .await
                .unwrap()
                .unwrap()
                .status(),
            PreviewJobStatus::Queued
        );
        storage.close().await;
    }

    #[tokio::test]
    async fn serve_composes_preview_runtime_without_network_or_keychain_access() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let storage = Storage::open(&data_dir).await.unwrap();
        let endpoint = Endpoint {
            base_url: "https://neodb.example".into(),
            account: "alice".into(),
            credential_ref: "credential-generation".into(),
        };
        storage.set_endpoint(Some(endpoint.clone())).await.unwrap();
        let profile = imdb_default();
        storage.save(profile.clone(), 0).await.unwrap();
        let now = SystemClock.now();
        let expires_at = collector_core::preview_jobs::PreviewRetention::default()
            .expires_at(now)
            .unwrap();
        for id in ["job:serve-queued", "job:serve-running"] {
            storage
                .submit(
                    PreviewSubmission::new(
                        id.into(),
                        endpoint.clone(),
                        profile.clone(),
                        vec![],
                        now,
                        expires_at,
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
        }
        assert!(
            storage
                .claim("job:serve-running".into(), 1, now)
                .await
                .unwrap()
        );
        storage
            .submit(
                PreviewSubmission::new(
                    "job:serve-expired".into(),
                    endpoint.clone(),
                    profile.clone(),
                    vec![],
                    1,
                    2,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        storage.close().await;
        let result = serve_until(
            config::Config {
                bind: "127.0.0.1".parse().unwrap(),
                port: 0,
                data_dir: data_dir.clone(),
                assets_dir: dir.path().join("missing"),
                preview_retention: collector_core::preview_jobs::PreviewRetention::new(7).unwrap(),
            },
            async {},
        )
        .await;
        assert!(result.is_ok(), "{result:?}");
        assert!(dir.path().join("data/collector.db").exists());
        let storage = Storage::open(&data_dir).await.unwrap();
        for id in ["job:serve-queued", "job:serve-running"] {
            let job = PreviewJobRepository::load(&storage, id.into())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(job.status(), PreviewJobStatus::Interrupted);
            assert_eq!(job.expires_at(), expires_at);
        }
        assert!(
            PreviewJobRepository::load(&storage, "job:serve-expired".into())
                .await
                .unwrap()
                .is_none()
        );
        assert!(storage.endpoint().await.unwrap().is_some());
        assert!(
            MappingRepository::load(&storage, profile.id.clone(), Some(profile.version))
                .await
                .unwrap()
                .is_some()
        );
        storage.close().await;
        drop(serve_lease::ServeLease::acquire(&data_dir).unwrap());
    }
}
