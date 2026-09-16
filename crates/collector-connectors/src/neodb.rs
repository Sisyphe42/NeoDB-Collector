use collector_core::{
    access::PortFuture,
    oauth::{NeoDbOAuthTransport, OAuthApp, OAuthIdentity, OAuthToken},
};
use collector_domain::ApiError;
use reqwest::{Client, Response, redirect::Policy};
use serde::{Deserialize, de::DeserializeOwned};
use std::time::Duration;
use url::Url;

const RESPONSE_LIMIT: usize = 64 * 1024;

pub struct NeoDbClient {
    http: Client,
}

impl NeoDbClient {
    pub fn new() -> Result<Self, ApiError> {
        let http = Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("NeoDB-Collector/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| upstream())?;
        Ok(Self { http })
    }
}

fn invalid_base() -> ApiError {
    ApiError::new(
        "neodb_base_url_invalid",
        "Use an HTTPS origin, or literal loopback HTTP, without credentials, query, fragment or base path.",
    )
}
fn upstream() -> ApiError {
    ApiError::new(
        "neodb_oauth_unavailable",
        "NeoDB authorization service is unavailable or returned an invalid response.",
    )
}
fn canonical(input: &str) -> Result<Url, ApiError> {
    let mut url = Url::parse(input).map_err(|_| invalid_base())?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(invalid_base());
    }
    match url.scheme() {
        "https" => {}
        "http" => {
            let loopback = match url.host() {
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                _ => false,
            };
            if !loopback {
                return Err(invalid_base());
            }
        }
        _ => return Err(invalid_base()),
    }
    url.set_path("");
    Ok(url)
}
fn endpoint(base: &str, path: &str) -> Result<Url, ApiError> {
    canonical(base)?.join(path).map_err(|_| invalid_base())
}
async fn limited<T: DeserializeOwned>(mut response: Response) -> Result<T, ApiError> {
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > RESPONSE_LIMIT as u64)
    {
        return Err(upstream());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| upstream())? {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_LIMIT {
            return Err(upstream());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| upstream())
}

#[derive(Deserialize)]
struct AppReply {
    client_id: String,
    client_secret: String,
}
#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    token_type: String,
    scope: String,
}
#[derive(Deserialize)]
struct IdentityReply {
    username: String,
}

