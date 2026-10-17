//! Shared Crusoe transport for P1 planning/simulation and P2 verification.
//! Model IDs are explicit; no fallback to a different provider or model.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Debug)]
pub struct BudgetExhausted(pub &'static str);
impl std::fmt::Display for BudgetExhausted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for BudgetExhausted {}

#[derive(Clone)]
pub struct CrusoeClient {
    http: reqwest::Client,
    base: String,
    key: String,
    pub model: String,
    reasoning_effort: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Usage {
    pub requests: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub reserved_tokens: u64,
    #[serde(default)]
    pub responses_without_usage: u32,
}
impl CrusoeClient {
    pub fn from_env() -> Result<Self> {
        let mut client = Self::new(
            std::env::var("CRUSOE_BASE_URL")
                .unwrap_or_else(|_| "https://api.inference.crusoecloud.com/v1".into()),
            std::env::var("CRUSOE_API_KEY").unwrap_or_default(),
            std::env::var("CRUSOE_MODEL").unwrap_or_default(),
        )?;
        let effort = std::env::var("CRUSOE_REASONING_EFFORT").unwrap_or_default();
        let effort = effort.trim();
        if !effort.is_empty() {
            if !matches!(effort, "none" | "low" | "medium" | "high" | "max") {
                bail!("Invalid CRUSOE_REASONING_EFFORT");
            }
            client.reasoning_effort = Some(effort.into());
        }
        Ok(client)
    }
    pub fn new(base: String, key: String, model: String) -> Result<Self> {
        let url = reqwest::Url::parse(&base)?;
        let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
        if url.scheme() != "https" && !(url.scheme() == "http" && local) {
            bail!("Crusoe base must be HTTPS (HTTP only for loopback tests)");
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("Invalid Crusoe base URL");
        }
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .connect_timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base: base.trim_end_matches('/').into(),
            key,
            model,
            reasoning_effort: None,
        })
    }
    pub fn configured(&self) -> bool {
        !self.key.trim().is_empty() && !self.model.trim().is_empty()
    }
    /// One HTTP attempt. The caller owns scenario retries and total request budgets.
    pub async fn json(&self, system: &str, data: &Value, usage: &mut Usage) -> Result<Value> {
        if !self.configured() {
            bail!(
                "Set CRUSOE_API_KEY and CRUSOE_MODEL on the server before running live experiments"
            );
        }
        if usage.requests >= 48 {
            return Err(BudgetExhausted("Experiment request budget exhausted (48)").into());
        }
        // UTF-8 bytes conservatively bound ordinary text tokens; reserve output even on failure.
        // This is a local admission budget, not an invoice or a provider tokenizer.
        let user = serde_json::to_string(data)?;
        let reservation = (system.len() + user.len() + 256) as u64 + 4096;
        if usage.reserved_tokens + reservation > 180_000 {
            return Err(BudgetExhausted("Experiment token reservation budget exhausted").into());
        }
        usage.reserved_tokens += reservation;
        usage.requests += 1;
        let mut request = json!({"model":self.model,"messages":[{"role":"system","content":system},{"role":"user","content":user}],"max_tokens":4096,"stream":false});
        // Reasoning tokens share the output budget. Set an explicit effort only
        // when configured for a model that supports it; never raise the budget.
        if let Some(effort) = &self.reasoning_effort {
            request["reasoning_effort"] = json!(effort);
        }
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base))
            .bearer_auth(&self.key)
            .json(&request)
            .send()
            .await
            .context("Crusoe request failed")?;
        if !response.status().is_success() {
            bail!(
                "Crusoe returned HTTP {}; no replacement model was used",
                response.status().as_u16()
            );
        }
        let body: Value = response.json().await.context("Invalid Crusoe response")?;
        if body["usage"]["prompt_tokens"].as_u64().is_none()
            || body["usage"]["completion_tokens"].as_u64().is_none()
        {
            usage.responses_without_usage += 1;
        }
        usage.input_tokens += body["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        usage.output_tokens += body["usage"]["completion_tokens"].as_u64().unwrap_or(0);
        if body["choices"][0]["finish_reason"].as_str() == Some("length") {
            bail!("Crusoe response exceeded output budget");
        }
        let content = body["choices"][0]["message"]["content"]
            .as_str()
            .context("Crusoe returned no answer content")?
            .trim();
        let content = content
            .strip_prefix("```json")
            .or_else(|| content.strip_prefix("```"))
            .and_then(|v| v.strip_suffix("```"))
            .unwrap_or(content)
            .trim();
        serde_json::from_str(content).context("Crusoe answer must be valid JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn explicit_reasoning_preserves_output_budget_and_parses_fenced_json() {
        let router = axum::Router::new().route(
            "/v1/chat/completions",
            axum::routing::post(|axum::Json(body): axum::Json<Value>| async move {
                assert_eq!(body["reasoning_effort"], "low");
                assert_eq!(body["max_tokens"], 4096);
                assert_eq!(body["model"], "fixture");
                axum::Json(json!({
                    "choices": [{"finish_reason":"stop", "message":{"content":"```json\n{\"ok\":true}\n```"}}],
                    "usage":{"prompt_tokens":10,"completion_tokens":5}
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let mut client = CrusoeClient::new(base, "fixture-key".into(), "fixture".into()).unwrap();
        client.reasoning_effort = Some("low".into());
        let mut usage = Usage::default();
        let result = client
            .json("Return JSON", &json!({"goal":"test"}), &mut usage)
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(usage.requests, 1);
        assert_eq!(usage.output_tokens, 5);
        assert!(usage.reserved_tokens >= 4096);
        task.abort();
    }
}
