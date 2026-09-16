//! OAuth use-case ports. Network and URL handling live in connector adapters.
use crate::access::PortFuture;

#[derive(Clone)]
pub struct OAuthApp {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone)]
pub struct OAuthToken {
    pub access_token: String,
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
        code: String,
        callback: String,
    ) -> PortFuture<'_, OAuthToken>;
    fn identity(&self, base: String, token: String) -> PortFuture<'_, OAuthIdentity>;
}
