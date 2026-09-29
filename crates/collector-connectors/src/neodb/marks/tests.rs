use super::*;
use crate::neodb::RESPONSE_LIMIT;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::Response as HttpResponse,
    routing::any,
};
use reqwest::{Client, redirect::Policy};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const UUID: &str = "4W2ZHSBFHfCbRhdBUlHhBS";
const DETAIL: &str = "/api/movie/4W2ZHSBFHfCbRhdBUlHhBS";

#[derive(Debug)]
struct Seen {
    uri: String,
    authorization: Option<String>,
    has_cookie: bool,
}

type Reply = (u16, Option<String>, String);

#[derive(Clone, Default)]
struct Mock {
    replies: Arc<Mutex<VecDeque<Reply>>>,
    seen: Arc<Mutex<Vec<Seen>>>,
}

async fn handle(State(mock): State<Mock>, request: Request) -> HttpResponse<Body> {
    assert_eq!(request.method(), "GET");
    mock.seen.lock().unwrap().push(Seen {
        uri: request.uri().to_string(),
        authorization: request
            .headers()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        has_cookie: request.headers().contains_key("cookie"),
    });
    let (status, location, body) = mock
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected request");
    let mut response = HttpResponse::builder().status(status);
    if let Some(location) = location {
        response = response.header("location", location);
    }
    response.body(Body::from(body)).unwrap()
}

async fn server() -> (String, Mock, tokio::task::JoinHandle<()>) {
    let mock = Mock::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(any(handle)).with_state(mock.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (base, mock, task)
}

fn enqueue(mock: &Mock, status: u16, location: Option<&str>, body: String) {
    mock.replies
        .lock()
        .unwrap()
        .push_back((status, location.map(str::to_owned), body));
}

fn expected(base: &str) -> CatalogItem {
    CatalogItem {
        uuid: UUID.into(),
        canonical_url: format!("{base}/movie/{UUID}"),
        detail_url: format!("{base}{DETAIL}"),
        media_type: "movie".into(),
        model: "Movie".into(),
        parent_uuid: None,
        external_ids: vec![("imdb:title".into(), "tt1234567".into())],
    }
}

fn mark(base: &str, rating: Value) -> Value {
    serde_json::json!({
        "item": {
            "id": format!("{base}/movie/{UUID}"),
            "uuid": UUID,
            "api_url": DETAIL,
            "category": "movie",
            "type": "Movie",
            "parent_uuid": null,
            "external_resources": [
                {"url": "https://www.imdb.com/title/tt7654321/"}
            ],
            "title": "discarded"
        },
        "owner": {"username": "alice", "display_name": "discarded"},
        "shelf_type": "complete",
        "rating_grade": rating,
        "comment_text": "discarded",
        "tags": ["discarded"]
    })
}

fn padded_mark(base: &str, total: usize) -> String {
    let mut body = mark(base, Value::from(8));
    body["padding"] = "".into();
    let base_len = body.to_string().len();
    assert!(base_len <= total);
    body["padding"] = "x".repeat(total - base_len).into();
    let body = body.to_string();
    assert_eq!(body.len(), total);
    body
}

async fn read(
    client: &NeoDbClient,
    base: String,
    token: &SecretString,
    account: &str,
    item: &CatalogItem,
) -> Result<MarkRead, ApiError> {
    client
        .read_mark(base, token, account.to_owned(), item)
        .await
}

#[tokio::test]
async fn sends_exact_authenticated_get_and_retains_only_supported_fields() {
    let (base, mock, task) = server().await;
    enqueue(&mock, 200, None, mark(&base, Value::from(8)).to_string());
    let item = expected(&base);
    let token: SecretString = "private-token".into();
    let result = read(
        &NeoDbClient::new().unwrap(),
        base.clone(),
        &token,
        "alice",
        &item,
    )
    .await
    .unwrap();
    let MarkRead::Present(snapshot) = result else {
        panic!("expected present mark")
    };
    assert_eq!(snapshot.owner, "alice");
    assert_eq!(snapshot.shelf_type, "complete");
    assert_eq!(snapshot.rating_grade, Some(8));
    assert_eq!(
        snapshot.item.external_ids,
        [("imdb:title".into(), "tt7654321".into())]
    );
    let seen = mock.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].uri, format!("/api/me/shelf/item/{UUID}"));
    assert_eq!(
        seen[0].authorization.as_deref(),
        Some("Bearer private-token")
    );
    assert!(!seen[0].has_cookie);
    task.abort();
}

