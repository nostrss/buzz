//! Outbound email through the Resend HTTP API. One request shape, so no SDK.

use std::time::Duration;

/// Resend client bound to the configured sender.
#[derive(Clone)]
pub struct Mailer {
    client: reqwest::Client,
    endpoint: url::Url,
    api_key: String,
    from: String,
}

impl Mailer {
    /// Build a mailer. `base_url` is `https://api.resend.com/` in production.
    pub fn new(base_url: &url::Url, api_key: String, from: String) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?,
            endpoint: base_url.join("emails")?,
            api_key,
            from,
        })
    }

    /// Send the login code to `to`. The code is never logged here.
    pub async fn send_login_code(&self, to: &str, code: &str) -> anyhow::Result<()> {
        let body = serde_json::json!({
            "from": self.from,
            "to": [to],
            "subject": format!("{code} is your Buzz login code"),
            "text": format!(
                "Your Buzz login code is {code}. It expires in 10 minutes.\n\nIf you did not request it, ignore this email."
            ),
            "html": format!(
                "<p>Your Buzz login code is</p><p style=\"font-size:28px;font-weight:700;letter-spacing:4px\">{code}</p><p>It expires in 10 minutes. If you did not request it, ignore this email.</p>"
            ),
        });
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        anyhow::ensure!(status.is_success(), "resend responded {status}");
        Ok(())
    }
}
