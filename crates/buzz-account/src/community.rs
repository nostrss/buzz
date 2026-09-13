//! Community creation: name rules, relay operator calls (NIP-98 signed with
//! the operator key), the one-per-account rule, and the Caddy on-demand TLS
//! `ask` endpoint.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use nostr::{EventBuilder, JsonUtil, Kind, Tag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::http::{authenticate, ApiError, AppState};

const MIN_NAME_LEN: usize = 3;
const MAX_NAME_LEN: usize = 30;
/// Labels that would collide with service hosts or read as official.
const RESERVED_NAMES: &[&str] = &["app", "auth", "www", "admin", "api", "mail", "relay"];

/// Normalize a typed name into a host label: trim, lowercase, spaces to
/// hyphens, runs of hyphens collapsed. Validation is separate so the UI can
/// show the normalized form next to the reason it was rejected.
pub fn normalize_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().to_ascii_lowercase().chars() {
        let ch = if ch.is_whitespace() { '-' } else { ch };
        if ch == '-' && out.ends_with('-') {
            continue;
        }
        out.push(ch);
    }
    out
}

/// Check a normalized name against `^[a-z0-9]([a-z0-9-]{1,28}[a-z0-9])?$` and
/// the reserved list. Returns the rejection reason.
pub fn validate_name(name: &str) -> Result<(), &'static str> {
    if name.len() < MIN_NAME_LEN {
        return Err("too_short");
    }
    if name.len() > MAX_NAME_LEN {
        return Err("too_long");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("invalid_characters");
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err("leading_or_trailing_hyphen");
    }
    if RESERVED_NAMES.contains(&name) {
        return Err("reserved");
    }
    Ok(())
}

/// `Authorization: Nostr <base64 event>` for a relay operator request. Signed
/// URL = the relay's `RELAY_OPERATOR_API_ORIGIN` (our `ACCOUNT_RELAY_URL`
/// origin) plus path and query, exactly as the relay reconstructs it.
fn nip98_header(
    state: &AppState,
    method: &str,
    url: &str,
    body: Option<&[u8]>,
) -> Result<String, ApiError> {
    let mut tags = vec![
        Tag::parse(["u", url]),
        Tag::parse(["method", method]),
        // Distinct event id per request: the relay's replay guard keys on it.
        Tag::parse(["nonce", &Uuid::new_v4().to_string()]),
    ];
    if let Some(body) = body {
        tags.push(Tag::parse(["payload", &hex::encode(Sha256::digest(body))]));
    }
    let tags = tags
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::internal("nip98 tag", e))?;
    let event = EventBuilder::new(Kind::HttpAuth, "")
        .tags(tags)
        .sign_with_keys(&state.config.operator_keys)
        .map_err(|e| ApiError::internal("nip98 sign", e))?;
    Ok(format!("Nostr {}", STANDARD.encode(event.as_json())))
}

/// Call a relay operator endpoint. `path_and_query` starts with `/`.
async fn relay_call(
    state: &AppState,
    method: reqwest::Method,
    path_and_query: &str,
    body: Option<serde_json::Value>,
) -> Result<(StatusCode, serde_json::Value), ApiError> {
    let url = format!(
        "{}{path_and_query}",
        state.config.relay_url.origin().ascii_serialization()
    );
    let body_bytes = body
        .as_ref()
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|e| ApiError::internal("relay body", e))?;
    let auth = nip98_header(state, method.as_str(), &url, body_bytes.as_deref())?;
    let mut req = state
        .relay
        .request(method, &url)
        .header(reqwest::header::AUTHORIZATION, auth);
    if let Some(bytes) = body_bytes {
        req = req
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(bytes);
    }
    let res = req
        .send()
        .await
        .map_err(|e| ApiError::internal("relay unreachable", e))?;
    let status = res.status();
    let json = res
        .json::<serde_json::Value>()
        .await
        .unwrap_or(serde_json::Value::Null);
    Ok((status, json))
}

fn relay_error(status: StatusCode, body: &serde_json::Value) -> ApiError {
    let message = body["error"]
        .as_str()
        .or_else(|| body["message"].as_str())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect::<String>();
    tracing::warn!(relay_status = %status, %message, "relay operator call failed");
    ApiError::new(StatusCode::BAD_GATEWAY, "relay_error")
        .with("relay_status", status.as_u16())
        .with("message", message)
}

#[derive(Deserialize)]
pub struct NameRequest {
    pub name: String,
}

#[derive(Serialize)]
pub struct CheckResponse {
    pub name: String,
    pub normalized: String,
    pub host: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
}

#[derive(Serialize)]
pub struct CreateResponse {
    pub host: String,
    pub relay_url: String,
    pub community_id: String,
}

