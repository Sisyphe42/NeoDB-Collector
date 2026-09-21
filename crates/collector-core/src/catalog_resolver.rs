//! Native catalog resolution, independent of HTTP and any async runtime.
use crate::{
    access::PortFuture,
    catalog::{
        CatalogFetch, CatalogItem, CatalogSearchPage, NeoDbCatalogSearchTransport,
        NeoDbCatalogTransport,
    },
};
use collector_domain::ApiError;
use std::time::Duration;

pub const POLL_INTERVAL: Duration = Duration::from_secs(15);
pub const RESOLUTION_BUDGET: Duration = Duration::from_secs(120);

/// Trusted built-in adapter output, not a wire DTO. The adapter validates that
/// canonical URL, identifier and model all describe the same source observation.
pub struct CatalogIdentity {
    pub source_url: String,
    pub media_type: String,
    pub model: String,
    pub scheme: String,
    pub external_id: String,
}

/// One resolution's monotonic clock and sticky cancellation signal.
pub trait ResolutionControl: Send + Sync {
    fn now(&self) -> Duration;
    /// Return Ok only at/after `at`; cancellation returns an error and wakes every
    /// registered waiter. Dropping the owner must cancel, not disable the signal.
    fn wait_until(&self, at: Duration) -> PortFuture<'_, ()>;
    /// Runtime-owned race. Cancel/expiry wins over a simultaneously ready result;
    /// drop unfinished work on interruption. Use one absolute deadline throughout.
    fn run<'a, T: Send + 'a>(
        &'a self,
        deadline: Duration,
        work: PortFuture<'a, T>,
    ) -> PortFuture<'a, T>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum Resolution {
    Matched(CatalogItem),
    ManuallySelected(CatalogItem),
    NeedsReview {
        item: CatalogItem,
        reason: &'static str,
    },
    Unsupported,
    RateLimited,
    Unavailable,
    /// Search is discovery evidence only; even an exact singleton needs review.
    SearchCandidates(CatalogSearchPage),
}

fn timeout() -> ApiError {
    ApiError::new(
        "catalog_resolution_timeout",
        "Catalog resolution exceeded its time budget.",
    )
}

fn validate_expected(expected: &CatalogIdentity) -> Result<(), ApiError> {
    if [
        &expected.source_url,
        &expected.media_type,
        &expected.model,
        &expected.scheme,
        &expected.external_id,
    ]
    .iter()
    .any(|s| s.is_empty())
    {
        Err(ApiError::new(
            "catalog_identity_invalid",
            "Expected a validated source identity.",
        ))
    } else {
        Ok(())
    }
}

pub fn validate_catalog_search_query(query: &str) -> Result<&str, ApiError> {
    fn invalid_query() -> ApiError {
        ApiError::new(
            "catalog_search_query_invalid",
            "Expected a nonblank search query of at most 512 UTF-8 bytes without controls.",
        )
    }
    if query.len() > 512 || query.chars().any(char::is_control) {
        return Err(invalid_query());
    }
    let query = query.trim();
    if query.is_empty() {
        Err(invalid_query())
    } else {
        Ok(query)
    }
}

fn invalid_reference() -> ApiError {
    ApiError::new(
        "neodb_catalog_reference_invalid",
        "Expected a supported NeoDB catalog detail reference.",
    )
}

fn classify(expected: &CatalogIdentity, item: CatalogItem) -> Resolution {
    let reason = if item.media_type != expected.media_type || item.model != expected.model {
        Some("item_granularity_mismatch")
    } else {
        let mut matching = false;
        let mut conflict = false;
        for (scheme, id) in &item.external_ids {
            if scheme == &expected.scheme {
                matching |= id == &expected.external_id;
                conflict |= id != &expected.external_id;
            }
        }
        if conflict {
            Some("item_identity_conflict")
        } else if !matching {
            Some("item_identity_missing")
        } else {
            None
        }
    };
    match reason {
        Some(reason) => Resolution::NeedsReview { item, reason },
        None => Resolution::Matched(item),
    }
}

async fn resolve_native_at_deadline(
    transport: &dyn NeoDbCatalogTransport,
    control: &impl ResolutionControl,
    base: &str,
    expected: &CatalogIdentity,
    deadline: Duration,
) -> Result<Resolution, ApiError> {
    loop {
        match control
            .run(
                deadline,
                transport.fetch(base.into(), expected.source_url.clone()),
            )
            .await?
        {
            CatalogFetch::Pending => {
                // Spacing starts after the response, so slow requests cannot
                // compress the interval. The same deadline includes all I/O.
                let next = control
                    .now()
                    .checked_add(POLL_INTERVAL)
                    .ok_or_else(timeout)?;
                control.run(deadline, control.wait_until(next)).await?;
            }
            CatalogFetch::Detail(reference) => {
                let item = control
                    .run(deadline, transport.detail(base.into(), reference))
                    .await?;
                return Ok(classify(expected, item));
            }
            CatalogFetch::Unsupported => return Ok(Resolution::Unsupported),
            CatalogFetch::RateLimited => return Ok(Resolution::RateLimited),
            CatalogFetch::Unavailable => return Ok(Resolution::Unavailable),
        }
    }
}

pub async fn resolve_native(
    transport: &dyn NeoDbCatalogTransport,
    control: &impl ResolutionControl,
    base: &str,
    expected: &CatalogIdentity,
) -> Result<Resolution, ApiError> {
    validate_expected(expected)?;
    let deadline = control
        .now()
        .checked_add(RESOLUTION_BUDGET)
        .ok_or_else(timeout)?;
    resolve_native_at_deadline(transport, control, base, expected, deadline).await
}

pub async fn resolve_with_search<T>(
    transport: &T,
    control: &impl ResolutionControl,
    base: &str,
    expected: &CatalogIdentity,
    query: &str,
) -> Result<Resolution, ApiError>
where
    T: NeoDbCatalogTransport + NeoDbCatalogSearchTransport,
{
    validate_expected(expected)?;
    // Validate before native I/O so trimming cannot launder control bytes.
    let query = validate_catalog_search_query(query)?.to_owned();
    let deadline = control
        .now()
        .checked_add(RESOLUTION_BUDGET)
        .ok_or_else(timeout)?;
    let native = resolve_native_at_deadline(transport, control, base, expected, deadline).await?;
    if native != Resolution::Unsupported {
        return Ok(native);
    }
    let page = control
        .run(
            deadline,
            transport.search(base.into(), query, expected.media_type.clone()),
        )
        .await?;
    Ok(Resolution::SearchCandidates(page))
}

/// Validate one explicit user selection against fresh detail evidence.
///
/// The caller must be trusted to invoke this only after an explicit user choice.
/// `reference` is an existing relative or same-origin API detail URL; the
/// restrictive transport remains the sole authority that parses and validates it.
pub async fn resolve_manual(
    transport: &dyn NeoDbCatalogTransport,
    control: &impl ResolutionControl,
    base: &str,
    expected: &CatalogIdentity,
    reference: &str,
) -> Result<Resolution, ApiError> {
    validate_expected(expected)?;
    if reference.trim().is_empty() {
        return Err(invalid_reference());
    }
    let deadline = control
        .now()
        .checked_add(RESOLUTION_BUDGET)
        .ok_or_else(timeout)?;
    let item = control
        .run(deadline, transport.detail(base.into(), reference.into()))
        .await?;
    Ok(match classify(expected, item) {
        Resolution::Matched(item)
        | Resolution::NeedsReview {
            item,
            reason: "item_identity_missing",
        } => Resolution::ManuallySelected(item),
        other => other,
    })
}

#[cfg(test)]
mod tests;
