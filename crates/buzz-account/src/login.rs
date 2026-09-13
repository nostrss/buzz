//! Email + 6-digit-code login, sessions, and the identity key issued on first
//! login (ADR 0001: the account service holds the key).

use axum::{extract::State, http::HeaderMap, http::StatusCode, Json};
use chrono::{DateTime, Utc};
use nostr::{Keys, ToBech32};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    crypto,
    http::{authenticate, ApiError, AppState},
};

/// Codes stop working this long after being issued.
const CODE_TTL_MINUTES: i64 = 10;
/// Wrong guesses allowed before the code is discarded.
const MAX_ATTEMPTS: i32 = 5;
/// Minimum gap between two codes for the same email.
const RESEND_COOLDOWN_SECS: i64 = 60;
/// Session lifetime.
const SESSION_DAYS: i64 = 90;

#[derive(Deserialize)]
pub struct StartRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub email: String,
    pub code: String,
}

/// A community the account belongs to. Only owned communities exist in this
/// service's tables; membership by invite lives on the relay.
#[derive(Serialize)]
pub struct CommunitySummary {
    pub host: String,
    pub role: &'static str,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub session_token: String,
    pub session_expires_at: DateTime<Utc>,
    pub pubkey: String,
    /// bech32 `nsec…`; the desktop commits it to the keychain and forgets it.
    pub secret_key: String,
    pub communities: Vec<CommunitySummary>,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub email: String,
    pub pubkey: String,
    pub communities: Vec<CommunitySummary>,
    pub can_create_community: bool,
}

fn normalize_email(raw: &str) -> Result<String, ApiError> {
    let email = raw.trim().to_ascii_lowercase();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.')
        })
        && !email.chars().any(char::is_whitespace);
    if valid {
        Ok(email)
    } else {
        Err(ApiError::bad_request("invalid_email"))
    }
}

/// `POST /v1/login/start`: issue a code and email it. Always answers the same
/// way for known and unknown addresses.
pub async fn start(
    State(state): State<AppState>,
    Json(req): Json<StartRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let email = normalize_email(&req.email)?;

    let recent: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT created_at FROM login_codes WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    if let Some(created_at) = recent {
        let elapsed = (Utc::now() - created_at).num_seconds();
        if elapsed < RESEND_COOLDOWN_SECS {
            return Err(
                ApiError::new(StatusCode::TOO_MANY_REQUESTS, "code_recently_sent")
                    .with("retry_after_seconds", RESEND_COOLDOWN_SECS - elapsed),
            );
        }
    }

    let code = crypto::new_login_code();
    let hash = crypto::hash_login_code(&email, &code);
    sqlx::query(
        "INSERT INTO login_codes (id, email, code_hash, expires_at) \
         VALUES ($1, $2, $3, now() + make_interval(mins => $4)) \
         ON CONFLICT (email) DO UPDATE SET \
           id = EXCLUDED.id, code_hash = EXCLUDED.code_hash, expires_at = EXCLUDED.expires_at, \
           attempts = 0, consumed_at = NULL, created_at = now()",
    )
    .bind(Uuid::new_v4())
    .bind(&email)
    .bind(&hash[..])
    .bind(CODE_TTL_MINUTES as i32)
    .execute(&state.pool)
    .await?;

    if let Err(error) = state.mailer.send_login_code(&email, &code).await {
        // Drop the code so the person can retry at once instead of waiting
        // out the cooldown for a mail that never left.
        let _ = sqlx::query("DELETE FROM login_codes WHERE email = $1")
            .bind(&email)
            .execute(&state.pool)
            .await;
        tracing::warn!(%error, "login code email failed");
        return Err(ApiError::new(StatusCode::BAD_GATEWAY, "mail_send_failed"));
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "status": "sent" })),
    ))
}

#[derive(sqlx::FromRow)]
struct CodeRow {
    code_hash: Vec<u8>,
    expires_at: DateTime<Utc>,
    attempts: i32,
    consumed_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: Uuid,
    pubkey: String,
    encrypted_secret_key: Vec<u8>,
    key_nonce: Vec<u8>,
}

