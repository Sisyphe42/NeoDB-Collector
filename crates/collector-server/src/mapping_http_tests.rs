use super::*;
use crate::access_tests::{login, request, setup};
use collector_core::{
    access::AccessRepository,
    mapping::{MappingRepository, imdb_default},
};
use serde_json::json;

const PATH: &str = "/api/v1/mapping-profiles/imdb_ratings";
const VALIDATE: &str = "/api/v1/mapping-profiles/validate";

#[tokio::test]
async fn validates_without_saving_and_preserves_versions_through_authenticated_http() {
    let (_dir, store, service, app) = setup().await;
    let (cookie, csrf) = login(&app, &service).await;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:19423"),
        ("x-csrf-token", csrf.as_str()),
    ];
    let mut profile = imdb_default();
    let yaml = mapping_profiles::encode(&profile, mapping_profiles::Format::Yaml).unwrap();
    let body = json!({"format":"yaml", "document":yaml});
    assert_eq!(request(&app, "POST", VALIDATE, body, &headers).await.0, 200);
    assert_eq!(
        request(&app, "GET", PATH, json!(null), &headers).await.0,
        404
    );
    let body = json!({"format":"yaml", "document":yaml, "expectedVersion":0});
    let (status, _, first) = request(&app, "PUT", PATH, body.clone(), &headers).await;
    assert_eq!(status, 200);
    assert_eq!(first["enabled"], false);
    assert_eq!(request(&app, "PUT", PATH, body, &headers).await.0, 409);
    profile.version = 2;
    profile.rules[1].enabled = true;
    let body = json!({"format":"json", "document":serde_json::to_string(&profile).unwrap(), "expectedVersion":1});
    assert_eq!(request(&app, "PUT", PATH, body, &headers).await.0, 200);
    assert_eq!(
        request(&app, "GET", PATH, json!(null), &headers).await.2["version"],
        2
    );
    assert_eq!(
        store
            .load(profile.id, Some(1))
            .await
            .unwrap()
            .unwrap()
            .version,
        1
    );
    let mut named = imdb_default();
    named.id = "validate".into();
    assert_eq!(request(&app, "PUT", VALIDATE, json!({"format":"json", "document":serde_json::to_string(&named).unwrap(), "expectedVersion":0}), &headers).await.0,200);
    assert_eq!(
        request(&app, "GET", VALIDATE, json!(null), &headers)
            .await
            .2["id"],
        "validate"
    );
    store.close().await;
}

#[tokio::test]
async fn denies_missing_csrf_invalid_bearer_fallback_and_revoked_clients() {
    let (_dir, store, service, app) = setup().await;
    let profile = imdb_default();
    let body = json!({"format":"json", "document":serde_json::to_string(&profile).unwrap(), "expectedVersion":0});
    assert_eq!(request(&app, "PUT", PATH, body.clone(), &[]).await.0, 401);
    let (cookie, csrf) = login(&app, &service).await;
    assert_eq!(
        request(
            &app,
            "PUT",
            PATH,
            body.clone(),
            &[("cookie", &cookie), ("origin", "http://127.0.0.1:19423")]
        )
        .await
        .0,
        403
    );
    assert_eq!(
        request(
            &app,
            "PUT",
            PATH,
            body.clone(),
            &[("cookie", &cookie), ("x-csrf-token", &csrf)]
        )
        .await
        .0,
        403
    );
    assert_eq!(
        request(
            &app,
            "GET",
            PATH,
            json!(null),
            &[("cookie", &cookie), ("authorization", "Bearer invalid")]
        )
        .await
        .0,
        401
    );
    let pairing = service
        .create_pairing(PairingRequest {
            client_name: "fixture".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let token = service
        .pair(PairingExchange {
            one_time_key: pairing.uri.split("#key=").nth(1).unwrap().into(),
            client_name: "fixture".into(),
            client_kind: "cli".into(),
        })
        .await
        .unwrap();
    let bearer = format!("Bearer {}", token.token);
    assert_eq!(
        request(&app, "PUT", PATH, body, &[("authorization", &bearer)])
            .await
            .0,
        200
    );
    assert_eq!(
        request(
            &app,
            "GET",
            PATH,
            json!(null),
            &[("authorization", &bearer)]
        )
        .await
        .0,
        200
    );
    store
        .revoke(token.client_id, "client".into())
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "GET",
            PATH,
            json!(null),
            &[("authorization", &bearer)]
        )
        .await
        .0,
        401
    );
    store.close().await;
}

#[tokio::test]
async fn rejects_invalid_documents_limits_and_mismatched_ids_without_persistence() {
    let (_dir, store, service, app) = setup().await;
    let (cookie, csrf) = login(&app, &service).await;
    let headers = [
        ("cookie", cookie.as_str()),
        ("origin", "http://127.0.0.1:19423"),
        ("x-csrf-token", csrf.as_str()),
    ];
    let profile = imdb_default();
    let document = serde_json::to_string(&profile).unwrap();
    for (doc, expected) in [
        (
            document.replace("\"enabled\":false", "\"enabled\":true"),
            400,
        ),
        (
            document.replace("\"id\":\"imdb_ratings\"", "\"id\":\"other\""),
            400,
        ),
        (
            document.replace("\"kind\":\"clamp\"", "\"kind\":\"execute\""),
            400,
        ),
        (" ".repeat(65_537), 413),
        ("x".repeat(512 * 1024), 413),
    ] {
        let (status, _, value) = request(
            &app,
            "PUT",
            PATH,
            json!({"format":"json","document":doc,"expectedVersion":0}),
            &headers,
        )
        .await;
        assert_eq!(status, expected);
        assert!(!value.to_string().contains("imdb_ratings"));
    }
    let invalid = json!({"format":"json","document":document,"unexpected":"private-payload"});
    assert_eq!(
        request(&app, "POST", VALIDATE, invalid, &headers).await.0,
        400
    );
    assert!(store.load(profile.id, None).await.unwrap().is_none());
    store.close().await;
    assert_eq!(
        request(&app, "GET", PATH, json!(null), &headers).await.0,
        503
    );
}
