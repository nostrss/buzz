//! Community creation API integration tests. See `common` for the harness
//! (fake Resend + fake relay) and the `ACCOUNT_TEST_DATABASE_URL` skip rule.

mod common;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use common::{harness, RelayMode, RelayRequest};
use nostr::JsonUtil;
use serde_json::json;
use sha2::{Digest, Sha256};

fn unique_name() -> String {
    format!("team-{}", &uuid::Uuid::new_v4().simple().to_string()[..8])
}

/// Decode the NIP-98 header and assert it signs `method url` (+ body).
fn assert_nip98(req: &RelayRequest, expected_url: &str) {
    let header = req.authorization.as_deref().expect("Authorization header");
    let encoded = header.strip_prefix("Nostr ").expect("Nostr scheme");
    let json = STANDARD.decode(encoded).expect("base64");
    let event = nostr::Event::from_json(json).expect("event json");
    event.verify().expect("valid signature");
    assert_eq!(event.kind, nostr::Kind::HttpAuth);
    assert_eq!(
        event.pubkey.to_hex(),
        nostr::Keys::parse("2222222222222222222222222222222222222222222222222222222222222222")
            .expect("operator key")
            .public_key()
            .to_hex()
    );
    let tag = |name: &str| -> Option<String> {
        event
            .tags
            .iter()
            .map(|t| t.as_slice())
            .find(|t| t.first().map(String::as_str) == Some(name))
            .and_then(|t| t.get(1).cloned())
    };
    assert_eq!(tag("u").as_deref(), Some(expected_url));
    assert_eq!(tag("method").as_deref(), Some(req.method.as_str()));
    if req.body.is_null() {
        assert!(tag("payload").is_none());
    } else {
        let body = serde_json::to_vec(&req.body).expect("body");
        assert_eq!(
            tag("payload").as_deref(),
            Some(hex::encode(Sha256::digest(&body)).as_str())
        );
    }
}

#[tokio::test]
async fn check_normalizes_validates_and_asks_the_relay() {
    let Some(h) = harness().await else { return };
    let (token, _) = h.login().await;

    let (status, body) = h
        .post(
            "/v1/communities/check",
            json!({ "name": "  My  Team " }),
            Some(&token),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["normalized"], "my-team");
    assert_eq!(body["host"], "my-team.app.test.invalid");
    assert_eq!(body["available"], true);
    assert!(body.get("reason").is_none());
    let last = h
        .relay_requests
        .lock()
        .expect("log")
        .last()
        .cloned()
        .expect("relay called");
    assert_eq!(last.method, "GET");
    assert_nip98(
        &last,
        &format!(
            "{}/operator/communities/availability?host=my-team.app.test.invalid",
            h.relay_origin
        ),
    );

    let (_, body) = h
        .post(
            "/v1/communities/check",
            json!({ "name": "taken-team" }),
            Some(&token),
        )
        .await;
    assert_eq!(body["available"], false);
    assert_eq!(body["reason"], "taken");

    // Invalid names never reach the relay.
    let before = h.relay_requests.lock().expect("log").len();
    for (name, reason) in [
        ("ab", "too_short"),
        ("admin", "reserved"),
        ("caf\u{e9}", "invalid_characters"),
        ("-x-", "leading_or_trailing_hyphen"),
    ] {
        let (status, body) = h
            .post(
                "/v1/communities/check",
                json!({ "name": name }),
                Some(&token),
            )
            .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["available"], false, "{name}");
        assert_eq!(body["reason"], reason, "{name}");
    }
    assert_eq!(h.relay_requests.lock().expect("log").len(), before);

    let (status, _) = h
        .post("/v1/communities/check", json!({ "name": "x" }), None)
        .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn create_signs_the_operator_call_and_records_the_community() {
    let Some(h) = harness().await else { return };
    let (token, pubkey) = h.login().await;
    let name = unique_name();
    let host = format!("{name}.app.test.invalid");

    let (status, body) = h
        .post("/v1/communities", json!({ "name": name }), Some(&token))
        .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["host"], host);
    assert_eq!(body["relay_url"], format!("wss://{host}"));
    let community_id = body["community_id"]
        .as_str()
        .expect("community_id")
        .to_owned();

    let last = h
        .relay_requests
        .lock()
        .expect("log")
        .last()
        .cloned()
        .expect("relay called");
    assert_eq!(last.method, "POST");
    assert_eq!(last.body["host"], host);
    assert_eq!(last.body["initial_owner_pubkey"], pubkey);
    assert_eq!(last.body["create_only"], true);
    assert_nip98(&last, &format!("{}/operator/communities", h.relay_origin));

    let (stored_name, stored_relay_id): (String, uuid::Uuid) =
        sqlx::query_as("SELECT name, relay_community_id FROM communities WHERE host = $1")
            .bind(&host)
            .fetch_one(&h.pool)
            .await
            .expect("community row");
    assert_eq!(stored_name, name);
    assert_eq!(stored_relay_id.to_string(), community_id);

    let (_, me) = h.get("/v1/me", Some(&token)).await;
    assert_eq!(
        me["communities"],
        json!([{ "host": host, "role": "owner" }])
    );
    assert_eq!(me["can_create_community"], false);

    // hosts/check knows the new host.
    let (status, _) = h.get(&format!("/v1/hosts/check?host={host}"), None).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn one_community_per_account() {
    let Some(h) = harness().await else { return };
    let (token, _) = h.login().await;
    let (status, _) = h
        .post(
            "/v1/communities",
            json!({ "name": unique_name() }),
            Some(&token),
        )
        .await;
    assert_eq!(status, 201);
    let before = h.relay_requests.lock().expect("log").len();

    let (status, body) = h
        .post(
            "/v1/communities",
            json!({ "name": unique_name() }),
            Some(&token),
        )
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["error"], "community_limit_reached");
    assert_eq!(
        h.relay_requests.lock().expect("log").len(),
        before,
        "relay not called"
    );
}

