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

async fn fixture_pool(temp: &tempfile::TempDir) -> Result<SqlitePool, &'static str> {
    let options = SqliteConnectOptions::new()
        .filename(temp.path().join("collector.db"))
        .foreign_keys(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|_| "legacy fixture pool open failed")
}

async fn strings(pool: &SqlitePool, statement: &str) -> Result<Vec<String>, &'static str> {
    sqlx::query_scalar(statement)
        .fetch_all(pool)
        .await
        .map_err(|_| "legacy fixture query failed")
}

async fn operational_rows(pool: &SqlitePool) -> Result<OperationalRows, &'static str> {
    Ok(OperationalRows {
        access: strings(pool, "SELECT quote(id)||'|'||quote(token_hash)||'|'||quote(kind)||'|'||quote(audience)||'|'||quote(name)||'|'||quote(client_kind)||'|'||quote(expires_at)||'|'||quote(revoked) FROM access_credentials ORDER BY id").await?,
        endpoint: strings(pool, "SELECT quote(singleton)||'|'||quote(base_url)||'|'||quote(account)||'|'||quote(credential_ref) FROM neodb_endpoint ORDER BY singleton").await?,
        profiles: strings(pool, "SELECT quote(id)||'|'||quote(version)||'|'||quote(profile_json) FROM mapping_profiles ORDER BY id,version").await?,
        jobs: strings(pool, "SELECT quote(id)||'|'||quote(profile_id)||'|'||quote(profile_version)||'|'||quote(base_url)||'|'||quote(account)||'|'||quote(credential_ref)||'|'||quote(status)||'|'||quote(attempt)||'|'||quote(created_at)||'|'||quote(updated_at)||'|'||quote(expires_at)||'|'||quote(observation_count) FROM preview_jobs ORDER BY id").await?,
        observations: strings(pool, "SELECT quote(job_id)||'|'||quote(ordinal)||'|'||quote(observation_json) FROM preview_observations ORDER BY job_id,ordinal").await?,
        results: strings(pool, "SELECT quote(job_id)||'|'||quote(ordinal)||'|'||quote(attempt)||'|'||quote(committed_at)||'|'||quote(result_json) FROM preview_results ORDER BY job_id,ordinal").await?,
    })
}

async fn migration_identities(pool: &SqlitePool) -> Result<Vec<MigrationIdentity>, &'static str> {
    Ok(sqlx::query_as::<_, (i64, String, i64, Vec<u8>)>(
        "SELECT version,description,success,checksum FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| "legacy migration identity query failed")?
    .into_iter()
    .map(
        |(version, description, success, checksum)| MigrationIdentity {
            version,
            description,
            success,
            checksum,
        },
    )
    .collect())
}

async fn migration_history(pool: &SqlitePool) -> Result<Vec<String>, &'static str> {
    strings(pool, "SELECT quote(version)||'|'||quote(description)||'|'||quote(installed_on)||'|'||quote(success)||'|'||hex(checksum)||'|'||quote(execution_time) FROM _sqlx_migrations ORDER BY version").await
}

async fn schema_snapshot(pool: &SqlitePool) -> Result<Vec<String>, &'static str> {
    let mut snapshot = strings(pool, "SELECT quote(type)||'|'||quote(name)||'|'||quote(tbl_name)||'|'||quote(sql) FROM sqlite_master WHERE name GLOB 'preview_*' AND sql IS NOT NULL ORDER BY type,name").await?;
    for table in ["preview_jobs", "preview_observations", "preview_results"] {
        snapshot.extend(strings(pool, &format!("SELECT 'column|{table}|'||quote(cid)||'|'||quote(name)||'|'||quote(type)||'|'||quote(\"notnull\")||'|'||quote(dflt_value)||'|'||quote(pk) FROM pragma_table_info('{table}') ORDER BY cid")).await?);
        snapshot.extend(strings(pool, &format!("SELECT 'index|{table}|'||quote(seq)||'|'||quote(name)||'|'||quote(\"unique\")||'|'||quote(origin)||'|'||quote(partial) FROM pragma_index_list('{table}') ORDER BY seq")).await?);
        snapshot.extend(strings(pool, &format!("SELECT 'fk|{table}|'||quote(id)||'|'||quote(seq)||'|'||quote(\"table\")||'|'||quote(\"from\")||'|'||quote(\"to\")||'|'||quote(on_update)||'|'||quote(on_delete)||'|'||quote(\"match\") FROM pragma_foreign_key_list('{table}') ORDER BY id,seq")).await?);
    }
    for index in [
        "preview_jobs_status_expiry",
        "preview_results_attempt_ordinal",
    ] {
        snapshot.extend(strings(pool, &format!("SELECT 'index_column|{index}|'||quote(seqno)||'|'||quote(cid)||'|'||quote(name) FROM pragma_index_info('{index}') ORDER BY seqno")).await?);
    }
    Ok(snapshot)
}

