use super::*;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::Response as HttpResponse,
    routing::any,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const SOURCE: &str = "https://www.imdb.com/title/tt1234567/";

#[tokio::test]
async fn chunked_body_without_content_length_is_still_bounded() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::task::spawn_blocking(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).unwrap() > 0);
        let body = "x".repeat(RESPONSE_LIMIT + 1);
        let wire = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        );
        // The bounded client may close before the server finishes its write.
        let _ = socket.write_all(wire.as_bytes());
    });
    let error = NeoDbClient::new()
        .unwrap()
        .detail(base, DETAIL.into())
        .await
        .unwrap_err();
    assert_eq!(error.code, "neodb_catalog_unavailable");
    task.await.unwrap();
}

#[tokio::test]
async fn search_chunked_body_without_content_length_is_still_bounded() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::task::spawn_blocking(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).unwrap() > 0);
        let body = "x".repeat(RESPONSE_LIMIT + 1);
        let wire = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        );
        let _ = socket.write_all(wire.as_bytes());
    });
    assert_eq!(
        NeoDbClient::new()
            .unwrap()
            .search(base, "title".into(), "movie".into())
            .await
            .unwrap_err()
            .code,
        "neodb_catalog_unavailable"
    );
    task.await.unwrap();
}

#[tokio::test]
async fn detail_and_search_timeouts_and_connection_failure_are_sanitized() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(any(|| async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        "private response"
    }));
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let client = NeoDbClient::new().unwrap();
    let start = Instant::now();
    assert_eq!(
        client
            .detail(base.clone(), DETAIL.into())
            .await
            .unwrap_err()
            .code,
        "neodb_catalog_unavailable"
    );
    assert!(start.elapsed() < Duration::from_secs(15));
    let start = Instant::now();
    assert_eq!(
        client
            .search(base.clone(), "title".into(), "movie".into())
            .await
            .unwrap_err()
            .code,
        "neodb_catalog_unavailable"
    );
    assert!(start.elapsed() < Duration::from_secs(15));
    task.abort();
    let _ = task.await;
    assert_eq!(
        client.fetch(base, SOURCE.into()).await.unwrap_err().code,
        "neodb_catalog_unavailable"
    );
}

#[tokio::test]
async fn duplicate_location_headers_are_rejected() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(any(|| async {
        HttpResponse::builder()
            .status(302)
            .header("location", DETAIL)
            .header("location", "/api/me")
            .body(Body::empty())
            .unwrap()
    }));
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    assert_eq!(
        NeoDbClient::new()
            .unwrap()
            .fetch(base, SOURCE.into())
            .await
            .unwrap_err()
            .code,
        "neodb_catalog_reference_invalid"
    );
    task.abort();
}
const DETAIL: &str = "/api/movie/4W2ZHSBFHfCbRhdBUlHhBS";
type Reply = (u16, Option<String>, String);
#[derive(Clone, Default)]
struct Mock {
    replies: Arc<Mutex<VecDeque<Reply>>>,
    seen: Arc<Mutex<Vec<String>>>,
}
async fn handle(State(mock): State<Mock>, request: Request) -> HttpResponse<Body> {
    assert_eq!(request.method(), "GET");
    assert!(!request.headers().contains_key("authorization"));
    assert!(!request.headers().contains_key("cookie"));
    mock.seen.lock().unwrap().push(request.uri().to_string());
    let (status, location, body) = mock
        .replies
        .lock()
        .unwrap()
        .pop_front()
        .expect("unexpected request");
    let mut response = HttpResponse::builder().status(status);
    if let Some(value) = location {
        response = response.header("location", value);
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
fn detail(base: &str, path: &str) -> serde_json::Value {
    let (media, model, uuid) = item_path(path).unwrap();
    serde_json::json!({
        "id": format!("{base}{}", path.strip_prefix("/api").unwrap()),
        "uuid": uuid, "api_url": path, "category": media, "type": model,
        "parent_uuid": null, "imdb": "tt1234567",
        "external_resources": [{"url": SOURCE}, {"url": "https://www.imdb.com/title/tt7654321/"}, {"url":"http://127.0.0.1/private"}],
        "title": "not retained", "description":"not retained"
    })
}

fn search_reply(base: &str, paths: &[&str], pages: i64, count: i64) -> serde_json::Value {
    serde_json::json!({
        "data": paths.iter().map(|path| detail(base, path)).collect::<Vec<_>>(),
        "pages": pages,
        "count": count
    })
}

#[tokio::test]
async fn fetch_is_single_attempt_public_and_has_no_refetch_parameter() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for (status, expected) in [
        (202, CatalogFetch::Pending),
        (422, CatalogFetch::Unsupported),
        (429, CatalogFetch::RateLimited),
        (404, CatalogFetch::Unavailable),
    ] {
        enqueue(&mock, status, None, "upstream-private-content".into());
        assert_eq!(
            client.fetch(base.clone(), SOURCE.into()).await.unwrap(),
            expected
        );
    }
    enqueue(&mock, 302, Some(DETAIL), String::new());
    assert_eq!(
        client.fetch(base.clone(), SOURCE.into()).await.unwrap(),
        CatalogFetch::Detail(format!("{base}{DETAIL}"))
    );
    let seen = mock.seen.lock().unwrap();
    assert_eq!(seen.len(), 5);
    for request in seen.iter() {
        let url = Url::parse(&format!("{base}{request}")).unwrap();
        assert_eq!(url.path(), "/api/catalog/fetch");
        assert_eq!(
            url.query_pairs().collect::<Vec<_>>(),
            [("url".into(), SOURCE.into())]
        );
    }
    task.abort();
}

#[tokio::test]
async fn search_is_one_public_first_page_request_and_normalizes_review_evidence() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    let mut body = search_reply(&base, &["/api/tv/abc", "/api/tv/season/def"], 2, 9);
    body["data"][0]["title"] = "  Show title  ".into();
    body["data"][1]["title"] = "Season title".into();
    enqueue(&mock, 200, None, body.to_string());
    let page = client
        .search(base.clone(), "  Search title  ".into(), "tv".into())
        .await
        .unwrap();
    assert!(page.more_available);
    assert_eq!(page.candidates.len(), 2);
    assert_eq!(page.candidates[0].title, "Show title");
    assert_eq!(page.candidates[0].item.model, "TVShow");
    assert_eq!(page.candidates[1].item.model, "TVSeason");
    assert_eq!(
        page.candidates[0].item.external_ids,
        [
            ("imdb:title".into(), "tt1234567".into()),
            ("imdb:title".into(), "tt7654321".into())
        ]
    );
    let seen = mock.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    let url = Url::parse(&format!("{base}{}", seen[0])).unwrap();
    assert_eq!(url.path(), "/api/catalog/search");
    assert_eq!(
        url.query_pairs().collect::<Vec<_>>(),
        [
            ("query".into(), "Search title".into()),
            ("category".into(), "tv".into()),
            ("page".into(), "1".into())
        ]
    );
    task.abort();
}

