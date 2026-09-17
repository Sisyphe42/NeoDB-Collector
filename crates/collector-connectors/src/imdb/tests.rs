use super::*;
const NOW: &str = "2026-09-17T00:00:00Z";
fn parse(text: &str) -> Result<ImportBatch, ImportError> {
    parse_ratings(text.as_bytes(), NOW)
}
fn failure(text: &str) -> ImportError {
    parse(text).err().expect("must reject")
}

#[test]
fn reordered_bom_multiline_and_unsupported_values_are_normalized() {
    let batch = parse("\u{feff}Title Type,Title,Your Rating,Const,Date Rated,IMDb Rating,URL\r\nmovie,\"Hello, \"\"world\"\"\n再见\",8,tt1234567,2020-01-01,2.0,https://untrusted.example\r\ntvSeries,Series,9,tt1234568,2020-01-02,1.0,ignored\r\ntvEpisode,Episode,10,tt1234569,2020-01-03,9.9,ignored\r\n").unwrap();
    assert_eq!(batch.observations.len(), 2);
    let one = &batch.observations[0];
    assert_eq!(
        one.item.title_hint.as_deref(),
        Some("Hello, \"world\"\n再见")
    );
    assert_eq!(
        one.item.canonical_url,
        "https://www.imdb.com/title/tt1234567/"
    );
    assert_eq!(one.item.external_ids["imdb:title"], "tt1234567");
    assert!(matches!(one.state["rating"], ScalarValue::Number(8.0)));
    assert_eq!(one.state.len(), 1);
    assert_eq!(batch.observations[1].item.media_type, "tv");
    assert_eq!(batch.skipped_rows[0].line, 5);
    assert_eq!(batch.skipped_columns, ["Date Rated", "IMDb Rating", "URL"]);
    let encoded = serde_json::to_string(&batch.observations).unwrap();
    assert!(!encoded.contains("2020-01") && !encoded.contains("untrusted"));
}

#[test]
fn supported_types_and_rating_limits() {
    for (kind, expected) in [
        ("movie", "movie"),
        ("tvMovie", "movie"),
        ("short", "movie"),
        ("tvSeries", "tv"),
        ("tvMiniSeries", "tv"),
    ] {
        for rating in [1, 10] {
            let batch = parse(&format!(
                "Const,Your Rating,Title Type\ntt1234567,{rating},{kind}\n"
            ))
            .unwrap();
            assert_eq!(batch.observations[0].item.media_type, expected);
        }
    }
    for rating in ["", "0", "11", "-1", "8.5", "NaN", "+8", "08", " 8"] {
        let error = failure(&format!(
            "Const,Your Rating,Title Type\ntt1234567,{rating},movie\n"
        ));
        assert_eq!(error.code, "imdb_rating_invalid");
        assert_eq!(error.line, 2);
    }
}

#[test]
fn duplicate_semantics_and_fingerprints_are_stable() {
    let batch = parse("Const,Your Rating,Title Type,Title\ntt1234567,8,movie,Original\ntt1234567,8,movie,Other hint\n").unwrap();
    assert_eq!(batch.observations.len(), 1);
    assert_eq!(batch.duplicate_rows, 1);
    let other = parse_ratings(
        b"Title Type,Const,Your Rating\nmovie,tt1234567,8\n",
        "later",
    )
    .unwrap();
    assert_eq!(
        batch.observations[0].idempotency_key,
        other.observations[0].idempotency_key
    );
    for row in ["tt1234567,9,movie", "tt1234567,8,tvSeries"] {
        assert_eq!(
            failure(&format!(
                "Const,Your Rating,Title Type\ntt1234567,8,movie\n{row}\n"
            ))
            .code,
            "csv_duplicate_conflict"
        );
    }
}

#[test]
fn malformed_structure_headers_encoding_and_identity_fail_without_raw_details() {
    for separator in ["\n", "\r\n", "\r"] {
        let text = format!(
            "Const,Your Rating,Title Type{separator}tt1234567,8,movie{separator}private-id,8,movie{separator}"
        );
        assert_eq!(failure(&text).line, 3);
    }
    for csv in [
        "Const,Your Rating,Title Type\ntt1234567,8,\"movie",
        "Const,Your Rating,Title Type\ntt1234567,8,mo\"vie",
        "Const,Your Rating,Title Type\ntt1234567,8,\"movie\"trailing",
        "Const,Your Rating,Title Type\ntt1234567,8",
        "Const,Your Rating,Title Type\ntt1234567,8,movie,extra",
    ] {
        assert_eq!(failure(csv).code, "csv_structure_invalid");
    }
    assert_eq!(
        failure("Const,Const,Title Type\n").code,
        "csv_header_invalid"
    );
    assert_eq!(
        failure("Const,IMDb Rating,Title Type\n").code,
        "csv_column_missing"
    );
    assert_eq!(
        parse_ratings(&[255], NOW).err().unwrap().code,
        "csv_encoding_invalid"
    );
    let error = failure("Const,Your Rating,Title Type\nprivate-id,8,movie\n");
    assert_eq!(error.code, "imdb_id_invalid");
    assert!(!format!("{error:?}").contains("private-id"));
    assert!(
        parse("Const,Your Rating,Title Type\n")
            .unwrap()
            .observations
            .is_empty()
    );
}

#[test]
fn byte_and_row_limits_include_skipped_and_duplicate_rows() {
    assert_eq!(
        parse_ratings(&vec![b'x'; MAX_BYTES + 1], NOW)
            .err()
            .unwrap()
            .code,
        "csv_too_large"
    );
    for row in ["tt1234567,8,movie\n", "tt1234567,8,tvEpisode\n"] {
        let mut text = format!("Const,Your Rating,Title Type\n{}", row.repeat(MAX_ROWS));
        assert!(parse(&text).is_ok());
        text.push_str(row);
        assert_eq!(failure(&text).code, "csv_too_many_rows");
    }
}
