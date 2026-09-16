use super::{Storage, database_error};
use collector_core::access::{
    AccessRecord, AccessRepository, ChallengeMatch, Endpoint, PortFuture,
};
use collector_domain::{ApiError, access::PairedClient};
use sqlx::Row;

impl AccessRepository for Storage {
    fn insert(&self, r: AccessRecord) -> PortFuture<'_, ()> {
        Box::pin(async move {
            sqlx::query("INSERT INTO access_credentials(id,token_hash,kind,audience,name,client_kind,expires_at) VALUES(?,?,?,?,?,?,?)")
                .bind(r.id).bind(r.hash).bind(r.kind).bind(r.audience).bind(r.name).bind(r.client_kind).bind(r.expires_at)
                .execute(&self.pool).await.map_err(|_| database_error())?;
            Ok(())
        })
    }
    fn exchange(&self, challenge: ChallengeMatch, r: AccessRecord) -> PortFuture<'_, ()> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(|_| database_error())?;
            let consumed = sqlx::query("DELETE FROM access_credentials WHERE token_hash=? AND kind=? AND audience=? AND name=? AND client_kind=? AND expires_at>? AND revoked=0")
                .bind(challenge.hash).bind(challenge.kind).bind(challenge.audience).bind(challenge.name).bind(challenge.client_kind).bind(challenge.now)
                .execute(&mut *tx).await.map_err(|_| database_error())?;
            if consumed.rows_affected() != 1 {
                return Err(ApiError::new(
                    "challenge_invalid",
                    "Key is invalid, expired, already consumed, or belongs to another client.",
                ));
            }
            sqlx::query("INSERT INTO access_credentials(id,token_hash,kind,audience,name,client_kind,expires_at) VALUES(?,?,?,?,?,?,?)")
                .bind(r.id).bind(r.hash).bind(r.kind).bind(r.audience).bind(r.name).bind(r.client_kind).bind(r.expires_at)
                .execute(&mut *tx).await.map_err(|_| database_error())?;
            tx.commit().await.map_err(|_| database_error())?;
            Ok(())
        })
    }
    fn find(
        &self,
        hash: String,
        kind: String,
        audience: String,
        now: i64,
    ) -> PortFuture<'_, Option<AccessRecord>> {
        Box::pin(async move {
            let r = sqlx::query("SELECT * FROM access_credentials WHERE token_hash=? AND kind=? AND audience=? AND revoked=0 AND expires_at>?")
                .bind(hash).bind(kind).bind(audience).bind(now).fetch_optional(&self.pool).await.map_err(|_| database_error())?;
            Ok(r.map(|r| AccessRecord {
                id: r.get("id"),
                hash: r.get("token_hash"),
                kind: r.get("kind"),
                audience: r.get("audience"),
                name: r.get("name"),
                client_kind: r.get("client_kind"),
                expires_at: r.get("expires_at"),
            }))
        })
    }
    fn revoke(&self, id: String, kind: String) -> PortFuture<'_, ()> {
        Box::pin(async move {
            sqlx::query("UPDATE access_credentials SET revoked=1 WHERE id=? AND kind=?")
                .bind(id)
                .bind(kind)
                .execute(&self.pool)
                .await
                .map_err(|_| database_error())?;
            Ok(())
        })
    }
    fn clients(&self) -> PortFuture<'_, Vec<PairedClient>> {
        Box::pin(async move {
            let rows = sqlx::query("SELECT id,name,client_kind,revoked FROM access_credentials WHERE kind='client' ORDER BY rowid DESC")
                .fetch_all(&self.pool).await.map_err(|_| database_error())?;
            Ok(rows
                .into_iter()
                .map(|r| PairedClient {
                    id: r.get("id"),
                    name: r.get("name"),
                    kind: r.get("client_kind"),
                    revoked: r.get::<i64, _>("revoked") != 0,
                })
                .collect())
        })
    }
    fn endpoint(&self) -> PortFuture<'_, Option<Endpoint>> {
        Box::pin(async move {
            let r = sqlx::query(
                "SELECT base_url,account,credential_ref FROM neodb_endpoint WHERE singleton=1",
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| database_error())?;
            Ok(r.map(|r| Endpoint {
                base_url: r.get("base_url"),
                account: r.get("account"),
                credential_ref: r.get("credential_ref"),
            }))
        })
    }
    fn set_endpoint(&self, endpoint: Option<Endpoint>) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if let Some(e) = endpoint {
                sqlx::query("INSERT INTO neodb_endpoint(singleton,base_url,account,credential_ref) VALUES(1,?,?,?) ON CONFLICT(singleton) DO UPDATE SET base_url=excluded.base_url,account=excluded.account,credential_ref=excluded.credential_ref")
                    .bind(e.base_url).bind(e.account).bind(e.credential_ref).execute(&self.pool).await.map_err(|_| database_error())?;
            } else {
                sqlx::query("DELETE FROM neodb_endpoint")
                    .execute(&self.pool)
                    .await
                    .map_err(|_| database_error())?;
            }
            Ok(())
        })
    }
}

