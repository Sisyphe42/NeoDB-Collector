//! Authenticated, bounded preview API calls for the CLI.
use super::{error, health, http, origin, reference, unavailable};
use collector_core::access::SecretStore;
use collector_core::secrets::{ExposeSecret, SecretString};
use collector_domain::{
    ApiError,
    preview::{
        PreviewAccepted, PreviewAdmissionFailure, PreviewCancelRequest,
        PreviewImdbResolutionRequest, PreviewImportAccepted, PreviewJobView, PreviewMutationResult,
        PreviewOutcomeView, PreviewResolutionChoice, PreviewResultPage, PreviewResumeRequest,
    },
};
use reqwest::{Client, Response, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::File,
    io::{IsTerminal, Read},
    path::Path,
    time::Duration,
};
use url::Url;

pub const MAX_IMPORT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_PREVIEW_RECORDS: usize = 10_000;
const NORMAL_RESPONSE_BYTES: usize = 64 * 1024;
const RESULTS_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const ADMISSION_UNCERTAIN: &str =
    "Preview admission outcome is uncertain; inspect status before acting again.";

pub enum PreviewFailure {
    Error(ApiError),
    Admission(PreviewAdmissionFailure),
}

impl std::fmt::Debug for PreviewFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error(error) => formatter.debug_tuple("Error").field(&error.code).finish(),
            Self::Admission(value) => formatter
                .debug_struct("Admission")
                .field("job_id", &value.job_id)
                .field("attempt", &value.attempt)
                .field("code", &value.error.code)
                .finish(),
        }
    }
}

impl From<ApiError> for PreviewFailure {
    fn from(value: ApiError) -> Self {
        Self::Error(value)
    }
}

pub fn read_import_file(path: &str) -> Result<Vec<u8>, ApiError> {
    if path == "-" {
        if std::io::stdin().is_terminal() {
            return Err(error(
                "preview_input_required",
                "Pipe CSV data to standard input when FILE is '-'.",
            ));
        }
        return read_import(std::io::stdin().lock());
    }
    let file = File::open(Path::new(path)).map_err(|_| input_invalid())?;
    read_import(file)
}

pub fn parse_resolution_inputs_path(value: &str) -> Result<String, String> {
    if value == "-" {
        Err("must be a file path; '-' is not supported".into())
    } else {
        Ok(value.to_owned())
    }
}

/// Validates the literal-loopback boundary without performing health or credential I/O.
pub fn validate_origin(input: &str) -> Result<(), ApiError> {
    origin(input).map(drop)
}

pub fn read_resolution_request(
    csv_path: &str,
    resolution_inputs_path: &str,
) -> Result<Vec<u8>, ApiError> {
    let csv = String::from_utf8(read_import_file(csv_path)?).map_err(|_| {
        error(
            "preview_input_encoding_invalid",
            "CSV input is not valid UTF-8.",
        )
    })?;
    let inputs = read_resolution_inputs_file(resolution_inputs_path)?;
    encode_resolution_request(csv, inputs)
}

fn read_resolution_inputs_file(path: &str) -> Result<Vec<PreviewResolutionChoice>, ApiError> {
    let file = File::open(Path::new(path)).map_err(|_| resolution_inputs_invalid())?;
    read_resolution_inputs(file)
}

pub(crate) fn read_resolution_inputs(
    reader: impl Read,
) -> Result<Vec<PreviewResolutionChoice>, ApiError> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_IMPORT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| resolution_inputs_invalid())?;
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(error(
            "preview_resolution_inputs_too_large",
            "Resolution inputs exceed the 10 MiB limit.",
        ));
    }
    serde_json::from_slice::<Vec<PreviewResolutionChoice>>(&bytes)
        .map_err(|_| resolution_inputs_invalid())
}

pub(crate) fn encode_resolution_request(
    csv: String,
    resolution_inputs: Vec<PreviewResolutionChoice>,
) -> Result<Vec<u8>, ApiError> {
    if resolution_inputs.len() > MAX_PREVIEW_RECORDS {
        return Err(error(
            "preview_resolution_inputs_too_many",
            "Resolution inputs exceed the 10,000 choice limit.",
        ));
    }
    let bytes = serde_json::to_vec(&PreviewImdbResolutionRequest {
        csv,
        resolution_inputs,
    })
    .map_err(|_| resolution_inputs_invalid())?;
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(error(
            "preview_request_too_large",
            "Encoded preview request exceeds the 10 MiB limit.",
        ));
    }
    Ok(bytes)
}