/// `POST /v1/login/verify`: exchange a code for a session and the identity key.
pub async fn verify(
    State(state): State<AppState>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, ApiError> {
    let email = normalize_email(&req.email)?;
    let code = req.code.trim();
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad_request("invalid_code_format"));
    }

    let mut tx = state.pool.begin().await?;
    let row: Option<CodeRow> = sqlx::query_as(
        "SELECT code_hash, expires_at, attempts, consumed_at FROM login_codes \
         WHERE email = $1 FOR UPDATE",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(ApiError::unauthorized("no_active_code"));
    };

    if row.consumed_at.is_some() || row.expires_at <= Utc::now() {
        sqlx::query("DELETE FROM login_codes WHERE email = $1")
            .bind(&email)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(ApiError::new(StatusCode::GONE, "code_expired"));
    }

    let expected = crypto::hash_login_code(&email, code);
    let matches = row.code_hash.len() == 32 && row.code_hash.ct_eq(&expected[..]).into();
    if !matches {
        let attempts = row.attempts + 1;
        if attempts >= MAX_ATTEMPTS {
            sqlx::query("DELETE FROM login_codes WHERE email = $1")
                .bind(&email)
                .execute(&mut *tx)
                .await?;
        } else {
            sqlx::query("UPDATE login_codes SET attempts = $2 WHERE email = $1")
                .bind(&email)
                .bind(attempts)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        return Err(ApiError::unauthorized("invalid_code")
            .with("remaining_attempts", (MAX_ATTEMPTS - attempts).max(0)));
    }

    // Consumed: a code is single-use.
    sqlx::query("DELETE FROM login_codes WHERE email = $1")
        .bind(&email)
        .execute(&mut *tx)
        .await?;

    let existing: Option<AccountRow> = sqlx::query_as(
        "SELECT id, pubkey, encrypted_secret_key, key_nonce FROM accounts WHERE email = $1",
    )
    .bind(&email)
    .fetch_optional(&mut *tx)
    .await?;

    let (account_id, keys) = match existing {
        Some(account) => {
            let secret = state
                .vault
                .open(
                    &account.encrypted_secret_key,
                    &account.key_nonce,
                    &account.pubkey,
                )
                .map_err(|e| ApiError::internal("unseal identity key", e))?;
            let keys = Keys::new(
                nostr::SecretKey::from_slice(&secret)
                    .map_err(|e| ApiError::internal("stored identity key", e))?,
            );
            (account.id, keys)
        }
        None => {
            let keys = Keys::generate();
            let pubkey = keys.public_key().to_hex();
            let (ciphertext, nonce) = state
                .vault
                .seal(&keys.secret_key().to_secret_bytes(), &pubkey);
            let id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO accounts (id, email, pubkey, encrypted_secret_key, key_nonce) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(&email)
            .bind(&pubkey)
            .bind(&ciphertext)
            .bind(&nonce[..])
            .execute(&mut *tx)
            .await?;
            tracing::info!(%id, "account created");
            (id, keys)
        }
    };

    let token = crypto::new_session_token();
    let token_hash = crypto::hash_session_token(&token);
    let expires_at = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    sqlx::query(
        "INSERT INTO sessions (id, account_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(account_id)
    .bind(&token_hash[..])
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let communities = communities_of(&state, account_id).await?;
    Ok(Json(VerifyResponse {
        session_token: token,
        session_expires_at: expires_at,
        pubkey: keys.public_key().to_hex(),
        secret_key: keys
            .secret_key()
            .to_bech32()
            .map_err(|e| ApiError::internal("encode nsec", e))?,
        communities,
    }))
}

/// `GET /v1/me`: who the bearer token belongs to.
pub async fn me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MeResponse>, ApiError> {
    let session = authenticate(&state, &headers).await?;
    let (email, pubkey): (String, String) =
        sqlx::query_as("SELECT email, pubkey FROM accounts WHERE id = $1")
            .bind(session.account_id)
            .fetch_one(&state.pool)
            .await?;
    let communities = communities_of(&state, session.account_id).await?;
    Ok(Json(MeResponse {
        email,
        pubkey,
        can_create_community: communities.is_empty(),
        communities,
    }))
}

/// `POST /v1/logout`: revoke the bearer session.
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let session = authenticate(&state, &headers).await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1")
        .bind(session.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn communities_of(
    state: &AppState,
    account_id: Uuid,
) -> Result<Vec<CommunitySummary>, ApiError> {
    let hosts: Vec<(String,)> =
        sqlx::query_as("SELECT host FROM communities WHERE account_id = $1 ORDER BY created_at")
            .bind(account_id)
            .fetch_all(&state.pool)
            .await?;
    Ok(hosts
        .into_iter()
        .map(|(host,)| CommunitySummary {
            host,
            role: "owner",
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn normalizes_case_and_whitespace() {
        assert_eq!(
            normalize_email("  Jin@Example.COM ").expect("valid"),
            "jin@example.com"
        );
    }

    #[test]
    fn rejects_malformed_addresses() {
        for bad in ["", "nope", "@x.com", "a@b", "a b@x.com", "a@.com"] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