impl Storage {
    pub async fn clean_expired_access(&self, now: i64) -> Result<(), ApiError> {
        sqlx::query("DELETE FROM access_credentials WHERE expires_at<=? AND kind<>'client'")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|_| database_error())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::SecureTokens;
    use collector_core::access::{AccessService, Clock, KEY_TTL, SESSION_TTL};
    use collector_domain::access::{PairingExchange, PairingRequest};
    use std::sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    };

    struct TestClock(AtomicI64);
    impl Clock for TestClock {
        fn now(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn service(storage: &Storage, clock: Arc<TestClock>) -> AccessService {
        AccessService {
            repository: Arc::new(storage.clone()),
            tokens: Arc::new(SecureTokens),
            clock,
            audience: "http://127.0.0.1:19423".into(),
        }
    }
    fn exchange(key: &str) -> PairingExchange {
        PairingExchange {
            one_time_key: key.into(),
            client_name: "CLI".into(),
            client_kind: "cli".into(),
        }
    }
    async fn pairing(service: &AccessService) -> String {
        service
            .create_pairing(PairingRequest {
                client_name: "CLI".into(),
                client_kind: "cli".into(),
            })
            .await
            .unwrap()
            .uri
            .split("#key=")
            .nth(1)
            .unwrap()
            .to_owned()
    }

    #[tokio::test]
    async fn sessions_are_single_use_expiring_and_hash_only() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let clock = Arc::new(TestClock(AtomicI64::new(1000)));
        let service = service(&storage, clock.clone());
        let key = service.bootstrap().await.unwrap();
        let (token, view) = service.open_session(&key).await.unwrap();
        assert_eq!(view.csrf_token, service.csrf(&token));
        assert!(service.open_session(&key).await.is_err());
        assert!(service.session(&token).await.is_ok());
        assert!(service.bearer(&token).await.is_err());
        let hashes: Vec<String> = sqlx::query_scalar("SELECT token_hash FROM access_credentials")
            .fetch_all(&storage.pool)
            .await
            .unwrap();
        assert_eq!(hashes, vec![service.tokens.digest(&token)]);
        assert!(!hashes.contains(&token));
        clock.0.store(1000 + SESSION_TTL, Ordering::SeqCst);
        assert!(service.session(&token).await.is_err());
        let expired = service.bootstrap().await.unwrap();
        clock.0.fetch_add(KEY_TTL, Ordering::SeqCst);
        assert!(service.open_session(&expired).await.is_err());
        storage.clean_expired_access(clock.now()).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM access_credentials")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        storage.close().await;
    }

    #[tokio::test]
    async fn pairing_binds_client_and_origin_and_races_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let clock = Arc::new(TestClock(AtomicI64::new(1000)));
        let service = service(&storage, clock.clone());
        let key = pairing(&service).await;
        let mut wrong = exchange(&key);
        wrong.client_name = "Imposter".into();
        assert!(service.pair(wrong).await.is_err());
        let other = AccessService {
            audience: "http://127.0.0.1:19424".into(),
            ..self::service(&storage, clock.clone())
        };
        assert!(other.pair(exchange(&key)).await.is_err());
        let (first, second) =
            tokio::join!(service.pair(exchange(&key)), service.pair(exchange(&key)));
        assert_ne!(first.is_ok(), second.is_ok());
        let paired = first.or(second).unwrap();
        assert_eq!(storage.clients().await.unwrap().len(), 1);
        assert!(service.bearer(&paired.token).await.is_ok());
        assert!(service.session(&paired.token).await.is_err());
        assert!(
            !paired
                .scopes
                .iter()
                .any(|s| s.contains("admin") || s == "settings.write")
        );
        storage
            .revoke(paired.client_id, "client".into())
            .await
            .unwrap();
        assert!(service.bearer(&paired.token).await.is_err());
        storage.close().await;
        let reopened = Storage::open(dir.path()).await.unwrap();
        assert!(
            self::service(&reopened, clock)
                .bearer(&paired.token)
                .await
                .is_err()
        );
        assert!(reopened.clients().await.unwrap()[0].revoked);
        reopened.close().await;
    }

    #[tokio::test]
    async fn failed_insert_rolls_back_challenge_consumption() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let clock = Arc::new(TestClock(AtomicI64::new(1000)));
        let service = service(&storage, clock);
        let key = pairing(&service).await;
        let invalid = AccessRecord {
            id: "invalid".into(),
            hash: "hash".into(),
            kind: "not-a-kind".into(),
            audience: service.audience.clone(),
            name: "CLI".into(),
            client_kind: "cli".into(),
            expires_at: i64::MAX,
        };
        assert!(
            storage
                .exchange(
                    ChallengeMatch {
                        hash: service.tokens.digest(&key),
                        kind: "pairing".into(),
                        audience: service.audience.clone(),
                        name: "CLI".into(),
                        client_kind: "cli".into(),
                        now: 1000
                    },
                    invalid
                )
                .await
                .is_err()
        );
        assert!(storage.clients().await.unwrap().is_empty());
        assert!(service.pair(exchange(&key)).await.is_ok());
        storage.close().await;
    }

    #[tokio::test]
    async fn pairing_expiry_and_singleton_endpoint() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::open(dir.path()).await.unwrap();
        let clock = Arc::new(TestClock(AtomicI64::new(1000)));
        let service = service(&storage, clock.clone());
        let key = pairing(&service).await;
        clock.0.fetch_add(KEY_TTL, Ordering::SeqCst);
        assert!(service.pair(exchange(&key)).await.is_err());
        for account in ["first", "second"] {
            storage
                .set_endpoint(Some(Endpoint {
                    base_url: "https://neodb.example".into(),
                    account: account.into(),
                    credential_ref: "opaque-reference".into(),
                }))
                .await
                .unwrap();
        }
        assert_eq!(storage.endpoint().await.unwrap().unwrap().account, "second");
        storage.set_endpoint(None).await.unwrap();
        assert!(storage.endpoint().await.unwrap().is_none());
        storage.close().await;
    }
}
