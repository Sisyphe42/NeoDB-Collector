//! Internal secret types shared by ports and adapters, not public wire DTOs.
//!
//! Plaintext access must be explicit at the consuming boundary. The serde feature
//! permits deserialization only; secrets must not acquire implicit serialization.
//!
//! ```compile_fail
//! use collector_core::secrets::SecretString;
//! let secret = SecretString::from("private");
//! let _ = serde_json::to_string(&secret);
//! ```
//!
//! ```compile_fail
//! use collector_core::secrets::SecretString;
//! let secret = SecretString::from("private");
//! let _ = format!("{secret}");
//! ```
pub use secrecy::{ExposeSecret, SecretString};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::{OAuthApp, OAuthToken};

    #[test]
    fn secrets_deserialize_and_debug_without_exposing_plaintext() {
        let secret: SecretString = serde_json::from_str("\"private-marker\"").unwrap();
        assert_eq!(secret.expose_secret(), "private-marker");
        let app = OAuthApp {
            client_id: "public-client".into(),
            client_secret: secret.clone(),
        };
        let token = OAuthToken {
            access_token: secret.clone(),
            scope: "read".into(),
        };
        for debug in [
            format!("{secret:?}"),
            format!("{app:?}"),
            format!("{token:?}"),
        ] {
            assert!(!debug.contains("private-marker"));
            assert!(debug.contains("REDACTED"));
        }
    }
}
