//! One bounded You.com search, with immutable source provenance. Search snippets
//! are untrusted external evidence, never instructions or replacement poll data.
//! Domain filters support source selection, not a claim that a source is correct.
//! The cutoff excludes known later publication dates. Undated snippets cannot
//! establish historical availability; search snippets are not crawled full pages.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Duration};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchQuery {
    pub query: String,
    pub include_domains: Vec<String>,
}
impl ResearchQuery {
    pub fn new(query: String, include_domains: Vec<String>) -> Result<Self, TransportError> {
        let value = Self {
            query,
            include_domains,
        };
        if !value.valid() {
            return Err(TransportError::InvalidConfiguration);
        }
        Ok(value)
    }
    fn valid(&self) -> bool {
        (3..=600).contains(&self.query.len())
            && self.query.trim().len() >= 3
            && !self.query.chars().any(char::is_control)
            && self.include_domains.len() <= 5
            && self.include_domains.iter().all(|d| valid_domain(d))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSource {
    pub id: String,
    pub title: String,
    pub url: String,
    pub snippets: Vec<String>,
    pub publication_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebResearch {
    pub provider: String,
    pub mode: String,
    pub query: ResearchQuery,
    pub as_of_date: String,
    pub fetched_at: String,
    /// ready, empty, or failed; never a silent offline fallback.
    pub status: String,
    pub search_calls: u32,
    pub error_code: Option<String>,
    pub sources: Vec<WebSource>,
}
impl WebResearch {
    pub(crate) fn is_ready(&self, as_of: &str) -> bool {
        let mut ids = BTreeSet::new();
        let mut urls = BTreeSet::new();
        self.provider == "you.com"
            && matches!(self.mode.as_str(), "live" | "mock")
            && self.query.valid()
            && valid_date(as_of)
            && self.as_of_date == as_of
            && chrono::DateTime::parse_from_rfc3339(&self.fetched_at).is_ok_and(|date| {
                date.with_timezone(&chrono::Utc)
                    .date_naive()
                    .format("%Y-%m-%d")
                    .to_string()
                    .as_str()
                    <= as_of
            })
            && self.status == "ready"
            && self.search_calls == 1
            && self.error_code.is_none()
            && (1..=5).contains(&self.sources.len())
            && self.sources.iter().all(|s| {
                matches!(
                    s.id.as_str(),
                    "web-1" | "web-2" | "web-3" | "web-4" | "web-5"
                ) && ids.insert(&s.id)
                    && safe_url(&s.url).is_some_and(|u| urls.insert(u.to_string()))
                    && !s.title.trim().is_empty()
                    && s.title.len() <= 600
                    && (1..=2).contains(&s.snippets.len())
                    && s.snippets
                        .iter()
                        .all(|v| !v.trim().is_empty() && v.len() <= 2400)
                    && s.publication_date
                        .as_ref()
                        .map_or(true, |d| valid_date(d) && d.as_str() <= as_of)
                    && domain_allowed(&s.url, &self.query.include_domains)
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchedVerification {
    /// Gate against this enriched immutable request, not the original request.
    pub request: VerificationRequest,
    pub decision: VerificationDecision,
}
impl ResearchedVerification {
    pub fn permits_completion(&self) -> bool {
        self.decision.permits_completion(&self.request)
    }
}

pub struct YouSearchClient {
    client: reqwest::Client,
    key: String,
    endpoint: String,
    mode: String,
}
impl std::fmt::Debug for YouSearchClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("YouSearchClient")
            .field("key", &"[REDACTED]")
            .field("mode", &self.mode)
            .finish()
    }
}
impl YouSearchClient {
    pub fn from_env() -> Result<Self, TransportError> {
        Self::new(std::env::var("YOU_API_KEY").map_err(|_| TransportError::MissingConfiguration)?)
    }
    pub fn new(key: String) -> Result<Self, TransportError> {
        Self::build(key, "https://ydc-index.io/v1/search".into(), "live".into())
    }
    fn build(key: String, endpoint: String, mode: String) -> Result<Self, TransportError> {
        if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(TransportError::InvalidConfiguration);
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| TransportError::InvalidConfiguration)?;
        Ok(Self {
            client,
            key,
            endpoint,
            mode,
        })
    }
    #[cfg(test)]
    pub(crate) fn for_test(endpoint: String) -> Self {
        Self::build("fixture-you-key".into(), endpoint, "mock".into()).unwrap()
    }
    /// At most one request, five results, no crawling, redirects, or retries.
    pub async fn search(&self, query: &ResearchQuery, as_of: &str) -> WebResearch {
        let mut research = WebResearch {
            provider: "you.com".into(),
            mode: self.mode.clone(),
            query: query.clone(),
            as_of_date: as_of.into(),
            fetched_at: if self.mode == "mock" {
                format!("{as_of}T00:00:00+00:00")
            } else {
                chrono::Utc::now().to_rfc3339()
            },
            status: "failed".into(),
            search_calls: 0,
            error_code: None,
            sources: vec![],
        };
        if !query.valid() || !valid_date(as_of) {
            research.error_code = Some("invalid_search_configuration".into());
            return research;
        }
        // Do not leak present-day facts into a frozen historical validation.
        if self.mode == "live"
            && chrono::Utc::now()
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
                .as_str()
                > as_of
        {
            research.error_code = Some("historical_search_not_allowed".into());
            return research;
        }
        research.search_calls = 1;
        match self.fetch(query, as_of).await {
            Ok(sources) => {
                research.status = if sources.is_empty() { "empty" } else { "ready" }.into();
                research.sources = sources;
            }
            Err(error) => research.error_code = Some(error.code().into()),
        }
        research
    }
    async fn fetch(
        &self,
        query: &ResearchQuery,
        as_of: &str,
    ) -> Result<Vec<WebSource>, TransportError> {
        let mut body = serde_json::json!({"query":query.query,"count":5,
            "freshness":format!("1900-01-01to{as_of}"),"safesearch":"strict","strict_operators":true});
        if !query.include_domains.is_empty() {
            body["include_domains"] = serde_json::json!(query.include_domains);
        }
        let mut response = self
            .client
            .post(&self.endpoint)
            .header("X-API-Key", &self.key)
            .json(&body)
            .send()
            .await
            .map_err(classify)?;
        if !response.status().is_success() {
            return Err(TransportError::HttpStatus(response.status().as_u16()));
        }
        if response.content_length().is_some_and(|n| n > 262_144) {
            return Err(TransportError::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(classify)? {
            if bytes.len() + chunk.len() > 262_144 {
                return Err(TransportError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let data: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| TransportError::MalformedResponse)?;
        let web = data["results"]["web"]
            .as_array()
            .ok_or(TransportError::MalformedResponse)?;
        let mut sources = Vec::new();
        let mut urls = BTreeSet::new();
        for row in web.iter().take(20) {
            let Some(url) = row["url"]
                .as_str()
                .and_then(safe_url)
                .filter(|u| domain_allowed(u.as_str(), &query.include_domains))
            else {
                continue;
            };
            if urls.contains(url.as_str()) {
                continue;
            }
            let date = row["page_age"]
                .as_str()
                .and_then(|v| v.get(..10))
                .filter(|d| valid_date(d))
                .map(str::to_owned);
            if date.as_ref().is_some_and(|d| d.as_str() > as_of) {
                continue;
            }
            let title = truncate(row["title"].as_str().unwrap_or(""), 200, 600);
            let mut snippets: Vec<String> = row["snippets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str())
                .filter(|s| !s.trim().is_empty())
                .take(2)
                .map(|s| truncate(s, 800, 2400))
                .collect();
            if snippets.is_empty() {
                if let Some(d) = row["description"].as_str().filter(|s| !s.trim().is_empty()) {
                    snippets.push(truncate(d, 800, 2400));
                }
            }
            if title.is_empty() || snippets.is_empty() {
                continue;
            }
            urls.insert(url.to_string());
            sources.push(WebSource {
                id: format!("web-{}", sources.len() + 1),
                title,
                url: url.to_string(),
                snippets,
                publication_date: date,
            });
            if sources.len() == 5 {
                break;
            }
        }
        Ok(sources)
    }
}
fn truncate(value: &str, chars: usize, bytes: usize) -> String {
    let mut result: String = value.trim().chars().take(chars).collect();
    if result.len() > bytes {
        let mut end = bytes;
        while !result.is_char_boundary(end) {
            end -= 1;
        }
        result.truncate(end);
    }
    result.trim().to_owned()
}
fn valid_date(value: &str) -> bool {
    value.len() == 10
        && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .is_ok_and(|date| date.format("%Y-%m-%d").to_string() == value)
}
fn valid_domain(value: &str) -> bool {
    value.len() <= 253
        && value.contains('.')
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
fn classify(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else {
        TransportError::Network
    }
}
fn safe_url(value: &str) -> Option<reqwest::Url> {
    if value.len() > 2048 || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let mut url = reqwest::Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?;
    if !valid_domain(host)
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.parse::<std::net::IpAddr>().is_ok()
    {
        return None;
    }
    url.set_fragment(None);
    (url.as_str().len() <= 2048).then_some(url)
}
fn domain_allowed(value: &str, domains: &[String]) -> bool {
    let Some(url) = safe_url(value) else {
        return false;
    };
    let host = url.host_str().unwrap_or("");
    domains.is_empty()
        || domains.iter().any(|d| {
            host.eq_ignore_ascii_case(d)
                || host
                    .to_ascii_lowercase()
                    .ends_with(&format!(".{}", d.to_ascii_lowercase()))
        })
}
