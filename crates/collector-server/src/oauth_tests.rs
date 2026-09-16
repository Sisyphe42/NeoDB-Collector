use super::*;
use axum::body::{Body, to_bytes};
use collector_core::{
    access::{AccessService, PortFuture, SecretStore},
    oauth::*,
};
use collector_storage::{
    Storage,
    credentials::{SecureTokens, SystemClock},
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Mutex};
use tower::ServiceExt;

#[derive(Default)]
struct Secrets(Mutex<HashMap<String, String>>);
impl SecretStore for Secrets {
    fn set(&self, r: String, s: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            self.0.lock().unwrap().insert(r, s);
            Ok(())
        })
    }
    fn get(&self, r: String) -> PortFuture<'_, String> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap()
                .get(&r)
                .cloned()
                .ok_or_else(|| ApiError::new("missing", "Missing."))
        })
    }
    fn delete(&self, r: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            self.0.lock().unwrap().remove(&r);
            Ok(())
        })
    }
}
struct Transport;
impl NeoDbOAuthTransport for Transport {
    fn canonical_base(&self, input: String) -> PortFuture<'_, String> {
        Box::pin(async move { Ok(input) })
    }
    fn register(&self, _: String, callback: String) -> PortFuture<'_, OAuthApp> {
        Box::pin(async move {
            assert_eq!(
                callback,
                "http://127.0.0.1:19423/api/v1/oauth/neodb/callback"
            );
            Ok(OAuthApp {
                client_id: "client".into(),
                client_secret: "secret-app".into(),
            })
        })
    }
    fn authorization_url(
        &self,
        base: String,
        _: String,
        _: String,
        state: String,
    ) -> PortFuture<'_, String> {
        Box::pin(async move { Ok(format!("{base}/oauth/authorize?state={state}&scope=read")) })
    }
    fn exchange(
        &self,
        _: String,
        _: OAuthApp,
        code: String,
        _: String,
    ) -> PortFuture<'_, OAuthToken> {
        Box::pin(async move {
            assert_eq!(code, "valid-code");
            Ok(OAuthToken {
                access_token: "secret-token".into(),
                scope: "read".into(),
            })
        })
    }
    fn identity(&self, _: String, _: String) -> PortFuture<'_, OAuthIdentity> {
        Box::pin(async {
            Ok(OAuthIdentity {
                username: "alice".into(),
            })
        })
    }
}
async fn call(
    app: &Router,
    method: &str,
    path: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:19423")
        .header("content-type", "application/json");
    for (key, value) in headers {
        builder = builder.header(*key, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(headers["referrer-policy"], "no-referrer");
    let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        !text.contains("secret-app")
            && !text.contains("secret-token")
            && !text.contains("valid-code")
    );
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn oauth_http_session_csrf_state_callback_and_account_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    let access = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(SystemClock),
        audience: "http://127.0.0.1:19423".into(),
    });
    let secrets = Arc::new(Secrets::default());
    let mut state = access::AccessState::new(access.clone());
    state.oauth = Some(Arc::new(OAuthService::new(
        Arc::new(storage.clone()),
        secrets.clone(),
        Arc::new(SecureTokens),
        Arc::new(SystemClock),
        Arc::new(Transport),
        access.audience.clone(),
    )));
    let app = protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        "127.0.0.1:19423".parse().unwrap(),
        Some(state),
    );
    let (token, session) = access
        .open_session(&access.bootstrap().await.unwrap())
        .await
        .unwrap();
    let cookie = format!("collector_session={token}");
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", access.audience.as_str()),
        ("x-csrf-token", session.csrf_token.as_str()),
    ];
    let body = json!({"baseUrl":"https://neodb.example"});
    assert_eq!(
        call(&app, "GET", "/api/v1/settings/neodb", Value::Null, &[])
            .await
            .0,
        401
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/oauth/neodb",
            body.clone(),
            &[("cookie", &cookie)]
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/v1/oauth/neodb",
            body.clone(),
            &[("authorization", "Bearer no"), ("cookie", &cookie)]
        )
        .await
        .0,
        403
    );
    for denied in [false, true] {
        let result = call(&app, "POST", "/api/v1/oauth/neodb", body.clone(), &headers).await;
        assert_eq!(result.0, 200);
        let url = result.2["authorizationUrl"].as_str().unwrap();
        let state = url
            .split("state=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap();
        assert_eq!(
            call(&app, "POST", "/api/v1/oauth/neodb", body.clone(), &headers)
                .await
                .0,
            409
        );
        let wrong = call(
            &app,
            "GET",
            "/api/v1/oauth/neodb/callback?state=wrong&code=valid-code",
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(wrong.1["location"], "/#oauth=failed");
        assert_eq!(
            call(&app, "GET", "/api/v1/settings/neodb", Value::Null, &headers)
                .await
                .2["pending"],
            true
        );
        let path = format!(
            "/api/v1/oauth/neodb/callback?state={state}&{}",
            if denied {
                "error=access_denied"
            } else {
                "code=valid-code"
            }
        );
        let callback = call(&app, "GET", &path, Value::Null, &[]).await;
        assert_eq!(callback.0, 303);
        assert_eq!(
            callback.1["location"],
            if denied {
                "/#oauth=failed"
            } else {
                "/#oauth=complete"
            }
        );
        assert_eq!(
            call(&app, "GET", &path, Value::Null, &[]).await.1["location"],
            "/#oauth=failed"
        );
        let status = call(&app, "GET", "/api/v1/settings/neodb", Value::Null, &headers)
            .await
            .2;
        assert_eq!(status["account"], "alice");
        assert_eq!(status["pending"], false);
    }
    call(&app, "POST", "/api/v1/oauth/neodb", body, &headers).await;
    assert_eq!(
        call(&app, "DELETE", "/api/v1/oauth/neodb", Value::Null, &headers)
            .await
            .0,
        200
    );
    assert_eq!(secrets.0.lock().unwrap().len(), 1);
    storage.close().await;
}