#[tokio::test]
async fn relay_failure_leaves_no_row() {
    let Some(h) = harness().await else { return };
    let (token, _) = h.login().await;

    h.set_relay_mode(RelayMode::Failing);
    let name = unique_name();
    let (status, body) = h
        .post("/v1/communities", json!({ "name": name }), Some(&token))
        .await;
    assert_eq!(status, 502, "{body}");
    assert_eq!(body["error"], "relay_error");
    assert_eq!(body["relay_status"], 500);

    h.set_relay_mode(RelayMode::HostExists);
    let (status, body) = h
        .post("/v1/communities", json!({ "name": name }), Some(&token))
        .await;
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["error"], "host_taken");

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM communities WHERE host = $1")
        .bind(format!("{name}.app.test.invalid"))
        .fetch_one(&h.pool)
        .await
        .expect("count");
    assert_eq!(rows, 0);

    // Still allowed to create once the relay is healthy.
    h.set_relay_mode(RelayMode::Created);
    let (status, _) = h
        .post("/v1/communities", json!({ "name": name }), Some(&token))
        .await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn invalid_names_are_rejected_before_the_relay() {
    let Some(h) = harness().await else { return };
    let (token, _) = h.login().await;
    let before = h.relay_requests.lock().expect("log").len();
    let (status, body) = h
        .post("/v1/communities", json!({ "name": "relay" }), Some(&token))
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error"], "invalid_name");
    assert_eq!(body["reason"], "reserved");
    assert_eq!(h.relay_requests.lock().expect("log").len(), before);
}

#[tokio::test]
async fn hosts_check_answers_caddy() {
    let Some(h) = harness().await else { return };
    for host in [
        "app.test.invalid",
        "auth.app.test.invalid",
        "AUTH.app.test.invalid.",
    ] {
        let (status, _) = h.get(&format!("/v1/hosts/check?host={host}"), None).await;
        assert_eq!(status, 200, "{host}");
    }
    for host in [
        "nobody.app.test.invalid",
        "evil.example.com",
        "test.invalid",
    ] {
        let (status, _) = h.get(&format!("/v1/hosts/check?host={host}"), None).await;
        assert_eq!(status, 404, "{host}");
    }
    let (status, _) = h.get("/v1/hosts/check", None).await;
    assert_eq!(status, 400);
}
