//! Immutable repair obligations bind a follow-up review to its original complaint.
use super::types::*;
use super::TransportError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairContext {
    pub previous_request_hash: String,
    pub run_id: String,
    pub plan_hash: String,
    pub previous_round: u8,
    pub obligations: Vec<RepairObligation>,
    pub previous_conclusions: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairObligation {
    pub repair_id: String,
    pub action: RepairAction,
    pub check_ids: Vec<String>,
    pub scenario_ids: Vec<String>,
    pub reason: String,
    pub before_hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairAssessment {
    pub repair_id: String,
    pub status: CheckStatus,
    pub reasoning: String,
    pub evidence: Vec<EvidenceRef>,
}

impl RepairContext {
    /// Call on the original immutable request and its actual decision, never a
    /// reconstructed request with replacement results. Plan revisions start a new
    /// verification lineage rather than reusing these obligations.
    pub fn from_review(
        previous: &VerificationRequest,
        decision: &VerificationDecision,
    ) -> Result<Self, TransportError> {
        if decision.schema_version != SCHEMA_VERSION
            || previous.schema_version != SCHEMA_VERSION
            || decision.run_id != previous.run_id
            || decision.plan_hash != previous.plan_hash
            || previous.plan_hash != plan_hash(&previous.goal, &previous.plan)
            || decision.request_hash != request_hash(previous)
            || decision.repair_round != previous.repair_round
            || previous.repair_round >= MAX_REPAIR_ROUND
            || decision.verdict != Verdict::Repair
            || decision.receipt.error_code.is_some()
            || decision.repair_requests.is_empty()
            || decision.repair_requests.len() > 128
            || decision.repair_requests.iter().any(|repair| {
                repair.check_ids.iter().any(|id| {
                    decision
                        .findings
                        .iter()
                        .filter(|finding| {
                            finding.check_id == *id
                                && matches!(
                                    finding.status,
                                    CheckStatus::Repair | CheckStatus::Blocked
                                )
                        })
                        .count()
                        != 1
                })
            })
        {
            return Err(TransportError::InvalidConfiguration);
        }
        let context = Self {
            previous_request_hash: decision.request_hash.clone(),
            run_id: previous.run_id.clone(),
            plan_hash: previous.plan_hash.clone(),
            previous_round: previous.repair_round,
            obligations: decision
                .repair_requests
                .iter()
                .map(|r| RepairObligation {
                    repair_id: r.id.clone(),
                    action: r.action,
                    check_ids: r.check_ids.clone(),
                    scenario_ids: r.scenario_ids.clone(),
                    reason: r.reason.clone(),
                    before_hashes: r
                        .scenario_ids
                        .iter()
                        .map(|id| (id.clone(), scenario_hash(previous, id)))
                        .collect(),
                })
                .collect(),
            previous_conclusions: previous.conclusions.clone(),
        };
        // Validate the same contract that a serialized follow-up must satisfy.
        let mut followup = previous.clone();
        followup.repair_round += 1;
        followup.repair_context = Some(context.clone());
        validate_context(&followup).map_err(|_| TransportError::InvalidConfiguration)?;
        Ok(context)
    }
}

fn id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn hash_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn unique(ids: &[String]) -> bool {
    ids.iter().collect::<BTreeSet<_>>().len() == ids.len()
}

/// History is optional for legacy P1 requests. Once supplied it must exactly
/// describe the immediately preceding round of this same run and immutable plan.
pub fn validate_context(request: &VerificationRequest) -> Result<(), &'static str> {
    let Some(context) = &request.repair_context else {
        return Ok(());
    };
    if context.run_id != request.run_id
        || context.plan_hash != request.plan_hash
        || context.plan_hash != plan_hash(&request.goal, &request.plan)
        || context.previous_round.checked_add(1) != Some(request.repair_round)
        || request.repair_round > MAX_REPAIR_ROUND
        || !hash_valid(&context.previous_request_hash)
    {
        return Err("repair_history_binding_mismatch");
    }
    if context.obligations.is_empty()
        || context.obligations.len() > 128
        || context.previous_conclusions.len() > 32
        || context
            .previous_conclusions
            .iter()
            .any(|s| s.len() > 32_768)
    {
        return Err("invalid_repair_history");
    }
    let mut repairs = BTreeSet::new();
    let known: BTreeSet<&str> = LOCAL_CHECKS
        .iter()
        .copied()
        .chain(SEMANTIC_CHECKS.iter().copied())
        .chain(["p2.web_sources", "p2.web_grounding", "p2.repair_resolution"])
        .chain(request.plan.completion_checks.iter().map(|c| c.id.as_str()))
        .collect();
    for obligation in &context.obligations {
        if !id_valid(&obligation.repair_id)
            || !repairs.insert(&obligation.repair_id)
            || obligation.reason.trim().is_empty()
            || obligation.reason.len() > 16_384
            || obligation.check_ids.is_empty()
            || obligation.check_ids.len() > 128
            || !unique(&obligation.check_ids)
            || !unique(&obligation.scenario_ids)
            || obligation
                .check_ids
                .iter()
                .any(|id| !known.contains(id.as_str()))
            || (obligation.action == RepairAction::RerunScenario
                && obligation.scenario_ids.is_empty())
            || obligation.scenario_ids.len() > 32
            || obligation
                .scenario_ids
                .iter()
                .any(|id| !request.plan.scenarios.iter().any(|s| s.id == *id))
            || obligation.before_hashes.len() != obligation.scenario_ids.len()
            || obligation.scenario_ids.iter().any(|id| {
                !obligation
                    .before_hashes
                    .get(id)
                    .is_some_and(|hash| hash_valid(hash))
            })
        {
            return Err("invalid_repair_obligation");
        }
    }
    Ok(())
}

fn metadata(key: &str) -> bool {
    matches!(
        key,
        "attempt"
            | "attempts"
            | "attempt_id"
            | "receipt_id"
            | "timestamp"
            | "timestamps"
            | "created_at"
            | "updated_at"
            | "completed_at"
            | "started_at"
            | "elapsed_ms"
            | "duration_ms"
            | "summary"
            | "rationale"
            | "explanation"
            | "notes"
            | "passed"
            | "valid"
            | "success"
            | "status"
            | "checked"
            | "probability_vectors_valid"
            | "aggregation_checked_by_p1"
            | "metadata"
            | "trace"
            | "request_id"
            | "response_id"
    )
}
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| !metadata(key))
                .map(|(key, value)| (key, canonical(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(values) => {
            let mut values: Vec<_> = values.into_iter().map(canonical).collect();
            // Keyed scientific records are sets: rearranging the same answers
            // or options is not a repaired result. Unkeyed/primitive vectors
            // retain order because their positions may carry scientific meaning.
            for key in ["archetype_id", "id", "option"] {
                let keys: Option<Vec<_>> = values
                    .iter()
                    .map(|value| value.get(key).and_then(Value::as_str))
                    .collect();
                if let Some(keys) = keys {
                    if keys.iter().collect::<BTreeSet<_>>().len() == values.len() {
                        values.sort_by(|left, right| left[key].as_str().cmp(&right[key].as_str()));
                        break;
                    }
                }
            }
            Value::Array(values)
        }
        value => value,
    }
}
fn scenario_hash(request: &VerificationRequest, id: &str) -> String {
    let values: Vec<_> = request
        .results
        .iter()
        .filter(|result| result.scenario_id == id)
        .map(|result| result.payload.clone())
        .collect();
    hex::encode(Sha256::digest(
        serde_json::to_vec(&canonical(json!(values))).expect("JSON serializes"),
    ))
}
fn normalized(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}
/// A changed receipt ID, attempt count, timestamp, or explanation is not repaired
/// scientific evidence. Every rerun obligation needs a changed scientific
/// payload. For other repair actions, a substantive conclusion revision can
/// legitimately fix an overclaim without rerunning an otherwise sound experiment.
pub fn unresolved(request: &VerificationRequest) -> bool {
    let Some(context) = &request.repair_context else {
        return false;
    };
    context.obligations.iter().any(|obligation| {
        let unchanged = obligation
            .scenario_ids
            .iter()
            .all(|id| obligation.before_hashes.get(id) == Some(&scenario_hash(request, id)));
        if obligation.action == RepairAction::RerunScenario {
            unchanged
        } else {
            unchanged
                && normalized(&context.previous_conclusions) == normalized(&request.conclusions)
        }
    })
}

/// Evidence changes are necessary but never sufficient: each original repair
/// receives an explicit assessment and all its original checks must now pass.
pub fn validate_resolution(
    request: &VerificationRequest,
    assessments: &[RepairAssessment],
    findings: &[Finding],
) -> Result<(), &'static str> {
    let Some(context) = &request.repair_context else {
        return if assessments.is_empty() {
            Ok(())
        } else {
            Err("unexpected_repair_assessment")
        };
    };
    if assessments.len() != context.obligations.len() {
        return Err("incomplete_repair_assessments");
    }
    let value = serde_json::to_value(request).map_err(|_| "invalid_repair_evidence")?;
    let mut seen = BTreeSet::new();
    for assessment in assessments {
        let Some(obligation) = context
            .obligations
            .iter()
            .find(|o| o.repair_id == assessment.repair_id)
        else {
            return Err("unknown_repair_assessment");
        };
        if !seen.insert(&assessment.repair_id)
            || assessment.reasoning.trim().len() < 20
            || assessment.reasoning.len() > 16_384
            || assessment.evidence.is_empty()
            || assessment.evidence.len() > 32
            || assessment.status == CheckStatus::NotReviewed
        {
            return Err("invalid_repair_assessment");
        }
        for evidence in &assessment.evidence {
            let segments: Vec<_> = evidence.pointer.split('/').collect();
            let result_proof = segments.get(1) == Some(&"results")
                && segments.get(3) == Some(&"payload")
                && segments.len() >= 5
                && !segments.iter().any(|segment| {
                    metadata(segment)
                        || matches!(
                            *segment,
                            "passed" | "valid" | "success" | "status" | "checked"
                        )
                });
            let conclusion_proof = segments.get(1) == Some(&"conclusions") && segments.len() == 3;
            let leaf = value.pointer(&evidence.pointer);
            if !(result_proof || conclusion_proof)
                || !leaf.is_some_and(|v| {
                    meaningful(v) && matches!(v, Value::Number(_) | Value::String(_))
                })
            {
                return Err("invalid_repair_evidence");
            }
        }
        if assessment.status == CheckStatus::Pass {
            if obligation.check_ids.iter().any(|id| {
                findings
                    .iter()
                    .filter(|f| f.check_id == *id && f.status == CheckStatus::Pass)
                    .count()
                    != 1
                    || findings
                        .iter()
                        .any(|f| f.check_id == *id && f.status != CheckStatus::Pass)
            }) {
                return Err("repair_original_check_unresolved");
            }
            let conclusion_changed =
                normalized(&context.previous_conclusions) != normalized(&request.conclusions);
            let changed: Vec<_> = obligation
                .scenario_ids
                .iter()
                .filter(|id| obligation.before_hashes.get(*id) != Some(&scenario_hash(request, id)))
                .collect();
            if changed.is_empty()
                && (obligation.action == RepairAction::RerunScenario || !conclusion_changed)
            {
                return Err("repair_evidence_unchanged");
            }
            let cites_change = assessment.evidence.iter().any(|evidence| {
                if obligation.action != RepairAction::RerunScenario
                    && conclusion_changed
                    && evidence.pointer.starts_with("/conclusions/")
                {
                    return true;
                }
                request.results.iter().enumerate().any(|(index, result)| {
                    changed.iter().any(|id| id.as_str() == result.scenario_id)
                        && (evidence.pointer == format!("/results/{index}")
                            || evidence.pointer.starts_with(&format!("/results/{index}/")))
                })
            });
            if !cites_change {
                return Err("repair_change_not_cited");
            }
        }
    }
    let expected = if assessments.iter().any(|a| a.status == CheckStatus::Blocked) {
        CheckStatus::Blocked
    } else if assessments.iter().any(|a| a.status == CheckStatus::Repair) {
        CheckStatus::Repair
    } else {
        CheckStatus::Pass
    };
    if findings
        .iter()
        .filter(|f| f.check_id == "p2.repair_resolution")
        .count()
        != 1
        || !findings
            .iter()
            .any(|f| f.check_id == "p2.repair_resolution" && f.status == expected)
    {
        return Err("inconsistent_repair_resolution");
    }
    Ok(())
}
fn meaningful(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        Value::Array(values) => !values.is_empty(),
        Value::Object(map) => !map.is_empty(),
        _ => true,
    }
}