#[tokio::test]
async fn search_validates_query_and_category_before_io() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for (query, category, code) in [
        ("", "movie", "catalog_search_query_invalid"),
        ("\ntitle", "movie", "catalog_search_query_invalid"),
        ("title", "book", "neodb_catalog_category_invalid"),
    ] {
        assert_eq!(
            client
                .search(base.clone(), query.into(), category.into())
                .await
                .unwrap_err()
                .code,
            code
        );
    }
    assert_eq!(
        client
            .search(base, format!("{}x", " ".repeat(512)), "movie".into())
            .await
            .unwrap_err()
            .code,
        "catalog_search_query_invalid"
    );
    assert!(mock.seen.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn search_rejects_malformed_or_unsafe_pages_atomically() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    let mut cases = vec![
        serde_json::json!({}),
        serde_json::json!({"data": [], "pages": -1, "count": 0}),
        serde_json::json!({"data": [], "pages": 0, "count": -1}),
        serde_json::json!({"data": [], "pages": 1.5, "count": 0}),
        serde_json::json!({"data": [], "pages": 1, "count": "0"}),
        serde_json::json!({
            "data": (0..21).map(|index| {
                let path = format!("/api/movie/Cap{index}");
                detail(&base, &path)
            }).collect::<Vec<_>>(),
            "pages": 1,
            "count": 21
        }),
    ];
    let mut duplicate = search_reply(&base, &[DETAIL, DETAIL], 1, 2);
    duplicate["data"][0]["title"] = "One".into();
    duplicate["data"][1]["title"] = "Two".into();
    cases.push(duplicate);
    let mut partial = search_reply(&base, &[DETAIL, "/api/movie/def"], 1, 2);
    partial["data"][0]["title"] = "Valid first".into();
    partial["data"][1]["title"] = "".into();
    cases.push(partial);
    for (key, value) in [
        ("title", serde_json::json!("")),
        ("title", serde_json::json!("bad\ntitle")),
        ("title", serde_json::json!("x".repeat(1025))),
        (
            "api_url",
            serde_json::json!("https://evil.example/api/movie/abc"),
        ),
        ("uuid", serde_json::json!("wrong")),
        ("category", serde_json::json!("tv")),
        ("type", serde_json::json!("TVShow")),
        ("id", serde_json::json!("https://evil.example/movie/abc")),
    ] {
        let mut body = search_reply(&base, &[DETAIL], 1, 1);
        body["data"][0]["title"] = "Title".into();
        body["data"][0][key] = value;
        cases.push(body);
    }
    cases.push(serde_json::json!({
        "data": [], "pages": 1, "count": 0, "padding": "x".repeat(RESPONSE_LIMIT)
    }));
    for body in cases {
        enqueue(&mock, 200, None, body.to_string());
        assert!(
            client
                .search(base.clone(), "title".into(), "movie".into())
                .await
                .is_err()
        );
    }
    task.abort();
}