/// `POST /v1/communities/check`: normalize, validate, and ask the relay
/// whether the host is free.
pub async fn check(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<NameRequest>,
) -> Result<Json<CheckResponse>, ApiError> {
    authenticate(&state, &headers).await?;
    let normalized = normalize_name(&req.name);
    let host = format!("{normalized}.{}", state.config.community_domain);
    if let Err(reason) = validate_name(&normalized) {
        return Ok(Json(CheckResponse {
            name: req.name,
            normalized,
            host,
            available: false,
            reason: Some(reason),
        }));
    }
    let (status, body) = relay_call(
        &state,
        reqwest::Method::GET,
        &format!("/operator/communities/availability?host={host}"),
        None,
    )
    .await?;
    if !status.is_success() {
        return Err(relay_error(status, &body));
    }
    let available = body["available"].as_bool().unwrap_or(false);
    Ok(Json(CheckResponse {
        name: req.name,
        normalized,
        host,
        available,
        reason: (!available).then_some("taken"),
    }))
}

/// `POST /v1/communities`: create `<name>.<domain>` on the relay with the
/// account as owner, then record it. One per account.
pub async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<NameRequest>,
) -> Result<(StatusCode, Json<CreateResponse>), ApiError> {
    let session = authenticate(&state, &headers).await?;
    let normalized = normalize_name(&req.name);
    if let Err(reason) = validate_name(&normalized) {
        return Err(ApiError::bad_request("invalid_name").with("reason", reason));
    }
    let host = format!("{normalized}.{}", state.config.community_domain);

    let (pubkey, owned): (String, i64) = sqlx::query_as(
        "SELECT a.pubkey, (SELECT count(*) FROM communities c WHERE c.account_id = a.id) \
         FROM accounts a WHERE a.id = $1",
    )
    .bind(session.account_id)
    .fetch_one(&state.pool)
    .await?;
    if owned > 0 {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "community_limit_reached",
        ));
    }

    let (status, body) = relay_call(
        &state,
        reqwest::Method::POST,
        "/operator/communities",
        Some(serde_json::json!({
            "host": host,
            "initial_owner_pubkey": pubkey,
            "create_only": true,
        })),
    )
    .await?;
    if status == StatusCode::CONFLICT {
        return Err(ApiError::new(StatusCode::CONFLICT, "host_taken"));
    }
    if !status.is_success() {
        return Err(relay_error(status, &body));
    }
    let relay_community_id: Uuid = body["community_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiError::internal("relay response", "missing community_id"))?;

    let inserted = sqlx::query(
        "INSERT INTO communities (id, account_id, name, host, relay_community_id) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(session.account_id)
    .bind(&normalized)
    .bind(&host)
    .bind(relay_community_id)
    .execute(&state.pool)
    .await;
    if let Err(error) = inserted {
        // Backstop for a concurrent create by the same account: the relay row
        // now exists under this owner; the operator can reconcile from logs.
        if error
            .as_database_error()
            .is_some_and(|e| e.is_unique_violation())
        {
            tracing::error!(%host, account = %session.account_id, "community created on relay but account already has one");
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "community_limit_reached",
            ));
        }
        return Err(error.into());
    }
    tracing::info!(%host, account = %session.account_id, "community created");
    Ok((
        StatusCode::CREATED,
        Json(CreateResponse {
            relay_url: format!("wss://{host}"),
            host,
            community_id: relay_community_id.to_string(),
        }),
    ))
}

#[derive(Deserialize)]
pub struct HostQuery {
    pub host: String,
}

/// `GET /v1/hosts/check?host=`: Caddy on-demand TLS `ask`. 200 only for the
/// service hosts and hosts this service created.
pub async fn hosts_check(
    State(state): State<AppState>,
    Query(q): Query<HostQuery>,
) -> Result<StatusCode, ApiError> {
    let host = q.host.trim().trim_end_matches('.').to_ascii_lowercase();
    let domain = &state.config.community_domain;
    if host == *domain || host == format!("auth.{domain}") {
        return Ok(StatusCode::OK);
    }
    let known: Option<i32> = sqlx::query_scalar("SELECT 1 FROM communities WHERE host = $1")
        .bind(&host)
        .fetch_optional(&state.pool)
        .await?;
    Ok(if known.is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_spaces_and_hyphen_runs() {
        assert_eq!(normalize_name("  My  Team -- Two "), "my-team-two");
        assert_eq!(normalize_name("Acme"), "acme");
    }

    #[test]
    fn validates_shape_and_reserved_words() {
        assert_eq!(validate_name("acme"), Ok(()));
        assert_eq!(validate_name("a-1"), Ok(()));
        assert_eq!(validate_name("ab"), Err("too_short"));
        assert_eq!(validate_name(&"a".repeat(31)), Err("too_long"));
        assert_eq!(validate_name("caf\u{e9}"), Err("invalid_characters"));
        assert_eq!(validate_name("a.b"), Err("invalid_characters"));
        assert_eq!(validate_name("-abc"), Err("leading_or_trailing_hyphen"));
        assert_eq!(validate_name("abc-"), Err("leading_or_trailing_hyphen"));
        for reserved in RESERVED_NAMES {
            assert_eq!(validate_name(reserved), Err("reserved"), "{reserved}");
        }
    }
}
