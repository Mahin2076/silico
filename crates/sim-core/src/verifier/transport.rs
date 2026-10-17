use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{future::Future, pin::Pin, time::Duration};

pub const DEFAULT_MODEL: &str = "nvidia/NVIDIA-Nemotron-3-Super-120B-A12B";
pub const MAX_RESPONSE_BYTES: usize = 65_536;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransportIdentity {
    pub mode: String,
    pub provider: String,
    pub model: String,
}

pub struct Completion {
    pub text: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("missing verifier configuration")]
    MissingConfiguration,
    #[error("invalid verifier configuration")]
    InvalidConfiguration,
    #[error("verifier request timed out")]
    Timeout,
    #[error("verifier transport failed")]
    Network,
    #[error("verifier provider returned HTTP {0}")]
    HttpStatus(u16),
    #[error("verifier response exceeds the size limit")]
    ResponseTooLarge,
    #[error("verifier provider returned malformed or incomplete output")]
    MalformedResponse,
    #[error("verifier provider returned a different model")]
    ModelMismatch,
}

impl TransportError {
    pub fn code(self) -> &'static str {
        match self {
            Self::MissingConfiguration => "missing_configuration",
            Self::InvalidConfiguration => "invalid_configuration",
            Self::Timeout => "timeout",
            Self::Network => "network_error",
            Self::HttpStatus(401 | 403) => "authentication_failed",
            Self::HttpStatus(429) => "rate_limited",
            Self::HttpStatus(_) => "provider_http_error",
            Self::ResponseTooLarge => "response_too_large",
            Self::MalformedResponse => "malformed_provider_response",
            Self::ModelMismatch => "model_mismatch",
        }
    }
}

pub type CompletionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Completion, TransportError>> + Send + 'a>>;

/// P1 may adapt its shared provider here. A fixture implementation MUST identify
/// itself as `mock`; no fixture fallback is supplied by the live implementation.
pub trait VerifierTransport: Send + Sync {
    fn identity(&self) -> TransportIdentity;
    fn complete<'a>(&'a self, system: &'a str, user: &'a str) -> CompletionFuture<'a>;
}

/// A bounded verifier-only adapter. It does not change P1's shared ModelClient,
/// model enum, cache, production config, or prediction engine.
pub struct CrusoeTransport {
    client: reqwest::Client,
    key: String,
    endpoint: String,
    model: String,
    mode: String,
}

impl std::fmt::Debug for CrusoeTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrusoeTransport")
            .field("model", &self.model)
            .field("mode", &self.mode)
            .field("key", &"[REDACTED]")
            .finish()
    }
}

impl CrusoeTransport {
    pub fn from_env() -> Result<Self, TransportError> {
        let key =
            std::env::var("CRUSOE_API_KEY").map_err(|_| TransportError::MissingConfiguration)?;
        let model = std::env::var("SILICO_VERIFIER_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        Self::new(key, model)
    }

    pub fn new(key: String, model: String) -> Result<Self, TransportError> {
        Self::build(
            key,
            model,
            "https://api.inference.crusoecloud.com/v1/chat/completions".into(),
            "live".into(),
        )
    }

    fn build(
        key: String,
        model: String,
        endpoint: String,
        mode: String,
    ) -> Result<Self, TransportError> {
        if key.is_empty()
            || key.chars().any(char::is_whitespace)
            || !model.starts_with("nvidia/")
            || !model.to_ascii_lowercase().contains("nemotron")
            || model.len() > 160
            || model.chars().any(char::is_control)
        {
            return Err(TransportError::InvalidConfiguration);
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| TransportError::InvalidConfiguration)?;
        Ok(Self {
            client,
            key,
            endpoint,
            model,
            mode,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(endpoint: String) -> Self {
        Self::build(
            "fixture-key-not-a-secret".into(),
            DEFAULT_MODEL.into(),
            endpoint,
            "mock".into(),
        )
        .unwrap()
    }
}

fn classify(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Network
    }
}

pub(crate) fn request_body(model: &str, system: &str, user: &str) -> serde_json::Value {
    json!({
        "model":model,
        "messages":[{"role":"system","content":system},{"role":"user","content":user}],
        "max_tokens":4096,"temperature":0,
        "response_format":{"type":"json_object"},
        "chat_template_kwargs":{"enable_thinking":false}
    })
}

pub(crate) fn decode_completion(
    bytes: &[u8],
    expected_model: &str,
) -> Result<Completion, TransportError> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(TransportError::ResponseTooLarge);
    }
    let body: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| TransportError::MalformedResponse)?;
    let model = body["model"]
        .as_str()
        .ok_or(TransportError::MalformedResponse)?;
    if model != expected_model {
        return Err(TransportError::ModelMismatch);
    }
    let choice = &body["choices"][0];
    if choice["finish_reason"].as_str() != Some("stop") {
        return Err(TransportError::MalformedResponse);
    }
    let text = choice["message"]["content"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or(TransportError::MalformedResponse)?;
    Ok(Completion {
        text: text.into(),
        model: model.into(),
    })
}

impl VerifierTransport for CrusoeTransport {
    fn identity(&self) -> TransportIdentity {
        TransportIdentity {
            mode: self.mode.clone(),
            provider: "crusoe-managed-inference".into(),
            model: self.model.clone(),
        }
    }

    fn complete<'a>(&'a self, system: &'a str, user: &'a str) -> CompletionFuture<'a> {
        Box::pin(async move {
            let mut response = self
                .client
                .post(&self.endpoint)
                .bearer_auth(&self.key)
                .json(&request_body(&self.model, system, user))
                .send()
                .await
                .map_err(classify)?;
            if !response.status().is_success() {
                // Upstream bodies can contain credentials or private prompts.
                // Report the status, never log or return the response body.
                return Err(TransportError::HttpStatus(response.status().as_u16()));
            }
            if response
                .content_length()
                .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
            {
                return Err(TransportError::ResponseTooLarge);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(classify)? {
                if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                    return Err(TransportError::ResponseTooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            decode_completion(&bytes, &self.model)
        })
    }
}
