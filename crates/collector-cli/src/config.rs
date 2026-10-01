use clap::Args;
use collector_core::preview_jobs::PreviewRetention;
use collector_domain::ApiError;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr},
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Args)]
pub struct Overrides {
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub bind: Option<String>,
    #[arg(long, global = true)]
    pub port: Option<String>,
    #[arg(long, global = true)]
    pub data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub assets_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub preview_retention_days: Option<String>,
}

#[derive(Debug)]
pub struct Config {
    pub bind: IpAddr,
    pub port: u16,
    pub data_dir: PathBuf,
    pub assets_dir: PathBuf,
    pub preview_retention: PreviewRetention,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    schema_version: u32,
    #[serde(default)]
    server: Server,
    #[serde(default)]
    storage: Storage,
    #[serde(default)]
    webui: Webui,
    #[serde(default)]
    preview: Preview,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Server {
    bind: Option<String>,
    port: Option<u16>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Storage {
    data_dir: Option<PathBuf>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Webui {
    assets_dir: Option<PathBuf>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preview {
    retention_days: Option<u32>,
}

fn invalid(field: &str, message: &str) -> ApiError {
    let mut error = ApiError::new("config_invalid", message);
    error.field = Some(field.into());
    error
}
fn absolute(base: &Path, value: &Path) -> PathBuf {
    if value.is_absolute() {
        value.into()
    } else {
        base.join(value)
    }
}

pub fn environment() -> BTreeMap<String, String> {
    std::env::vars()
        .filter(|(key, _)| key.starts_with("NEODB_COLLECTOR_"))
        .collect()
}

pub fn default_data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "neodb-collector").map(|p| p.data_local_dir().into())
}

pub fn resolve(
    args: &Overrides,
    env: &BTreeMap<String, String>,
    cwd: &Path,
    default_data: Option<&Path>,
) -> Result<Config, ApiError> {
    let get = |key: &str| env.get(&format!("NEODB_COLLECTOR_{key}"));
    let config_path = args
        .config
        .clone()
        .or_else(|| get("CONFIG").map(PathBuf::from))
        .map(|p| absolute(cwd, &p));
    let (file, file_base) = if let Some(path) = config_path {
        let contents = std::fs::read_to_string(&path)
            .map_err(|_| invalid("config", "Cannot read configuration file."))?;
        let file: FileConfig = serde_saphyr::from_str(&contents)
            .map_err(|_| invalid("config", "Invalid YAML or unknown configuration field."))?;
        if file.schema_version != 1 {
            return Err(invalid(
                "schema_version",
                "Only configuration schema_version 1 is supported.",
            ));
        }
        (file, path.parent().unwrap_or(cwd).to_path_buf())
    } else {
        (
            FileConfig {
                schema_version: 1,
                ..Default::default()
            },
            cwd.into(),
        )
    };

    let bind: IpAddr = args
        .bind
        .clone()
        .or_else(|| get("BIND").cloned())
        .or(file.server.bind)
        .unwrap_or_else(|| Ipv4Addr::LOCALHOST.to_string())
        .parse()
        .map_err(|_| invalid("server.bind", "Bind must be a loopback IP address."))?;
    if !bind.is_loopback() {
        return Err(invalid(
            "server.bind",
            "Only loopback binding is supported in this milestone.",
        ));
    }
    let port = args
        .port
        .clone()
        .or_else(|| get("PORT").cloned())
        .or(file.server.port.map(|p| p.to_string()))
        .unwrap_or_else(|| "19423".into())
        .parse::<u16>()
        .map_err(|_| invalid("server.port", "Port must be an integer from 0 to 65535."))?;
    let retention_days = args
        .preview_retention_days
        .clone()
        .or_else(|| get("PREVIEW_RETENTION_DAYS").cloned())
        .or(file.preview.retention_days.map(|days| days.to_string()))
        .unwrap_or_else(|| PreviewRetention::default().days().to_string())
        .parse::<u32>()
        .map_err(|_| {
            invalid(
                "preview.retention_days",
                "Preview retention must be a positive whole number of days.",
            )
        })?;
    let preview_retention = PreviewRetention::new(retention_days).map_err(|_| {
        invalid(
            "preview.retention_days",
            "Preview retention must be a positive whole number of days.",
        )
    })?;
    let path_value = |cli: &Option<PathBuf>,
                      env_key: &str,
                      file_value: Option<PathBuf>,
                      fallback: Option<&Path>|
     -> Result<PathBuf, ApiError> {
        let (base, value) = if let Some(v) = cli {
            (cwd, v.clone())
        } else if let Some(v) = get(env_key) {
            (cwd, PathBuf::from(v))
        } else if let Some(v) = file_value {
            (file_base.as_path(), v)
        } else {
            (
                cwd,
                fallback
                    .ok_or_else(|| {
                        invalid(
                            "storage.data_dir",
                            "No OS application-data directory; specify --data-dir.",
                        )
                    })?
                    .into(),
            )
        };
        if value.as_os_str().is_empty() {
            return Err(invalid(env_key, "Path cannot be empty."));
        }
        Ok(absolute(base, &value))
    };
    Ok(Config {
        bind,
        port,
        data_dir: path_value(
            &args.data_dir,
            "DATA_DIR",
            file.storage.data_dir,
            default_data,
        )?,
        assets_dir: path_value(
            &args.assets_dir,
            "ASSETS_DIR",
            file.webui.assets_dir,
            Some(Path::new("apps/webui/dist")),
        )?,
        preview_retention,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precedence_and_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.yaml");
        std::fs::write(&path, "schema_version: 1\nserver:\n  port: 2000\nstorage:\n  data_dir: records\nwebui:\n  assets_dir: ui\npreview:\n  retention_days: 35\n").unwrap();
        let mut args = Overrides {
            config: Some(path),
            ..Default::default()
        };
        let cwd = dir.path().join("cwd");
        let mut env = BTreeMap::new();
        let cfg = resolve(&args, &env, &cwd, None).unwrap();
        assert_eq!(cfg.port, 2000);
        assert_eq!(cfg.preview_retention.days(), 35);
        assert_eq!(cfg.data_dir, dir.path().join("records"));
        assert_eq!(cfg.assets_dir, dir.path().join("ui"));
        env.insert("NEODB_COLLECTOR_PORT".into(), "3000".into());
        env.insert("NEODB_COLLECTOR_DATA_DIR".into(), "env-data".into());
        assert_eq!(resolve(&args, &env, &cwd, None).unwrap().port, 3000);
        args.port = Some("4000".into());
        let cfg = resolve(&args, &env, &cwd, None).unwrap();
        assert_eq!(cfg.port, 4000);
        assert_eq!(cfg.data_dir, cwd.join("env-data"));
        env.insert("NEODB_COLLECTOR_PREVIEW_RETENTION_DAYS".into(), "45".into());
        assert_eq!(
            resolve(&args, &env, &cwd, None)
                .unwrap()
                .preview_retention
                .days(),
            45
        );
        args.preview_retention_days = Some("60".into());
        assert_eq!(
            resolve(&args, &env, &cwd, None)
                .unwrap()
                .preview_retention
                .days(),
            60
        );
        args.data_dir = Some("cli-data".into());
        assert_eq!(
            resolve(&args, &env, &cwd, None).unwrap().data_dir,
            cwd.join("cli-data")
        );
    }
    #[test]
    fn defaults_and_invalid_configs() {
        let dir = tempfile::tempdir().unwrap();
        let env = BTreeMap::new();
        let args = Overrides::default();
        let cfg = resolve(&args, &env, dir.path(), Some(dir.path())).unwrap();
        assert!(resolve(&args, &env, dir.path(), None).is_err());
        assert_eq!(cfg.port, 19423);
        assert!(cfg.bind.is_loopback());
        assert_eq!(cfg.preview_retention.days(), 30);
        for bind in ["0.0.0.0", "::", "192.168.1.2", "localhost"] {
            assert!(
                resolve(
                    &Overrides {
                        bind: Some(bind.into()),
                        ..Default::default()
                    },
                    &env,
                    dir.path(),
                    Some(dir.path())
                )
                .is_err()
            );
        }
        for port in ["-1", "65536", "abc"] {
            assert!(
                resolve(
                    &Overrides {
                        port: Some(port.into()),
                        ..Default::default()
                    },
                    &env,
                    dir.path(),
                    Some(dir.path())
                )
                .is_err()
            );
        }
        for yaml in [
            "schema_version: 2",
            "schema_version: 1\nsecret: x",
            "schema_version: 1\nserver:\n  unknown: true",
            "server: {}",
            "schema_version: 1\nserver:\n  port: 70000",
            "schema_version: 1\npreview:\n  unknown: true",
            "schema_version: 1\npreview:\n  retention_days: 0",
            "schema_version: 1\npreview:\n  retention_days: -1",
            "schema_version: 1\npreview:\n  retention_days: 4294967296",
        ] {
            let path = dir.path().join("invalid.yaml");
            std::fs::write(&path, yaml).unwrap();
            assert!(
                resolve(
                    &Overrides {
                        config: Some(path),
                        ..Default::default()
                    },
                    &env,
                    dir.path(),
                    Some(dir.path())
                )
                .is_err(),
                "{yaml}"
            );
        }
        for value in ["0", "-1", "4294967296", "1.5"] {
            assert!(
                resolve(
                    &Overrides {
                        preview_retention_days: Some(value.into()),
                        ..Default::default()
                    },
                    &env,
                    dir.path(),
                    Some(dir.path())
                )
                .is_err(),
                "{value}"
            );
        }
        for value in ["0", "-1", "4294967296", "1.5"] {
            let invalid_env = BTreeMap::from([(
                "NEODB_COLLECTOR_PREVIEW_RETENTION_DAYS".into(),
                value.into(),
            )]);
            assert!(
                resolve(
                    &Overrides::default(),
                    &invalid_env,
                    dir.path(),
                    Some(dir.path())
                )
                .is_err(),
                "{value}"
            );
        }
    }
}