fn resolution_inputs_invalid() -> ApiError {
    error(
        "preview_resolution_inputs_invalid",
        "Resolution inputs must be a valid UTF-8 JSON array.",
    )
}

pub(crate) fn read_import(reader: impl Read) -> Result<Vec<u8>, ApiError> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_IMPORT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| input_invalid())?;
    if bytes.len() > MAX_IMPORT_BYTES {
        return Err(error(
            "preview_input_too_large",
            "CSV input exceeds the 10 MiB limit.",
        ));
    }
    Ok(bytes)
}

fn input_invalid() -> ApiError {
    error("preview_input_invalid", "Cannot read CSV input.")
}

pub struct PreviewClient {
    client: Client,
    base: Url,
    token: SecretString,
    request_timeout: Duration,
}

impl PreviewClient {
    /// Validates and prepares every local structured input before health/token I/O.
    pub async fn connect_with_resolution_request(
        input: &str,
        secrets: &dyn SecretStore,
        csv_path: &str,
        resolution_inputs_path: &str,
    ) -> Result<(Self, Vec<u8>), ApiError> {
        validate_origin(input)?;
        let encoded_request = read_resolution_request(csv_path, resolution_inputs_path)?;
        let client = Self::connect(input, secrets).await?;
        Ok((client, encoded_request))
    }

    /// Performs compatibility preflight and loads this origin's token once.
    pub async fn connect(input: &str, secrets: &dyn SecretStore) -> Result<Self, ApiError> {
        Self::connect_with_timeout(input, secrets, Duration::from_secs(30)).await
    }

    pub(crate) async fn connect_with_timeout(
        input: &str,
        secrets: &dyn SecretStore,
        request_timeout: Duration,
    ) -> Result<Self, ApiError> {
        let base = origin(input)?;
        let client = http()?;
        health(&client, &base).await?;
        let token = secrets.get(reference(&base)).await?;
        collector_core::access::validate_key(token.expose_secret())?;
        Ok(Self {
            client,
            base,
            token,
            request_timeout,
        })
    }

    pub async fn import_imdb_ratings(
        &self,
        bytes: Vec<u8>,
        profile_id: &str,
        profile_version: u32,
    ) -> Result<PreviewImportAccepted, PreviewFailure> {
        if bytes.len() > MAX_IMPORT_BYTES || !valid_id(profile_id) || profile_version == 0 {
            return Err(unavailable().into());
        }
        self.import_imdb_payload(bytes, "text/csv", profile_id, profile_version)
            .await
    }

    pub async fn import_imdb_ratings_with_resolution(
        &self,
        encoded_request: Vec<u8>,
        profile_id: &str,
        profile_version: u32,
    ) -> Result<PreviewImportAccepted, PreviewFailure> {
        if encoded_request.len() > MAX_IMPORT_BYTES {
            return Err(unavailable().into());
        }
        self.import_imdb_payload(
            encoded_request,
            "application/json",
            profile_id,
            profile_version,
        )
        .await
    }

    async fn import_imdb_payload(
        &self,
        bytes: Vec<u8>,
        content_type: &'static str,
        profile_id: &str,
        profile_version: u32,
    ) -> Result<PreviewImportAccepted, PreviewFailure> {
        if !valid_id(profile_id) || profile_version == 0 {
            return Err(unavailable().into());
        }
        let mut url = self.url("api/v1/imports/imdb/ratings")?;
        url.query_pairs_mut()
            .append_pair("profileId", profile_id)
            .append_pair("profileVersion", &profile_version.to_string());
        let response = self
            .request(
                self.client
                    .post(url)
                    .header("content-type", content_type)
                    .body(bytes),
            )
            .await?;
        let value: PreviewImportAccepted = decode(
            response,
            StatusCode::ACCEPTED,
            NORMAL_RESPONSE_BYTES,
            Some(AdmissionExpectation::Initial),
        )
        .await?;
        validate_accepted(&value.preview)?;
        if value.preview.attempt != 1 {
            return Err(unavailable().into());
        }
        validate_counts(&value)?;
        Ok(value)
    }

