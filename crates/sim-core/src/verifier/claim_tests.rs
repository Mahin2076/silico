use super::claims::{evidence_catalog, validate_claims};
use super::*;
use serde_json::json;

fn request() -> VerificationRequest {
    serde_json::from_str(include_str!("fixtures/complete.json")).unwrap()
}

fn comparison() -> ClaimAssessment {
    ClaimAssessment {
        conclusion_index: 0,
        kind: ClaimKind::Comparison,
        status: CheckStatus::Pass,
        evidence: vec![
            ClaimEvidence { pointer: "/results/1/payload/weighted_yes_share".into(), observed: json!(0.65) },
            ClaimEvidence { pointer: "/results/0/payload/weighted_yes_share".into(), observed: json!(0.6) },
        ],
        reasoning: "B exceeds A by 0.05 in raw modeled share units; this supports a synthetic comparison, not a causal or real-population claim.".into(),
        comparison: Some(ComparisonProof {
            left_pointer: "/results/1/payload/weighted_yes_share".into(),
            right_pointer: "/results/0/payload/weighted_yes_share".into(),
            relation: ComparisonRelation::Greater,
            difference: 0.05,
        }),
    }
}

#[test]
fn claim_comparison_accepts_exact_evidence_and_checked_arithmetic() {
    let req = request();
    assert_eq!(
        validate_claims(&req, &[comparison()], CheckStatus::Pass),
        Ok(())
    );
    let mut reverse = comparison();
    let proof = reverse.comparison.as_mut().unwrap();
    std::mem::swap(&mut proof.left_pointer, &mut proof.right_pointer);
    proof.relation = ComparisonRelation::Less;
    proof.difference = -0.05;
    assert_eq!(validate_claims(&req, &[reverse], CheckStatus::Pass), Ok(()));
}

#[test]
fn claim_false_pass_matrix_rejects_wrong_numbers_directions_units_and_citations() {
    let req = request();
    let mut cases = Vec::new();
    let mut c = comparison();
    c.evidence[0].observed = json!(0.85);
    cases.push(("invented number", c));
    let mut c = comparison();
    c.evidence[0].observed = json!(65);
    cases.push(("converted observed percentage", c));
    let mut c = comparison();
    c.comparison.as_mut().unwrap().relation = ComparisonRelation::Less;
    cases.push(("reversed direction", c));
    let mut c = comparison();
    c.comparison.as_mut().unwrap().difference = 5.0;
    cases.push(("percentage-point unit error", c));
    let mut c = comparison();
    c.comparison.as_mut().unwrap().difference = 0.15;
    cases.push(("wrong magnitude", c));
    let mut c = comparison();
    c.comparison = None;
    cases.push(("missing proof", c));
    let mut c = comparison();
    c.evidence.pop();
    cases.push(("uncited operand", c));
    let mut c = comparison();
    c.evidence[0].pointer = "/results/0/status".into();
    c.evidence[0].observed = json!("complete");
    cases.push(("completed status", c));
    let mut c = comparison();
    c.evidence[0].pointer = "/results/0/payload/fixture".into();
    c.evidence[0].observed = json!(true);
    cases.push(("fixture flag", c));
    let mut c = comparison();
    c.evidence[0].pointer = "/results/0/payload".into();
    c.evidence[0].observed = req.results[0].payload.clone();
    cases.push(("broad payload", c));
    let mut c = comparison();
    c.evidence[0].pointer = "/conclusions/0".into();
    c.evidence[0].observed = json!(req.conclusions[0]);
    cases.push(("self citation", c));
    let mut c = comparison();
    c.evidence[0].pointer = "/deterministic_findings/0/passed".into();
    c.evidence[0].observed = json!(true);
    cases.push(("attestation as proof", c));
    for (name, candidate) in cases {
        assert!(
            validate_claims(&req, &[candidate], CheckStatus::Pass).is_err(),
            "false pass: {name}"
        );
    }
}

#[test]
fn claim_coverage_and_status_cannot_hide_unreviewed_or_rejected_conclusions() {
    let mut req = request();
    assert!(validate_claims(&req, &[], CheckStatus::Pass).is_err());
    req.conclusions
        .push("An additional unsupported causal claim.".into());
    assert!(validate_claims(&req, &[comparison(), comparison()], CheckStatus::Pass).is_err());
    let mut rejected = comparison();
    rejected.conclusion_index = 1;
    rejected.status = CheckStatus::Repair;
    rejected.evidence.clear();
    rejected.comparison = None;
    rejected.reasoning =
        "The synthetic comparison supplies no causal evidence for the second conclusion.".into();
    assert!(validate_claims(&req, &[comparison(), rejected.clone()], CheckStatus::Pass).is_err());
    assert_eq!(
        validate_claims(&req, &[comparison(), rejected.clone()], CheckStatus::Repair),
        Ok(())
    );
    rejected.status = CheckStatus::Blocked;
    assert!(validate_claims(&req, &[comparison(), rejected.clone()], CheckStatus::Repair).is_err());
    assert_eq!(
        validate_claims(&req, &[comparison(), rejected], CheckStatus::Blocked),
        Ok(())
    );
}

