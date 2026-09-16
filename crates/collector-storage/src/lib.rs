use collector_core::StorageHealth;
use collector_domain::ApiError;
use sqlx::{
    Connection, SqliteConnection, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{future::Future, path::Path, pin::Pin, time::Duration};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

mod access;
pub mod credentials;

#[derive(Clone)]
pub struct Storage {
    pool: SqlitePool,
}

fn database_error() -> ApiError {
    ApiError::new(
        "database_unavailable",
        "Cannot open or validate the operational database.",
    )
}

impl Storage {
    pub async fn open(data_dir: &Path) -> Result<Self, ApiError> {
        let path = data_dir.join("collector.db");
        // Inspect existing files read-only before SQLx can create a migration ledger.
        if path.exists() {
            let options = SqliteConnectOptions::new().filename(&path).read_only(true);
            let mut conn = SqliteConnection::connect_with(&options)
                .await
                .map_err(|_| database_error())?;
            let exists: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM sqlite_master WHERE name = '_sqlx_migrations'",
            )
            .fetch_one(&mut conn)
            .await
            .map_err(|_| database_error())?;
            if exists > 0 {
                let versions: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
                    .fetch_all(&mut conn)
                    .await
                    .map_err(|_| database_error())?;
                let latest = MIGRATOR.iter().map(|m| m.version).max().unwrap_or(0);
                if versions.iter().any(|v| *v > latest) {
                    conn.close().await.map_err(|_| database_error())?;
                    return Err(ApiError::new(
                        "database_version_unsupported",
                        "Database was migrated by a newer Core. Use a compatible binary.",
                    ));
                }
            }
            conn.close().await.map_err(|_| database_error())?;
        }
        tokio::fs::create_dir_all(data_dir).await.map_err(|_| {
            ApiError::new("data_dir_unavailable", "Cannot create the data directory.")
        })?;
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .map_err(|_| database_error())?;
        if MIGRATOR.run(&pool).await.is_err() {
            pool.close().await;
            return Err(ApiError::new(
                "database_migration_failed",
                "Database migration validation failed; no automatic repair was attempted.",
            ));
        }
        Ok(Self { pool })
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

impl StorageHealth for Storage {
    fn is_ready(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        Box::pin(async {
            matches!(
                sqlx::query_scalar::<_, i32>("SELECT 1")
                    .fetch_one(&self.pool)
                    .await,
                Ok(1)
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_reopen_with_operational_tables_only() {
        let dir = tempfile::tempdir().unwrap();
        for _ in 0..2 {
            let storage = Storage::open(dir.path()).await.unwrap();
            assert!(storage.is_ready().await);
            let names: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
            )
            .fetch_all(&storage.pool)
            .await
            .unwrap();
            assert_eq!(
                names,
                vec!["_sqlx_migrations", "access_credentials", "neodb_endpoint"]
            );
            storage.close().await;
            assert!(!storage.is_ready().await);
        }
    }

    #[tokio::test]
    async fn foundation_database_upgrades_to_access_schema() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        // Recreate the exact committed v1 shape and ledger in an isolated fixture.
        sqlx::raw_sql("DROP TABLE access_credentials; DROP TABLE neodb_endpoint; DELETE FROM _sqlx_migrations WHERE version=2;")
            .execute(&storage.pool).await.unwrap();
        storage.close().await;
        let upgraded = Storage::open(dir.path()).await.unwrap();
        let latest: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
            .fetch_one(&upgraded.pool)
            .await
            .unwrap();
        assert_eq!(latest, 2);
        assert!(upgraded.is_ready().await);
        upgraded.close().await;
    }

    #[tokio::test]
    async fn newer_database_is_rejected_without_modifying_file() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        sqlx::query("UPDATE _sqlx_migrations SET version = 999 WHERE version = 2")
            .execute(&storage.pool)
            .await
            .unwrap();
        storage.close().await;
        let path = dir.path().join("collector.db");
        let before = std::fs::read(&path).unwrap();
        let error = Storage::open(dir.path()).await.err().unwrap();
        assert_eq!(error.code, "database_version_unsupported");
        assert_eq!(before, std::fs::read(&path).unwrap());
    }
}