async fn canonical_schema(version: i64) -> Result<Vec<String>, &'static str> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .map_err(|_| "canonical legacy schema pool open failed")?;
    let result = async {
        for migration in [
            include_str!("../../../../collector-storage/migrations/0001_baseline.sql"),
            include_str!("../../../../collector-storage/migrations/0002_access.sql"),
            include_str!("../../../../collector-storage/migrations/0003_mapping_profiles.sql"),
            include_str!("../../../../collector-storage/migrations/0004_preview_jobs.sql"),
            include_str!("../../../../collector-storage/migrations/0005_preview_results.sql"),
        ] {
            sqlx::raw_sql(migration)
                .execute(&pool)
                .await
                .map_err(|_| "canonical legacy migration failed")?;
        }
        if version >= 6 {
            sqlx::raw_sql(include_str!(
                "../../../../collector-storage/migrations/0006_preview_search_inputs.sql"
            ))
            .execute(&pool)
            .await
            .map_err(|_| "canonical search migration failed")?;
        }
        if version >= 7 {
            sqlx::raw_sql(include_str!(
                "../../../../collector-storage/migrations/0007_preview_manual_inputs.sql"
            ))
            .execute(&pool)
            .await
            .map_err(|_| "canonical manual migration failed")?;
        }
        schema_snapshot(&pool).await
    }
    .await;
    pool.close().await;
    result
}

async fn check_foreign_keys(pool: &SqlitePool) -> Result<(), &'static str> {
    let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(pool)
        .await
        .map_err(|_| "legacy foreign key setting query failed")?;
    if enabled != 1 {
        return Err("legacy fixture foreign keys were disabled");
    }
    let violations: Vec<String> = strings(
        pool,
        "SELECT quote(\"table\")||'|'||quote(rowid)||'|'||quote(parent)||'|'||quote(fkid) FROM pragma_foreign_key_check ORDER BY \"table\",rowid,fkid",
    )
    .await?;
    if !violations.is_empty() {
        return Err("legacy fixture foreign key integrity failed");
    }
    Ok(())
}

pub(super) async fn try_downgrade_closed_fixture(
    temp: &tempfile::TempDir,
    prior: PriorSchema,
) -> Result<LegacyFixtureProof, &'static str> {
    let pool = fixture_pool(temp).await?;
    let result = async {
    check_foreign_keys(&pool).await?;
    let rows = operational_rows(&pool).await?;
    if rows.access.is_empty()
        || rows.endpoint.len() != 1
        || rows.profiles.len() != 2
        || rows.jobs.len() != 1
        || rows.observations.len() != 2
        || rows.results.len() != 1
    {
        return Err("legacy fixture populated operational row counts changed");
    }
    let non_null_later_inputs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM preview_observations WHERE search_query IS NOT NULL OR manual_selection IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| "legacy fixture input query failed")?;
    if non_null_later_inputs != 0 {
        return Err("legacy fixture inputs were not Native only");
    }

    let original_identities = migration_identities(&pool).await?;
    if original_identities
            .iter()
            .map(|row| row.version)
            .collect::<Vec<_>>() != (1..=7).collect::<Vec<_>>()
        || !original_identities.iter().all(|row| row.success == 1)
    {
        return Err("legacy fixture original migration identities changed");
    }
    let full_history = migration_history(&pool).await?;

    sqlx::raw_sql(
        "DROP TRIGGER preview_observations_resolution_input_insert;
         DROP TRIGGER preview_observations_resolution_input_update;
         ALTER TABLE preview_observations DROP COLUMN manual_selection;
         DELETE FROM _sqlx_migrations WHERE version >= 7;",
    )
    .execute(&pool)
    .await
    .map_err(|_| "legacy fixture manual input downgrade failed")?;
    if matches!(prior, PriorSchema::V5) {
        sqlx::raw_sql(
            "ALTER TABLE preview_observations DROP COLUMN search_query;
             DELETE FROM _sqlx_migrations WHERE version >= 6;",
        )
        .execute(&pool)
        .await
        .map_err(|_| "legacy fixture search input downgrade failed")?;
    }

    if operational_rows(&pool).await? != rows {
        return Err("legacy fixture downgrade changed operational rows");
    }
    check_foreign_keys(&pool).await?;
    if schema_snapshot(&pool).await? != canonical_schema(prior.version()).await? {
        return Err("legacy fixture did not match committed prior schema");
    }
    let retained_identities = migration_identities(&pool).await?;
    if retained_identities != original_identities[..prior.version() as usize] {
        return Err("legacy fixture retained migration identities changed");
    }
    let retained_history = migration_history(&pool).await?;
    if retained_history != full_history[..prior.version() as usize] {
        return Err("legacy fixture retained migration history changed");
    }

    Ok(LegacyFixtureProof {
        prior,
        rows,
        original_identities,
        retained_history,
    })
    }.await;
    pool.close().await;
    result
}

