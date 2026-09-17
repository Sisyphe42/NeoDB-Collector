-- Normalized configuration only, never source observations or raw imports.
CREATE TABLE mapping_profiles (
    id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 4294967295),
    profile_json TEXT NOT NULL CHECK (length(CAST(profile_json AS BLOB)) <= 65536),
    PRIMARY KEY (id, version)
);
