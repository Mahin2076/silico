use super::repairs::*;
use super::types::*;
use serde_json::json;

fn request() -> VerificationRequest {
    serde_json::from_str(include_str!("fixtures/complete.json")).unwrap()
}
fn repaired_request() -> VerificationRequest {
    let previous = request();
    let decision = VerificationDecision {
        schema_version: SCHEMA_VERSION,
        run_id: previous.run_id.clone(),
        plan_hash: previous.plan_hash.clone(),
        request_hash: request_hash(&previous),
        repair_round: 0,
        verdict: Verdict::Repair,
        summary: "Results require a corrected comparison".into(),
        claim_assessments: vec![],
        repair_assessments: vec![],
        findings: vec![Finding {
            check_id: "p2.conclusion_support".into(),
            status: CheckStatus::Repair,
            summary: "Comparison requires correction".into(),
            evidence: vec![],
        }],
        repair_requests: vec![RepairRequest {
            id: "repair-comparison".into(),
            action: RepairAction::RerunScenario,
            check_ids: vec!["p2.conclusion_support".into()],
            scenario_ids: vec!["message-a".into()],
            reason: "Correct the numerical comparison using scenario A evidence".into(),
        }],
        receipt: VerificationReceipt {
            mode: "mock".into(),
            provider: "fixture".into(),
            model: "nemotron".into(),
            prompt_version: PROMPT_VERSION.into(),
            model_calls: 1,
            elapsed_ms: 0,
            error_code: None,
        },
    };
    let mut next = previous.clone();
    next.repair_round = 1;
    next.repair_context = Some(RepairContext::from_review(&previous, &decision).unwrap());
    next
}
fn findings(status: CheckStatus) -> Vec<Finding> {
    vec![
        Finding {
            check_id: "p2.conclusion_support".into(),
            status,
            summary: "Checked the corrected comparison".into(),
            evidence: vec![],
        },
        Finding {
            check_id: "p2.repair_resolution".into(),
            status: CheckStatus::Pass,
            summary: "Original complaint addressed".into(),
            evidence: vec![],
        },
    ]
}

fn assessment() -> RepairAssessment {
    RepairAssessment {repair_id:"repair-comparison".into(),status:CheckStatus::Pass,
        reasoning:"The corrected scenario A evidence addresses the original numerical comparison complaint.".into(),
        evidence:vec![EvidenceRef{pointer:"/results/0/payload/weighted_yes_share".into()}]}
}

#[test]
fn repair_metadata_and_paraphrased_explanation_are_not_progress() {
    let mut next = repaired_request();
    assert!(unresolved(&next));
    next.results[0].id = "fresh-receipt".into();
    next.results[0].payload["attempt"] = json!(2);
    next.results[0].payload["timestamp"] = json!("2026-10-17T21:00:00Z");
    next.results[0].payload["summary"] = json!("The rerun is now improved and valid.");
    next.conclusions = next
        .conclusions
        .iter()
        .map(|s| format!("  {} \n", s))
        .collect();
    assert!(unresolved(&next));
    assert_eq!(
        validate_resolution(&next, &[assessment()], &findings(CheckStatus::Pass)),
        Err("repair_evidence_unchanged")
    );
}

#[test]
fn repair_changed_data_requires_original_check_and_explicit_assessment() {
    let mut next = repaired_request();
    next.results[0].payload["weighted_yes_share"] = json!(0.55);
    assert!(!unresolved(&next));
    assert!(validate_context(&next).is_ok());
    assert_eq!(
        validate_resolution(&next, &[], &findings(CheckStatus::Pass)),
        Err("incomplete_repair_assessments")
    );
    assert_eq!(
        validate_resolution(&next, &[assessment()], &findings(CheckStatus::Repair)),
        Err("repair_original_check_unresolved")
    );
    assert!(validate_resolution(&next, &[assessment()], &findings(CheckStatus::Pass)).is_ok());
}

#[test]
fn repair_rejects_stale_plan_run_round_and_fabricated_obligation() {
    let valid = repaired_request();
    for mutation in 0..6 {
        let mut next = valid.clone();
        match mutation {
            0 => next.run_id.push_str("-other"),
            1 => next.plan.scenarios[0].inputs["stimulus"] = json!("changed stimulus"),
            2 => next.repair_round = 2,
            3 => next.repair_context.as_mut().unwrap().obligations[0]
                .scenario_ids
                .push("invented-scenario".into()),
            4 => next.repair_context.as_mut().unwrap().obligations[0]
                .check_ids
                .push("invented-check".into()),
            _ => next.repair_context.as_mut().unwrap().previous_request_hash = "invalid".into(),
        }
        assert!(validate_context(&next).is_err(), "mutation {mutation}");
    }
}

#[test]
fn repair_requires_exact_ids_and_evidence_for_changed_target() {
    let mut next = repaired_request();
    next.results[0].payload["weighted_yes_share"] = json!(0.55);
    let mut review = assessment();
    review.repair_id = "another-repair".into();
    assert_eq!(
        validate_resolution(&next, &[review], &findings(CheckStatus::Pass)),
        Err("unknown_repair_assessment")
    );
    let mut review = assessment();
    review.evidence[0].pointer = "/results/1/payload/weighted_yes_share".into();
    assert_eq!(
        validate_resolution(&next, &[review], &findings(CheckStatus::Pass)),
        Err("repair_change_not_cited")
    );
    let mut review = assessment();
    review.evidence[0].pointer = "/repair_context/obligations/0/reason".into();
    assert_eq!(
        validate_resolution(&next, &[review], &findings(CheckStatus::Pass)),
        Err("invalid_repair_evidence")
    );
}

