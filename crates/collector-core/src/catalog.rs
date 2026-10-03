//! Internal evidence port. Fetch may create public catalog entries, never marks.
use crate::access::PortFuture;
use collector_domain::ApiError;

pub const MAX_CATALOG_REFERENCE_BYTES: usize = 512;

/// Shared built-in IMDb identity predicate. Connector IDs remain open strings;
/// callers opt into this rule only for the explicit `imdb:title` scheme.
pub fn valid_imdb_title_id(value: &str) -> bool {
    let digits = value.strip_prefix("tt").unwrap_or("");
    (7..=16).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

#[derive(Debug, PartialEq, Eq)]
pub enum CatalogFetch {
    Pending,
    Detail(String),
    Unsupported,
    RateLimited,
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CatalogItem {
    pub uuid: String,
    pub canonical_url: String,
    pub detail_url: String,
    pub media_type: String,
    pub model: String,
    pub parent_uuid: Option<String>,
    /// Preserve distinct claims, including contradictions, for the resolver.
    pub external_ids: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogReference {
    uuid: String,
    media_type: String,
    model: String,
}

impl CatalogReference {
    pub fn from_item(base: &str, item: &CatalogItem) -> Result<Self, ApiError> {
        validate_catalog_item(base, item)?;
        Self::from_stored(
            item.uuid.clone(),
            item.media_type.clone(),
            item.model.clone(),
        )
    }

    pub fn from_stored(uuid: String, media_type: String, model: String) -> Result<Self, ApiError> {
        if !valid_catalog_uuid(&uuid) || catalog_path(&media_type, &model).is_none() {
            return Err(invalid_catalog_reference());
        }
        Ok(Self {
            uuid,
            media_type,
            model,
        })
    }

    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn detail_path(&self) -> String {
        format!(
            "/api{}{}",
            catalog_path(&self.media_type, &self.model).expect("validated catalog reference"),
            self.uuid
        )
    }
}

pub(crate) fn validate_catalog_item(base: &str, item: &CatalogItem) -> Result<(), ApiError> {
    let Some(path) = catalog_path(&item.media_type, &item.model) else {
        return Err(invalid_catalog_reference());
    };
    if !valid_catalog_uuid(&item.uuid)
        || item
            .parent_uuid
            .as_deref()
            .is_some_and(|value| !valid_catalog_uuid(value))
        || item.canonical_url != format!("{base}{path}{}", item.uuid)
        || item.detail_url != format!("{base}/api{path}{}", item.uuid)
        || item.external_ids.len() > 128
        || item
            .external_ids
            .iter()
            .any(|(scheme, id)| !bounded(scheme, 128) || !bounded(id, 4096))
        || item.external_ids.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(invalid_catalog_reference());
    }
    Ok(())
}

fn catalog_path(media_type: &str, model: &str) -> Option<&'static str> {
    match (media_type, model) {
        ("movie", "Movie") => Some("/movie/"),
        ("tv", "TVShow") => Some("/tv/"),
        ("tv", "TVSeason") => Some("/tv/season/"),
        ("tv", "TVEpisode") => Some("/tv/episode/"),
        _ => None,
    }
}

fn valid_catalog_uuid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 22 && value.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn bounded(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn invalid_catalog_reference() -> ApiError {
    ApiError::new(
        "neodb_catalog_reference_invalid",
        "Expected a supported NeoDB catalog detail reference.",
    )
}

#[derive(Debug, PartialEq, Eq)]
pub struct CatalogSearchCandidate {
    pub title: String,
    pub item: CatalogItem,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CatalogSearchPage {
    pub candidates: Vec<CatalogSearchCandidate>,
    /// Advisory only. Estimated upstream counts must never prove absence.
    pub more_available: bool,
}

pub trait NeoDbCatalogTransport: Send + Sync {
    /// One attempt only. Callers must space pending polls >=15s and stop at 120s.
    fn fetch(&self, base: String, source_url: String) -> PortFuture<'_, CatalogFetch>;
    /// Revalidate references before I/O; this is not an arbitrary URL proxy.
    fn detail(&self, base: String, reference: String) -> PortFuture<'_, CatalogItem>;
}

pub trait NeoDbCatalogSearchTransport: Send + Sync {
    /// One first-page local-index search only. The adapter validates category,
    /// query, response bounds and every candidate before returning evidence.
    fn search(
        &self,
        base: String,
        query: String,
        category: String,
    ) -> PortFuture<'_, CatalogSearchPage>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> CatalogItem {
        CatalogItem {
            uuid: "Resolved123".into(),
            canonical_url: "https://neodb.example/tv/Resolved123".into(),
            detail_url: "https://neodb.example/api/tv/Resolved123".into(),
            media_type: "tv".into(),
            model: "TVShow".into(),
            parent_uuid: None,
            external_ids: vec![],
        }
    }

    #[test]
    fn compact_reference_validates_item_and_derives_only_safe_relative_paths() {
        for (media_type, model, path) in [
            ("movie", "Movie", "/api/movie/Resolved123"),
            ("tv", "TVShow", "/api/tv/Resolved123"),
            ("tv", "TVSeason", "/api/tv/season/Resolved123"),
            ("tv", "TVEpisode", "/api/tv/episode/Resolved123"),
        ] {
            let reference = CatalogReference::from_stored(
                "Resolved123".into(),
                media_type.into(),
                model.into(),
            )
            .unwrap();
            assert_eq!(reference.detail_path(), path);
        }
        assert!(
            CatalogReference::from_stored(
                "A123456789012345678901".into(),
                "movie".into(),
                "Movie".into()
            )
            .is_ok()
        );

        for (uuid, media_type, model) in [
            ("", "tv", "TVShow"),
            ("A1234567890123456789012", "tv", "TVShow"),
            ("../secret", "tv", "TVShow"),
            ("abc-def", "tv", "TVShow"),
            ("abc%2Fdef", "tv", "TVShow"),
            ("abc?x", "tv", "TVShow"),
            ("abc#x", "tv", "TVShow"),
            ("é", "tv", "TVShow"),
            ("abc\ndef", "tv", "TVShow"),
            ("abc", "movie", "TVShow"),
            ("abc", "tv", "Unknown"),
        ] {
            assert_eq!(
                CatalogReference::from_stored(uuid.into(), media_type.into(), model.into())
                    .unwrap_err()
                    .code,
                "neodb_catalog_reference_invalid"
            );
        }

        for hostile in [
            {
                let mut value = item();
                value.detail_url = "https://evil.example/api/tv/Resolved123".into();
                value
            },
            {
                let mut value = item();
                value.detail_url = "https://user@neodb.example/api/tv/Resolved123".into();
                value
            },
            {
                let mut value = item();
                value.detail_url = "https://neodb.example/api/tv/Resolved123?token=x".into();
                value
            },
            {
                let mut value = item();
                value.canonical_url = "https://neodb.example/movie/Resolved123".into();
                value
            },
            {
                let mut value = item();
                value.parent_uuid = Some("../parent".into());
                value
            },
        ] {
            assert_eq!(
                CatalogReference::from_item("https://neodb.example", &hostile)
                    .unwrap_err()
                    .code,
                "neodb_catalog_reference_invalid"
            );
        }
    }
}
