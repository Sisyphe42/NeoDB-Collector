-- Historical normalized preview evidence and completed lifecycle.
DROP INDEX preview_jobs_status_expiry;

ALTER TABLE preview_observations RENAME TO preview_observations_v4;
ALTER TABLE preview_jobs RENAME TO preview_jobs_v4;

CREATE TABLE preview_jobs (
    id TEXT NOT NULL PRIMARY KEY
        CHECK (length(id) BETWEEN 1 AND 128 AND id NOT GLOB '*[^A-Za-z0-9_.:-]*'),
    profile_id TEXT NOT NULL,
    profile_version INTEGER NOT NULL CHECK (profile_version BETWEEN 1 AND 4294967295),
    base_url TEXT NOT NULL,
    account TEXT NOT NULL,
    credential_ref TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued','running','interrupted','failed','cancelled','completed')),
    attempt INTEGER NOT NULL CHECK (attempt BETWEEN 1 AND 4294967295),
    created_at INTEGER NOT NULL CHECK (created_at >= 0),
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    expires_at INTEGER NOT NULL CHECK (expires_at > created_at),
    observation_count INTEGER NOT NULL CHECK (observation_count BETWEEN 0 AND 10000),
    FOREIGN KEY (profile_id, profile_version)
        REFERENCES mapping_profiles(id, version)
);

CREATE TABLE preview_observations (
    job_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 9999),
    observation_json TEXT NOT NULL
        CHECK (length(CAST(observation_json AS BLOB)) <= 16384),
    PRIMARY KEY (job_id, ordinal),
    FOREIGN KEY (job_id) REFERENCES preview_jobs(id) ON DELETE CASCADE
);

INSERT INTO preview_jobs
SELECT id, profile_id, profile_version, base_url, account, credential_ref,
       status, attempt, created_at, updated_at, expires_at, observation_count
FROM preview_jobs_v4;

INSERT INTO preview_observations
SELECT job_id, ordinal, observation_json FROM preview_observations_v4;

DROP TABLE preview_observations_v4;
DROP TABLE preview_jobs_v4;

CREATE INDEX preview_jobs_status_expiry ON preview_jobs(status, expires_at);

CREATE TABLE preview_results (
    job_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 9999),
    attempt INTEGER NOT NULL CHECK (attempt BETWEEN 1 AND 4294967295),
    committed_at INTEGER NOT NULL CHECK (committed_at >= 0),
    result_json TEXT NOT NULL
        CHECK (length(CAST(result_json AS BLOB)) <= 65536),
    PRIMARY KEY (job_id, ordinal),
    FOREIGN KEY (job_id, ordinal)
        REFERENCES preview_observations(job_id, ordinal) ON DELETE CASCADE
);
CREATE INDEX preview_results_attempt_ordinal
    ON preview_results(job_id, attempt, ordinal);