#[test]
fn repair_retains_actual_scientific_identifiers() {
    let mut next = repaired_request();
    next.results[0].payload["sample_id"] = json!("changed-scientific-sample");
    assert!(!unresolved(&next));
}

#[test]
fn repair_can_resolve_overclaim_by_correcting_conclusion_with_evidence() {
    let mut next = repaired_request();
    next.repair_context.as_mut().unwrap().obligations[0].action =
        RepairAction::RequestClarification;
    next.conclusions[0] = "Revised conclusion: these synthetic observations do not establish a causal real-world effect.".into();
    let mut review = assessment();
    review.evidence[0].pointer = "/conclusions/0".into();
    assert!(!unresolved(&next));
    assert!(validate_resolution(&next, &[review], &findings(CheckStatus::Pass)).is_ok());
}

#[test]
fn rerun_cannot_be_resolved_by_paraphrasing_conclusion() {
    let mut next = repaired_request();
    next.conclusions[0] = "Completely reworded but identical experimental outcome.".into();
    assert!(unresolved(&next));
    let mut review = assessment();
    review.evidence[0].pointer = "/conclusions/0".into();
    assert_eq!(
        validate_resolution(&next, &[review], &findings(CheckStatus::Pass)),
        Err("repair_evidence_unchanged")
    );
}
#[test]
fn repair_metadata_cannot_prove_resolution_or_override_aggregate() {
    let mut next = repaired_request();
    next.results[0].payload["passed"] = json!(true);
    assert!(unresolved(&next));
    next.results[0].payload["weighted_yes_share"] = json!(0.55);
    let mut review = assessment();
    review.evidence[0].pointer = "/results/0/payload/passed".into();
    assert_eq!(
        validate_resolution(&next, &[review], &findings(CheckStatus::Pass)),
        Err("invalid_repair_evidence")
    );
    let mut all = findings(CheckStatus::Pass);
    all[1].status = CheckStatus::Repair;
    assert_eq!(
        validate_resolution(&next, &[assessment()], &all),
        Err("inconsistent_repair_resolution")
    );
    let mut negative = assessment();
    negative.status = CheckStatus::Blocked;
    let mut all = findings(CheckStatus::Repair);
    assert_eq!(
        validate_resolution(&next, &[negative.clone()], &all),
        Err("inconsistent_repair_resolution")
    );
    all[1].status = CheckStatus::Blocked;
    assert!(validate_resolution(&next, &[negative], &all).is_ok());
}

#[test]
fn repair_keyed_record_reordering_and_transport_metadata_are_not_progress() {
    let previous = request();
    for key in ["archetype_id", "id", "option"] {
        let mut first = repaired_request();
        first.results[0].payload["answers"] = json!([
            {key:"first", "probability":0.4}, {key:"second", "probability":0.6}
        ]);
        first.results[0].payload["probability_vector"] = json!([0.4, 0.6]);
        // Capture the same original obligation with this scientific baseline.
        let mut decision: VerificationDecision = serde_json::from_value(json!({
            "schema_version":1,"run_id":first.run_id,"plan_hash":first.plan_hash,
            "request_hash":request_hash(&first),"repair_round":first.repair_round,
            "verdict":"repair","summary":"Correct the comparison",
            "findings":[{"check_id":"p2.conclusion_support","status":"repair","summary":"Numerical comparison still needs correction","evidence":[]}],
            "repair_requests":[{"id":"repair-records","action":"rerun_scenario","check_ids":["p2.conclusion_support"],"scenario_ids":["message-a"],"reason":"Correct scientific probability data"}],
            "receipt":{"mode":"mock","provider":"fixture","model":"nemotron","prompt_version":PROMPT_VERSION,"model_calls":1,"elapsed_ms":0,"error_code":null}
        })).unwrap();
        // Use a first-round request to keep this test independent of max-round budgets.
        first.repair_round = previous.repair_round;
        first.repair_context = None;
        decision.repair_round = first.repair_round;
        decision.request_hash = request_hash(&first);
        let context = RepairContext::from_review(&first, &decision).unwrap();
        first.repair_round += 1;
        first.repair_context = Some(context);
        first.results[0].payload["answers"]
            .as_array_mut()
            .unwrap()
            .reverse();
        first.results[0].payload["metadata"] = json!({"attempt":9,"new_diagnostic":true});
        first.results[0].payload["trace"] = json!(["different trace"]);
        first.results[0].payload["request_id"] = json!("request-new");
        first.results[0].payload["response_id"] = json!("response-new");
        assert!(unresolved(&first), "reordered key {key}");
        first.results[0].payload["answers"][0]["probability"] = json!(0.7);
        assert!(!unresolved(&first), "changed scientific value for {key}");
        first.results[0].payload["answers"][0]["probability"] = json!(0.6);
        first.results[0].payload["probability_vector"]
            .as_array_mut()
            .unwrap()
            .reverse();
        assert!(
            !unresolved(&first),
            "primitive probability positions retain meaning"
        );
    }
}
