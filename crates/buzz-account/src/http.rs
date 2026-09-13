//! HTTP surface: router, shared state, JSON error shape, and bearer-session
//! authentication.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use sqlx::PgPool;

use crate::{crypto::KeyVault, login, mail::Mailer, Config};

/// Shared handler state.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub vault: KeyVault,
    pub mailer: Mailer,
}

impl AppState {
    /// Assemble state from validated config and a migrated pool.
    pub fn new(config: Config, pool: PgPool) -> anyhow::Result<Self> {
        let vault = KeyVault::new(&config.master_key);
        let mailer = Mailer::new(
            &config.resend_base_url,
            config.resend_api_key.clone(),
            config.email_from.clone(),
        )?;
        Ok(Self {
            pool,
            config: Arc::new(config),
            vault,
            mailer,
        })
    }
}

/// Build the service router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/_liveness", get(live))
        .route("/_readiness", get(ready))
        .route("/v1/login/start", post(login::start))
        .route("/v1/login/verify", post(login::verify))
        .route("/v1/logout", post(login::logout))
        .route("/v1/me", get(login::me))
        .with_state(state)
}

/// JSON error response: `{"error": "<code>", ...extra}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str) -> Self {
        Self {
            status,
            code,
            extra: serde_json::Map::new(),
        }
    }

    pub fn with(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.extra.insert(key.to_owned(), value.into());
        self
    }

    pub fn bad_request(code: &'static str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code)
    }

    pub fn unauthorized(code: &'static str) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, code)
    }

    /// Internal failure. The cause is logged, never returned.
    pub fn internal(context: &'static str, error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, context, "internal error");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::internal("database", error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = self.extra;
        body.insert("error".into(), self.code.into());
        (self.status, Json(serde_json::Value::Object(body))).into_response()
    }
}

/// A live session resolved from the `Authorization: Bearer` header.
#[derive(Debug, Clone, Copy)]
pub struct Session {
    pub id: uuid::Uuid,
    pub account_id: uuid::Uuid,
}

/// Resolve the bearer token to an unexpired, unrevoked session.
pub async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<Session, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing_token"))?;
    let hash = crate::crypto::hash_session_token(token);
    let row: Option<(uuid::Uuid, uuid::Uuid)> = sqlx::query_as(
        "SELECT id, account_id FROM sessions \
         WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > now()",
    )
    .bind(&hash[..])
    .fetch_optional(&state.pool)
    .await?;
    let (id, account_id) = row.ok_or_else(|| ApiError::unauthorized("invalid_token"))?;
    Ok(Session { id, account_id })
}

async fn live() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "alive" }))
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(_) => (
            StatusCode::OK,
            Json(serde_json::json!({ "status": "ready" })),
        ),
        Err(error) => {
            tracing::warn!(%error, "readiness probe failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "status": "not_ready" })),
            )
        }
    }
}
