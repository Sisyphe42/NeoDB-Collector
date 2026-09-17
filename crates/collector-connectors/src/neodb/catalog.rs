use super::{NeoDbClient, RESPONSE_LIMIT, canonical};
use crate::imdb::valid_title_id;
use collector_core::{
    access::PortFuture,
    catalog::{CatalogFetch, CatalogItem, NeoDbCatalogTransport},
};
use collector_domain::ApiError;
use reqwest::{Response, StatusCode};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use url::Url;

const MAX_REDIRECTS: usize = 3;
const DETAIL_BUDGET: Duration = Duration::from_secs(10);

fn unavailable() -> ApiError {
    ApiError::new(
        "neodb_catalog_unavailable",
        "NeoDB catalog is unavailable or returned invalid evidence.",
    )
}
fn unsafe_reference() -> ApiError {
    ApiError::new(
        "neodb_catalog_reference_invalid",
        "Expected a same-instance supported catalog detail reference.",
    )
}

// Deliberately exact, not URL-normalized: do not launder paths or userinfo into
// a supported URL. Other source rules belong beside their built-in adapters.
fn imdb_url(value: &str) -> Option<&str> {
    let id = value
        .strip_prefix("https://www.imdb.com/title/")?
        .strip_suffix('/')?;
    valid_title_id(id).then_some(id)
}

fn item_path(path: &str) -> Option<(&str, &str, &str)> {
    let (prefix, uuid) = path.rsplit_once('/')?;
    if uuid.is_empty() || uuid.len() > 22 || !uuid.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let (media, model) = match prefix {
        "/api/movie" => ("movie", "Movie"),
        "/api/tv" => ("tv", "TVShow"),
        "/api/tv/season" => ("tv", "TVSeason"),
        "/api/tv/episode" => ("tv", "TVEpisode"),
        _ => return None,
    };
    Some((media, model, uuid))
}

fn detail_url(base: &Url, value: &str) -> Result<Url, ApiError> {
    // Reject before parsing: URL parsers normalize dot segments and backslashes.
    if value.is_empty()
        || value.len() > 2048
        || value.bytes().any(|b| b.is_ascii_control() || b == b' ')
        || value.contains(['%', '\\', '?', '#'])
        || value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
    {
        return Err(unsafe_reference());
    }
    let url = if value.starts_with('/') && !value.starts_with("//") {
        base.join(value)
    } else {
        Url::parse(value)
    }
    .map_err(|_| unsafe_reference())?;
    if url.origin() != base.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || item_path(url.path()).is_none()
    {
        return Err(unsafe_reference());
    }
    Ok(url)
}

fn location(base: &Url, response: &Response) -> Result<Url, ApiError> {
    let mut values = response.headers().get_all(reqwest::header::LOCATION).iter();
    let value = values
        .next()
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unsafe_reference)?;
    if values.next().is_some() {
        return Err(unsafe_reference());
    }
    detail_url(base, value)
}

#[derive(Deserialize)]
struct DetailReply {
    id: String,
    uuid: String,
    api_url: String,
    category: String,
    #[serde(rename = "type")]
    model: String,
    parent_uuid: Option<String>,
    imdb: Option<String>,
    external_resources: Option<Vec<Resource>>,
}
#[derive(Deserialize)]
struct Resource {
    url: String,
}

async fn read_detail(
    mut response: Response,
    base: &Url,
    requested: &Url,
) -> Result<CatalogItem, ApiError> {
    if response
        .content_length()
        .is_some_and(|n| n > RESPONSE_LIMIT as u64)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_LIMIT {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    let reply: DetailReply = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    let (media, model, uuid) = item_path(requested.path()).ok_or_else(unavailable)?;
    let canonical_url = base
        .join(
            requested
                .path()
                .strip_prefix("/api")
                .ok_or_else(unavailable)?,
        )
        .map_err(|_| unavailable())?;
    if reply.uuid != uuid
        || reply.category != media
        || reply.model != model
        || reply.id != canonical_url.as_str()
        || detail_url(base, &reply.api_url)? != *requested
        || reply.parent_uuid.as_ref().is_some_and(|id| {
            id.is_empty() || id.len() > 22 || !id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
    {
        return Err(unavailable());
    }
    let mut ids = BTreeSet::new();
    if let Some(id) = reply.imdb {
        if !valid_title_id(&id) {
            return Err(unavailable());
        }
        ids.insert(("imdb:title".to_owned(), id));
    }
    for resource in reply.external_resources.unwrap_or_default() {
        if let Some(id) = imdb_url(&resource.url) {
            ids.insert(("imdb:title".to_owned(), id.to_owned()));
        }
    }
    Ok(CatalogItem {
        uuid: reply.uuid,
        canonical_url: canonical_url.into(),
        detail_url: requested.to_string(),
        media_type: reply.category,
        model: reply.model,
        parent_uuid: reply.parent_uuid,
        external_ids: ids.into_iter().collect(),
    })
}

impl NeoDbCatalogTransport for NeoDbClient {
    fn fetch(&self, base: String, source_url: String) -> PortFuture<'_, CatalogFetch> {
        Box::pin(async move {
            if imdb_url(&source_url).is_none() {
                return Err(ApiError::new(
                    "catalog_source_url_invalid",
                    "Expected a supported public canonical item URL.",
                ));
            }
            let base = canonical(&base)?;
            let mut url = base.join("/api/catalog/fetch").map_err(|_| unavailable())?;
            url.query_pairs_mut().append_pair("url", &source_url);
            let response = self.http.get(url).send().await.map_err(|_| unavailable())?;
            match response.status() {
                StatusCode::ACCEPTED => Ok(CatalogFetch::Pending),
                StatusCode::FOUND => Ok(CatalogFetch::Detail(location(&base, &response)?.into())),
                StatusCode::UNPROCESSABLE_ENTITY => Ok(CatalogFetch::Unsupported),
                StatusCode::TOO_MANY_REQUESTS => Ok(CatalogFetch::RateLimited),
                StatusCode::NOT_FOUND => Ok(CatalogFetch::Unavailable),
                _ => Err(unavailable()),
            }
        })
    }

    fn detail(&self, base: String, reference: String) -> PortFuture<'_, CatalogItem> {
        Box::pin(async move {
            let base = canonical(&base)?;
            let mut url = detail_url(&base, &reference)?;
            let mut visited = BTreeSet::new();
            let started = Instant::now();
            for hop in 0..=MAX_REDIRECTS {
                if !visited.insert(url.to_string()) {
                    return Err(unsafe_reference());
                }
                let remaining = DETAIL_BUDGET
                    .checked_sub(started.elapsed())
                    .filter(|d| !d.is_zero())
                    .ok_or_else(unavailable)?;
                let response = self
                    .http
                    .get(url.clone())
                    .timeout(remaining)
                    .send()
                    .await
                    .map_err(|_| unavailable())?;
                match response.status() {
                    StatusCode::OK => return read_detail(response, &base, &url).await,
                    StatusCode::FOUND if hop < MAX_REDIRECTS => url = location(&base, &response)?,
                    StatusCode::TOO_MANY_REQUESTS => {
                        return Err(ApiError::new(
                            "neodb_catalog_rate_limited",
                            "NeoDB catalog rate limit reached; try later.",
                        ));
                    }
                    _ => return Err(unavailable()),
                }
            }
            Err(unavailable())
        })
    }
}

#[cfg(test)]
mod tests;
