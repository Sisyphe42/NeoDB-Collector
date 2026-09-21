//! OAuth use-case ports. Network and URL handling live in connector adapters.
use crate::access::{
    AccessRecord, AccessRepository, Clock, Endpoint, PortFuture, SecretStore, TokenSource,
};
use crate::secrets::{ExposeSecret, SecretString};
use collector_domain::ApiError;
use std::sync::{Arc, Mutex, MutexGuard};

pub const OAUTH_STATE_TTL: i64 = 600;

#[derive(Clone, Debug)]
pub struct OAuthApp {
    pub client_id: String,
    pub client_secret: SecretString,
}

#[derive(Clone, Debug)]
pub struct OAuthToken {
    pub access_token: SecretString,
    pub scope: String,
}

#[derive(Clone)]
pub struct OAuthIdentity {
    pub username: String,
}

pub trait NeoDbOAuthTransport: Send + Sync {
    fn canonical_base(&self, input: String) -> PortFuture<'_, String>;
    fn register(&self, base: String, callback: String) -> PortFuture<'_, OAuthApp>;
    fn authorization_url(
        &self,
        base: String,
        client_id: String,
        callback: String,
        state: String,
    ) -> PortFuture<'_, String>;
    fn exchange(
        &self,
        base: String,
        app: OAuthApp,
        code: SecretString,
        callback: String,
    ) -> PortFuture<'_, OAuthToken>;
    fn identity<'a>(
        &'a self,
        base: String,
        token: &'a SecretString,
    ) -> PortFuture<'a, OAuthIdentity>;
}

pub struct OAuthStartResult {
    pub authorization_url: String,
    pub expires_at: i64,
}

pub struct OAuthCompletion {
    pub base_url: String,
    pub account: String,
}

pub struct OAuthStatus {
    pub connected: bool,
    pub base_url: Option<String>,
    pub account: Option<String>,
    pub pending: bool,
}

#[derive(Clone)]
struct Pending {
    owner_session_id: String,
    state_hash: String,
    base_url: String,
    callback: String,
    client_id: String,
    app_secret_ref: String,
    expires_at: i64,
}

enum FlowState {
    Idle,
    Starting { owner_session_id: String },
    Pending(Pending),
    Consumed,
}

struct Reservation<'a> {
    service: &'a OAuthService,
    armed: bool,
}
impl<'a> Reservation<'a> {
    fn new(service: &'a OAuthService) -> Self {
        Self {
            service,
            armed: true,
        }
    }
}
impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if self.armed
            && let Ok(mut state) = self.service.state.lock()
        {
            *state = FlowState::Idle;
        }
    }
}

pub struct OAuthService {
    repository: Arc<dyn AccessRepository>,
    secrets: Arc<dyn SecretStore>,
    tokens: Arc<dyn TokenSource>,
    clock: Arc<dyn Clock>,
    transport: Arc<dyn NeoDbOAuthTransport>,
    audience: String,
    state: Mutex<FlowState>,
    cleanup: Mutex<Vec<String>>,
}

impl OAuthService {
    pub fn new(
        repository: Arc<dyn AccessRepository>,
        secrets: Arc<dyn SecretStore>,
        tokens: Arc<dyn TokenSource>,
        clock: Arc<dyn Clock>,
        transport: Arc<dyn NeoDbOAuthTransport>,
        audience: String,
    ) -> Self {
        Self {
            repository,
            secrets,
            tokens,
            clock,
            transport,
            audience,
            state: Mutex::new(FlowState::Idle),
            cleanup: Mutex::new(Vec::new()),
        }
    }

