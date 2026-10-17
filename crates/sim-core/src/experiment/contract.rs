use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub scenario_id: String,
    pub question: String,
    pub description: String,
    pub options: Vec<String>,
    pub assumptions: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub check_id: String,
    pub kind: String,
    pub requirement: String,
    pub scenario_ids: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub goal: String,
    pub city: String,
    pub simulation_id: String,
    pub workspace: String,
    pub audience_hash: String,
    pub context_hash: String,
    pub as_of_date: String,
    pub model: String,
    pub scenarios: Vec<Scenario>,
    pub checks: Vec<Check>,
    pub max_repair_rounds: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probability {
    pub option: String,
    pub probability: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResult {
    pub scenario_id: String,
    pub attempt: u32,
    pub status: String,
    pub audience_hash: String,
    pub context_hash: String,
    pub model: String,
    pub as_of_date: String,
    pub expected_archetype_ids: Vec<String>,
    pub answered_archetype_ids: Vec<String>,
    pub distribution: Vec<Probability>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub check_id: String,
    pub status: String,
    pub scenario_ids: Vec<String>,
    pub evidence: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repair {
    pub action: String,
    pub scenario_ids: Vec<String>,
    pub check_ids: Vec<String>,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierRequest {
    pub schema_version: String,
    pub run_id: String,
    pub plan_hash: String,
    pub round: u32,
    pub plan: Plan,
    pub results: Vec<ScenarioResult>,
    pub deterministic_checks: Vec<Finding>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierResponse {
    pub schema_version: String,
    pub run_id: String,
    pub plan_hash: String,
    pub round: u32,
    pub verdict: String,
    pub checks: Vec<Finding>,
    pub repair_requests: Vec<Repair>,
    pub summary: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Archetype {
    pub id: String,
    pub profile: String,
    pub weight: f64,
    pub member_ids: Vec<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupAnswer {
    pub archetype_id: String,
    pub probabilities: Vec<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub sequence: usize,
    pub at: String,
    pub stage: String,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchAssessment {
    pub assessment: String,
    pub source_ids: Vec<String>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchFollowup {
    pub trigger_request_hash: String,
    pub check_ids: Vec<String>,
    pub objection: String,
    pub query: crate::verifier::ResearchQuery,
    pub web: Option<crate::verifier::WebResearch>,
    #[serde(default)]
    pub assessment: Option<ResearchAssessment>,
    #[serde(default = "legacy_assessment_attempts")]
    pub assessment_attempts: u32,
    pub error: Option<String>,
}
// Older research receipts made at most one assessment request.
fn legacy_assessment_attempts() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Run {
    pub run_id: String,
    pub workspace: String,
    pub request_hash: String,
    pub mode: String,
    pub status: String,
    pub round: u32,
    pub goal: String,
    pub city: String,
    pub plan: Option<Plan>,
    pub plan_hash: String,
    pub context: Value,
    pub archetypes: Vec<Archetype>,
    pub residents: usize,
    pub results: Vec<ScenarioResult>,
    pub answers: std::collections::BTreeMap<String, Vec<GroupAnswer>>,
    pub checks: Vec<Finding>,
    pub verifier: Option<VerifierResponse>,
    #[serde(default)]
    pub p2_reviews: Vec<super::p2::Review>,
    #[serde(default)]
    pub research: Option<ResearchFollowup>,
    pub events: Vec<Event>,
    pub usage: crate::crusoe::Usage,
    pub error: Option<String>,
    pub inject_missing: bool,
}
impl Run {
    pub fn event(&mut self, stage: &str, message: impl Into<String>) {
        self.events.push(Event {
            sequence: self.events.len() + 1,
            at: chrono::Utc::now().to_rfc3339(),
            stage: stage.into(),
            message: message.into(),
        });
    }
    pub fn verifier_request(&self) -> Option<VerifierRequest> {
        Some(VerifierRequest {
            schema_version: "0.1".into(),
            run_id: self.run_id.clone(),
            plan_hash: self.plan_hash.clone(),
            round: self.round,
            plan: self.plan.clone()?,
            results: self.results.clone(),
            deterministic_checks: self.checks.clone(),
        })
    }
}