#[test]
fn claim_external_fact_requires_snippet_not_source_title_url_or_modeled_number() {
    let mut req = request();
    req.web_research = Some(WebResearch {
        provider: "you.com".into(),
        mode: "mock".into(),
        query: ResearchQuery::new("synthetic polling validation".into(), vec![]).unwrap(),
        as_of_date: "2026-10-17".into(),
        fetched_at: "2026-10-17T00:00:00Z".into(),
        status: "ready".into(),
        search_calls: 1,
        error_code: None,
        sources: vec![WebSource {
            id: "web-1".into(),
            title: "Survey methodology".into(),
            url: "https://example.com/methods".into(),
            snippets: vec![
                "Synthetic estimates require validation with observed responses.".into(),
            ],
            publication_date: None,
        }],
    });
    let mut claim = comparison();
    claim.kind = ClaimKind::ExternalFact;
    claim.comparison = None;
    assert!(validate_claims(&req, &[claim.clone()], CheckStatus::Pass).is_err());
    for (field, observed) in [
        ("title", json!("Survey methodology")),
        ("url", json!("https://example.com/methods")),
    ] {
        claim.evidence = vec![ClaimEvidence {
            pointer: format!("/web_research/sources/0/{field}"),
            observed,
        }];
        assert!(validate_claims(&req, &[claim.clone()], CheckStatus::Pass).is_err());
    }
    claim.evidence = vec![ClaimEvidence {
        pointer: "/web_research/sources/0/snippets/0".into(),
        observed: json!("Synthetic estimates require validation with observed responses."),
    }];
    claim.reasoning = "The excerpt explicitly recommends observed-response validation; it does not establish the modeled percentages.".into();
    assert_eq!(
        validate_claims(&req, &[claim.clone()], CheckStatus::Pass),
        Ok(())
    );
    // Reject fabricated quotations even on a rejection, rather than logging fake evidence.
    claim.status = CheckStatus::Repair;
    claim.evidence[0].observed = json!("Every simulated forecast is proven accurate.");
    assert!(validate_claims(&req, &[claim], CheckStatus::Repair).is_err());
}

#[test]
fn claim_limitations_and_bounds_are_explicit() {
    let req = request();
    let mut claim = comparison();
    claim.kind = ClaimKind::Limitation;
    claim.comparison = None;
    assert!(validate_claims(&req, &[claim.clone()], CheckStatus::Pass).is_err());
    claim.evidence = vec![ClaimEvidence {
        pointer: "/plan/assumptions/0".into(),
        observed: json!(req.plan.assumptions[0]),
    }];
    assert_eq!(
        validate_claims(&req, &[claim.clone()], CheckStatus::Pass),
        Ok(())
    );
    claim.reasoning = " ".into();
    assert!(validate_claims(&req, &[claim.clone()], CheckStatus::Pass).is_err());
    claim.reasoning = "x".repeat(2401);
    assert!(validate_claims(&req, &[claim.clone()], CheckStatus::Pass).is_err());
    claim.reasoning = "Explicitly labeled synthetic limitation.".into();
    claim.evidence = vec![claim.evidence[0].clone(); 13];
    assert!(validate_claims(&req, &[claim], CheckStatus::Pass).is_err());
}

#[test]
fn claim_replays_live_false_pass_despite_correct_numeric_citations() {
    let mut req = request();
    let mut claim = comparison();
    claim.reasoning = "The conclusion reports 85% versus 60% while the actual values are 65% versus 60%, but it is a labeled synthetic fixture.".into();
    for text in [
        "In this synthetic fixture, B has 85% versus A at 60%, a twenty-five percentage point difference.",
        "B has eighty-five percent versus A at sixty percent, a twenty-five percentage-point difference in this fixture.",
        "B has 65% versus A at 60%, a fifty percentage point difference.",
        "B has 65% versus A at 60%, a 25 pp difference.",
        "B has 0.85 modeled support compared with A at 0.60.",
        "B has -65% modeled support versus A at 60%.",
    ] {
        req.conclusions[0] = text.into();
        assert_eq!(validate_claims(&req, &[claim.clone()], CheckStatus::Pass), Err("claim_numeric_mismatch"), "{text}");
    }
    for text in [
        "B has 65% versus A at 60%, a five percentage point difference in this synthetic fixture.",
        "B has sixty-five percent versus A at sixty percent, a five percentage-point difference.",
        "B has 65.0000% versus A at 60.0000%, a 5.0000 pp difference.",
        "B has 0.65 versus A at 0.60, with a 0.05 modeled difference.",
        "B has .65 versus A at .60, with a .05 modeled difference.",
        "B has 65% versus A at 60%, an 8.3333% relative increase.",
    ] {
        req.conclusions[0] = text.into();
        assert_eq!(
            validate_claims(&req, &[claim.clone()], CheckStatus::Pass),
            Ok(()),
            "{text}"
        );
    }
}

