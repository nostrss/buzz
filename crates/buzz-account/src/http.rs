//! HTTP surface. Only health probes exist yet; login and provisioning routes
//! land in later tickets.

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use sqlx::PgPool;

/// Shared handler state.
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
}

/// Build the service router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/_liveness", get(live))
        .route("/_readiness", get(ready))
        .with_state(state)
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
