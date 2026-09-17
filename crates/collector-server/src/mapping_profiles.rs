//! Bounded configuration codec; no routes or persistence in this slice.
use collector_domain::{ApiError, mapping::MappingProfile};

pub use collector_domain::mapping::MAX_PROFILE_BYTES;

#[derive(Clone, Copy)]
pub enum Format {
    Json,
    Yaml,
}

fn invalid(format: Format) -> ApiError {
    ApiError::new(
        match format {
            Format::Json => "mapping_json_invalid",
            Format::Yaml => "mapping_yaml_invalid",
        },
        "Mapping document does not match the bounded declarative schema.",
    )
}

pub fn decode(input: &str, format: Format) -> Result<MappingProfile, ApiError> {
    if input.len() > MAX_PROFILE_BYTES {
        return Err(ApiError::new(
            "mapping_document_too_large",
            "Mapping document exceeds 64 KiB.",
        ));
    }
    let options = serde_saphyr::options! {
        reject_unsupported_tags: true,
        duplicate_keys: serde_saphyr::options::DuplicateKeyPolicy::Error,
        merge_keys: serde_saphyr::options::MergeKeyPolicy::Error,
        budget: serde_saphyr::budget! {
            max_depth: 16,
            max_documents: 1,
            max_aliases: 0,
            max_anchors: 0,
            max_merge_keys: 0,
            max_events: 20_000,
            max_nodes: 10_000,
            max_total_scalar_bytes: MAX_PROFILE_BYTES,
        },
    };
    // YAML's duplicate-key policy also covers nested enum maps in JSON, where
    // serde_json's normal BTreeMap deserialization would otherwise keep the last.
    let profile: MappingProfile =
        serde_saphyr::from_str_with_options(input, options).map_err(|_| invalid(format))?;
    if matches!(format, Format::Json) {
        let json: MappingProfile = serde_json::from_str(input).map_err(|_| invalid(format))?;
        if json != profile {
            return Err(invalid(format));
        }
    }
    collector_core::mapping::validate(&profile)?;
    Ok(profile)
}

pub fn encode(profile: &MappingProfile, format: Format) -> Result<String, ApiError> {
    collector_core::mapping::validate(profile)?;
    let output = match format {
        Format::Json => serde_json::to_string_pretty(profile).map_err(|_| invalid(format))?,
        Format::Yaml => serde_saphyr::to_string(profile).map_err(|_| invalid(format))?,
    };
    // Export must be re-importable under the exact same budgets and semantics.
    if decode(&output, format)? != *profile {
        return Err(invalid(format));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOLDEN: &str = r#"schemaVersion: 1
id: imdb_ratings
version: 1
sourceConnectorId: imdb:ratings_csv
sinkConnectorId: neodb
enabled: false
rules:
  - id: rating
    enabled: true
    target: rating_grade
    operation: {kind: clamp, from: rating, min: 0, max: 10}
  - id: rating_implies_complete
    enabled: false
    target: shelf_type
    operation:
      kind: condition
      from: rating
      test: {kind: exists}
      value: complete
"#;

    #[test]
    fn golden_and_both_exports_normalize_to_one_profile() {
        let profile = collector_core::mapping::imdb_default();
        assert!(decode(GOLDEN, Format::Yaml).unwrap() == profile);
        for format in [Format::Yaml, Format::Json] {
            let text = encode(&profile, format).unwrap();
            assert!(decode(&text, format).unwrap() == profile);
            assert!(encode(&decode(&text, format).unwrap(), format).unwrap() == text);
        }
        assert!(decode(GOLDEN, Format::Json).is_err());
    }

    #[test]
    fn rejects_unknown_fields_code_activation_and_duplicate_keys() {
        for input in [
            format!("{GOLDEN}\nunknown: true"),
            GOLDEN.replace("kind: clamp", "kind: javascript"),
            GOLDEN.replace("min: 0", "unknown: 0"),
            GOLDEN.replace("version: 1", "version: 0"),
            GOLDEN.replacen("enabled: false", "enabled: true", 1),
            GOLDEN.replace("id: imdb_ratings", "id: first\nid: second"),
            GOLDEN.replace(
                "{kind: clamp, from: rating, min: 0, max: 10}",
                "{kind: enum_map, from: rating, values: {done: 1, done: 2}}",
            ),
        ] {
            assert!(decode(&input, Format::Yaml).is_err());
        }
        let json = encode(&collector_core::mapping::imdb_default(), Format::Json).unwrap();
        let duplicate = json.replace(
            "\"schemaVersion\": 1",
            "\"schemaVersion\": 1, \"schemaVersion\": 1",
        );
        assert!(decode(&duplicate, Format::Json).is_err());
        let mut profile = collector_core::mapping::imdb_default();
        profile.rules[0].operation = collector_domain::mapping::Operation::EnumMap {
            from: "status".into(),
            values: std::collections::BTreeMap::from([(
                "done".into(),
                collector_domain::observation::ScalarValue::Number(1.0),
            )]),
        };
        let json = encode(&profile, Format::Json)
            .unwrap()
            .replace("\"done\": 1.0", "\"done\": 1.0, \"done\": 2.0");
        assert!(decode(&json, Format::Json).is_err());
    }

    #[test]
    fn rejects_yaml_expansion_tags_multiple_documents_and_budgets() {
        for input in [
            GOLDEN.replace("id: imdb_ratings", "id: &id imdb_ratings"),
            GOLDEN.replace("id: imdb_ratings", "id: *missing"),
            GOLDEN.replace("id: imdb_ratings", "id: !include private.yaml"),
            GOLDEN.replace("id: imdb_ratings", "id: !execute arbitrary"),
            format!("{GOLDEN}\n---\n{GOLDEN}"),
            format!("{GOLDEN}\n<<: {{id: other}}"),
            format!("{}0{}", "[".repeat(20), "]".repeat(20)),
            " ".repeat(MAX_PROFILE_BYTES + 1),
        ] {
            assert!(decode(&input, Format::Yaml).is_err());
        }
    }
}
