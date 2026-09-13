//! Login API integration tests against a real Postgres and a fake Resend.
//! Set `ACCOUNT_TEST_DATABASE_URL` (e.g. `postgres://buzz:pw@127.0.0.1:55432/buzz_account_test`);
//! the tests skip themselves when it is unset.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{routing::post, Json, Router};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use buzz_account::{db, http, Config};
use serde_json::{json, Value};

struct Harness {
    base: String,
    mails: Arc<Mutex<Vec<Value>>>,
    pool: sqlx::PgPool,
    client: reqwest::Client,
}

async fn harness() -> Option<Harness> {
    let Ok(database_url) = std::env::var("ACCOUNT_TEST_DATABASE_URL") else {
        eprintln!("ACCOUNT_TEST_DATABASE_URL unset; skipping login integration test");
        return None;
    };

    // Fake Resend: records every /emails body.
    let mails: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = mails.clone();
    let fake = Router::new().route(
        "/emails",
        post(move |Json(body): Json<Value>| {
            sink.lock().expect("mail sink").push(body);
            async { Json(json!({ "id": "fake" })) }
        }),
    );
    let fake_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake resend");
    let fake_addr = fake_listener.local_addr().expect("fake addr");
    tokio::spawn(async move {
        let _ = axum::serve(fake_listener, fake).await;
    });

    let env: HashMap<String, String> = [
        ("ACCOUNT_DATABASE_URL", database_url.as_str()),
        ("ACCOUNT_RELAY_URL", "http://relay.invalid:3000"),
        (
            "ACCOUNT_OPERATOR_SECRET_KEY",
            "2222222222222222222222222222222222222222222222222222222222222222",
        ),
        ("ACCOUNT_MASTER_KEY", &STANDARD.encode([5u8; 32])),
        ("RESEND_API_KEY", "re_test"),
        ("RESEND_BASE_URL", &format!("http://{fake_addr}/")),
        ("ACCOUNT_EMAIL_FROM", "noreply@test.invalid"),
        ("ACCOUNT_COMMUNITY_DOMAIN", "app.test.invalid"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    let config = Config::from_map(&env).expect("test config");
    let pool = db::connect_and_migrate(&database_url)
        .await
        .expect("migrate test database");
    let app = http::router(http::AppState::new(config, pool.clone()).expect("state"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind app");
    let addr = listener.local_addr().expect("app addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Some(Harness {
        base: format!("http://{addr}"),
        mails,
        pool,
        client: reqwest::Client::new(),
    })
}

impl Harness {
    fn email(&self) -> String {
        format!("user-{}@test.invalid", uuid::Uuid::new_v4())
    }

    async fn post(&self, path: &str, body: Value, token: Option<&str>) -> (u16, Value) {
        let mut req = self.client.post(format!("{}{path}", self.base)).json(&body);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let res = req.send().await.expect("request");
        let status = res.status().as_u16();
        let body = res.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }

    async fn get(&self, path: &str, token: Option<&str>) -> (u16, Value) {
        let mut req = self.client.get(format!("{}{path}", self.base));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let res = req.send().await.expect("request");
        let status = res.status().as_u16();
        let body = res.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }

    /// Request a code and read it back from the fake mailbox.
    async fn start(&self, email: &str) -> String {
        let (status, body) = self
            .post("/v1/login/start", json!({ "email": email }), None)
            .await;
        assert_eq!(status, 202, "start: {body}");
        let mails = self.mails.lock().expect("mail sink");
        let mail = mails
            .iter()
            .rev()
            .find(|m| m["to"][0] == email)
            .unwrap_or_else(|| panic!("no mail to {email}"))
            .clone();
        drop(mails);
        assert_eq!(mail["from"], "noreply@test.invalid");
        let subject = mail["subject"].as_str().expect("subject");
        let code: String = subject.chars().take_while(char::is_ascii_digit).collect();
        assert_eq!(code.len(), 6, "subject carries the code: {subject}");
        assert!(mail["text"].as_str().expect("text").contains(&code));
        code
    }

    async fn verify(&self, email: &str, code: &str) -> (u16, Value) {
        self.post(
            "/v1/login/verify",
            json!({ "email": email, "code": code }),
            None,
        )
        .await
    }
}

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
