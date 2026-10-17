use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const PROMPT_VERSION: &str = "silico-p2-verifier-v1";
pub const MAX_REPAIR_ROUND: u8 = 2;
pub const LOCAL_CHECKS: &[&str] = &[
    "p2.input_integrity",
    "p2.scenario_coverage",
    "p2.context_consistency",
    "p2.audience_consistency",
    "p2.deterministic_checks",
    "p2.evidence_completeness",
];
pub const SEMANTIC_CHECKS: &[&str] = &[
    "p2.goal_alignment",
    "p2.explicit_assumptions",
    "p2.conclusion_support",
];

/// P1 freezes these values before executing any scenario. Hashes are caller
/// attestations: the verifier checks consistency, not underlying Census truth.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenContext {
    pub population_hash: String,
    pub model: String,
    pub as_of_date: String,
    pub evidence_hash: String,
    pub seed: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    pub description: String,
    pub audience_id: String,
    /// Exact immutable question, framing, options, stimulus, and audience filters.
    pub inputs: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckKind {
    Deterministic,
    Semantic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionCheck {
    pub id: String,
    pub description: String,
    pub kind: CheckKind,
    pub required: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentPlan {
    pub context: FrozenContext,
    pub assumptions: Vec<String>,
    pub scenarios: Vec<Scenario>,
    pub completion_checks: Vec<CompletionCheck>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultStatus {
    Complete,
    Failed,
    Pending,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResult {
    pub id: String,
    pub scenario_id: String,
    pub plan_hash: String,
    pub context: FrozenContext,
    pub audience_id: String,
    pub status: ResultStatus,
    /// The original simulation receipt, not a model-authored replacement.
    pub payload: Value,
}

/// An RFC 6901 JSON pointer into this exact VerificationRequest. Model-generated
/// prose never counts as evidence by itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub pointer: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeterministicFinding {
    pub check_id: String,
    pub passed: bool,
    pub detail: String,
    pub scenario_ids: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationRequest {
    pub schema_version: u32,
    pub run_id: String,
    pub plan_hash: String,
    pub repair_round: u8,
    pub goal: String,
    pub plan: ExperimentPlan,
    pub results: Vec<ScenarioResult>,
    pub deterministic_findings: Vec<DeterministicFinding>,
    pub conclusions: Vec<String>,
}

/// Object keys are sorted recursively. Array ordering is deliberately significant.
/// Use this function rather than hashing raw input JSON or pretty-printed text.
pub fn plan_hash(goal: &str, plan: &ExperimentPlan) -> String {
    hash_value(serde_json::json!({"schema_version":SCHEMA_VERSION,"goal":goal,"plan":plan}))
}

pub fn request_hash(request: &VerificationRequest) -> String {
    hash_value(serde_json::to_value(request).expect("typed request serializes"))
}

fn hash_value(value: Value) -> String {
    fn canonical(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let sorted: std::collections::BTreeMap<_, _> = map.into_iter().collect();
                Value::Object(
                    sorted
                        .into_iter()
                        .map(|(key, value)| (key, canonical(value)))
                        .collect(),
                )
            }
            Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
            other => other,
        }
    }
    hex::encode(Sha256::digest(
        serde_json::to_vec(&canonical(value)).expect("JSON value serializes"),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Pass,
    Repair,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pass,
    Repair,
    Blocked,
    NotReviewed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub check_id: String,
    pub status: CheckStatus,
    pub summary: String,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    RerunScenario,
    RevisePlan,
    RequestClarification,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRequest {
    pub id: String,
    pub action: RepairAction,
    pub check_ids: Vec<String>,
    pub scenario_ids: Vec<String>,
    pub reason: String,
}

/// This is the strict model-output contract. Local gates are added by Rust;
/// Nemotron cannot replace or downgrade deterministic findings.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelReview {
    pub schema_version: u32,
    pub run_id: String,
    pub plan_hash: String,
    pub request_hash: String,
    pub repair_round: u8,
    pub verdict: Verdict,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub repair_requests: Vec<RepairRequest>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReceipt {
    pub mode: String,
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
    pub model_calls: u32,
    pub elapsed_ms: u64,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationDecision {
    pub schema_version: u32,
    pub run_id: String,
    pub plan_hash: String,
    pub request_hash: String,
    pub repair_round: u8,
    pub verdict: Verdict,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub repair_requests: Vec<RepairRequest>,
    pub receipt: VerificationReceipt,
}

impl VerificationDecision {
    /// Consumers should use this rather than accepting a bare model verdict.
    pub fn permits_completion(&self, request: &VerificationRequest) -> bool {
        self.verdict == Verdict::Pass
            && self.schema_version == SCHEMA_VERSION
            && self.run_id == request.run_id
            && self.plan_hash == plan_hash(&request.goal, &request.plan)
            && self.plan_hash == request.plan_hash
            && self.request_hash == request_hash(request)
            && self.repair_round == request.repair_round
            && self.repair_round <= MAX_REPAIR_ROUND
            && self.receipt.mode == "live"
            && self.receipt.model_calls == 1
            && self.receipt.error_code.is_none()
            && self.repair_requests.is_empty()
            && !self.findings.is_empty()
            && self.findings.iter().all(|f| f.status == CheckStatus::Pass)
            && LOCAL_CHECKS
                .iter()
                .chain(SEMANTIC_CHECKS.iter())
                .all(|id| self.findings.iter().filter(|f| f.check_id == *id).count() == 1)
            && request
                .plan
                .completion_checks
                .iter()
                .filter(|c| c.required)
                .all(|c| self.findings.iter().filter(|f| f.check_id == c.id).count() == 1)
    }
}