#[test]
fn claim_numeric_notation_respects_printed_precision_and_equal_results() {
    let mut req = request();
    req.results[1].payload["weighted_yes_share"] = json!(0.652345678);
    req.results[0].payload["weighted_yes_share"] = json!(0.602345678);
    req.conclusions[0] =
        "B has 65.2346% versus A at 60.2346%, a five percentage point modeled difference.".into();
    let mut claim = comparison();
    claim.evidence[0].observed = json!(0.652345678);
    claim.evidence[1].observed = json!(0.602345678);
    assert_eq!(
        validate_claims(&req, &[claim.clone()], CheckStatus::Pass),
        Ok(())
    );
    req.conclusions[0] = "B has 65.2300% versus A at 60.2346%.".into();
    assert_eq!(
        validate_claims(&req, &[claim.clone()], CheckStatus::Pass),
        Err("claim_numeric_mismatch")
    );
    req.results[0].payload["weighted_yes_share"] = json!(0.652345678);
    claim.evidence[1].observed = json!(0.652345678);
    let proof = claim.comparison.as_mut().unwrap();
    proof.relation = ComparisonRelation::Equal;
    proof.difference = 0.0;
    req.conclusions[0] =
        "Both have 65.2346% modeled support, a zero percentage point difference.".into();
    assert_eq!(validate_claims(&req, &[claim], CheckStatus::Pass), Ok(()));
}

#[test]
fn claim_catalog_is_exact_balanced_and_prioritizes_native_aggregates() {
    let mut req = request();
    for result in &mut req.results {
        result.payload["receipt"] = json!({"distribution":{"yes":0.65,"no":0.35}});
        result.payload["aaa_raw_answers"] = json!((0..200).collect::<Vec<_>>());
    }
    let catalog = evidence_catalog(&req);
    let document = serde_json::to_value(&req).unwrap();
    for entry in &catalog {
        assert_eq!(document.pointer(&entry.pointer), Some(&entry.observed));
        assert!(!entry.observed.is_boolean());
        assert!(!entry.observed.is_object());
        assert!(!entry.observed.is_array());
    }
    for index in 0..2 {
        assert!(catalog
            .iter()
            .any(|e| e.pointer == format!("/results/{index}/payload/weighted_yes_share")));
        assert!(catalog
            .iter()
            .any(|e| e.pointer == format!("/results/{index}/payload/receipt/distribution/yes")));
        assert!(
            catalog
                .iter()
                .filter(|e| e.pointer.starts_with(&format!("/results/{index}/")))
                .count()
                <= 8
        );
    }
    assert!(catalog.iter().any(|e| e.pointer == "/plan/assumptions/0"));
    assert_eq!(catalog[0].pointer, "/results/0/payload/weighted_yes_share");
    assert_eq!(catalog[1].pointer, "/results/1/payload/weighted_yes_share");
}

#[test]
fn claim_catalog_has_hard_entry_and_byte_limits_without_truncating_values() {
    let mut req = request();
    let result = req.results[0].clone();
    req.results = vec![result; 64];
    let catalog = evidence_catalog(&req);
    assert!(catalog.len() <= 128);
    assert!(serde_json::to_vec(&catalog).unwrap().len() <= 16_384);
    for index in 0..64 {
        assert!(catalog
            .iter()
            .any(|e| e.pointer == format!("/results/{index}/payload/weighted_yes_share")));
    }
    req.plan.assumptions = vec!["x".repeat(20_000)];
    req.results.truncate(1);
    req.results[0].payload["very_long_text"] = json!("y".repeat(20_000));
    req.results[0].payload["escaped/key~"] = json!(0.75);
    let catalog = evidence_catalog(&req);
    assert!(serde_json::to_vec(&catalog).unwrap().len() <= 16_384);
    assert!(!catalog.iter().any(|e| e.pointer == "/plan/assumptions/0"));
    assert!(!catalog
        .iter()
        .any(|e| e.pointer.ends_with("/very_long_text")));
    assert!(catalog
        .iter()
        .any(|e| e.pointer.ends_with("/escaped~1key~0")));
    let document = serde_json::to_value(req).unwrap();
    for entry in catalog {
        assert_eq!(document.pointer(&entry.pointer), Some(&entry.observed));
    }
}

#[test]
fn claim_attempt_and_explanatory_metadata_are_not_numeric_support_or_catalog_evidence() {
    let mut req = request();
    req.conclusions[0] = "The modeled support is 100%.".into();
    let mut claim = comparison();
    claim.kind = ClaimKind::ModeledResult;
    claim.comparison = None;
    for field in [
        "attempt",
        "attempt_id",
        "timestamp",
        "created_at",
        "updated_at",
        "elapsed_ms",
        "summary",
        "rationale",
        "explanation",
        "notes",
    ] {
        req.results[0].payload[field] = json!(1);
        let pointer = format!("/results/0/payload/{field}");
        claim.evidence = vec![ClaimEvidence {
            pointer: pointer.clone(),
            observed: json!(1),
        }];
        assert_eq!(
            validate_claims(&req, &[claim.clone()], CheckStatus::Pass),
            Err("claim_evidence_kind"),
            "{field}"
        );
        assert!(!evidence_catalog(&req).iter().any(|e| e.pointer == pointer));
    }
}