#[tokio::test]
async fn distinguishes_explicit_null_from_missing_or_invalid_rating() {
    let (base, mock, task) = server().await;
    let item = expected(&base);
    let token: SecretString = "token".into();
    enqueue(&mock, 200, None, mark(&base, Value::Null).to_string());
    let MarkRead::Present(snapshot) = read(
        &NeoDbClient::new().unwrap(),
        base.clone(),
        &token,
        "alice",
        &item,
    )
    .await
    .unwrap() else {
        panic!("expected present mark")
    };
    assert_eq!(snapshot.rating_grade, None);

    let mut missing = mark(&base, Value::Null);
    missing.as_object_mut().unwrap().remove("rating_grade");
    let invalid = [
        missing,
        mark(&base, Value::from(0)),
        mark(&base, Value::from(11)),
        mark(&base, serde_json::json!(1.5)),
        mark(&base, Value::String("8".into())),
    ];
    for body in invalid {
        enqueue(&mock, 200, None, body.to_string());
        assert_eq!(
            read(
                &NeoDbClient::new().unwrap(),
                base.clone(),
                &token,
                "alice",
                &item,
            )
            .await
            .unwrap_err()
            .code,
            "neodb_mark_unavailable"
        );
    }
    task.abort();
}

#[tokio::test]
async fn accepts_all_supported_shelves_and_rating_boundaries() {
    let (base, mock, task) = server().await;
    let item = expected(&base);
    let token: SecretString = "token".into();
    for shelf in ["wishlist", "progress", "complete", "dropped"] {
        for grade in [1, 10] {
            let mut body = mark(&base, Value::from(grade));
            body["shelf_type"] = shelf.into();
            enqueue(&mock, 200, None, body.to_string());
            let MarkRead::Present(snapshot) = read(
                &NeoDbClient::new().unwrap(),
                base.clone(),
                &token,
                "alice",
                &item,
            )
            .await
            .unwrap() else {
                panic!("expected present mark")
            };
            assert_eq!(snapshot.shelf_type, shelf);
            assert_eq!(snapshot.rating_grade, Some(grade));
        }
    }
    task.abort();
}

#[tokio::test]
async fn validates_shelf_owner_and_embedded_item_binding() {
    let (base, mock, task) = server().await;
    let item = expected(&base);
    let token: SecretString = "token".into();
    for shelf in ["", "reading", "Complete", "complete\n"] {
        let mut body = mark(&base, Value::from(8));
        body["shelf_type"] = shelf.into();
        enqueue(&mock, 200, None, body.to_string());
        assert_eq!(
            read(
                &NeoDbClient::new().unwrap(),
                base.clone(),
                &token,
                "alice",
                &item
            )
            .await
            .unwrap_err()
            .code,
            "neodb_mark_unavailable"
        );
    }
    let mut wrong_owner = mark(&base, Value::from(8));
    wrong_owner["owner"]["username"] = "bob".into();
    enqueue(&mock, 200, None, wrong_owner.to_string());
    assert_eq!(
        read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &item
        )
        .await
        .unwrap_err()
        .code,
        "neodb_mark_account_changed"
    );

    for (key, value) in [
        ("uuid", Value::String("wrong".into())),
        ("api_url", Value::String("/api/movie/wrong".into())),
        ("category", Value::String("tv".into())),
        ("type", Value::String("TVShow".into())),
    ] {
        let mut body = mark(&base, Value::from(8));
        body["item"][key] = value;
        enqueue(&mock, 200, None, body.to_string());
        assert_eq!(
            read(
                &NeoDbClient::new().unwrap(),
                base.clone(),
                &token,
                "alice",
                &item
            )
            .await
            .unwrap_err()
            .code,
            "neodb_mark_unavailable"
        );
    }
    task.abort();
}

#[tokio::test]
async fn accepts_normalized_relative_expectation_and_rejects_changed_parent() {
    let (base, mock, task) = server().await;
    let token: SecretString = "token".into();
    let mut relative = expected(&base);
    relative.detail_url = DETAIL.into();
    enqueue(&mock, 200, None, mark(&base, Value::from(8)).to_string());
    assert!(
        read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &relative,
        )
        .await
        .is_ok()
    );

    let mut expected_parent = expected(&base);
    expected_parent.parent_uuid = Some("ParentOne".into());
    let mut changed = mark(&base, Value::from(8));
    changed["item"]["parent_uuid"] = "ParentTwo".into();
    enqueue(&mock, 200, None, changed.to_string());
    assert_eq!(
        read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &expected_parent,
        )
        .await
        .unwrap_err()
        .code,
        "neodb_mark_identity_changed"
    );
    task.abort();
}

