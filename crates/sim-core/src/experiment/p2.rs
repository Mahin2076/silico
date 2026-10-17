//! Lossless bridge from P1 receipts to P2's immutable, hash-bound contract.
use super::{checks, contract as p1, hash};
use crate::verifier as p2;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub(super) struct Transport(pub Box<dyn p2::VerifierTransport>);
impl p2::VerifierTransport for Transport {
    fn identity(&self) -> p2::TransportIdentity {
        self.0.identity()
    }
    fn complete<'a>(&'a self, system: &'a str, user: &'a str) -> p2::CompletionFuture<'a> {
        self.0.complete(system, user)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub request: p2::VerificationRequest,
    pub decision: p2::VerificationDecision,
}

// Hash arbitrary legacy IDs instead of replacing punctuation (which can collide).
fn check_id(id: &str) -> String {
    format!("p1.{}", hash(&id))
}

pub fn request(run: &p1::Run) -> Result<p2::VerificationRequest> {
    let plan = run.plan.as_ref().context("Plan is not ready")?;
    ensure!(
        hash(&run.plan) == run.plan_hash,
        "P1 plan changed after freezing"
    );
    ensure!(
        hash(&run.context) == plan.context_hash,
        "P1 evidence changed after freezing"
    );
    ensure!(run.goal == plan.goal, "P1 goal changed after freezing");
    let seed = run.context["seed"]
        .as_u64()
        .context("Frozen seed missing")?;
    let context = p2::FrozenContext {
        population_hash: plan.audience_hash.clone(),
        model: plan.model.clone(),
        as_of_date: plan.as_of_date.clone(),
        evidence_hash: plan.context_hash.clone(),
        seed,
    };
    let frozen = p2::ExperimentPlan {
        context,
        assumptions: plan.scenarios.iter().flat_map(|s| s.assumptions.iter().map(move |a| format!("{}: {a}", s.scenario_id))).collect(),
        scenarios: plan.scenarios.iter().map(|s| p2::Scenario {
            id: s.scenario_id.clone(), description: s.description.clone(), audience_id: plan.audience_hash.clone(),
            inputs: json!({"scenario":s,"city":plan.city,"frozen_context":run.context,"p1_plan_hash":run.plan_hash}),
        }).collect(),
        completion_checks: plan.checks.iter().map(|c| p2::CompletionCheck {
            id: check_id(&c.check_id), description: c.requirement.clone(), required: true,
            kind: if c.kind == "semantic" { p2::CheckKind::Semantic } else { p2::CheckKind::Deterministic },
        }).collect(),
    };
    let plan_hash = p2::plan_hash(&run.goal, &frozen);
    let results = run.results.iter().enumerate().map(|(i, r)| p2::ScenarioResult {
        id: format!("result-{i}-attempt-{}", r.attempt), scenario_id: r.scenario_id.clone(), plan_hash: plan_hash.clone(),
        // Preserve each result's actual identity; never relabel stale results.
        context: p2::FrozenContext { population_hash:r.audience_hash.clone(), model:r.model.clone(), as_of_date:r.as_of_date.clone(), evidence_hash:r.context_hash.clone(), seed },
        audience_id: r.audience_hash.clone(),
        status: match r.status.as_str() { "complete" => p2::ResultStatus::Complete, "failed" => p2::ResultStatus::Failed, _ => p2::ResultStatus::Pending },
        payload: json!({"receipt":r,"answers":run.answers.get(&r.scenario_id),"archetype_weights":run.archetypes.iter().map(|a| json!({"id":a.id,"weight":a.weight})).collect::<Vec<_>>(),"p1_plan_hash":run.plan_hash}),
    }).collect();
    // Recompute from original answers; never trust cached finding labels.
    let deterministic_findings = checks::checks(run)
        .iter()
        .map(|f| p2::DeterministicFinding {
            check_id: check_id(&f.check_id),
            passed: f.status == "pass",
            detail: f.evidence.clone(),
            scenario_ids: f.scenario_ids.clone(),
            evidence: vec![
                p2::EvidenceRef {
                    pointer: "/results".into(),
                },
                p2::EvidenceRef {
                    pointer: "/plan/scenarios".into(),
                },
            ],
        })
        .collect();
    let mut conclusions = vec!["These are hypothetical model estimates for seeded synthetic residents, aggregated with Census person weights. They are not observed behavior, validated forecasts, or causal evidence. Only the frozen scenario assumptions are used; no external research is included.".into()];
    for r in run.results.iter().filter(|r| r.status == "complete") {
        conclusions.push(format!(
            "{}: model-estimated weighted outcome probabilities: {}.",
            r.scenario_id,
            r.distribution
                .iter()
                .map(|p| format!("{} = {:.4}%", p.option, p.probability * 100.0))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(p2::VerificationRequest {
        schema_version: p2::SCHEMA_VERSION,
        run_id: run.run_id.clone(),
        plan_hash,
        repair_round: run.round.try_into().context("Invalid repair round")?,
        goal: run.goal.clone(),
        plan: frozen,
        results,
        deterministic_findings,
        conclusions,
        web_research: None,
    })
}

/// Keep P1's presentation/repair format while retaining the full P2 review.
/// Completion always depends on permits_completion, including live provenance.
pub fn response(run: &p1::Run, review: &Review) -> Result<p1::VerifierResponse> {
    ensure!(
        p2::request_hash(&request(run)?) == p2::request_hash(&review.request),
        "P2 review belongs to a stale experiment"
    );
    let d = &review.decision;
    ensure!(
        d.run_id == review.request.run_id
            && d.plan_hash == review.request.plan_hash
            && d.request_hash == p2::request_hash(&review.request)
            && d.repair_round == review.request.repair_round,
        "P2 decision identity mismatch"
    );
    let pass = d.permits_completion(&review.request);
    let verdict = match d.verdict {
        p2::Verdict::Pass if pass => "pass",
        p2::Verdict::Repair => "repair",
        _ => "blocked",
    };
    let plan = run.plan.as_ref().context("Missing plan")?;
    let mut findings = checks::checks(run);
    for c in plan.checks.iter().filter(|c| c.kind == "semantic") {
        findings.push(p1::Finding {
            check_id: c.check_id.clone(),
            scenario_ids: c.scenario_ids.clone(),
            status: if pass { "pass" } else { "unknown" }.into(),
            evidence: d.summary.clone(),
        });
    }
    let repairs = d
        .repair_requests
        .iter()
        .map(|r| p1::Repair {
            action: match r.action {
                p2::RepairAction::RerunScenario => "rerun_scenario",
                p2::RepairAction::RevisePlan => "revise_plan",
                p2::RepairAction::RequestClarification => "request_clarification",
            }
            .into(),
            scenario_ids: r.scenario_ids.clone(),
            // P2's extra integrity/semantic checks roll up into P1's semantic gate.
            // The exact original IDs and JSON evidence remain in Review.decision.
            check_ids: plan
                .checks
                .iter()
                .filter(|c| c.kind == "semantic")
                .map(|c| c.check_id.clone())
                .collect(),
            reason: r.reason.clone(),
        })
        .collect();
    Ok(p1::VerifierResponse {
        schema_version: "0.1".into(),
        run_id: run.run_id.clone(),
        plan_hash: run.plan_hash.clone(),
        round: run.round,
        verdict: verdict.into(),
        checks: findings,
        repair_requests: repairs,
        summary: if d.verdict == p2::Verdict::Pass && !pass {
            "P2 completion gate rejected this review; a valid live review is required.".into()
        } else {
            d.summary.clone()
        },
    })
}

/// Explicit retries reuse intact simulation evidence, never restart paid scenarios.
pub fn can_retry(run: &p1::Run) -> bool {
    run.mode == "live"
        && run.plan.is_some()
        && run.p2_reviews.len() < 3
        && (run.status == "awaiting_verifier"
            || (["blocked", "needs_input"].contains(&run.status.as_str())
                && run.p2_reviews.last().is_some_and(|r| {
                    matches!(
                        r.decision.receipt.error_code.as_deref(),
                        Some(
                            "invalid_review_evidence"
                                | "malformed_review"
                                | "review_identity_mismatch"
                                | "invalid_review_coverage"
                                | "invalid_repair_request"
                                | "missing_targeted_repair"
                                | "inconsistent_review_verdict"
                                | "timeout"
                                | "network_error"
                                | "provider_http_error"
                                | "rate_limited"
                                | "authentication_failed"
                                | "malformed_provider_response"
                                | "model_mismatch"
                                | "response_too_large"
                        )
                    )
                })))
}