#[tokio::test]
async fn search_estimates_empty_pages_without_claiming_absence() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for (pages, count, expected_more) in [(0, 0, false), (1, 0, false), (1, 3, true)] {
        enqueue(
            &mock,
            200,
            None,
            search_reply(&base, &[], pages, count).to_string(),
        );
        let page = client
            .search(base.clone(), "title".into(), "movie".into())
            .await
            .unwrap();
        assert!(page.candidates.is_empty());
        assert_eq!(page.more_available, expected_more);
    }
    task.abort();
}

#[tokio::test]
async fn search_accepts_exactly_twenty_unique_candidates() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    let data = (0..20)
        .map(|index| {
            let path = format!("/api/movie/Item{index}");
            detail(&base, &path)
        })
        .collect::<Vec<_>>();
    enqueue(
        &mock,
        200,
        None,
        serde_json::json!({"data": data, "pages": 1, "count": 20}).to_string(),
    );
    let page = client
        .search(base, "title".into(), "movie".into())
        .await
        .unwrap();
    assert_eq!(page.candidates.len(), 20);
    assert!(!page.more_available);
    task.abort();
}

#[tokio::test]
async fn search_does_not_follow_redirect_retry_or_expose_bodies() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for (status, expected_code) in [
        (302, "neodb_catalog_unavailable"),
        (429, "neodb_catalog_rate_limited"),
        (500, "neodb_catalog_unavailable"),
    ] {
        enqueue(
            &mock,
            status,
            Some("/api/catalog/search?query=stolen&category=movie&page=1"),
            "private-upstream-body".into(),
        );
        let error = client
            .search(base.clone(), "title".into(), "movie".into())
            .await
            .unwrap_err();
        assert_eq!(error.code, expected_code);
        assert!(!error.message.contains("private-upstream-body"));
    }
    assert_eq!(mock.seen.lock().unwrap().len(), 3);
    task.abort();
}

#[tokio::test]
async fn rejects_source_urls_before_io() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for source in [
        "http://127.0.0.1/private",
        "https://www.imdb.com@evil.example/title/tt1234567/",
        "https://www.imdb.com/title/tt1234567/?x=1",
        "https://www.imdb.com/title/tt1234567/#x",
        "https://www.imdb.com/title/tt123/",
        "https://www.imdb.com/title/../title/tt1234567/",
        "https://www.imdb.com/title/%74t1234567/",
        "https://www.imdb.com/title/tt1234567/extra",
        "https://m.imdb.com/title/tt1234567/",
    ] {
        assert_eq!(
            client
                .fetch(base.clone(), source.into())
                .await
                .unwrap_err()
                .code,
            "catalog_source_url_invalid"
        );
    }
    assert!(mock.seen.lock().unwrap().is_empty());
    task.abort();
}

#[tokio::test]
async fn rejects_unsafe_references_for_fetch_and_direct_detail_without_following() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for reference in [
        "https://evil.example/api/movie/abc",
        "//evil.example/api/movie/abc",
        "/api/me",
        "/api/movie/abc?key=secret",
        "/api/movie/abc#fragment",
        "/api/movie/%61bc",
        "/api/../api/movie/abc",
        "/api\\movie/abc",
        "/api/movie/abc/credit/",
        "api/movie/abc",
        "/api/movie/abc/",
        "/api/movie/abc-def",
    ] {
        let count = mock.seen.lock().unwrap().len();
        assert_eq!(
            client
                .detail(base.clone(), reference.into())
                .await
                .unwrap_err()
                .code,
            "neodb_catalog_reference_invalid"
        );
        assert_eq!(mock.seen.lock().unwrap().len(), count);
        enqueue(&mock, 302, Some(reference), String::new());
        assert_eq!(
            client
                .fetch(base.clone(), SOURCE.into())
                .await
                .unwrap_err()
                .code,
            "neodb_catalog_reference_invalid"
        );
        assert_eq!(mock.seen.lock().unwrap().len(), count + 1);
    }
    for reference in [
        format!("{base}/api/movie/abc"),
        format!("{base}/api/tv/season/abc"),
    ] {
        assert!(detail_url(&canonical(&base).unwrap(), &reference).is_ok());
    }
    let origin = canonical("https://neodb.example").unwrap();
    for reference in [
        "http://neodb.example/api/movie/abc",
        "https://neodb.example:8443/api/movie/abc",
        "https://user@neodb.example/api/movie/abc",
    ] {
        assert!(detail_url(&origin, reference).is_err());
    }
    task.abort();
}

