use super::{NeoDbClient, canonical, read_bounded_json};
use crate::imdb::valid_title_id;
use collector_core::{
    access::PortFuture,
    catalog::{
        CatalogFetch, CatalogItem, CatalogSearchCandidate, CatalogSearchPage,
        NeoDbCatalogSearchTransport, NeoDbCatalogTransport,
    },
    catalog_resolver::validate_catalog_search_query,
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
pub(super) struct DetailReply {
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

async fn read_reply<T: serde::de::DeserializeOwned>(response: Response) -> Result<T, ApiError> {
    read_bounded_json(response, unavailable).await
}

async fn read_detail(
    response: Response,
    base: &Url,
    requested: &Url,
) -> Result<CatalogItem, ApiError> {
    let reply: DetailReply = read_reply(response).await?;
    normalize_detail(reply, base, requested)
}

pub(super) fn normalize_detail(
    reply: DetailReply,
    base: &Url,
    requested: &Url,
) -> Result<CatalogItem, ApiError> {
    if detail_url(base, &reply.api_url)? != *requested {
        return Err(unavailable());
    }
    let canonical_url = validate_item_identity(
        base,
        requested,
        &reply.uuid,
        &reply.id,
        &reply.category,
        &reply.model,
        reply.parent_uuid.as_deref(),
    )?;
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

pub(super) fn validate_expected_item(base: &Url, item: &CatalogItem) -> Result<Url, ApiError> {
    let requested = detail_url(base, &item.detail_url).map_err(|_| unavailable())?;
    validate_item_identity(
        base,
        &requested,
        &item.uuid,
        &item.canonical_url,
        &item.media_type,
        &item.model,
        item.parent_uuid.as_deref(),
    )?;
    Ok(requested)
}

fn validate_item_identity(
    base: &Url,
    requested: &Url,
    uuid: &str,
    canonical: &str,
    media: &str,
    model: &str,
    parent_uuid: Option<&str>,
) -> Result<Url, ApiError> {
    let (expected_media, expected_model, expected_uuid) =
        item_path(requested.path()).ok_or_else(unavailable)?;
    let expected_canonical = base
        .join(
            requested
                .path()
                .strip_prefix("/api")
                .ok_or_else(unavailable)?,
        )
        .map_err(|_| unavailable())?;
    if uuid != expected_uuid
        || media != expected_media
        || model != expected_model
        || canonical != expected_canonical.as_str()
        || parent_uuid.is_some_and(|id| {
            id.is_empty() || id.len() > 22 || !id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
    {
        return Err(unavailable());
    }
    Ok(expected_canonical)
}

#[derive(Deserialize)]
struct SearchReply {
    data: Vec<SearchItem>,
    pages: i64,
    count: i64,
}

#[derive(Deserialize)]
struct SearchItem {
    title: String,
    #[serde(flatten)]
    detail: DetailReply,
}

fn search_category(value: &str) -> Result<&str, ApiError> {
    match value {
        "movie" | "tv" => Ok(value),
        _ => Err(ApiError::new(
            "neodb_catalog_category_invalid",
            "Expected a supported NeoDB catalog search category.",
        )),
    }
}

fn search_title(value: String) -> Result<String, ApiError> {
    if value.len() > 1024 || value.chars().any(char::is_control) {
        return Err(unavailable());
    }
    let value = value.trim();
    if value.is_empty() {
        Err(unavailable())
    } else {
        Ok(value.to_owned())
    }
}

async fn read_search(
    response: Response,
    base: &Url,
    category: &str,
) -> Result<CatalogSearchPage, ApiError> {
    let reply: SearchReply = read_reply(response).await?;
    if reply.pages < 0 || reply.count < 0 || reply.data.len() > 20 {
        return Err(unavailable());
    }
    let more_available = reply.pages > 1 || reply.count as u64 > reply.data.len() as u64;
    let mut uuids = BTreeSet::new();
    let mut candidates = Vec::with_capacity(reply.data.len());
    for result in reply.data {
        let title = search_title(result.title)?;
        let requested = detail_url(base, &result.detail.api_url)?;
        let item = normalize_detail(result.detail, base, &requested)?;
        if item.media_type != category || !uuids.insert(item.uuid.clone()) {
            return Err(unavailable());
        }
        candidates.push(CatalogSearchCandidate { title, item });
    }
    Ok(CatalogSearchPage {
        candidates,
        more_available,
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

impl NeoDbCatalogSearchTransport for NeoDbClient {
    fn search(
        &self,
        base: String,
        query: String,
        category: String,
    ) -> PortFuture<'_, CatalogSearchPage> {
        Box::pin(async move {
            let query = validate_catalog_search_query(&query)?.to_owned();
            let category = search_category(&category)?.to_owned();
            let base = canonical(&base)?;
            let mut url = base
                .join("/api/catalog/search")
                .map_err(|_| unavailable())?;
            url.query_pairs_mut()
                .append_pair("query", &query)
                .append_pair("category", &category)
                .append_pair("page", "1");
            let response = self.http.get(url).send().await.map_err(|_| unavailable())?;
            match response.status() {
                StatusCode::OK => read_search(response, &base, &category).await,
                StatusCode::TOO_MANY_REQUESTS => Err(ApiError::new(
                    "neodb_catalog_rate_limited",
                    "NeoDB catalog rate limit reached; try later.",
                )),
                _ => Err(unavailable()),
            }
        })
    }
}

#[cfg(test)]
mod tests;