#[tokio::test]
async fn rejects_invalid_input_before_network_io() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    let token: SecretString = "token".into();
    for account in ["", "   ", "bad\nname", &"x".repeat(257)] {
        assert_eq!(
            read(&client, base.clone(), &token, account, &expected(&base))
                .await
                .unwrap_err()
                .code,
            "neodb_mark_account_invalid"
        );
    }
    for raw in ["", "bad\ntoken"] {
        let invalid: SecretString = raw.to_owned().into();
        assert_eq!(
            read(&client, base.clone(), &invalid, "alice", &expected(&base))
                .await
                .unwrap_err()
                .code,
            "neodb_mark_token_invalid"
        );
    }
    for mutate in 0..6 {
        let mut item = expected(&base);
        match mutate {
            0 => item.uuid = "wrong".into(),
            1 => item.canonical_url = format!("{base}/movie/wrong"),
            2 => item.detail_url = "/api/me".into(),
            3 => item.media_type = "tv".into(),
            4 => item.model = "TVShow".into(),
            _ => item.parent_uuid = Some("bad-parent".into()),
        }
        assert_eq!(
            read(&client, base.clone(), &token, "alice", &item)
                .await
                .unwrap_err()
                .code,
            "neodb_mark_expectation_invalid"
        );
    }
    let mut foreign = expected(&base);
    foreign.detail_url = format!("http://127.0.0.1:9{DETAIL}");
    assert_eq!(
        read(&client, base.clone(), &token, "alice", &foreign)
            .await
            .unwrap_err()
            .code,
        "neodb_mark_expectation_invalid"
    );
    assert!(mock.seen.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn only_exact_bounded_mark_not_found_means_missing_shelf() {
    let (base, mock, task) = server().await;
    let item = expected(&base);
    let token: SecretString = "token".into();
    for (body, expected_code) in [
        (serde_json::json!({"message":"Mark not found"}), None),
        (
            serde_json::json!({"message":"Item not found"}),
            Some("neodb_mark_item_unavailable"),
        ),
        (
            serde_json::json!({"message":"mark not found"}),
            Some("neodb_mark_unavailable"),
        ),
        (serde_json::json!({}), Some("neodb_mark_unavailable")),
    ] {
        enqueue(&mock, 404, None, body.to_string());
        let result = read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &item,
        )
        .await;
        if let Some(code) = expected_code {
            assert_eq!(result.unwrap_err().code, code);
        } else {
            assert_eq!(result.unwrap(), MarkRead::MissingShelf);
        }
    }
    enqueue(
        &mock,
        404,
        None,
        serde_json::json!({"message":"Mark not found", "padding":"x".repeat(RESPONSE_LIMIT)})
            .to_string(),
    );
    assert_eq!(
        read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &item
        )
        .await
        .unwrap_err()
        .code,
        "neodb_mark_unavailable"
    );
    task.abort();
}

#[tokio::test]
async fn classifies_statuses_and_never_follows_redirects() {
    let (base, mock, task) = server().await;
    let item = expected(&base);
    let token: SecretString = "token".into();
    for (status, expected_code) in [
        (302, "neodb_mark_identity_changed"),
        (301, "neodb_mark_unavailable"),
        (307, "neodb_mark_unavailable"),
        (308, "neodb_mark_unavailable"),
        (401, "neodb_mark_unauthorized"),
        (403, "neodb_mark_forbidden"),
        (429, "neodb_mark_rate_limited"),
        (500, "neodb_mark_unavailable"),
    ] {
        enqueue(
            &mock,
            status,
            Some("http://127.0.0.1:9/private-token-target"),
            "private-upstream-body".into(),
        );
        let error = read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &item,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, expected_code);
        assert!(!error.message.contains("private"));
    }
    assert_eq!(mock.seen.lock().unwrap().len(), 8);
    task.abort();
}

#[tokio::test]
async fn malformed_declared_and_chunked_oversize_responses_are_sanitized() {
    let (base, mock, task) = server().await;
    let item = expected(&base);
    let token: SecretString = "secret-marker".into();
    for body in [
        "private-body".into(),
        padded_mark(&base, RESPONSE_LIMIT + 1),
    ] {
        enqueue(&mock, 200, None, body);
        let error = read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &item,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "neodb_mark_unavailable");
        assert!(!error.message.contains("private-body"));
        assert!(!error.message.contains("secret-marker"));
        assert!(!error.message.contains(&base));
    }
    enqueue(&mock, 200, None, padded_mark(&base, RESPONSE_LIMIT));
    assert!(
        read(
            &NeoDbClient::new().unwrap(),
            base.clone(),
            &token,
            "alice",
            &item
        )
        .await
        .is_ok()
    );
    task.abort();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let raw_base = format!("http://{}", listener.local_addr().unwrap());
    let reply_base = raw_base.clone();
    let raw_task = tokio::task::spawn_blocking(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).unwrap() > 0);
        let body = padded_mark(&reply_base, RESPONSE_LIMIT + 1);
        let wire = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        );
        let _ = socket.write_all(wire.as_bytes());
    });
    assert_eq!(
        read(
            &NeoDbClient::new().unwrap(),
            raw_base.clone(),
            &token,
            "alice",
            &expected(&raw_base),
        )
        .await
        .unwrap_err()
        .code,
        "neodb_mark_unavailable"
    );
    raw_task.await.unwrap();
}

#[tokio::test]
async fn timeout_and_connection_failure_are_sanitized() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(any(|| async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        "private response"
    }));
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let client = NeoDbClient {
        http: Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap(),
    };
    let token: SecretString = "secret-marker".into();
    let started = Instant::now();
    let error = read(&client, base.clone(), &token, "alice", &expected(&base))
        .await
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(error.code, "neodb_mark_unavailable");
    assert!(!error.message.contains("secret-marker"));
    task.abort();
    let _ = task.await;
    assert_eq!(
        read(&client, base.clone(), &token, "alice", &expected(&base))
            .await
            .unwrap_err()
            .code,
        "neodb_mark_unavailable"
    );
}
