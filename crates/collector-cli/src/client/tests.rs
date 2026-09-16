use super::*;
use collector_core::access::{AccessRepository, AccessService, PortFuture};
use collector_domain::access::PairingRequest;
use collector_storage::{Storage, credentials::SystemClock};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
struct Secrets {
    values: Mutex<HashMap<String, String>>,
    fail: AtomicBool,
    fail_token: AtomicBool,
}
impl SecretStore for Secrets {
    fn set(&self, reference: String, secret: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if self.fail.load(Ordering::SeqCst)
                || (reference.starts_with("collector-cli:")
                    && self.fail_token.load(Ordering::SeqCst))
            {
                return Err(error(
                    "credential_store_unavailable",
                    "Credential store unavailable.",
                ));
            }
            self.values.lock().unwrap().insert(reference, secret);
            Ok(())
        })
    }
    fn get(&self, reference: String) -> PortFuture<'_, String> {
        Box::pin(async move {
            self.values
                .lock()
                .unwrap()
                .get(&reference)
                .cloned()
                .ok_or_else(|| error("credential_store_unavailable", "Credential missing."))
        })
    }
    fn delete(&self, reference: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            self.values.lock().unwrap().remove(&reference);
            Ok(())
        })
    }
}

#[test]
fn rejects_unsafe_origins_links_and_oversized_stdin() {
    for input in [
        "https://example.com",
        "http://localhost:19423",
        "http://192.168.1.1",
        "http://user@127.0.0.1",
        "http://127.0.0.1/path",
        "http://127.0.0.1?key=x",
        "http://127.0.0.1#key=x",
        "http://127.0.0.1:0",
    ] {
        assert!(origin(input).is_err(), "{input}");
    }
    assert!(origin("http://[::1]:19423").is_ok());
    let key = "a".repeat(43);
    assert!(pairing(&format!("http://127.0.0.1:19423/pair#key={key}")).is_ok());
    for input in [
        format!("http://127.0.0.1/pair?key={key}"),
        format!("http://127.0.0.1/pair#key={key}&key={key}"),
        format!("http://127.0.0.1/other#key={key}"),
    ] {
        assert!(pairing(&input).is_err());
    }
    assert!(read_bounded("x".repeat(4097).as_bytes()).is_err());
}

#[tokio::test]
async fn pairs_through_real_http_stores_only_in_fake_keychain_and_detects_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::open(dir.path()).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}");
    let access = Arc::new(AccessService {
        repository: Arc::new(storage.clone()),
        tokens: Arc::new(SecureTokens),
        clock: Arc::new(SystemClock),
        audience: base.clone(),
    });
    let app = collector_server::protected_router(
        Arc::new(storage.clone()),
        dir.path().into(),
        address,
        Some(collector_server::access::AccessState::new(access.clone())),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let link = access
        .create_pairing(PairingRequest {
            client_name: "Collector CLI".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let secrets = Secrets::default();
    secrets.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        pair(&link.uri, "Collector CLI", &secrets)
            .await
            .err()
            .unwrap()
            .code,
        "credential_store_unavailable"
    );
    secrets.fail.store(false, Ordering::SeqCst);
    assert_eq!(
        pair(&link.uri, "Wrong name", &secrets)
            .await
            .err()
            .unwrap()
            .code,
        "authentication_required"
    );
    let paired = pair(&link.uri, "Collector CLI", &secrets).await.unwrap();
    let saved_token = secrets
        .values
        .lock()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    assert_eq!(secrets.values.lock().unwrap().len(), 1);
    assert!(
        !serde_json::to_string(&paired)
            .unwrap()
            .contains(&saved_token)
    );
    assert!(!paired.to_string().contains(&saved_token));
    let report = status(&base, &secrets).await.unwrap();
    assert!(report.authenticated);
    assert_eq!(report.client.id, paired.client_id);
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains(&saved_token)
    );
    assert_eq!(
        pair(&link.uri, "Collector CLI", &secrets)
            .await
            .err()
            .unwrap()
            .code,
        "authentication_required"
    );
    assert_eq!(
        secrets.values.lock().unwrap().values().next().unwrap(),
        &saved_token
    );
    let next = access
        .create_pairing(PairingRequest {
            client_name: "Collector CLI".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    secrets.fail_token.store(true, Ordering::SeqCst);
    assert_eq!(
        pair(&next.uri, "Collector CLI", &secrets)
            .await
            .err()
            .unwrap()
            .code,
        "pairing_storage_failed"
    );
    assert_eq!(
        secrets.values.lock().unwrap().values().next().unwrap(),
        &saved_token
    );
    secrets.fail_token.store(false, Ordering::SeqCst);
    storage
        .revoke(paired.client_id, "client".into())
        .await
        .unwrap();
    assert_eq!(
        status(&base, &secrets).await.err().unwrap().code,
        "authentication_required"
    );
    server.abort();
    let _ = server.await;
    storage.close().await;
}

#[tokio::test]
async fn refuses_redirects_incompatible_versions_and_oversized_responses() {
    use axum::{Json, Router, response::Redirect, routing::get};
    for mode in ["redirect", "version", "oversized"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = match mode {
            "redirect" => Router::new().route("/api/v1/health", get(|| async { Redirect::temporary("http://127.0.0.1:1/private") })),
            "version" => Router::new().route("/api/v1/health", get(|| async { Json(serde_json::json!({"status":"ok","storage":"ready","coreVersion":"0.1.0","apiVersion":"v2","schemaVersion":1})) })),
            _ => Router::new().route("/api/v1/health", get(|| async { "x".repeat(65537) })),
        };
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let secrets = Secrets::default();
        let failure = status(&base, &secrets).await.err().unwrap();
        assert_eq!(
            failure.code,
            if mode == "version" {
                "core_incompatible"
            } else {
                "core_unavailable"
            }
        );
        assert!(secrets.values.lock().unwrap().is_empty());
        server.abort();
        let _ = server.await;
    }
}
