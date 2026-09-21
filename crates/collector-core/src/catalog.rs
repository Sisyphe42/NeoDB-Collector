//! Internal evidence port. Fetch may create public catalog entries, never marks.
use crate::access::PortFuture;

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
