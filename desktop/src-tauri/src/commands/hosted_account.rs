//! Fork-only: hosted-service account login (email + 6-digit code).
//!
//! The account service holds the identity key (ADR 0001). On a successful
//! code exchange the returned `nsec` is committed straight into the keychain
//! through the same path `import_identity` uses, so the key never crosses the
//! webview boundary. The session token lives in the same keychain blob under
//! [`SESSION_KEY`]; `sign_out` wipes both.

use std::time::Duration;

use serde::Serialize;
use tauri::Manager;

use crate::app_state::AppState;

/// Keychain blob entry holding the account-service session token.
const SESSION_KEY: &str = "hosted-account-session";

#[derive(Serialize)]
pub(crate) struct HostedLoginResult {
    pub pubkey: String,
    pub communities: serde_json::Value,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("http client: {error}"))
}

fn api_url(base_url: &str, path: &str) -> Result<url::Url, String> {
    let base = url::Url::parse(base_url.trim_end_matches('/'))
        .map_err(|error| format!("invalid account service URL: {error}"))?;
    base.join(path)
        .map_err(|error| format!("invalid account service URL: {error}"))
}

/// Send a request and return the JSON body. Structured API errors
/// (`{"error": ...}`) come back as `Ok` so the frontend can branch on the
/// code; only transport failures and non-JSON bodies are `Err`.
async fn send_json(
    request: reqwest::RequestBuilder,
) -> Result<(reqwest::StatusCode, serde_json::Value), String> {
    let response = request
        .send()
        .await
        .map_err(|error| format!("account service unreachable: {error}"))?;
    let status = response.status();
    if status == reqwest::StatusCode::NO_CONTENT {
        return Ok((status, serde_json::Value::Null));
    }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("invalid account service response: {error}"))?;
    if !status.is_success() && value.get("error").is_none() {
        return Err(format!("account service failed (HTTP {status})"));
    }
    Ok((status, value))
}

fn session_store() -> &'static crate::secret_store::SecretStore {
    crate::secret_store::SecretStore::shared(crate::app_state::keyring_service())
}

fn load_session_token() -> Option<String> {
    match session_store().load(SESSION_KEY) {
        Ok(token) => token.filter(|t| !t.is_empty()),
        Err(error) => {
            eprintln!("buzz-desktop: hosted session load failed: {error}");
            None
        }
    }
}

/// Request a login code for `email`.
#[tauri::command]
pub async fn hosted_login_start(
    base_url: String,
    email: String,
) -> Result<serde_json::Value, String> {
    let (_, body) = send_json(
        client()?
            .post(api_url(&base_url, "/v1/login/start")?)
            .json(&serde_json::json!({ "email": email })),
    )
    .await?;
    Ok(body)
}

/// Exchange the emailed code for a session, commit the returned identity key
/// to the keychain, and remember the session token.
#[tauri::command]
pub async fn hosted_login_verify(
    base_url: String,
    email: String,
    code: String,
    app_handle: tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let (_, body) = send_json(
        client()?
            .post(api_url(&base_url, "/v1/login/verify")?)
            .json(&serde_json::json!({ "email": email, "code": code })),
    )
    .await?;
    if body.get("error").is_some() {
        return Ok(body);
    }

    let nsec = body
        .get("secret_key")
        .and_then(serde_json::Value::as_str)
        .ok_or("account service response has no secret_key")?
        .to_owned();
    let session_token = body
        .get("session_token")
        .and_then(serde_json::Value::as_str)
        .ok_or("account service response has no session_token")?
        .to_owned();
    let communities = body
        .get("communities")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));

    let result = tokio::task::spawn_blocking(move || {
        let keys = crate::key_backup::recover_keys_from_input(&nsec, None)?;
        let state = app_handle.state::<AppState>();
        let _mutation_guard = state.identity_mutation.lock().map_err(|e| e.to_string())?;
        let data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| format!("app data dir: {e}"))?;
        std::fs::create_dir_all(&data_dir).map_err(|e| format!("create app data dir: {e}"))?;
        let key_path = data_dir.join("identity.key");
        let (pubkey, _storage) =
            super::identity::commit_imported_identity(&state, &data_dir, keys, |keys| {
                crate::app_state::persist_imported_identity(
                    session_store(),
                    keys,
                    &key_path,
                    &data_dir,
                )
            })?;

        // Best effort: without a stored session the next launch simply asks
        // for a code again, which is preferable to failing the login.
        if let Err(error) = session_store().store(SESSION_KEY, &session_token) {
            eprintln!("buzz-desktop: hosted session store failed: {error}");
        }

        Ok::<_, String>(HostedLoginResult {
            pubkey: pubkey.to_hex(),
            communities,
        })
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))??;

    serde_json::to_value(result).map_err(|e| e.to_string())
}

/// Validate the stored session. Returns `null` when there is no usable
/// session; a rejected token is forgotten so the next launch asks again.
#[tauri::command]
pub async fn hosted_session_me(base_url: String) -> Result<serde_json::Value, String> {
    let Some(token) = load_session_token() else {
        return Ok(serde_json::Value::Null);
    };
    let (status, body) = send_json(
        client()?
            .get(api_url(&base_url, "/v1/me")?)
            .bearer_auth(&token),
    )
    .await?;
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let _ = session_store().delete(SESSION_KEY);
        return Ok(serde_json::Value::Null);
    }
    if body.get("error").is_some() {
        return Ok(serde_json::Value::Null);
    }
    Ok(body)
}

/// Revoke the session server-side (best effort), then hand over to the
/// regular sign-out, which wipes the keychain blob (session and identity key)
/// and relaunches into first-run.
#[tauri::command]
pub async fn hosted_logout(base_url: String, app: tauri::AppHandle) -> Result<(), String> {
    if let Some(token) = load_session_token() {
        if let Ok(client) = client() {
            let _ = client
                .post(api_url(&base_url, "/v1/logout")?)
                .bearer_auth(&token)
                .send()
                .await;
        }
        let _ = session_store().delete(SESSION_KEY);
    }
    super::identity::sign_out(app).await
}

async fn authenticated_post(
    base_url: &str,
    path: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let Some(token) = load_session_token() else {
        return Ok(serde_json::json!({ "error": "missing_token" }));
    };
    let (status, body) = send_json(
        client()?
            .post(api_url(base_url, path)?)
            .bearer_auth(&token)
            .json(&body),
    )
    .await?;
    if status == reqwest::StatusCode::UNAUTHORIZED {
        let _ = session_store().delete(SESSION_KEY);
    }
    Ok(body)
}

/// Normalize a community name and ask whether its host is free.
#[tauri::command]
pub async fn hosted_community_check(
    base_url: String,
    name: String,
) -> Result<serde_json::Value, String> {
    authenticated_post(
        &base_url,
        "/v1/communities/check",
        serde_json::json!({ "name": name }),
    )
    .await
}

/// Create the account's community on the relay. Returns `{host, relay_url,
/// community_id}` or a structured error (`community_limit_reached`,
/// `host_taken`, `invalid_name`, `relay_error`).
#[tauri::command]
pub async fn hosted_community_create(
    base_url: String,
    name: String,
) -> Result<serde_json::Value, String> {
    authenticated_post(
        &base_url,
        "/v1/communities",
        serde_json::json!({ "name": name }),
    )
    .await
}
