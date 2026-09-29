//! Internal normalized personal-mark read port. This exposes no write capability.
use crate::{access::PortFuture, catalog::CatalogItem, secrets::SecretString};

#[derive(Debug, PartialEq, Eq)]
pub struct MarkSnapshot {
    pub item: CatalogItem,
    pub owner: String,
    pub shelf_type: String,
    pub rating_grade: Option<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MarkRead {
    Present(Box<MarkSnapshot>),
    /// Confirms only that no shelf member exists; independent fields stay unknown.
    MissingShelf,
}

pub trait NeoDbMarkTransport: Send + Sync {
    /// Performs one restrictive authenticated read against an already resolved item.
    fn read_mark<'a>(
        &'a self,
        base: String,
        token: &'a SecretString,
        expected_account: String,
        expected_item: &'a CatalogItem,
    ) -> PortFuture<'a, MarkRead>;
}