    pub async fn status(&self, id: &str) -> Result<PreviewJobView, PreviewFailure> {
        validate_id(id)?;
        let value: PreviewJobView = decode(
            self.request(self.client.get(self.url(&format!("api/v1/previews/{id}"))?))
                .await?,
            StatusCode::OK,
            NORMAL_RESPONSE_BYTES,
            None,
        )
        .await?;
        validate_job(&value, id)?;
        Ok(value)
    }

    pub async fn results(
        &self,
        id: &str,
        attempt: u32,
        offset: u32,
        limit: u32,
    ) -> Result<PreviewResultPage, PreviewFailure> {
        validate_id(id)?;
        if attempt == 0 || !(1..=100).contains(&limit) {
            return Err(unavailable().into());
        }
        let mut url = self.url(&format!("api/v1/previews/{id}/results"))?;
        url.query_pairs_mut()
            .append_pair("attempt", &attempt.to_string())
            .append_pair("offset", &offset.to_string())
            .append_pair("limit", &limit.to_string());
        let value: PreviewResultPage = decode(
            self.request(self.client.get(url)).await?,
            StatusCode::OK,
            RESULTS_RESPONSE_BYTES,
            None,
        )
        .await?;
        validate_page(&value, id, attempt, offset, limit)?;
        Ok(value)
    }

    pub async fn cancel(
        &self,
        id: &str,
        expected_attempt: u32,
    ) -> Result<PreviewMutationResult, PreviewFailure> {
        validate_id(id)?;
        if expected_attempt == 0 {
            return Err(unavailable().into());
        }
        self.mutation(
            self.client
                .post(self.url(&format!("api/v1/previews/{id}/cancel"))?)
                .json(&PreviewCancelRequest { expected_attempt }),
        )
        .await
    }

    pub async fn resume(
        &self,
        id: &str,
        expected_attempt: u32,
    ) -> Result<PreviewAccepted, PreviewFailure> {
        validate_id(id)?;
        if expected_attempt == 0 {
            return Err(unavailable().into());
        }
        let next_attempt = expected_attempt.checked_add(1).ok_or_else(|| {
            PreviewFailure::Error(error(
                "preview_attempt_exhausted",
                "Preview attempt generation is exhausted.",
            ))
        })?;
        let value: PreviewAccepted = decode(
            self.request(
                self.client
                    .post(self.url(&format!("api/v1/previews/{id}/resume"))?)
                    .json(&PreviewResumeRequest { expected_attempt }),
            )
            .await?,
            StatusCode::ACCEPTED,
            NORMAL_RESPONSE_BYTES,
            Some(AdmissionExpectation::Exact {
                id,
                attempt: next_attempt,
            }),
        )
        .await?;
        validate_accepted(&value)?;
        if value.job_id != id || value.attempt != next_attempt {
            return Err(unavailable().into());
        }
        Ok(value)
    }

    pub async fn delete(&self, id: &str) -> Result<PreviewMutationResult, PreviewFailure> {
        validate_id(id)?;
        self.mutation(
            self.client
                .delete(self.url(&format!("api/v1/previews/{id}"))?),
        )
        .await
    }

    async fn mutation(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<PreviewMutationResult, PreviewFailure> {
        decode(
            self.request(request).await?,
            StatusCode::OK,
            NORMAL_RESPONSE_BYTES,
            None,
        )
        .await
    }

    fn url(&self, path: &str) -> Result<Url, PreviewFailure> {
        let url = self.base.join(path).map_err(|_| unavailable())?;
        // A valid ASCII identifier can still be a URL dot segment. Never send
        // the stored credential to a different route after URL normalization.
        if url.path() != format!("/{path}") {
            return Err(unavailable().into());
        }
        Ok(url)
    }

    async fn request(&self, request: reqwest::RequestBuilder) -> Result<Response, PreviewFailure> {
        request
            .bearer_auth(self.token.expose_secret())
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|_| unavailable().into())
    }
}

enum AdmissionExpectation<'a> {
    Initial,
    Exact { id: &'a str, attempt: u32 },
}