pub(super) async fn try_check_upgraded_fixture(
    temp: &tempfile::TempDir,
    proof: &LegacyFixtureProof,
) -> Result<(), &'static str> {
    let pool = fixture_pool(temp).await?;
    let result = async {
    check_foreign_keys(&pool).await?;
    if operational_rows(&pool).await? != proof.rows {
        return Err("legacy fixture upgrade changed operational rows");
    }
    let null_later_inputs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM preview_observations WHERE search_query IS NULL AND manual_selection IS NULL",
    )
    .fetch_one(&pool)
    .await
    .map_err(|_| "legacy fixture upgraded input query failed")?;
    if null_later_inputs != proof.rows.observations.len() as i64 {
        return Err("legacy fixture upgraded inputs were not NULL");
    }
    if schema_snapshot(&pool).await? != canonical_schema(7).await? {
        return Err("legacy fixture upgrade did not restore committed current schema");
    }
    let identities = migration_identities(&pool).await?;
    if identities != proof.original_identities {
        return Err("legacy fixture upgraded migration identities changed");
    }
    let history = migration_history(&pool).await?;
    if history.get(..proof.prior.version() as usize) != Some(proof.retained_history.as_slice()) {
        return Err("legacy fixture upgraded migration history prefix changed");
    }
    Ok(())
    }.await;
    pool.close().await;
    result
}

// CLI scenarios retain their assertion-style interface; browser ownership uses
// the same complete proof with errors returned only after every pool is closed.
pub(super) async fn downgrade_closed_fixture(
    temp: &tempfile::TempDir,
    prior: PriorSchema,
) -> LegacyFixtureProof {
    try_downgrade_closed_fixture(temp, prior)
        .await
        .expect("E5e legacy fixture proof failed")
}

pub(super) async fn assert_upgraded_fixture(temp: &tempfile::TempDir, proof: &LegacyFixtureProof) {
    try_check_upgraded_fixture(temp, proof)
        .await
        .expect("E5e upgraded fixture proof failed");
}

#[tokio::test]
async fn fallible_legacy_proof_rejects_unpopulated_fixture_and_releases_database() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path()).await.unwrap();
    storage.close().await;
    let error = tokio::time::timeout(
        Duration::from_secs(5),
        try_downgrade_closed_fixture(&temp, PriorSchema::V5),
    )
    .await
    .expect("legacy proof failure must be bounded")
    .err()
    .expect("empty operational state cannot prove a populated legacy fixture");
    assert_eq!(
        error,
        "legacy fixture populated operational row counts changed"
    );
    // Windows removal also checks that the failure path released SQLite handles.
    temp.close()
        .expect("legacy proof failure retained database handles");
}

#[tokio::test]
async fn fallible_migration_queries_are_static_and_close_after_malformed_schema() {
    let temp = tempfile::tempdir().unwrap();
    let storage = Storage::open(temp.path()).await.unwrap();
    storage.close().await;
    let pool = fixture_pool(&temp).await.unwrap();
    let proof = LegacyFixtureProof {
        prior: PriorSchema::V5,
        rows: operational_rows(&pool).await.unwrap(),
        original_identities: migration_identities(&pool).await.unwrap(),
        retained_history: migration_history(&pool).await.unwrap()[..5].to_vec(),
    };
    sqlx::raw_sql("DROP TABLE preview_results;")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let errors = tokio::time::timeout(Duration::from_secs(5), async {
        let downgrade = try_downgrade_closed_fixture(&temp, PriorSchema::V6)
            .await
            .err();
        let upgrade = try_check_upgraded_fixture(&temp, &proof).await.err();
        (downgrade, upgrade)
    })
    .await
    .expect("malformed legacy proof failures must be bounded");
    assert_eq!(
        errors,
        (
            Some("legacy fixture query failed"),
            Some("legacy fixture query failed")
        )
    );
    temp.close()
        .expect("malformed proof retained database handles");
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
