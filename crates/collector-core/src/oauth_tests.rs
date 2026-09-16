use crate::{
    access::{
        AccessRecord, AccessRepository, Clock, Endpoint, PortFuture, SecretStore, TokenSource,
    },
    oauth::{
        NeoDbOAuthTransport, OAUTH_STATE_TTL, OAuthApp, OAuthIdentity, OAuthService, OAuthToken,
    },
};
use collector_domain::{ApiError, access::PairedClient};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
    },
};

#[derive(Default)]
struct Repo {
    endpoint: Mutex<Option<Endpoint>>,
    fail_set: AtomicBool,
}
impl AccessRepository for Repo {
    fn insert(&self, _: AccessRecord) -> PortFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn exchange(&self, _: crate::access::ChallengeMatch, _: AccessRecord) -> PortFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn find(
        &self,
        _: String,
        _: String,
        _: String,
        _: i64,
    ) -> PortFuture<'_, Option<AccessRecord>> {
        Box::pin(async { Ok(None) })
    }
    fn revoke(&self, _: String, _: String) -> PortFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn clients(&self) -> PortFuture<'_, Vec<PairedClient>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn endpoint(&self) -> PortFuture<'_, Option<Endpoint>> {
        Box::pin(async { Ok(self.endpoint.lock().unwrap().clone()) })
    }
    fn set_endpoint(&self, endpoint: Option<Endpoint>) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if self.fail_set.swap(false, Ordering::SeqCst) {
                return Err(ApiError::new(
                    "database_unavailable",
                    "Database unavailable.",
                ));
            }
            *self.endpoint.lock().unwrap() = endpoint;
            Ok(())
        })
    }
}

#[derive(Default)]
struct Secrets {
    values: Mutex<HashMap<String, String>>,
    fail_set: AtomicBool,
    fail_delete_ref: Mutex<Option<String>>,
}
impl SecretStore for Secrets {
    fn set(&self, reference: String, secret: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            // Model an adapter that persists before reporting failure.
            self.values.lock().unwrap().insert(reference, secret);
            if self.fail_set.swap(false, Ordering::SeqCst) {
                return Err(ApiError::new(
                    "credential_store_unavailable",
                    "Credential store unavailable.",
                ));
            }
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
                .ok_or_else(|| ApiError::new("credential_store_unavailable", "Credential missing."))
        })
    }
    fn delete(&self, reference: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if self.fail_delete_ref.lock().unwrap().as_deref() == Some(&reference) {
                return Err(ApiError::new(
                    "credential_store_unavailable",
                    "Credential store unavailable.",
                ));
            }
            self.values.lock().unwrap().remove(&reference);
            Ok(())
        })
    }
}

#[derive(Default)]
struct Tokens(AtomicUsize);
impl TokenSource for Tokens {
    fn random(&self) -> Result<String, ApiError> {
        Ok(format!(
            "{:0>43}",
            self.0.fetch_add(1, Ordering::SeqCst) + 1
        ))
    }
    fn digest(&self, value: &str) -> String {
        format!("digest:{value}")
    }
}
struct TestClock(AtomicI64);
impl Clock for TestClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct Transport {
    state: Mutex<String>,
    fail: Mutex<Option<&'static str>>,
    scope: Mutex<String>,
    register_calls: AtomicUsize,
    block: Mutex<Option<&'static str>>,
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
}
impl Transport {
    async fn pause(&self, step: &str) {
        let blocked = self.block.lock().unwrap().as_deref() == Some(step);
        if blocked {
            self.entered.notify_one();
            self.release.notified().await;
        }
    }
    fn maybe(&self, step: &'static str) -> Result<(), ApiError> {
        if *self.fail.lock().unwrap() == Some(step) {
            Err(ApiError::new(
                "neodb_oauth_unavailable",
                "OAuth unavailable.",
            ))
        } else {
            Ok(())
        }
    }
}
impl NeoDbOAuthTransport for Transport {
    fn canonical_base(&self, input: String) -> PortFuture<'_, String> {
        Box::pin(async move {
            self.maybe("canonical")?;
            Ok(input.trim_end_matches('/').into())
        })
    }
    fn register(&self, _: String, _: String) -> PortFuture<'_, OAuthApp> {
        Box::pin(async move {
            self.maybe("register")?;
            self.register_calls.fetch_add(1, Ordering::SeqCst);
            self.pause("register").await;
            Ok(OAuthApp {
                client_id: "client".into(),
                client_secret: "app-secret".into(),
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
        Box::pin(async move {
            self.maybe("authorize")?;
            *self.state.lock().unwrap() = state.clone();
            Ok(format!("{base}/oauth/authorize?state={state}&scope=read"))
        })
    }
    fn exchange(&self, _: String, _: OAuthApp, _: String, _: String) -> PortFuture<'_, OAuthToken> {
        Box::pin(async move {
            self.maybe("exchange")?;
            self.pause("exchange").await;
            Ok(OAuthToken {
                access_token: "access-token".into(),
                scope: self.scope.lock().unwrap().clone(),
            })
        })
    }
    fn identity(&self, _: String, _: String) -> PortFuture<'_, OAuthIdentity> {
        Box::pin(async move {
            self.maybe("identity")?;
            Ok(OAuthIdentity {
                username: "alice".into(),
            })
        })
    }
}