async fn decode<T: DeserializeOwned>(
    response: Response,
    expected: StatusCode,
    cap: usize,
    admission: Option<AdmissionExpectation<'_>>,
) -> Result<T, PreviewFailure> {
    let status = response.status();
    let bytes = collect(response, cap).await?;
    if status == expected {
        return serde_json::from_slice(&bytes).map_err(|_| unavailable().into());
    }
    if !status.is_client_error() && !status.is_server_error() {
        return Err(unavailable().into());
    }
    if matches!(status.as_u16(), 400 | 409 | 413 | 503)
        && let Some(expectation) = admission
        && let Some(value) = decode_admission_failure(&bytes)
    {
        let matches = match expectation {
            AdmissionExpectation::Initial => value.attempt == 1,
            AdmissionExpectation::Exact { id, attempt } => {
                value.job_id == id && value.attempt == attempt
            }
        };
        if valid_id(&value.job_id) && matches {
            return Err(PreviewFailure::Admission(PreviewAdmissionFailure {
                job_id: value.job_id,
                attempt: value.attempt,
                error: sanitize_error(value.error, true),
            }));
        }
    }
    let parsed = decode_api_error(&bytes).map(|value| sanitize_error(value, false));
    if status == StatusCode::UNAUTHORIZED {
        return Err(error(
            "authentication_required",
            "Stored CLI authorization is missing, invalid or revoked.",
        )
        .into());
    }
    Err(parsed.unwrap_or_else(unavailable).into())
}

async fn collect(mut response: Response, cap: usize) -> Result<Vec<u8>, PreviewFailure> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|len| len > cap)
        {
            return Err(unavailable().into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn sanitize_error(value: ApiError, ambiguous_admission: bool) -> ApiError {
    let message = match value.code.as_str() {
        "authentication_required" => "Stored CLI authorization is missing, invalid or revoked.",
        "preview_job_not_found" => "Preview job was not found or is no longer retained.",
        "preview_attempt_changed" => "Preview attempt changed; inspect current status.",
        "preview_runtime_busy" => "Preview runtime is busy; no job was admitted.",
        "preview_runtime_duplicate" => "This preview job is already admitted locally.",
        "preview_unavailable" => "Preview service is unavailable.",
        "preview_cancel_failed" => "Preview cancellation could not be confirmed.",
        "preview_delete_failed" => "Preview deletion could not be confirmed.",
        "neodb_not_connected" => "NeoDB is not connected.",
        "mapping_profile_not_found" => "Mapping profile revision was not found.",
        "request_invalid" => "Core rejected the request.",
        "csv_too_large" => "CSV input exceeds the supported limit.",
        "csv_encoding_invalid" => "CSV input is not valid UTF-8.",
        "csv_structure_invalid" | "csv_header_invalid" | "csv_column_missing" => {
            "CSV structure is invalid or required columns are missing."
        }
        "csv_too_many_rows" => "CSV input has too many rows.",
        "csv_duplicate_conflict" => "CSV input contains conflicting duplicate records.",
        "imdb_id_invalid" | "imdb_rating_invalid" | "imdb_title_too_long" => {
            "CSV input contains an invalid IMDb value."
        }
        "catalog_search_query_invalid" => "A resolution search query is invalid.",
        "preview_search_plan_invalid" => "Preview search choices are invalid.",
        "preview_manual_plan_invalid" => "Preview manual choices are invalid.",
        "preview_resolution_plan_invalid" => "Preview resolution choices conflict.",
        "neodb_catalog_reference_invalid" => "A NeoDB catalog reference is invalid.",
        "preview_binding_changed"
        | "preview_profile_changed"
        | "preview_transition_invalid"
        | "preview_attempt_exhausted"
        | "preview_runtime_job_expired"
        | "preview_runtime_job_invalid" => {
            "Preview cannot make the requested lifecycle transition; inspect current status."
        }
        "preview_submit_failed" | "preview_resume_failed" => {
            "Preview admission could not be confirmed; inspect status before acting again."
        }
        "preview_runtime_closed" => "Preview runtime is unavailable.",
        _ => {
            let mut clean = unavailable();
            if ambiguous_admission {
                clean.message = ADMISSION_UNCERTAIN.into();
            }
            return clean;
        }
    };
    let mut clean = error(
        &value.code,
        if ambiguous_admission {
            ADMISSION_UNCERTAIN
        } else {
            message
        },
    );
    clean.field = value.field.filter(|field| valid_field(field));
    clean.retryable = false;
    clean
}

fn decode_api_error(bytes: &[u8]) -> Option<ApiError> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "code" | "message" | "field" | "retryable"))
        || !object.contains_key("code")
        || !object.contains_key("message")
        || !object.contains_key("retryable")
    {
        return None;
    }
    serde_json::from_value(value).ok()
}

