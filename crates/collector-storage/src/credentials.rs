use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use collector_core::access::{Clock, PortFuture, SecretStore, TokenSource};
use collector_domain::ApiError;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

// Native stores have ordering caveats even though the Rust wrapper is thread-safe.
static KEYRING_ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .min(i64::MAX as u64) as i64
    }
}
pub struct SecureTokens;
impl TokenSource for SecureTokens {
    fn random(&self) -> Result<String, ApiError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|_| ApiError::new("entropy_unavailable", "OS randomness is unavailable."))?;
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn digest(&self, value: &str) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(value.as_bytes()))
    }
}
pub struct OsSecretStore;
fn unavailable() -> ApiError {
    ApiError::new(
        "credential_store_unavailable",
        "OS credential store is unavailable or the credential is missing; unlock it and retry. No plaintext fallback is used.",
    )
}
fn entry(reference: &str) -> Result<keyring::Entry, ApiError> {
    keyring::Entry::new("org.neodb.collector", reference).map_err(|_| unavailable())
}
impl SecretStore for OsSecretStore {
    fn set(&self, reference: String, secret: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let _guard = KEYRING_ACCESS.lock().map_err(|_| unavailable())?;
                entry(&reference)?
                    .set_password(&secret)
                    .map_err(|_| unavailable())
            })
            .await
            .map_err(|_| unavailable())?
        })
    }
    fn get(&self, reference: String) -> PortFuture<'_, String> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let _guard = KEYRING_ACCESS.lock().map_err(|_| unavailable())?;
                entry(&reference)?.get_password().map_err(|_| unavailable())
            })
            .await
            .map_err(|_| unavailable())?
        })
    }
    fn delete(&self, reference: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let _guard = KEYRING_ACCESS.lock().map_err(|_| unavailable())?;
                match entry(&reference)?.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                    Err(_) => Err(unavailable()),
                }
            })
            .await
            .map_err(|_| unavailable())?
        })
    }
}