impl NeoDbOAuthTransport for NeoDbClient {
    fn canonical_base(&self, input: String) -> PortFuture<'_, String> {
        Box::pin(async move {
            Ok(canonical(&input)?
                .to_string()
                .trim_end_matches('/')
                .to_owned())
        })
    }
    fn register(&self, base: String, callback: String) -> PortFuture<'_, OAuthApp> {
        Box::pin(async move {
            let reply: AppReply = limited(
                self.http
                    .post(endpoint(&base, "/api/v1/apps")?)
                    .form(&[
                        ("client_name", "NeoDB Collector"),
                        ("redirect_uris", callback.as_str()),
                        ("website", "https://github.com/neodb-social/neodb"),
                    ])
                    .send()
                    .await
                    .map_err(|_| upstream())?,
            )
            .await?;
            if reply.client_id.is_empty() || reply.client_secret.is_empty() {
                return Err(upstream());
            }
            Ok(OAuthApp {
                client_id: reply.client_id,
                client_secret: reply.client_secret,
            })
        })
    }
    fn authorization_url(
        &self,
        base: String,
        client_id: String,
        callback: String,
        state: String,
    ) -> PortFuture<'_, String> {
        Box::pin(async move {
            if client_id.is_empty() || state.is_empty() {
                return Err(upstream());
            }
            let mut url = endpoint(&base, "/oauth/authorize")?;
            url.query_pairs_mut()
                .append_pair("response_type", "code")
                .append_pair("client_id", &client_id)
                .append_pair("redirect_uri", &callback)
                .append_pair("scope", "read")
                .append_pair("state", &state);
            Ok(url.into())
        })
    }
    fn exchange(
        &self,
        base: String,
        app: OAuthApp,
        code: String,
        callback: String,
    ) -> PortFuture<'_, OAuthToken> {
        Box::pin(async move {
            if code.is_empty() {
                return Err(upstream());
            }
            let reply: TokenReply = limited(
                self.http
                    .post(endpoint(&base, "/oauth/token")?)
                    .form(&[
                        ("client_id", app.client_id.as_str()),
                        ("client_secret", app.client_secret.as_str()),
                        ("code", code.as_str()),
                        ("redirect_uri", callback.as_str()),
                        ("grant_type", "authorization_code"),
                    ])
                    .send()
                    .await
                    .map_err(|_| upstream())?,
            )
            .await?;
            if reply.access_token.is_empty()
                || reply.token_type != "Bearer"
                || reply.scope.split_whitespace().any(|s| s != "read")
                || !reply.scope.split_whitespace().any(|s| s == "read")
            {
                return Err(upstream());
            }
            Ok(OAuthToken {
                access_token: reply.access_token,
                scope: reply.scope,
            })
        })
    }
    fn identity(&self, base: String, token: String) -> PortFuture<'_, OAuthIdentity> {
        Box::pin(async move {
            if token.is_empty() {
                return Err(upstream());
            }
            let reply: IdentityReply = limited(
                self.http
                    .get(endpoint(&base, "/api/me")?)
                    .bearer_auth(token)
                    .send()
                    .await
                    .map_err(|_| upstream())?,
            )
            .await?;
            if reply.username.trim().is_empty() {
                return Err(upstream());
            }
            Ok(OAuthIdentity {
                username: reply.username,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Form, Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        response::Redirect,
        routing::{get, post},
    };
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    #[derive(Clone, Default)]
    struct Seen(Arc<Mutex<Vec<String>>>);
    async fn app(
        State(seen): State<Seen>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        assert_eq!(
            form.get("client_name").map(String::as_str),
            Some("NeoDB Collector")
        );
        assert!(
            form.get("redirect_uris")
                .unwrap()
                .ends_with("/api/v1/oauth/neodb/callback")
        );
        seen.0.lock().unwrap().push("apps".into());
        Json(
            serde_json::json!({"client_id":"client", "client_secret":"secret", "extra":"accepted"}),
        )
    }
    async fn token(
        State(seen): State<Seen>,
        Form(form): Form<HashMap<String, String>>,
    ) -> Json<serde_json::Value> {
        assert_eq!(
            form.get("grant_type").map(String::as_str),
            Some("authorization_code")
        );
        assert_eq!(form.get("code").map(String::as_str), Some("code"));
        seen.0.lock().unwrap().push("token".into());
        Json(serde_json::json!({"access_token":"access", "token_type":"Bearer", "scope":"read"}))
    }
    async fn me(State(seen): State<Seen>, headers: HeaderMap) -> Json<serde_json::Value> {
        assert_eq!(headers.get("authorization").unwrap(), "Bearer access");
        seen.0.lock().unwrap().push("me".into());
        Json(serde_json::json!({"username":"alice", "display_name":"Alice"}))
    }
    async fn server(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        (base, task)
    }

    #[tokio::test]
    async fn validates_and_canonicalizes_only_safe_origins() {
        let client = NeoDbClient::new().unwrap();
        for valid in [
            "https://neodb.example",
            "https://neodb.example/",
            "http://127.0.0.1:8000",
            "http://[::1]:8000",
        ] {
            assert!(client.canonical_base(valid.into()).await.is_ok(), "{valid}");
        }
        for invalid in [
            "http://neodb.example",
            "http://localhost:8000",
            "ftp://127.0.0.1",
            "https://user@neodb.example",
            "https://neodb.example/base",
            "https://neodb.example/?q=x",
            "https://neodb.example/#x",
        ] {
            assert_eq!(
                client
                    .canonical_base(invalid.into())
                    .await
                    .unwrap_err()
                    .code,
                "neodb_base_url_invalid",
                "{invalid}"
            );
        }
    }

    #[tokio::test]
    async fn uses_only_reviewed_read_scope_oauth_endpoints() {
        let seen = Seen::default();
        let router = Router::new()
            .route("/api/v1/apps", post(app))
            .route("/oauth/token", post(token))
            .route("/api/me", get(me))
            .with_state(seen.clone());
        let (base, task) = server(router).await;
        let callback = "http://127.0.0.1:19423/api/v1/oauth/neodb/callback".to_owned();
        let client = NeoDbClient::new().unwrap();
        let app = client
            .register(base.clone(), callback.clone())
            .await
            .unwrap();
        let authorize = client
            .authorization_url(
                base.clone(),
                app.client_id.clone(),
                callback.clone(),
                "state".into(),
            )
            .await
            .unwrap();
        let authorize = Url::parse(&authorize).unwrap();
        assert_eq!(authorize.path(), "/oauth/authorize");
        let pairs: HashMap<_, _> = authorize.query_pairs().into_owned().collect();
        assert_eq!(pairs.get("scope").map(String::as_str), Some("read"));
        assert_eq!(pairs.get("state").map(String::as_str), Some("state"));
        let token = client
            .exchange(base.clone(), app, "code".into(), callback)
            .await
            .unwrap();
        assert_eq!(token.scope, "read");
        assert_eq!(
            client
                .identity(base, token.access_token)
                .await
                .unwrap()
                .username,
            "alice"
        );
        assert_eq!(*seen.0.lock().unwrap(), ["apps", "token", "me"]);
        task.abort();
    }

    #[tokio::test]
    async fn rejects_redirects_write_scope_oversize_and_upstream_details() {
        let redirect = Router::new().route(
            "/api/v1/apps",
            post(|| async { Redirect::temporary("http://127.0.0.1:9/stolen") }),
        );
        let (base, task) = server(redirect).await;
        let client = NeoDbClient::new().unwrap();
        let error = match client
            .register(base, "http://127.0.0.1/callback".into())
            .await
        {
            Ok(_) => panic!("redirect must be rejected"),
            Err(error) => error,
        };
        assert_eq!(error.code, "neodb_oauth_unavailable");
        assert!(!error.message.contains("127.0.0.1:9"));
        task.abort();

        let invalid = Router::new()
            .route("/oauth/token", post(|| async { Json(serde_json::json!({"access_token":"private", "token_type":"Bearer", "scope":"read write"})) }))
            .route("/api/me", get(|| async { (StatusCode::OK, "x".repeat(RESPONSE_LIMIT + 1)) }));
        let (base, task) = server(invalid).await;
        let app = OAuthApp {
            client_id: "id".into(),
            client_secret: "secret".into(),
        };
        assert!(
            client
                .exchange(
                    base.clone(),
                    app,
                    "code".into(),
                    "http://127.0.0.1/callback".into()
                )
                .await
                .is_err()
        );
        assert!(client.identity(base, "access".into()).await.is_err());
        task.abort();
    }
}
