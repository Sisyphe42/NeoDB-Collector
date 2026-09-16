//! Public access contracts. Secret-bearing DTOs deliberately do not implement Debug.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapExchange {
    pub key: String,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub csrf_token: String,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairingRequest {
    pub client_name: String,
    pub client_kind: String,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairingLink {
    pub uri: String,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PairingExchange {
    pub one_time_key: String,
    pub client_name: String,
    pub client_kind: String,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct PairingToken {
    pub client_id: String,
    pub token: String,
    pub token_type: String,
    pub scopes: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairedClient {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub revoked: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientList {
    pub clients: Vec<PairedClient>,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct ActionResult {
    pub ok: bool,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NeoDbStartRequest {
    pub base_url: String,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NeoDbAuthorization {
    pub authorization_url: String,
    pub expires_at: i64,
}

#[derive(Serialize, Deserialize, JsonSchema, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NeoDbStatus {
    pub connected: bool,
    pub base_url: Option<String>,
    pub account: Option<String>,
    pub pending: bool,
}