struct Fixture {
    service: OAuthService,
    repo: Arc<Repo>,
    secrets: Arc<Secrets>,
    clock: Arc<TestClock>,
    transport: Arc<Transport>,
}
fn session(id: &str) -> AccessRecord {
    AccessRecord {
        id: id.into(),
        hash: String::new(),
        kind: "session".into(),
        audience: "http://127.0.0.1:19423".into(),
        name: String::new(),
        client_kind: String::new(),
        expires_at: i64::MAX,
    }
}
fn fixture() -> Fixture {
    let repo = Arc::new(Repo::default());
    let secrets = Arc::new(Secrets::default());
    let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
    let transport = Arc::new(Transport {
        scope: Mutex::new("read".into()),
        ..Transport::default()
    });
    let service = OAuthService::new(
        repo.clone(),
        secrets.clone(),
        Arc::new(Tokens::default()),
        clock.clone(),
        transport.clone(),
        "http://127.0.0.1:19423".into(),
    );
    Fixture {
        service,
        repo,
        secrets,
        clock,
        transport,
    }
}
async fn start(f: &Fixture, owner: &AccessRecord) -> String {
    let result = f
        .service
        .start(
            owner,
            "https://neodb.example/".into(),
            "http://127.0.0.1:19423/api/v1/oauth/neodb/callback".into(),
        )
        .await
        .unwrap();
    assert_eq!(result.expires_at, 1_000 + OAUTH_STATE_TTL);
    assert!(result.authorization_url.contains("scope=read"));
    f.transport.state.lock().unwrap().clone()
}
fn error_code<T>(result: Result<T, ApiError>) -> String {
    result.err().expect("expected error").code
}

const CALLBACK: &str = "http://127.0.0.1:19423/api/v1/oauth/neodb/callback";

#[tokio::test]
async fn failed_rollback_cleanup_is_explicit_and_retried_without_changing_old_account() {
    let f = fixture();
    *f.repo.endpoint.lock().unwrap() = Some(Endpoint {
        base_url: "https://old.example".into(),
        account: "old".into(),
        credential_ref: "old-token".into(),
    });
    f.secrets
        .values
        .lock()
        .unwrap()
        .insert("old-token".into(), "old-secret".into());
    let state = start(&f, &session("owner")).await;
    let new_ref = format!("neodb-token:{:0>43}", 2);
    *f.secrets.fail_delete_ref.lock().unwrap() = Some(new_ref.clone());
    f.repo.fail_set.store(true, Ordering::SeqCst);
    assert_eq!(
        error_code(f.service.complete(&state, "code".into()).await),
        "oauth_credential_cleanup_failed"
    );
    assert_eq!(
        f.service.status().await.unwrap().account.as_deref(),
        Some("old")
    );
    assert_eq!(
        f.repo
            .endpoint
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .credential_ref,
        "old-token"
    );
    *f.secrets.fail_delete_ref.lock().unwrap() = None;
    f.service.status().await.unwrap();
    assert!(!f.secrets.values.lock().unwrap().contains_key(&new_ref));
    assert_eq!(
        f.secrets
            .values
            .lock()
            .unwrap()
            .get("old-token")
            .map(String::as_str),
        Some("old-secret")
    );
}

#[tokio::test]
async fn every_transport_failure_releases_flow_and_removes_temporary_credentials() {
    for step in ["canonical", "register", "authorize", "exchange", "identity"] {
        let f = fixture();
        let owner = session("owner");
        if matches!(step, "exchange" | "identity") {
            let state = start(&f, &owner).await;
            *f.transport.fail.lock().unwrap() = Some(step);
            assert_eq!(
                error_code(f.service.complete(&state, "code".into()).await),
                "neodb_oauth_unavailable"
            );
        } else {
            *f.transport.fail.lock().unwrap() = Some(step);
            assert_eq!(
                error_code(
                    f.service
                        .start(&owner, "https://neodb.example".into(), CALLBACK.into())
                        .await
                ),
                "neodb_oauth_unavailable"
            );
        }
        assert!(f.secrets.values.lock().unwrap().is_empty(), "{step}");
        assert!(!f.service.status().await.unwrap().pending);
        *f.transport.fail.lock().unwrap() = None;
        start(&f, &owner).await;
    }
}

