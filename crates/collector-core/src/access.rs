//! Access use cases and ports; no OS, HTTP, SQL or async-runtime dependency.
use crate::secrets::SecretString;
use collector_domain::{ApiError, access::*};
use std::{future::Future, pin::Pin, sync::Arc};

pub type PortFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ApiError>> + Send + 'a>>;
pub const PAIRING_SCOPES: &[&str] = &[
    "settings.read",
    "imports.read",
    "imports.submit",
    "mapping.read",
    "mapping.write",
];
pub const KEY_TTL: i64 = 600;
pub const SESSION_TTL: i64 = 8 * 60 * 60;

#[derive(Clone)]
pub struct AccessRecord {
    pub id: String,
    pub hash: String,
    pub kind: String,
    pub audience: String,
    pub name: String,
    pub client_kind: String,
    pub expires_at: i64,
}
#[derive(Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub account: String,
    pub credential_ref: String,
}

pub struct ChallengeMatch {
    pub hash: String,
    pub kind: String,
    pub audience: String,
    pub name: String,
    pub client_kind: String,
    pub now: i64,
}

pub trait AccessRepository: Send + Sync {
    fn insert(&self, record: AccessRecord) -> PortFuture<'_, ()>;
    /// Atomically consumes a live challenge and inserts its replacement credential.
    fn exchange(&self, challenge: ChallengeMatch, replacement: AccessRecord) -> PortFuture<'_, ()>;
    fn find(
        &self,
        hash: String,
        kind: String,
        audience: String,
        now: i64,
    ) -> PortFuture<'_, Option<AccessRecord>>;
    fn revoke(&self, id: String, kind: String) -> PortFuture<'_, ()>;
    fn clients(&self) -> PortFuture<'_, Vec<PairedClient>>;
    fn endpoint(&self) -> PortFuture<'_, Option<Endpoint>>;
    fn set_endpoint(&self, endpoint: Option<Endpoint>) -> PortFuture<'_, ()>;
}

pub trait SecretStore: Send + Sync {
    fn set(&self, reference: String, secret: SecretString) -> PortFuture<'_, ()>;
    fn get(&self, reference: String) -> PortFuture<'_, SecretString>;
    fn delete(&self, reference: String) -> PortFuture<'_, ()>;
}
pub trait TokenSource: Send + Sync {
    fn random(&self) -> Result<String, ApiError>;
    fn digest(&self, value: &str) -> String;
}
pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}

pub struct AccessService {
    pub repository: Arc<dyn AccessRepository>,
    pub tokens: Arc<dyn TokenSource>,
    pub clock: Arc<dyn Clock>,
    pub audience: String,
}

impl AccessService {
    fn record(
        &self,
        kind: &str,
        secret: &str,
        ttl: i64,
        name: &str,
        client_kind: &str,
    ) -> Result<AccessRecord, ApiError> {
        Ok(AccessRecord {
            id: self.tokens.random()?,
            hash: self.tokens.digest(secret),
            kind: kind.into(),
            audience: self.audience.clone(),
            name: name.into(),
            client_kind: client_kind.into(),
            expires_at: self.clock.now().saturating_add(ttl),
        })
    }
    pub async fn bootstrap(&self) -> Result<String, ApiError> {
        let secret = self.tokens.random()?;
        self.repository
            .insert(self.record("bootstrap", &secret, KEY_TTL, "", "")?)
            .await?;
        Ok(secret)
    }
    pub async fn open_session(&self, key: &str) -> Result<(String, SessionView), ApiError> {
        validate_key(key)?;
        let token = self.tokens.random()?;
        let record = self.record("session", &token, SESSION_TTL, "", "")?;
        let expires_at = record.expires_at;
        self.repository
            .exchange(
                ChallengeMatch {
                    hash: self.tokens.digest(key),
                    kind: "bootstrap".into(),
                    audience: self.audience.clone(),
                    name: "".into(),
                    client_kind: "".into(),
                    now: self.clock.now(),
                },
                record,
            )
            .await?;
        Ok((
            token.clone(),
            SessionView {
                csrf_token: self.csrf(&token),
                expires_at,
            },
        ))
    }
    pub fn csrf(&self, session: &str) -> String {
        self.tokens.digest(&format!("collector-csrf:{session}"))
    }
    pub async fn session(&self, token: &str) -> Result<AccessRecord, ApiError> {
        self.authenticate(token, "session").await
    }
    pub async fn bearer(&self, token: &str) -> Result<AccessRecord, ApiError> {
        self.authenticate(token, "client").await
    }
    async fn authenticate(&self, token: &str, kind: &str) -> Result<AccessRecord, ApiError> {
        validate_key(token)?;
        self.repository
            .find(
                self.tokens.digest(token),
                kind.into(),
                self.audience.clone(),
                self.clock.now(),
            )
            .await?
            .ok_or_else(|| {
                ApiError::new(
                    "authentication_required",
                    "Credential is invalid, expired or revoked.",
                )
            })
    }
    pub async fn create_pairing(&self, request: PairingRequest) -> Result<PairingLink, ApiError> {
        validate_client(&request.client_name, &request.client_kind)?;
        let secret = self.tokens.random()?;
        let record = self.record(
            "pairing",
            &secret,
            KEY_TTL,
            &request.client_name,
            &request.client_kind,
        )?;
        let expires_at = record.expires_at;
        self.repository.insert(record).await?;
        Ok(PairingLink {
            uri: format!("{}/pair#key={secret}", self.audience),
            expires_at,
        })
    }
    pub async fn pair(&self, request: PairingExchange) -> Result<PairingToken, ApiError> {
        validate_key(&request.one_time_key)?;
        validate_client(&request.client_name, &request.client_kind)?;
        let token = self.tokens.random()?;
        let record = self.record(
            "client",
            &token,
            i64::MAX - self.clock.now(),
            &request.client_name,
            &request.client_kind,
        )?;
        let id = record.id.clone();
        self.repository
            .exchange(
                ChallengeMatch {
                    hash: self.tokens.digest(&request.one_time_key),
                    kind: "pairing".into(),
                    audience: self.audience.clone(),
                    name: request.client_name,
                    client_kind: request.client_kind,
                    now: self.clock.now(),
                },
                record,
            )
            .await?;
        Ok(PairingToken {
            client_id: id,
            token,
            token_type: "Bearer".into(),
            scopes: PAIRING_SCOPES.iter().map(|s| (*s).into()).collect(),
        })
    }
}

pub fn validate_key(key: &str) -> Result<(), ApiError> {
    if key.len() != 43
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ApiError::new(
            "authentication_required",
            "Invalid credential.",
        ));
    }
    Ok(())
}
pub fn validate_client(name: &str, kind: &str) -> Result<(), ApiError> {
    if name.trim().is_empty()
        || name.len() > 80
        || name.chars().any(char::is_control)
        || !["cli", "browser_extension", "local_plugin"].contains(&kind)
    {
        return Err(ApiError::new(
            "pairing_scope_invalid",
            "Provide a valid name and supported client kind.",
        ));
    }
    Ok(())
}