fn decode_admission_failure(bytes: &[u8]) -> Option<PreviewAdmissionFailure> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let object = value.as_object()?;
    if object.len() != 3
        || object
            .keys()
            .any(|key| !matches!(key.as_str(), "jobId" | "attempt" | "error"))
    {
        return None;
    }
    let nested = serde_json::to_vec(object.get("error")?).ok()?;
    decode_api_error(&nested)?;
    serde_json::from_value(value).ok()
}

fn valid_field(field: &str) -> bool {
    matches!(
        field,
        "id" | "attempt"
            | "expectedAttempt"
            | "offset"
            | "limit"
            | "profileId"
            | "profileVersion"
            | "contentType"
            | "body"
            | "Const"
            | "Your Rating"
            | "Title"
            | "Title Type"
    )
}

fn validate_accepted(value: &PreviewAccepted) -> Result<(), PreviewFailure> {
    if !valid_id(&value.job_id) || value.attempt == 0 || value.executable {
        return Err(unavailable().into());
    }
    Ok(())
}

fn validate_counts(value: &PreviewImportAccepted) -> Result<(), PreviewFailure> {
    if value.observation_count > MAX_PREVIEW_RECORDS as u32
        || value.skipped_row_count > MAX_PREVIEW_RECORDS as u32
        || value.duplicate_row_count > MAX_PREVIEW_RECORDS as u32
        || value.skipped_column_count > MAX_IMPORT_BYTES as u32
    {
        return Err(unavailable().into());
    }
    Ok(())
}

fn validate_job(value: &PreviewJobView, id: &str) -> Result<(), PreviewFailure> {
    if value.id != id
        || value.attempt == 0
        || value.profile_version == 0
        || value.created_at < 0
        || value.updated_at < 0
        || value.expires_at < 0
        || value.observation_count > MAX_PREVIEW_RECORDS as u32
        || !value.historical
        || value.executable
    {
        return Err(unavailable().into());
    }
    Ok(())
}

fn validate_page(
    value: &PreviewResultPage,
    id: &str,
    attempt: u32,
    offset: u32,
    limit: u32,
) -> Result<(), PreviewFailure> {
    if value.job_id != id
        || value.attempt != attempt
        || value.offset != offset
        || value.limit != limit
        || !value.historical
        || value.executable
        || value.items.len() > limit as usize
    {
        return Err(unavailable().into());
    }
    let mut previous = None;
    for item in &value.items {
        let outcome_valid = match &item.outcome {
            PreviewOutcomeView::Compared {
                observed_at,
                comparison,
                ..
            } => {
                *observed_at >= 0
                    && comparison.profile_id == item.profile_id
                    && comparison.profile_version == item.profile_version
            }
            _ => true,
        };
        if item.attempt != attempt
            || !item.historical
            || item.executable
            || item.profile_version == 0
            || item.committed_at < 0
            || item.ordinal > 9_999
            || item.ordinal < offset
            || !outcome_valid
            || previous.is_some_and(|ordinal| item.ordinal <= ordinal)
        {
            return Err(unavailable().into());
        }
        previous = Some(item.ordinal);
    }
    Ok(())
}

