//! Thin local API client; never opens Core's database.
use collector_core::access::{SecretStore, TokenSource};
use collector_core::secrets::{ExposeSecret, SecretString};
use collector_domain::{
    API_VERSION, ApiError, HealthDocument, SCHEMA_VERSION,
    access::{PairedClient, PairingExchange, PairingToken},
};
use collector_storage::credentials::SecureTokens;
use reqwest::{Client, Response, redirect::Policy};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    io::{IsTerminal, Read},
    time::Duration,
};
use url::{Host, Url};

pub mod preview;

#[cfg(test)]
mod preview_tests;

#[cfg(test)]
mod preview_integration_tests;

fn error(code: &str, message: &str) -> ApiError {
    ApiError::new(code, message)
}
fn unavailable() -> ApiError {
    error(
        "core_unavailable",
        "Core is unavailable or returned an invalid response.",
    )
}
fn invalid() -> ApiError {
    error(
        "core_url_invalid",
        "Use a literal loopback HTTP origin without credentials, path, query or fragment.",
    )
}

fn origin(input: &str) -> Result<Url, ApiError> {
    let url = Url::parse(input).map_err(|_| invalid())?;
    let local = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if !local
        || url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port() == Some(0)
    {
        return Err(invalid());
    }
    Ok(url)
}

fn pairing(input: &str) -> Result<(Url, String), ApiError> {
    let mut url = Url::parse(input.trim()).map_err(|_| invalid())?;
    if url.path() != "/pair" || url.query().is_some() {
        return Err(invalid());
    }
    let fragment = url.fragment().ok_or_else(invalid)?;
    let pairs: Vec<_> = url::form_urlencoded::parse(fragment.as_bytes()).collect();
    if pairs.len() != 1 || pairs[0].0 != "key" {
        return Err(invalid());
    }
    let key = pairs[0].1.to_string();
    collector_core::access::validate_key(&key)?;
    url.set_fragment(None);
    url.set_path("/");
    Ok((origin(url.as_str())?, key))
}

pub fn read_pairing() -> Result<String, ApiError> {
    if std::io::stdin().is_terminal() {
        return Err(error(
            "pairing_input_required",
            "Pipe the one-time pairing link to standard input; do not pass secrets as command arguments.",
        ));
    }
    read_bounded(std::io::stdin().lock())
}
fn read_bounded(reader: impl Read) -> Result<String, ApiError> {
    let mut text = String::new();
    reader
        .take(4097)
        .read_to_string(&mut text)
        .map_err(|_| error("pairing_input_invalid", "Cannot read pairing link."))?;
    if text.len() > 4096 {
        return Err(error(
            "pairing_input_invalid",
            "Pairing link exceeds the input limit.",
        ));
    }
    Ok(text)
}
fn reference(base: &Url) -> String {
    format!("collector-cli:{}", SecureTokens.digest(base.as_str()))
}
fn http() -> Result<Client, ApiError> {
    Client::builder()
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| unavailable())
}
async fn decode<T: DeserializeOwned>(mut response: Response) -> Result<T, ApiError> {
    if response.status().as_u16() == 401 {
        return Err(error(
            "authentication_required",
            "Pairing is invalid, expired or revoked; create a new link in WebUI.",
        ));
    }
    if !response.status().is_success() {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len() + chunk.len() > 65536 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}
async fn health(client: &Client, base: &Url) -> Result<HealthDocument, ApiError> {
    let doc: HealthDocument = decode(
        client
            .get(base.join("api/v1/health").map_err(|_| invalid())?)
            .send()
            .await
            .map_err(|_| unavailable())?,
    )
    .await?;
    if doc.api_version != API_VERSION || doc.schema_version != SCHEMA_VERSION {
        return Err(error(
            "core_incompatible",
            "Core API/schema version is incompatible.",
        ));
    }
    Ok(doc)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairReport {
    pub paired: bool,
    pub core_url: String,
    pub client_id: String,
}
impl std::fmt::Display for PairReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Paired with {} (client {}).",
            self.core_url, self.client_id
        )
    }
}

pub async fn pair(
    input: &str,
    name: &str,
    secrets: &dyn SecretStore,
) -> Result<PairReport, ApiError> {
    let (base, key) = pairing(input)?;
    collector_core::access::validate_client(name, "cli")?;
    let client = http()?;
    health(&client, &base).await?;
    // Fail before consuming a link if the native store cannot write/delete.
    let probe = format!("collector-cli-probe:{}", SecureTokens.random()?);
    if let Err(failure) = secrets.set(probe.clone(), "probe".into()).await {
        let _ = secrets.delete(probe).await;
        return Err(failure);
    }
    secrets.delete(probe).await?;
    let response = client
        .post(
            base.join("api/v1/pairings/exchange")
                .map_err(|_| invalid())?,
        )
        .json(&PairingExchange {
            one_time_key: key,
            client_name: name.into(),
            client_kind: "cli".into(),
        })
        .send()
        .await
        .map_err(|_| unavailable())?;
    let paired: PairingToken = decode(response).await?;
    let token = SecretString::from(paired.token);
    collector_core::access::validate_key(token.expose_secret())?;
    collector_core::access::validate_key(&paired.client_id)?;
    if paired.token_type != "Bearer" {
        return Err(unavailable());
    }
    secrets.set(reference(&base), token).await.map_err(|_| error(
        "pairing_storage_failed", "Core paired the client but saving its token failed. Revoke that client in WebUI and pair again.",
    ))?;
    Ok(PairReport {
        paired: true,
        core_url: base.origin().ascii_serialization(),
        client_id: paired.client_id,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusReport {
    pub authenticated: bool,
    pub core_url: String,
    pub client: PairedClient,
    pub health: HealthDocument,
}
impl std::fmt::Display for StatusReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Authenticated with {} as {} (client {}). Core {} / API {}.",
            self.core_url,
            self.client.name,
            self.client.id,
            self.health.core_version,
            self.health.api_version
        )
    }
}
pub async fn status(input: &str, secrets: &dyn SecretStore) -> Result<StatusReport, ApiError> {
    let base = origin(input)?;
    let client = http()?;
    let health = health(&client, &base).await?;
    let token = secrets.get(reference(&base)).await?;
    collector_core::access::validate_key(token.expose_secret())?;
    let identity: PairedClient = decode(
        client
            .get(base.join("api/v1/client").map_err(|_| invalid())?)
            .bearer_auth(token.expose_secret())
            .send()
            .await
            .map_err(|_| unavailable())?,
    )
    .await?;
    if identity.revoked || identity.kind != "cli" {
        return Err(unavailable());
    }
    Ok(StatusReport {
        authenticated: true,
        core_url: base.origin().ascii_serialization(),
        client: identity,
        health,
    })
}

#[cfg(test)]
mod tests;
