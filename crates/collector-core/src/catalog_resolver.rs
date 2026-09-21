//! Native catalog resolution, independent of HTTP and any async runtime.
use crate::{
    access::PortFuture,
    catalog::{CatalogFetch, CatalogItem, NeoDbCatalogTransport},
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
