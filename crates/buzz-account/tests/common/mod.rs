//! Shared integration-test harness: the real router on a real Postgres, with
//! a fake Resend and a fake relay operator API recording what they receive.
//! Set `ACCOUNT_TEST_DATABASE_URL`; tests skip themselves when it is unset.
#![allow(dead_code)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use buzz_account::{db, http, Config};
use serde_json::{json, Value};

/// One recorded relay operator request.
#[derive(Clone, Debug)]
pub struct RelayRequest {
    pub method: String,
    pub uri: String,
    pub authorization: Option<String>,
    pub body: Value,
}

/// What the fake relay answers to `POST /operator/communities`.
#[derive(Clone, Copy, Debug)]
pub enum RelayMode {
    Created,
    HostExists,
    Failing,
}

#[derive(Clone)]
struct FakeRelay {
    requests: Arc<Mutex<Vec<RelayRequest>>>,
    mode: Arc<Mutex<RelayMode>>,
}

pub struct Harness {
    pub base: String,
    pub relay_origin: String,
    pub mails: Arc<Mutex<Vec<Value>>>,
    pub relay_requests: Arc<Mutex<Vec<RelayRequest>>>,
    relay_mode: Arc<Mutex<RelayMode>>,
    pub pool: sqlx::PgPool,
    pub client: reqwest::Client,
}

pub async fn harness() -> Option<Harness> {
    let Ok(database_url) = std::env::var("ACCOUNT_TEST_DATABASE_URL") else {
        eprintln!("ACCOUNT_TEST_DATABASE_URL unset; skipping integration test");
        return None;
    };

    // Fake Resend: records every /emails body.
    let mails: Arc<Mutex<Vec<Value>>> = Arc::default();
    let sink = mails.clone();
    let fake_resend = Router::new().route(
        "/emails",
        post(move |Json(body): Json<Value>| {
            sink.lock().expect("mail sink").push(body);
            async { Json(json!({ "id": "fake" })) }
        }),
    );
    let resend_addr = serve(fake_resend).await;

    // Fake relay operator API.
    let relay = FakeRelay {
        requests: Arc::default(),
        mode: Arc::new(Mutex::new(RelayMode::Created)),
    };
    let fake_relay = Router::new()
        .route(
            "/operator/communities/availability",
            get(relay_availability),
        )
        .route("/operator/communities", post(relay_create))
        .with_state(relay.clone());
    let relay_addr = serve(fake_relay).await;
    let relay_origin = format!("http://{relay_addr}");

    let env: HashMap<String, String> = [
        ("ACCOUNT_DATABASE_URL", database_url.as_str()),
        ("ACCOUNT_RELAY_URL", relay_origin.as_str()),
        (
            "ACCOUNT_OPERATOR_SECRET_KEY",
            "2222222222222222222222222222222222222222222222222222222222222222",
        ),
        ("ACCOUNT_MASTER_KEY", &STANDARD.encode([5u8; 32])),
        ("RESEND_API_KEY", "re_test"),
        ("RESEND_BASE_URL", &format!("http://{resend_addr}/")),
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
    let addr = serve(app).await;

    Some(Harness {
        base: format!("http://{addr}"),
        relay_origin,
        mails,
        relay_requests: relay.requests,
        relay_mode: relay.mode,
        pool,
        client: reqwest::Client::new(),
    })
}

async fn serve(app: Router) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    addr
}

async fn relay_availability(
    State(relay): State<FakeRelay>,
    headers: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Json<Value> {
    let host = q.get("host").cloned().unwrap_or_default();
    relay
        .requests
        .lock()
        .expect("relay log")
        .push(RelayRequest {
            method: "GET".into(),
            uri: format!("/operator/communities/availability?host={host}"),
            authorization: header(&headers),
            body: Value::Null,
        });
    // Anything containing "taken" is occupied.
    Json(json!({
        "host": host,
        "normalized_host": host,
        "available": !host.contains("taken"),
        "community_id": Value::Null,
    }))
}

async fn relay_create(
    State(relay): State<FakeRelay>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (axum::http::StatusCode, Json<Value>) {
    relay
        .requests
        .lock()
        .expect("relay log")
        .push(RelayRequest {
            method: "POST".into(),
            uri: "/operator/communities".into(),
            authorization: header(&headers),
            body: body.clone(),
        });
    let mode = *relay.mode.lock().expect("relay mode");
    match mode {
        RelayMode::Created => (
            axum::http::StatusCode::OK,
            Json(json!({
                "community_id": uuid::Uuid::new_v4().to_string(),
                "host": body["host"],
                "status": "created",
                "owner_pubkey": body["initial_owner_pubkey"],
            })),
        ),
        RelayMode::HostExists => (
            axum::http::StatusCode::CONFLICT,
            Json(json!({ "error": "community already exists" })),
        ),
        RelayMode::Failing => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "operator community persistence failed" })),
        ),
    }
}

fn header(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
}

impl Harness {
    pub fn email(&self) -> String {
        format!("user-{}@test.invalid", uuid::Uuid::new_v4())
    }

    pub fn set_relay_mode(&self, mode: RelayMode) {
        *self.relay_mode.lock().expect("relay mode") = mode;
    }

    pub async fn post(&self, path: &str, body: Value, token: Option<&str>) -> (u16, Value) {
        let mut req = self.client.post(format!("{}{path}", self.base)).json(&body);
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        let res = req.send().await.expect("request");
        let status = res.status().as_u16();
        let body = res.json::<Value>().await.unwrap_or(Value::Null);
        (status, body)
    }

    pub async fn get(&self, path: &str, token: Option<&str>) -> (u16, Value) {
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
    pub async fn start(&self, email: &str) -> String {
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

    pub async fn verify(&self, email: &str, code: &str) -> (u16, Value) {
        self.post(
            "/v1/login/verify",
            json!({ "email": email, "code": code }),
            None,
        )
        .await
    }

    /// Fresh account; returns (session token, pubkey hex).
    pub async fn login(&self) -> (String, String) {
        let email = self.email();
        let code = self.start(&email).await;
        let (status, body) = self.verify(&email, &code).await;
        assert_eq!(status, 200, "{body}");
        (
            body["session_token"].as_str().expect("token").to_owned(),
            body["pubkey"].as_str().expect("pubkey").to_owned(),
        )
    }
}