#[tokio::test]
async fn callback_expiry_scope_and_credential_failures_never_connect() {
    for failure in ["expired", "scope", "get", "set", "delete"] {
        let f = fixture();
        let state = start(&f, &session("owner")).await;
        let app_ref = f
            .secrets
            .values
            .lock()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        let expected = match failure {
            "expired" => {
                f.clock.0.store(1600, Ordering::SeqCst);
                "oauth_state_expired"
            }
            "scope" => {
                *f.transport.scope.lock().unwrap() = "read write".into();
                "oauth_scope_invalid"
            }
            "get" => {
                f.secrets.values.lock().unwrap().clear();
                "credential_store_unavailable"
            }
            "set" => {
                f.secrets.fail_set.store(true, Ordering::SeqCst);
                "credential_store_unavailable"
            }
            _ => {
                *f.secrets.fail_delete_ref.lock().unwrap() = Some(app_ref);
                "oauth_credential_cleanup_failed"
            }
        };
        assert_eq!(
            error_code(f.service.complete(&state, "code".into()).await),
            expected
        );
        assert!(f.repo.endpoint.lock().unwrap().is_none());
        assert!(
            !f.secrets
                .values
                .lock()
                .unwrap()
                .values()
                .any(|v| v == "access-token")
        );
        *f.secrets.fail_delete_ref.lock().unwrap() = None;
        assert!(!f.service.status().await.unwrap().pending);
        assert!(f.secrets.values.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn starting_and_consumed_reservations_exclude_concurrent_requests() {
    for step in ["register", "exchange"] {
        let f = fixture();
        let owner = session("owner");
        let state = if step == "exchange" {
            start(&f, &owner).await
        } else {
            String::new()
        };
        *f.transport.block.lock().unwrap() = Some(step);
        let mut operation = Box::pin(async {
            if step == "register" {
                f.service
                    .start(&owner, "https://neodb.example".into(), CALLBACK.into())
                    .await
                    .map(|_| ())
            } else {
                f.service.complete(&state, "code".into()).await.map(|_| ())
            }
        });
        tokio::select! {
            _ = f.transport.entered.notified() => {},
            _ = &mut operation => panic!("Operation should be blocked"),
        }
        assert!(f.service.status().await.unwrap().pending);
        assert_eq!(
            error_code(
                f.service
                    .start(&owner, "https://other.example".into(), CALLBACK.into())
                    .await
            ),
            "oauth_in_progress"
        );
        assert_eq!(
            error_code(f.service.complete(&state, "replay".into()).await),
            "oauth_in_progress"
        );
        f.transport.release.notify_one();
        operation.await.unwrap();
        assert_eq!(f.transport.register_calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn interrupted_futures_and_restart_orphans_are_recoverable() {
    let f = fixture();
    let owner = session("owner");
    let state = start(&f, &owner).await;
    *f.transport.block.lock().unwrap() = Some("exchange");
    let mut operation = Box::pin(f.service.complete(&state, "code".into()));
    tokio::select! {
        _ = f.transport.entered.notified() => {},
        _ = &mut operation => panic!("Operation should be blocked"),
    }
    drop(operation);
    assert!(!f.service.status().await.unwrap().pending);
    assert!(f.secrets.values.lock().unwrap().is_empty());
    *f.transport.block.lock().unwrap() = None;
    start(&f, &owner).await;
    let restarted = OAuthService::new(
        f.repo.clone(),
        f.secrets.clone(),
        Arc::new(Tokens::default()),
        f.clock.clone(),
        f.transport.clone(),
        owner.audience,
    );
    assert!(!restarted.status().await.unwrap().pending);
    assert!(f.secrets.values.lock().unwrap().is_empty());
}

#[tokio::test]
async fn session_and_callback_are_bound_to_this_core() {
    let f = fixture();
    for kind in ["client", "session"] {
        let mut owner = session("owner");
        owner.kind = kind.into();
        if kind == "session" {
            owner.audience = "http://127.0.0.1:9999".into();
        }
        assert_eq!(
            error_code(
                f.service
                    .start(&owner, "https://neodb.example".into(), CALLBACK.into())
                    .await
            ),
            "session_required"
        );
    }
    assert_eq!(
        error_code(
            f.service
                .start(
                    &session("owner"),
                    "https://neodb.example".into(),
                    "https://foreign.example/callback".into()
                )
                .await
        ),
        "oauth_callback_invalid"
    );
    assert_eq!(f.transport.register_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn success_consumes_state_replaces_endpoint_and_cleans_secrets() {
    let f = fixture();
    *f.repo.endpoint.lock().unwrap() = Some(Endpoint {
        base_url: "https://old.example".into(),
        account: "old".into(),
        credential_ref: "old-token".into(),
    });
    f.secrets
        .values
        .lock()
        .unwrap()
        .insert("old-token".into(), "old-secret".into());
    let state = start(&f, &session("owner")).await;
    assert!(
        !f.secrets
            .values
            .lock()
            .unwrap()
            .values()
            .any(|value| value == &state)
    );
    let completed = f.service.complete(&state, "code".into()).await.unwrap();
    assert_eq!(completed.account, "alice");
    assert_eq!(completed.base_url, "https://neodb.example");
    assert_eq!(
        error_code(f.service.complete(&state, "code".into()).await),
        "oauth_not_pending"
    );
    let saved = f.repo.endpoint.lock().unwrap().clone().unwrap();
    assert_eq!(saved.account, "alice");
    assert_eq!(
        f.secrets
            .values
            .lock()
            .unwrap()
            .get(&saved.credential_ref)
            .map(String::as_str),
        Some("access-token")
    );
    assert!(!f.secrets.values.lock().unwrap().contains_key("old-token"));
    let status = f.service.status().await.unwrap();
    assert!(status.connected && !status.pending);
    assert_eq!(status.account.as_deref(), Some("alice"));
}

#[tokio::test]
async fn invalid_state_does_not_consume_but_matching_state_is_single_use_on_failure() {
    let f = fixture();
    let state = start(&f, &session("owner")).await;
    assert_eq!(
        error_code(f.service.complete("wrong", "code".into()).await),
        "oauth_state_invalid"
    );
    *f.transport.fail.lock().unwrap() = Some("exchange");
    assert_eq!(
        error_code(f.service.complete(&state, "code".into()).await),
        "neodb_oauth_unavailable"
    );
    assert_eq!(
        error_code(f.service.complete(&state, "code".into()).await),
        "oauth_not_pending"
    );
    assert!(f.secrets.values.lock().unwrap().is_empty());
}

#[tokio::test]
async fn expiry_cancel_owner_binding_and_concurrent_start_are_enforced() {
    let f = fixture();
    let owner = session("owner");
    start(&f, &owner).await;
    assert_eq!(
        error_code(f.service.cancel(&session("other")).await),
        "oauth_cancel_forbidden"
    );
    f.service.cancel(&owner).await.unwrap();
    f.service.cancel(&owner).await.unwrap();
    assert!(f.secrets.values.lock().unwrap().is_empty());

    start(&f, &owner).await;
    f.clock.0.store(1_000 + OAUTH_STATE_TTL, Ordering::SeqCst);
    let status = f.service.status().await.unwrap();
    assert!(!status.pending);
    assert!(f.secrets.values.lock().unwrap().is_empty());

    f.clock.0.store(2_000, Ordering::SeqCst);
    let (one, two) = tokio::join!(
        f.service.start(
            &owner,
            "https://one.example".into(),
            "http://127.0.0.1:19423/api/v1/oauth/neodb/callback".into()
        ),
        f.service.start(
            &owner,
            "https://two.example".into(),
            "http://127.0.0.1:19423/api/v1/oauth/neodb/callback".into()
        )
    );
    assert_ne!(one.is_ok(), two.is_ok());
    assert_eq!(f.transport.register_calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn failures_clean_new_material_and_preserve_or_truthfully_report_connection() {
    let f = fixture();
    f.secrets.fail_set.store(true, Ordering::SeqCst);
    assert_eq!(
        error_code(
            f.service
                .start(
                    &session("owner"),
                    "https://neodb.example".into(),
                    "http://127.0.0.1:19423/api/v1/oauth/neodb/callback".into()
                )
                .await
        ),
        "credential_store_unavailable"
    );
    assert!(!f.service.status().await.unwrap().pending);

    let old = Endpoint {
        base_url: "https://old.example".into(),
        account: "old".into(),
        credential_ref: "old-token".into(),
    };
    *f.repo.endpoint.lock().unwrap() = Some(old.clone());
    f.secrets
        .values
        .lock()
        .unwrap()
        .insert("old-token".into(), "old-secret".into());
    let state = start(&f, &session("owner")).await;
    f.repo.fail_set.store(true, Ordering::SeqCst);
    assert_eq!(
        error_code(f.service.complete(&state, "code".into()).await),
        "database_unavailable"
    );
    assert_eq!(
        f.repo.endpoint.lock().unwrap().as_ref().unwrap().account,
        "old"
    );
    assert_eq!(f.secrets.values.lock().unwrap().len(), 1);

    let state = start(&f, &session("owner")).await;
    *f.secrets.fail_delete_ref.lock().unwrap() = Some("old-token".into());
    assert_eq!(
        error_code(f.service.complete(&state, "code".into()).await),
        "oauth_credential_cleanup_failed"
    );
    assert_eq!(
        f.repo.endpoint.lock().unwrap().as_ref().unwrap().account,
        "alice"
    );
    assert!(f.service.status().await.unwrap().connected);
}
