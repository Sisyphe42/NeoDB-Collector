use super::{NeoDbClient, canonical, read_bounded_json};
use crate::neodb::catalog::{DetailReply, normalize_detail, validate_expected_item};
use collector_core::{
    access::PortFuture,
    catalog::CatalogItem,
    marks::{MarkRead, MarkSnapshot, NeoDbMarkTransport},
    secrets::{ExposeSecret, SecretString},
};
use collector_domain::ApiError;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;

fn unavailable() -> ApiError {
    ApiError::new(
        "neodb_mark_unavailable",
        "NeoDB personal mark is unavailable or returned an invalid response.",
    )
}

fn invalid_expectation() -> ApiError {
    ApiError::new(
        "neodb_mark_expectation_invalid",
        "Expected a consistent same-instance supported catalog item.",
    )
}

fn invalid_account() -> ApiError {
    ApiError::new(
        "neodb_mark_account_invalid",
        "Expected a nonblank connected NeoDB account name.",
    )
}

fn invalid_token() -> ApiError {
    ApiError::new(
        "neodb_mark_token_invalid",
        "Expected a valid connected NeoDB bearer credential.",
    )
}

#[derive(Deserialize)]
struct OwnerReply {
    username: String,
}

#[derive(Deserialize)]
struct MarkReply {
    item: DetailReply,
    owner: OwnerReply,
    shelf_type: String,
    // Value makes this field required while still distinguishing explicit null.
    rating_grade: Value,
}

#[derive(Deserialize)]
struct ErrorReply {
    message: String,
}

fn rating(value: Value) -> Result<Option<u8>, ApiError> {
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => number
            .as_u64()
            .filter(|grade| (1..=10).contains(grade))
            .map(|grade| Some(grade as u8))
            .ok_or_else(unavailable),
        _ => Err(unavailable()),
    }
}

fn supported_shelf(value: &str) -> bool {
    matches!(value, "wishlist" | "progress" | "complete" | "dropped")
}

fn account_valid(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn same_expected_item(
    actual: &CatalogItem,
    expected: &CatalogItem,
    normalized_detail: &str,
) -> bool {
    actual.uuid == expected.uuid
        && actual.canonical_url == expected.canonical_url
        && actual.detail_url == normalized_detail
        && actual.media_type == expected.media_type
        && actual.model == expected.model
        && actual.parent_uuid == expected.parent_uuid
}

impl NeoDbMarkTransport for NeoDbClient {
    fn read_mark<'a>(
        &'a self,
        base: String,
        token: &'a SecretString,
        expected_account: String,
        expected_item: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead> {
        Box::pin(async move {
            let base = canonical(&base)?;
            let expected_detail =
                validate_expected_item(&base, expected_item).map_err(|_| invalid_expectation())?;
            if !account_valid(&expected_account) {
                return Err(invalid_account());
            }
            if token.expose_secret().is_empty() {
                return Err(invalid_token());
            }

            let url = base
                .join(&format!("/api/me/shelf/item/{}", expected_item.uuid))
                .map_err(|_| invalid_expectation())?;
            let request = self
                .http
                .get(url)
                .bearer_auth(token.expose_secret())
                .build()
                .map_err(|_| invalid_token())?;
            let response = self
                .http
                .execute(request)
                .await
                .map_err(|_| unavailable())?;
            match response.status() {
                StatusCode::OK => {
                    let reply: MarkReply = read_bounded_json(response, unavailable).await?;
                    let item = normalize_detail(reply.item, &base, &expected_detail)
                        .map_err(|_| unavailable())?;
                    if !same_expected_item(&item, expected_item, expected_detail.as_str()) {
                        return Err(ApiError::new(
                            "neodb_mark_identity_changed",
                            "NeoDB catalog identity changed; resolve the item again.",
                        ));
                    }
                    if reply.owner.username != expected_account {
                        return Err(ApiError::new(
                            "neodb_mark_account_changed",
                            "NeoDB mark owner no longer matches the connected account.",
                        ));
                    }
                    if !supported_shelf(&reply.shelf_type) {
                        return Err(unavailable());
                    }
                    Ok(MarkRead::Present(Box::new(MarkSnapshot {
                        item,
                        owner: reply.owner.username,
                        shelf_type: reply.shelf_type,
                        rating_grade: rating(reply.rating_grade)?,
                    })))
                }
                StatusCode::NOT_FOUND => {
                    let reply: ErrorReply = read_bounded_json(response, unavailable).await?;
                    match reply.message.as_str() {
                        "Mark not found" => Ok(MarkRead::MissingShelf),
                        "Item not found" => Err(ApiError::new(
                            "neodb_mark_item_unavailable",
                            "NeoDB catalog item is unavailable.",
                        )),
                        _ => Err(unavailable()),
                    }
                }
                StatusCode::FOUND => Err(ApiError::new(
                    "neodb_mark_identity_changed",
                    "NeoDB catalog identity changed; resolve the item again.",
                )),
                StatusCode::UNAUTHORIZED => Err(ApiError::new(
                    "neodb_mark_unauthorized",
                    "NeoDB rejected the connected credential.",
                )),
                StatusCode::FORBIDDEN => Err(ApiError::new(
                    "neodb_mark_forbidden",
                    "NeoDB denied access to the personal mark.",
                )),
                StatusCode::TOO_MANY_REQUESTS => Err(ApiError::new(
                    "neodb_mark_rate_limited",
                    "NeoDB personal-mark rate limit reached; try later.",
                )),
                _ => Err(unavailable()),
            }
        })
    }
}

#[cfg(test)]
mod tests;
