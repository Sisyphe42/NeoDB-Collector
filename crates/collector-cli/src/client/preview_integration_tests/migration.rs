use super::*;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

#[derive(Clone, Copy, Debug)]
pub(super) enum PriorSchema {
    V5,
    V6,
}

impl PriorSchema {
    fn version(self) -> i64 {
        match self {
            Self::V5 => 5,
            Self::V6 => 6,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct OperationalRows {
    access: Vec<String>,
    endpoint: Vec<String>,
    profiles: Vec<String>,
    jobs: Vec<String>,
    observations: Vec<String>,
    results: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MigrationIdentity {
    version: i64,
    description: String,
    success: i64,
    checksum: Vec<u8>,
}

pub(super) struct LegacyFixtureProof {
    prior: PriorSchema,
    rows: OperationalRows,
    original_identities: Vec<MigrationIdentity>,
    retained_history: Vec<String>,
}

async fn fixture_pool(temp: &tempfile::TempDir) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(temp.path().join("collector.db"))
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap()
}

async fn strings(pool: &SqlitePool, statement: &str) -> Vec<String> {
    sqlx::query_scalar(statement).fetch_all(pool).await.unwrap()
}

async fn operational_rows(pool: &SqlitePool) -> OperationalRows {
    OperationalRows {
        access: strings(pool, "SELECT quote(id)||'|'||quote(token_hash)||'|'||quote(kind)||'|'||quote(audience)||'|'||quote(name)||'|'||quote(client_kind)||'|'||quote(expires_at)||'|'||quote(revoked) FROM access_credentials ORDER BY id").await,
        endpoint: strings(pool, "SELECT quote(singleton)||'|'||quote(base_url)||'|'||quote(account)||'|'||quote(credential_ref) FROM neodb_endpoint ORDER BY singleton").await,
        profiles: strings(pool, "SELECT quote(id)||'|'||quote(version)||'|'||quote(profile_json) FROM mapping_profiles ORDER BY id,version").await,
        jobs: strings(pool, "SELECT quote(id)||'|'||quote(profile_id)||'|'||quote(profile_version)||'|'||quote(base_url)||'|'||quote(account)||'|'||quote(credential_ref)||'|'||quote(status)||'|'||quote(attempt)||'|'||quote(created_at)||'|'||quote(updated_at)||'|'||quote(expires_at)||'|'||quote(observation_count) FROM preview_jobs ORDER BY id").await,
        observations: strings(pool, "SELECT quote(job_id)||'|'||quote(ordinal)||'|'||quote(observation_json) FROM preview_observations ORDER BY job_id,ordinal").await,
        results: strings(pool, "SELECT quote(job_id)||'|'||quote(ordinal)||'|'||quote(attempt)||'|'||quote(committed_at)||'|'||quote(result_json) FROM preview_results ORDER BY job_id,ordinal").await,
    }
}

async fn migration_identities(pool: &SqlitePool) -> Vec<MigrationIdentity> {
    sqlx::query_as::<_, (i64, String, i64, Vec<u8>)>(
        "SELECT version,description,success,checksum FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(
        |(version, description, success, checksum)| MigrationIdentity {
            version,
            description,
            success,
            checksum,
        },
    )
    .collect()
}

async fn migration_history(pool: &SqlitePool) -> Vec<String> {
    strings(pool, "SELECT quote(version)||'|'||quote(description)||'|'||quote(installed_on)||'|'||quote(success)||'|'||hex(checksum)||'|'||quote(execution_time) FROM _sqlx_migrations ORDER BY version").await
}

async fn schema_snapshot(pool: &SqlitePool) -> Vec<String> {
    let mut snapshot = strings(pool, "SELECT quote(type)||'|'||quote(name)||'|'||quote(tbl_name)||'|'||quote(sql) FROM sqlite_master WHERE name GLOB 'preview_*' AND sql IS NOT NULL ORDER BY type,name").await;
    for table in ["preview_jobs", "preview_observations", "preview_results"] {
        snapshot.extend(strings(pool, &format!("SELECT 'column|{table}|'||quote(cid)||'|'||quote(name)||'|'||quote(type)||'|'||quote(\"notnull\")||'|'||quote(dflt_value)||'|'||quote(pk) FROM pragma_table_info('{table}') ORDER BY cid")).await);
        snapshot.extend(strings(pool, &format!("SELECT 'index|{table}|'||quote(seq)||'|'||quote(name)||'|'||quote(\"unique\")||'|'||quote(origin)||'|'||quote(partial) FROM pragma_index_list('{table}') ORDER BY seq")).await);
        snapshot.extend(strings(pool, &format!("SELECT 'fk|{table}|'||quote(id)||'|'||quote(seq)||'|'||quote(\"table\")||'|'||quote(\"from\")||'|'||quote(\"to\")||'|'||quote(on_update)||'|'||quote(on_delete)||'|'||quote(\"match\") FROM pragma_foreign_key_list('{table}') ORDER BY id,seq")).await);
    }
    for index in [
        "preview_jobs_status_expiry",
        "preview_results_attempt_ordinal",
    ] {
        snapshot.extend(strings(pool, &format!("SELECT 'index_column|{index}|'||quote(seqno)||'|'||quote(cid)||'|'||quote(name) FROM pragma_index_info('{index}') ORDER BY seqno")).await);
    }
    snapshot
}

async fn canonical_schema(version: i64) -> Vec<String> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    for migration in [
        include_str!("../../../../collector-storage/migrations/0001_baseline.sql"),
        include_str!("../../../../collector-storage/migrations/0002_access.sql"),
        include_str!("../../../../collector-storage/migrations/0003_mapping_profiles.sql"),
        include_str!("../../../../collector-storage/migrations/0004_preview_jobs.sql"),
        include_str!("../../../../collector-storage/migrations/0005_preview_results.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&pool).await.unwrap();
    }
    if version >= 6 {
        sqlx::raw_sql(include_str!(
            "../../../../collector-storage/migrations/0006_preview_search_inputs.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    if version >= 7 {
        sqlx::raw_sql(include_str!(
            "../../../../collector-storage/migrations/0007_preview_manual_inputs.sql"
        ))
        .execute(&pool)
        .await
        .unwrap();
    }
    let snapshot = schema_snapshot(&pool).await;
    pool.close().await;
    snapshot
}

async fn assert_foreign_keys(pool: &SqlitePool) {
    let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(enabled, 1);
    let violations: Vec<String> = strings(
        pool,
        "SELECT quote(\"table\")||'|'||quote(rowid)||'|'||quote(parent)||'|'||quote(fkid) FROM pragma_foreign_key_check ORDER BY \"table\",rowid,fkid",
    )
    .await;
    assert!(
        violations.is_empty(),
        "foreign-key violations: {violations:?}"
    );
}

pub(super) async fn downgrade_closed_fixture(
    temp: &tempfile::TempDir,
    prior: PriorSchema,
) -> LegacyFixtureProof {
    let pool = fixture_pool(temp).await;
    assert_foreign_keys(&pool).await;
    let rows = operational_rows(&pool).await;
    assert!(!rows.access.is_empty());
    assert_eq!(rows.endpoint.len(), 1);
    assert_eq!(rows.profiles.len(), 2);
    assert_eq!(rows.jobs.len(), 1);
    assert_eq!(rows.observations.len(), 2);
    assert_eq!(rows.results.len(), 1);
    let non_null_later_inputs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM preview_observations WHERE search_query IS NOT NULL OR manual_selection IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        non_null_later_inputs, 0,
        "E5e variants are deliberately Native only"
    );

    let original_identities = migration_identities(&pool).await;
    assert_eq!(
        original_identities
            .iter()
            .map(|row| row.version)
            .collect::<Vec<_>>(),
        (1..=7).collect::<Vec<_>>()
    );
    assert!(original_identities.iter().all(|row| row.success == 1));
    let full_history = migration_history(&pool).await;

    sqlx::raw_sql(
        "DROP TRIGGER preview_observations_resolution_input_insert;
         DROP TRIGGER preview_observations_resolution_input_update;
         ALTER TABLE preview_observations DROP COLUMN manual_selection;
         DELETE FROM _sqlx_migrations WHERE version >= 7;",
    )
    .execute(&pool)
    .await
    .unwrap();
    if matches!(prior, PriorSchema::V5) {
        sqlx::raw_sql(
            "ALTER TABLE preview_observations DROP COLUMN search_query;
             DELETE FROM _sqlx_migrations WHERE version >= 6;",
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    assert_eq!(operational_rows(&pool).await, rows);
    assert_foreign_keys(&pool).await;
    assert_eq!(
        schema_snapshot(&pool).await,
        canonical_schema(prior.version()).await
    );
    let retained_identities = migration_identities(&pool).await;
    assert_eq!(
        retained_identities,
        original_identities[..prior.version() as usize]
    );
    let retained_history = migration_history(&pool).await;
    assert_eq!(retained_history, full_history[..prior.version() as usize]);
    pool.close().await;

    LegacyFixtureProof {
        prior,
        rows,
        original_identities,
        retained_history,
    }
}

pub(super) async fn assert_upgraded_fixture(temp: &tempfile::TempDir, proof: &LegacyFixtureProof) {
    let pool = fixture_pool(temp).await;
    assert_foreign_keys(&pool).await;
    assert_eq!(operational_rows(&pool).await, proof.rows);
    let null_later_inputs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM preview_observations WHERE search_query IS NULL AND manual_selection IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(null_later_inputs, proof.rows.observations.len() as i64);
    assert_eq!(schema_snapshot(&pool).await, canonical_schema(7).await);
    let identities = migration_identities(&pool).await;
    assert_eq!(identities, proof.original_identities);
    let history = migration_history(&pool).await;
    assert_eq!(
        history[..proof.prior.version() as usize],
        proof.retained_history
    );
    pool.close().await;
}

#[tokio::test]
async fn cli_populated_v5_upgrade_preserves_restart_resume_delete_chain() {
    tokio::time::timeout(
        Duration::from_secs(20),
        super::restart::run_restart_resume_delete(Some(PriorSchema::V5)),
    )
    .await
    .expect("E5e populated v5 migration acceptance must finish within its outer bound");
}

#[tokio::test]
async fn cli_populated_v6_upgrade_preserves_restart_resume_delete_chain() {
    tokio::time::timeout(
        Duration::from_secs(20),
        super::restart::run_restart_resume_delete(Some(PriorSchema::V6)),
    )
    .await
    .expect("E5e populated v6 migration acceptance must finish within its outer bound");
}
