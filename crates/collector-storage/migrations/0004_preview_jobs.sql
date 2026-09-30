-- Bounded normalized preview input and operational lifecycle only.
CREATE TABLE preview_jobs (
    id TEXT NOT NULL PRIMARY KEY
        CHECK (length(id) BETWEEN 1 AND 128 AND id NOT GLOB '*[^A-Za-z0-9_.:-]*'),
    profile_id TEXT NOT NULL,
    profile_version INTEGER NOT NULL CHECK (profile_version BETWEEN 1 AND 4294967295),
    base_url TEXT NOT NULL,
    account TEXT NOT NULL,
    credential_ref TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued','running','interrupted','failed','cancelled')),
    attempt INTEGER NOT NULL CHECK (attempt BETWEEN 1 AND 4294967295),
    created_at INTEGER NOT NULL CHECK (created_at >= 0),
    updated_at INTEGER NOT NULL CHECK (updated_at >= created_at),
    expires_at INTEGER NOT NULL CHECK (expires_at > created_at),
    observation_count INTEGER NOT NULL CHECK (observation_count BETWEEN 0 AND 10000),
    FOREIGN KEY (profile_id, profile_version)
        REFERENCES mapping_profiles(id, version)
);
CREATE INDEX preview_jobs_status_expiry ON preview_jobs(status, expires_at);

CREATE TABLE preview_observations (
    job_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal BETWEEN 0 AND 9999),
    observation_json TEXT NOT NULL
        CHECK (length(CAST(observation_json AS BLOB)) <= 16384),
    PRIMARY KEY (job_id, ordinal),
    FOREIGN KEY (job_id) REFERENCES preview_jobs(id) ON DELETE CASCADE
);
