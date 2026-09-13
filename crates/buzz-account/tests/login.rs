//! Login API integration tests. See `common` for the harness and the
//! `ACCOUNT_TEST_DATABASE_URL` skip rule.

mod common;

use common::harness;
use serde_json::json;

#[tokio::test]
async fn first_login_issues_a_sealed_identity_key_and_a_session() {
    let Some(h) = harness().await else { return };
    let email = h.email();
    let code = h.start(&email).await;

    let (status, body) = h.verify(&email, &code).await;
    assert_eq!(status, 200, "{body}");
    let token = body["session_token"].as_str().expect("token").to_owned();
    let pubkey = body["pubkey"].as_str().expect("pubkey").to_owned();
    let nsec = body["secret_key"].as_str().expect("nsec").to_owned();
    assert_eq!(pubkey.len(), 64);
    assert!(nsec.starts_with("nsec1"));
    assert_eq!(body["communities"], json!([]));

    // The key is the one the pubkey belongs to.
    let keys = nostr::Keys::parse(&nsec).expect("nsec parses");
    assert_eq!(keys.public_key().to_hex(), pubkey);

    // Stored at rest as ciphertext, not the raw secret.
    let (stored, nonce): (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT encrypted_secret_key, key_nonce FROM accounts WHERE email = $1")
            .bind(&email)
            .fetch_one(&h.pool)
            .await
            .expect("account row");
    let raw = keys.secret_key().to_secret_bytes();
    assert!(!stored.windows(32).any(|w| w == raw));
    assert_eq!(nonce.len(), 12);

    // Session works and reports the account.
    let (status, me) = h.get("/v1/me", Some(&token)).await;
    assert_eq!(status, 200, "{me}");
    assert_eq!(me["email"], email);
    assert_eq!(me["pubkey"], pubkey);
    assert_eq!(me["can_create_community"], true);

    // Session token is stored hashed and expires in ~90 days.
    let (hashed_exists, days): (bool, f64) = sqlx::query_as(
        "SELECT NOT EXISTS (SELECT 1 FROM sessions WHERE token_hash = $1), \
                (EXTRACT(EPOCH FROM (max(expires_at) - now())) / 86400)::float8 \
         FROM sessions",
    )
    .bind(token.as_bytes())
    .fetch_one(&h.pool)
    .await
    .expect("session row");
    assert!(hashed_exists);
    assert!((89.9..=90.1).contains(&days), "expiry {days} days");
}

#[tokio::test]
async fn second_login_returns_the_same_identity_key() {
    let Some(h) = harness().await else { return };
    let email = h.email();
    let code = h.start(&email).await;
    let (_, first) = h.verify(&email, &code).await;

    let code = h.start(&email).await;
    let (status, second) = h.verify(&email, &code).await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(first["pubkey"], second["pubkey"]);
    assert_eq!(first["secret_key"], second["secret_key"]);
    assert_ne!(first["session_token"], second["session_token"]);
}

#[tokio::test]
async fn a_second_code_within_a_minute_is_refused() {
    let Some(h) = harness().await else { return };
    let email = h.email();
    h.start(&email).await;
    let (status, body) = h
        .post("/v1/login/start", json!({ "email": email }), None)
        .await;
    assert_eq!(status, 429, "{body}");
    assert_eq!(body["error"], "code_recently_sent");
    assert!(body["retry_after_seconds"].as_i64().expect("retry") > 0);
}

#[tokio::test]
async fn five_wrong_guesses_discard_the_code() {
    let Some(h) = harness().await else { return };
    let email = h.email();
    let code = h.start(&email).await;
    let wrong = if code == "000000" { "000001" } else { "000000" };

    for remaining in (0..5).rev() {
        let (status, body) = h.verify(&email, wrong).await;
        assert_eq!(status, 401, "{body}");
        assert_eq!(body["error"], "invalid_code");
        assert_eq!(body["remaining_attempts"], remaining);
    }
    let (status, body) = h.verify(&email, &code).await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["error"], "no_active_code");
}

#[tokio::test]
async fn expired_codes_are_rejected_and_single_use_holds() {
    let Some(h) = harness().await else { return };
    let email = h.email();
    let code = h.start(&email).await;
    sqlx::query("UPDATE login_codes SET expires_at = now() - interval '1 second' WHERE email = $1")
        .bind(&email)
        .execute(&h.pool)
        .await
        .expect("expire");
    let (status, body) = h.verify(&email, &code).await;
    assert_eq!(status, 410, "{body}");
    assert_eq!(body["error"], "code_expired");

    let code = h.start(&email).await;
    let (status, _) = h.verify(&email, &code).await;
    assert_eq!(status, 200);
    let (status, body) = h.verify(&email, &code).await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["error"], "no_active_code");
}

#[tokio::test]
async fn logout_revokes_the_session() {
    let Some(h) = harness().await else { return };
    let email = h.email();
    let code = h.start(&email).await;
    let (_, body) = h.verify(&email, &code).await;
    let token = body["session_token"].as_str().expect("token").to_owned();

    let (status, _) = h.post("/v1/logout", json!({}), Some(&token)).await;
    assert_eq!(status, 204);
    let (status, body) = h.get("/v1/me", Some(&token)).await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["error"], "invalid_token");
    let (status, body) = h.get("/v1/me", None).await;
    assert_eq!(status, 401, "{body}");
    assert_eq!(body["error"], "missing_token");
}

#[tokio::test]
async fn malformed_input_is_a_bad_request() {
    let Some(h) = harness().await else { return };
    let (status, body) = h
        .post("/v1/login/start", json!({ "email": "not-an-email" }), None)
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "invalid_email");
    let (status, body) = h.verify(&h.email(), "12ab").await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "invalid_code_format");
}