fn validate_id(id: &str) -> Result<(), PreviewFailure> {
    if valid_id(id) {
        Ok(())
    } else {
        Err(unavailable().into())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
}

pub fn parse_id(value: &str) -> Result<String, String> {
    if valid_id(value) {
        Ok(value.to_owned())
    } else {
        Err("must be 1..128 ASCII letters, digits, '_', '.', ':', or '-'".into())
    }
}

pub fn print_json<T: Serialize>(value: &T) -> String {
    use std::fmt::Write;
    let json = serde_json::to_string_pretty(value).expect("Domain DTO is serializable");
    let mut output = String::with_capacity(json.len());
    for character in json.chars() {
        // serde_json handles ASCII controls. Escape remaining control and bidi
        // formatting characters without losing ordinary Unicode or JSON values.
        if (!character.is_ascii() && character.is_control())
            || matches!(character, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            write!(output, "\\u{:04x}", u32::from(character))
                .expect("String writing is infallible");
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod invariant_tests {
    use super::*;

    fn job() -> PreviewJobView {
        serde_json::from_value(serde_json::json!({
            "id":"job:1","status":"running","attempt":2,"createdAt":1,"updatedAt":2,
            "expiresAt":3,"observationCount":1,"profileId":"profile:1","profileVersion":4,
            "baseUrl":"https://neodb.example","account":"alice","bindingState":"current",
            "historical":true,"executable":false
        }))
        .unwrap()
    }

    fn page() -> PreviewResultPage {
        serde_json::from_value(serde_json::json!({
            "jobId":"job:1","attempt":2,"offset":7,"limit":9,"historical":true,"executable":false,
            "items":[{
                "ordinal":8,"attempt":2,"committedAt":10,"sourceConnectorId":"imdb-ratings-csv",
                "sourceRecordId":"imdb:title:tt1","sourceFingerprint":"fp","profileId":"profile:1",
                "profileVersion":4,"baseUrl":"https://neodb.example","account":"alice",
                "historical":true,"executable":false,
                "outcome":{"kind":"compared","item":{"uuid":"u","canonicalUrl":"https://neodb.example/m/u",
                    "detailUrl":"https://neodb.example/api/m/u","mediaType":"movie","model":"Movie","parentUuid":null,
                    "externalIds":[{"scheme":"imdb:title","value":"tt1"},{"scheme":"imdb:title","value":"tt2"}]},
                    "observedAt":9,"comparison":{"profileId":"profile:1","profileVersion":4,"fields":[
                        {"ruleId":"known","field":"rating","suggestion":8,"sink":{"kind":"known","value":7},"status":"needs_review","reason":"different"},
                        {"ruleId":"empty","field":"comment","suggestion":"x","sink":{"kind":"empty"},"status":"accepted","reason":"empty"},
                        {"ruleId":"unknown","field":"shelf","suggestion":null,"sink":{"kind":"unknown"},"status":"needs_review","reason":"unknown"}
                    ]},"provenance":"automatic"}
            }]
        })).unwrap()
    }

    #[test]
    fn job_invariants_are_independent() {
        assert!(validate_job(&job(), "job:1").is_ok());
        let mut cases = Vec::new();
        let mut value = job();
        value.id = "job:other".into();
        cases.push(value);
        let mut value = job();
        value.attempt = 0;
        cases.push(value);
        let mut value = job();
        value.profile_version = 0;
        cases.push(value);
        let mut value = job();
        value.created_at = -1;
        cases.push(value);
        let mut value = job();
        value.updated_at = -1;
        cases.push(value);
        let mut value = job();
        value.expires_at = -1;
        cases.push(value);
        let mut value = job();
        value.observation_count = 10_001;
        cases.push(value);
        let mut value = job();
        value.historical = false;
        cases.push(value);
        let mut value = job();
        value.executable = true;
        cases.push(value);
        for value in cases {
            assert!(validate_job(&value, "job:1").is_err());
        }
    }

    #[test]
    fn nonempty_sparse_page_preserves_evidence_and_checks_each_safety_echo() {
        let mut valid = page();
        valid.items[0].ordinal = 20; // Not offset + limit: sparse ordinal paging.
        assert!(validate_page(&valid, "job:1", 2, 7, 9).is_ok());
        let rendered = print_json(&valid);
        assert!(rendered.contains("tt1") && rendered.contains("tt2"));
        assert!(
            rendered.contains("\"known\"")
                && rendered.contains("\"empty\"")
                && rendered.contains("\"unknown\"")
        );

        let mut cases = Vec::new();
        let mut value = page();
        value.job_id = "job:other".into();
        cases.push(value);
        let mut value = page();
        value.attempt = 3;
        cases.push(value);
        let mut value = page();
        value.offset = 6;
        cases.push(value);
        let mut value = page();
        value.limit = 8;
        cases.push(value);
        let mut value = page();
        value.historical = false;
        cases.push(value);
        let mut value = page();
        value.executable = true;
        cases.push(value);
        let mut value = page();
        value.items[0].attempt = 3;
        cases.push(value);
        let mut value = page();
        value.items[0].ordinal = 10_000;
        cases.push(value);
        let mut value = page();
        value.items[0].ordinal = 6;
        cases.push(value);
        let mut value = page();
        value.items.push(value.items[0].clone());
        cases.push(value);
        let mut value = page();
        let original = value.items[0].clone();
        value.items = (0..10)
            .map(|index| {
                let mut row = original.clone();
                row.ordinal = 7 + index;
                row
            })
            .collect();
        cases.push(value);
        let mut value = page();
        value.items[0].committed_at = -1;
        cases.push(value);
        let mut value = page();
        value.items[0].historical = false;
        cases.push(value);
        let mut value = page();
        value.items[0].executable = true;
        cases.push(value);
        let mut value = page();
        value.items[0].profile_version = 0;
        cases.push(value);
        let mut value = page();
        if let PreviewOutcomeView::Compared { observed_at, .. } = &mut value.items[0].outcome {
            *observed_at = -1;
        }
        cases.push(value);
        let mut value = page();
        if let PreviewOutcomeView::Compared { comparison, .. } = &mut value.items[0].outcome {
            comparison.profile_version = 3;
        }
        cases.push(value);
        let mut value = page();
        if let PreviewOutcomeView::Compared { comparison, .. } = &mut value.items[0].outcome {
            comparison.profile_id = "profile:other".into();
        }
        cases.push(value);
        for value in cases {
            assert!(validate_page(&value, "job:1", 2, 7, 9).is_err());
        }
    }

    #[test]
    fn domain_deserialization_rejects_unknown_fields() {
        let mut value = serde_json::to_value(job()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), true.into());
        assert!(serde_json::from_value::<PreviewJobView>(value).is_err());
    }

    #[test]
    fn receipt_bounds_and_unknown_failure_diagnostics_are_checked_independently() {
        let receipt = PreviewImportAccepted {
            preview: PreviewAccepted {
                job_id: "job:new".into(),
                attempt: 1,
                executable: false,
            },
            observation_count: 10_000,
            skipped_row_count: 10_000,
            skipped_column_count: MAX_IMPORT_BYTES as u32,
            duplicate_row_count: 10_000,
        };
        assert!(validate_counts(&receipt).is_ok());
        for field in 0..4 {
            let mut value = receipt.clone();
            match field {
                0 => value.observation_count += 1,
                1 => value.skipped_row_count += 1,
                2 => value.skipped_column_count += 1,
                _ => value.duplicate_row_count += 1,
            }
            assert!(validate_counts(&value).is_err());
        }
        for mutation in 0..4 {
            let mut value = receipt.preview.clone();
            match mutation {
                0 => value.job_id = "bad/id".into(),
                1 => value.job_id = "x".repeat(129),
                2 => value.attempt = 0,
                _ => value.executable = true,
            }
            assert!(validate_accepted(&value).is_err());
        }
        let mut unsafe_error = error("private_token_value", "credential secret");
        unsafe_error.field = Some("credentialRef".into());
        unsafe_error.retryable = true;
        let clean = sanitize_error(unsafe_error, true);
        assert_eq!(clean.code, "core_unavailable");
        assert_eq!(clean.message, ADMISSION_UNCERTAIN);
        assert!(clean.field.is_none());
        assert!(!clean.retryable);
        assert!(decode_api_error(br#"{"code":"request_invalid","message":"private","retryable":false,"secret":"token"}"#).is_none());
        assert!(decode_admission_failure(br#"{"jobId":"job:new","attempt":1,"error":{"code":"request_invalid","message":"private","retryable":false,"secret":"token"}}"#).is_none());
    }

    #[test]
    fn structured_semantic_errors_keep_only_known_codes_and_static_messages() {
        let cases = [
            (
                "catalog_search_query_invalid",
                "A resolution search query is invalid.",
            ),
            (
                "preview_search_plan_invalid",
                "Preview search choices are invalid.",
            ),
            (
                "preview_manual_plan_invalid",
                "Preview manual choices are invalid.",
            ),
            (
                "preview_resolution_plan_invalid",
                "Preview resolution choices conflict.",
            ),
            (
                "neodb_catalog_reference_invalid",
                "A NeoDB catalog reference is invalid.",
            ),
        ];
        for (code, message) in cases {
            let mut upstream = ApiError::new(code, "private query token=SECRET");
            upstream.field = Some("resolutionInputs[9].query".into());
            upstream.retryable = true;
            let clean = sanitize_error(upstream, false);
            assert_eq!(clean.code, code);
            assert_eq!(clean.message, message);
            assert!(clean.field.is_none());
            assert!(!clean.retryable);
        }

        let unknown = sanitize_error(
            ApiError::new("future_private_code", "token=SECRET private query"),
            false,
        );
        assert_eq!(unknown.code, "core_unavailable");
        assert!(!unknown.message.contains("SECRET"));
    }
}
