CREATE TABLE access_credentials (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('bootstrap', 'session', 'pairing', 'client')),
    audience TEXT NOT NULL,
    name TEXT NOT NULL,
    client_kind TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked INTEGER NOT NULL DEFAULT 0 CHECK (revoked IN (0,1))
);
CREATE INDEX access_expiry ON access_credentials(expires_at);
CREATE TABLE neodb_endpoint (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    base_url TEXT NOT NULL,
    account TEXT NOT NULL,
    credential_ref TEXT NOT NULL
);
