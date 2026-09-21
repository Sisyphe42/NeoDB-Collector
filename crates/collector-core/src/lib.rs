//! Deterministic application status and the storage health port.
use collector_domain::{API_VERSION, HealthDocument, HealthStatus, SCHEMA_VERSION, StorageStatus};
use std::{future::Future, pin::Pin};

pub mod access;
pub mod catalog;
pub mod catalog_resolver;
pub mod mapping;
pub mod oauth;
#[cfg(test)]
mod oauth_tests;
pub mod secrets;

pub trait StorageHealth: Send + Sync {
    fn is_ready(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;
}

pub async fn health(storage: &dyn StorageHealth, core_version: &str) -> HealthDocument {
    let ready = storage.is_ready().await;
    HealthDocument {
        status: if ready {
            HealthStatus::Ok
        } else {
            HealthStatus::Degraded
        },
        storage: if ready {
            StorageStatus::Ready
        } else {
            StorageStatus::Unavailable
        },
        core_version: core_version.into(),
        api_version: API_VERSION.into(),
        schema_version: SCHEMA_VERSION,
    }
}
