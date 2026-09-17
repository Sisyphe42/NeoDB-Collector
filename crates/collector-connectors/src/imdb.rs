//! Bounded IMDb ratings archive reader. No network or persistence.
use collector_domain::observation::{Observation, ObservedItem, ScalarValue};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const CONNECTOR_ID: &str = "imdb:ratings_csv";
pub const MAX_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_ROWS: usize = 10_000;

#[derive(Debug, PartialEq, Eq)]
pub struct ImportError {
    pub code: &'static str,
    pub line: u64,
    pub field: Option<&'static str>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct SkippedRow {
    pub line: u64,
    pub reason: &'static str,
}
pub struct ImportBatch {
    pub observations: Vec<Observation>,
    pub skipped_rows: Vec<SkippedRow>,
    // Header names only; no unsupported cell values are retained.
    pub skipped_columns: Vec<String>,
    pub duplicate_rows: usize,
}
fn invalid(code: &'static str, line: u64, field: Option<&'static str>) -> ImportError {
    ImportError { code, line, field }
}

// csv intentionally tolerates malformed quoting. Reject it before record parsing.
fn quotes(bytes: &[u8]) -> Result<(), ImportError> {
    enum State {
        Start,
        Bare,
        Quoted,
        End,
    }
    let mut state = State::Start;
    let mut line = 1;
    for (index, &byte) in bytes.iter().enumerate() {
        state = match (&state, byte) {
            (State::Start, b'"') => State::Quoted,
            (State::Start | State::Bare, b',' | b'\r' | b'\n') => State::Start,
            (State::Bare, b'"') => return Err(invalid("csv_structure_invalid", line, None)),
            (State::Start | State::Bare, _) => State::Bare,
            (State::Quoted, b'"') => State::End,
            (State::Quoted, _) => State::Quoted,
            (State::End, b'"') => State::Quoted,
            (State::End, b',' | b'\r' | b'\n') => State::Start,
            (State::End, _) => return Err(invalid("csv_structure_invalid", line, None)),
        };
        if byte == b'\n' || (byte == b'\r' && bytes.get(index + 1) != Some(&b'\n')) {
            line += 1;
        }
    }
    if matches!(state, State::Quoted) {
        return Err(invalid("csv_structure_invalid", line, None));
    }
    Ok(())
}

fn physical_line(text: &str, position: Option<&csv::Position>, cursor: &mut (usize, u64)) -> u64 {
    let mut offset = position.map_or(0, |p| p.byte() as usize);
    // csv may leave a CRLF's LF at the start position of the next record.
    while matches!(text.as_bytes().get(offset), Some(b'\r' | b'\n')) {
        offset += 1;
    }
    while cursor.0 < offset {
        let byte = text.as_bytes()[cursor.0];
        if byte == b'\n' || (byte == b'\r' && text.as_bytes().get(cursor.0 + 1) != Some(&b'\n')) {
            cursor.1 += 1;
        }
        cursor.0 += 1;
    }
    cursor.1
}

/// observed_at is a trusted ingestion timestamp supplied by the caller, not CSV.
/// All-or-nothing: callers receive no partial batch when validation fails.
pub fn parse_ratings(input: &[u8], observed_at: &str) -> Result<ImportBatch, ImportError> {
    if input.len() > MAX_BYTES {
        return Err(invalid("csv_too_large", 1, None));
    }
    let text = std::str::from_utf8(input).map_err(|_| invalid("csv_encoding_invalid", 1, None))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    quotes(text.as_bytes())?;
    let mut line_cursor = (0, 1);
    let mut reader = csv::ReaderBuilder::new().from_reader(text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|_| invalid("csv_structure_invalid", 1, None))?
        .clone();
    let mut names = HashSet::new();
    if headers.iter().any(|h| h.is_empty() || !names.insert(h)) {
        return Err(invalid("csv_header_invalid", 1, None));
    }
    let column = |name| {
        headers
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| invalid("csv_column_missing", 1, Some(name)))
    };
    let id_col = column("Const")?;
    let rating_col = column("Your Rating")?;
    let type_col = column("Title Type")?;
    let title_col = headers.iter().position(|h| h == "Title");
    let mut batch = ImportBatch {
        observations: Vec::new(),
        skipped_rows: Vec::new(),
        duplicate_rows: 0,
        skipped_columns: headers
            .iter()
            .filter(|h| !["Const", "Your Rating", "Title Type", "Title"].contains(h))
            .map(str::to_owned)
            .collect(),
    };
    let mut seen = HashMap::<String, (String, u8)>::new();
    for (index, record) in reader.records().enumerate() {
        let record = record.map_err(|e| {
            invalid(
                "csv_structure_invalid",
                physical_line(text, e.position(), &mut line_cursor),
                None,
            )
        })?;
        let line = physical_line(text, record.position(), &mut line_cursor);
        if index >= MAX_ROWS {
            return Err(invalid("csv_too_many_rows", line, None));
        }
        let media_type = match &record[type_col] {
            "movie" | "tvMovie" | "short" => "movie",
            "tvSeries" | "tvMiniSeries" => "tv",
            _ => {
                batch.skipped_rows.push(SkippedRow {
                    line,
                    reason: "media_type_unsupported",
                });
                continue;
            }
        };
        let id = &record[id_col];
        let digits = id.strip_prefix("tt").unwrap_or("");
        if digits.len() < 7 || digits.len() > 16 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid("imdb_id_invalid", line, Some("Const")));
        }
        let raw_rating = &record[rating_col];
        let rating = raw_rating
            .parse::<u8>()
            .ok()
            .filter(|n| (1..=10).contains(n) && n.to_string() == raw_rating)
            .ok_or_else(|| invalid("imdb_rating_invalid", line, Some("Your Rating")))?;
        if let Some((previous_type, previous_rating)) = seen.get(id) {
            if previous_type != media_type || *previous_rating != rating {
                return Err(invalid("csv_duplicate_conflict", line, Some("Const")));
            }
            batch.duplicate_rows += 1;
            continue;
        }
        seen.insert(id.into(), (media_type.into(), rating));
        let title_hint = title_col
            .map(|c| record[c].trim())
            .filter(|t| !t.is_empty())
            .map(str::to_owned);
        if title_hint.as_ref().is_some_and(|s| s.len() > 4096) {
            return Err(invalid("imdb_title_too_long", line, Some("Title")));
        }
        // Length-prefixed fields avoid collisions from ambiguous concatenation.
        let mut digest = Sha256::new();
        for part in [CONNECTOR_ID, id, media_type, raw_rating] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part.as_bytes());
        }
        batch.observations.push(Observation {
            schema_version: 1,
            source_connector_id: CONNECTOR_ID.into(),
            source_record_id: id.into(),
            observed_at: observed_at.into(),
            idempotency_key: format!("{CONNECTOR_ID}:{:x}", digest.finalize()),
            item: ObservedItem {
                media_type: media_type.into(),
                canonical_url: format!("https://www.imdb.com/title/{id}/"),
                external_ids: BTreeMap::from([("imdb:title".into(), id.into())]),
                title_hint,
            },
            state: BTreeMap::from([("rating".into(), ScalarValue::Number(rating.into()))]),
        });
    }
    Ok(batch)
}

#[cfg(test)]
mod tests;