#[tokio::test]
async fn detail_preserves_granularity_and_conflicting_supported_ids_only() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for path in [
        DETAIL,
        "/api/tv/abc",
        "/api/tv/season/abc",
        "/api/tv/episode/abc",
    ] {
        enqueue(&mock, 200, None, detail(&base, path).to_string());
        let item = client.detail(base.clone(), path.into()).await.unwrap();
        assert_eq!(item.model, item_path(path).unwrap().1);
        assert_eq!(
            item.external_ids,
            [
                ("imdb:title".into(), "tt1234567".into()),
                ("imdb:title".into(), "tt7654321".into())
            ]
        );
        assert_eq!(item.detail_url, format!("{base}{path}"));
    }
    task.abort();
}

#[tokio::test]
async fn validates_every_redirect_and_bounds_hops_and_cycles() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    enqueue(&mock, 302, Some("/api/tv/abc"), String::new());
    enqueue(&mock, 200, None, detail(&base, "/api/tv/abc").to_string());
    assert_eq!(
        client
            .detail(base.clone(), DETAIL.into())
            .await
            .unwrap()
            .model,
        "TVShow"
    );
    enqueue(&mock, 302, Some(DETAIL), String::new());
    assert_eq!(
        client
            .detail(base.clone(), DETAIL.into())
            .await
            .unwrap_err()
            .code,
        "neodb_catalog_reference_invalid"
    );
    enqueue(&mock, 302, Some("/api/me"), String::new());
    assert!(client.detail(base.clone(), DETAIL.into()).await.is_err());
    for target in [
        "/api/movie/a",
        "/api/movie/b",
        "/api/movie/c",
        "/api/movie/d",
    ] {
        enqueue(&mock, 302, Some(target), String::new());
    }
    assert!(client.detail(base.clone(), DETAIL.into()).await.is_err());
    assert_eq!(mock.seen.lock().unwrap().len(), 8);
    task.abort();
}

#[tokio::test]
async fn rejects_malformed_oversize_and_inconsistent_evidence() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for body in [
        "private upstream error".into(),
        "x".repeat(RESPONSE_LIMIT + 1),
        "{}".into(),
    ] {
        enqueue(&mock, 200, None, body);
        let error = client
            .detail(base.clone(), DETAIL.into())
            .await
            .unwrap_err();
        assert_eq!(error.code, "neodb_catalog_unavailable");
        assert!(!error.message.contains("private"));
    }
    for (key, value) in [
        ("uuid", "wrong"),
        ("id", "https://evil.example/movie/abc"),
        ("api_url", "/api/movie/other"),
        ("category", "book"),
        ("type", "TVSeason"),
        ("imdb", "not-an-id"),
        ("parent_uuid", "../bad"),
    ] {
        let mut reply = detail(&base, DETAIL);
        reply[key] = value.into();
        enqueue(&mock, 200, None, reply.to_string());
        assert!(
            client.detail(base.clone(), DETAIL.into()).await.is_err(),
            "{key}"
        );
    }
    let mut missing = detail(&base, DETAIL);
    missing["imdb"] = serde_json::Value::Null;
    missing["external_resources"] = serde_json::Value::Null;
    enqueue(&mock, 200, None, missing.to_string());
    assert!(
        client
            .detail(base.clone(), DETAIL.into())
            .await
            .unwrap()
            .external_ids
            .is_empty()
    );
    task.abort();
}

#[tokio::test]
async fn unexpected_statuses_and_missing_location_are_sanitized() {
    let (base, mock, task) = server().await;
    let client = NeoDbClient::new().unwrap();
    for status in [200, 301, 307, 308, 401, 403, 500] {
        enqueue(&mock, status, Some("/api/me"), "secret-body".into());
        let error = client.fetch(base.clone(), SOURCE.into()).await.unwrap_err();
        assert_eq!(error.code, "neodb_catalog_unavailable");
        assert!(!error.message.contains("secret-body"));
    }
    enqueue(&mock, 302, None, "secret-body".into());
    assert_eq!(
        client
            .fetch(base.clone(), SOURCE.into())
            .await
            .unwrap_err()
            .code,
        "neodb_catalog_reference_invalid"
    );
    for status in [202, 404, 422, 429, 500] {
        enqueue(&mock, status, None, "secret-body".into());
        let error = client
            .detail(base.clone(), DETAIL.into())
            .await
            .unwrap_err();
        assert_eq!(
            error.code,
            if status == 429 {
                "neodb_catalog_rate_limited"
            } else {
                "neodb_catalog_unavailable"
            }
        );
    }
    task.abort();
}