    fn state(&self) -> Result<MutexGuard<'_, FlowState>, ApiError> {
        self.state.lock().map_err(|_| {
            ApiError::new(
                "oauth_state_unavailable",
                "Authorization state is unavailable; restart Core and retry.",
            )
        })
    }

    fn app_secret_ref(&self) -> String {
        format!("neodb-oauth-app:{}", self.tokens.digest(&self.audience))
    }

    fn track(&self, reference: String) {
        let mut refs = self.cleanup.lock().unwrap_or_else(|e| e.into_inner());
        if !refs.contains(&reference) {
            refs.push(reference);
        }
    }

    async fn reconcile(&self) -> Result<(), ApiError> {
        let current = self.repository.endpoint().await?;
        let refs = self
            .cleanup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        for reference in refs {
            if !current
                .as_ref()
                .is_some_and(|e| e.credential_ref == reference)
            {
                self.secrets.delete(reference.clone()).await.map_err(|_| {
                    ApiError::new(
                        "oauth_credential_cleanup_failed",
                        "Credential cleanup failed; query status for the current connection.",
                    )
                })?;
            }
            self.cleanup
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|r| r != &reference);
        }
        Ok(())
    }

    fn reserve(&self, owner: &str) -> Result<(), ApiError> {
        let mut state = self.state()?;
        if !matches!(*state, FlowState::Idle) {
            return Err(ApiError::new(
                "oauth_in_progress",
                "Another NeoDB authorization is already in progress.",
            ));
        }
        *state = FlowState::Starting {
            owner_session_id: owner.to_owned(),
        };
        Ok(())
    }

    /// The caller must supply a freshly authenticated AccessService session.
    /// Compose one shared service per Core audience, not one per request.
    pub async fn start(
        &self,
        session: &AccessRecord,
        requested_base: String,
        callback: String,
    ) -> Result<OAuthStartResult, ApiError> {
        if session.kind != "session"
            || session.audience != self.audience
            || session.expires_at <= self.clock.now()
            || session.id.is_empty()
        {
            return Err(ApiError::new(
                "session_required",
                "An authenticated local session is required.",
            ));
        }
        if callback != format!("{}/api/v1/oauth/neodb/callback", self.audience) {
            return Err(ApiError::new(
                "oauth_callback_invalid",
                "Callback must belong to this Core instance.",
            ));
        }
        self.reserve(&session.id)?;
        let mut reservation = Reservation::new(self);
        let app_ref = self.app_secret_ref();
        self.track(app_ref.clone());
        let result = async {
            // Fixed per-Core reference makes a crash-orphan recoverable on the next start.
            self.reconcile().await?;
            let base = self.transport.canonical_base(requested_base).await?;
            let app = self
                .transport
                .register(base.clone(), callback.clone())
                .await?;
            self.track(app_ref.clone());
            self.secrets.set(app_ref.clone(), app.client_secret).await?;
            let raw_state = self.tokens.random()?;
            let expires_at = self.clock.now().saturating_add(OAUTH_STATE_TTL);
            let authorization_url = self
                .transport
                .authorization_url(
                    base.clone(),
                    app.client_id.clone(),
                    callback.clone(),
                    raw_state.clone(),
                )
                .await?;
            Ok::<_, ApiError>((
                OAuthStartResult {
                    authorization_url,
                    expires_at,
                },
                Pending {
                    owner_session_id: session.id.clone(),
                    state_hash: self.tokens.digest(&raw_state),
                    base_url: base,
                    callback,
                    client_id: app.client_id,
                    app_secret_ref: app_ref.clone(),
                    expires_at,
                },
            ))
        }
        .await;

        match result {
            Ok((view, pending)) => {
                let mut state = self.state()?;
                *state = FlowState::Pending(pending);
                reservation.armed = false;
                Ok(view)
            }
            Err(error) => {
                let cleanup = self.reconcile().await;
                cleanup?;
                Err(error)
            }
        }
    }

    fn take_pending(&self, raw_state: &str) -> Result<(Pending, bool), ApiError> {
        let (pending, expired) = {
            let mut state = self.state()?;
            let result = match &*state {
                FlowState::Idle => {
                    return Err(ApiError::new(
                        "oauth_not_pending",
                        "No NeoDB authorization is pending.",
                    ));
                }
                FlowState::Starting { .. } | FlowState::Consumed => {
                    return Err(ApiError::new(
                        "oauth_in_progress",
                        "NeoDB authorization is still starting.",
                    ));
                }
                FlowState::Pending(pending) => {
                    let expired = self.clock.now() >= pending.expires_at;
                    if !expired && self.tokens.digest(raw_state) != pending.state_hash {
                        return Err(ApiError::new(
                            "oauth_state_invalid",
                            "OAuth state is invalid.",
                        ));
                    }
                    (pending.clone(), expired)
                }
            };
            *state = FlowState::Consumed;
            result
        };
        Ok((pending, expired))
    }

    pub async fn complete(
        &self,
        raw_state: &str,
        code: SecretString,
    ) -> Result<OAuthCompletion, ApiError> {
        let (pending, expired) = self.take_pending(raw_state)?;
        let _reservation = Reservation::new(self);
        let result = async {
            if expired {
                return Err(ApiError::new(
                    "oauth_state_expired",
                    "OAuth state expired; start again.",
                ));
            }
            if code.expose_secret().is_empty() {
                return Err(ApiError::new(
                    "oauth_denied",
                    "Authorization was denied or incomplete.",
                ));
            }
            let client_secret = self.secrets.get(pending.app_secret_ref.clone()).await;
            let exchanged = match client_secret {
                Ok(client_secret) => {
                    self.transport
                        .exchange(
                            pending.base_url.clone(),
                            OAuthApp {
                                client_id: pending.client_id,
                                client_secret,
                            },
                            code,
                            pending.callback,
                        )
                        .await
                }
                Err(error) => Err(error),
            };
            self.secrets
                .delete(pending.app_secret_ref)
                .await
                .map_err(|_| {
                    ApiError::new(
                        "oauth_credential_cleanup_failed",
                        "Temporary credential cleanup failed.",
                    )
                })?;
            let token = exchanged?;
            if token.scope.split_whitespace().next().is_none()
                || token.scope.split_whitespace().any(|s| s != "read")
                || token.access_token.expose_secret().is_empty()
            {
                return Err(ApiError::new(
                    "oauth_scope_invalid",
                    "NeoDB returned an unsupported authorization scope.",
                ));
            }
            let identity = self
                .transport
                .identity(pending.base_url.clone(), &token.access_token)
                .await?;
            let token_ref = format!("neodb-token:{}", self.tokens.random()?);
            let previous = self.repository.endpoint().await?;
            if previous
                .as_ref()
                .is_some_and(|e| e.credential_ref == token_ref)
            {
                return Err(ApiError::new(
                    "oauth_credential_collision",
                    "Credential reference collision.",
                ));
            }
            self.track(token_ref.clone());
            self.secrets
                .set(token_ref.clone(), token.access_token)
                .await?;
            let endpoint = Endpoint {
                base_url: pending.base_url.clone(),
                account: identity.username.clone(),
                credential_ref: token_ref.clone(),
            };
            if let Some(previous) = previous {
                self.track(previous.credential_ref);
            }
            self.repository.set_endpoint(Some(endpoint)).await?;
            Ok(OAuthCompletion {
                base_url: pending.base_url,
                account: identity.username,
            })
        }
        .await;
        self.reconcile().await?;
        result
    }

    pub async fn cancel(&self, session: &AccessRecord) -> Result<(), ApiError> {
        if session.kind != "session"
            || session.audience != self.audience
            || session.expires_at <= self.clock.now()
        {
            return Err(ApiError::new(
                "session_required",
                "An authenticated local session is required.",
            ));
        }
        let pending = {
            let mut state = self.state()?;
            let pending = match &*state {
                FlowState::Idle => return Ok(()),
                FlowState::Consumed => {
                    return Err(ApiError::new(
                        "oauth_in_progress",
                        "Authorization is completing.",
                    ));
                }
                FlowState::Starting { owner_session_id } if owner_session_id == &session.id => {
                    return Err(ApiError::new(
                        "oauth_in_progress",
                        "Authorization is still starting; retry cancellation shortly.",
                    ));
                }
                FlowState::Starting { .. } => {
                    return Err(ApiError::new(
                        "oauth_cancel_forbidden",
                        "Only the initiating session can cancel authorization.",
                    ));
                }
                FlowState::Pending(Pending {
                    owner_session_id, ..
                }) if owner_session_id != &session.id => {
                    return Err(ApiError::new(
                        "oauth_cancel_forbidden",
                        "Only the initiating session can cancel authorization.",
                    ));
                }
                FlowState::Pending(pending) => pending.clone(),
            };
            *state = FlowState::Consumed;
            pending
        };
        let _reservation = Reservation::new(self);
        self.track(pending.app_secret_ref);
        self.reconcile().await
    }

    pub async fn status(&self) -> Result<OAuthStatus, ApiError> {
        let (expired, clean) = {
            let mut state = self.state()?;
            match &*state {
                FlowState::Pending(pending) if self.clock.now() >= pending.expires_at => {
                    let expired = pending.clone();
                    *state = FlowState::Consumed;
                    (Some(expired), true)
                }
                FlowState::Idle => {
                    *state = FlowState::Consumed;
                    (None, true)
                }
                _ => (None, false),
            }
        };
        let _reservation = clean.then(|| Reservation::new(self));
        if clean {
            if let Some(expired) = expired {
                self.track(expired.app_secret_ref);
            }
            self.secrets.delete(self.app_secret_ref()).await?;
            // Cleanup failures must not hide a successfully replaced connection.
            let _ = self.reconcile().await;
        }
        let endpoint = self.repository.endpoint().await?;
        let pending = !clean && !matches!(*self.state()?, FlowState::Idle);
        Ok(OAuthStatus {
            connected: endpoint.is_some(),
            base_url: endpoint.as_ref().map(|value| value.base_url.clone()),
            account: endpoint.map(|value| value.account),
            pending,
        })
    }
}
