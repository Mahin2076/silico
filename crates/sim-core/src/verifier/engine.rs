use super::{transport::MAX_RESPONSE_BYTES, *};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;

const SYSTEM_PROMPT: &str = include_str!("prompt.txt");

#[derive(Clone, Copy, Debug)]
pub struct VerifierLimits {
    pub timeout: Duration,
    pub max_inflight: usize,
}

impl Default for VerifierLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(35),
            max_inflight: 2,
        }
    }
}

pub struct Verifier<T> {
    transport: T,
    limits: VerifierLimits,
    permits: Arc<Semaphore>,
}

impl<T: VerifierTransport> Verifier<T> {
    /// Search only after deterministic preflight passes. Return the exact enriched
    /// request so P1 can bind completion to both the experiment and its citations.
    pub async fn verify_with_research(
        &self,
        request: &VerificationRequest,
        query: &ResearchQuery,
        search: &YouSearchClient,
    ) -> Result<ResearchedVerification, TransportError> {
        if request.web_research.is_some() {
            return Err(TransportError::InvalidConfiguration);
        }
        let input = serde_json::to_value(request).expect("typed request serializes");
        let mut repairs = Vec::new();
        if invalid_input(request, &input).is_some()
            || local_checks(request, &input, &mut repairs)
                .iter()
                .any(|f| f.status != CheckStatus::Pass)
        {
            return Ok(ResearchedVerification {
                request: request.clone(),
                decision: self.verify(request).await,
            });
        }
        // Share the verifier's concurrency budget for search as well as inference.
        // Queue wait is bounded independently; the search HTTP deadline is 10s.
        let web = match tokio::time::timeout(Duration::from_secs(10), self.permits.acquire()).await
        {
            Ok(Ok(permit)) => {
                let web = search.search(query, &request.plan.context.as_of_date).await;
                drop(permit);
                web
            }
            _ => WebResearch {
                provider: "you.com".into(),
                mode: "live".into(),
                query: query.clone(),
                as_of_date: request.plan.context.as_of_date.clone(),
                fetched_at: chrono::Utc::now().to_rfc3339(),
                status: "failed".into(),
                search_calls: 0,
                error_code: Some("search_queue_timeout".into()),
                sources: vec![],
            },
        };
        let mut enriched = request.clone();
        enriched.web_research = Some(web);
        let decision = self.verify(&enriched).await;
        Ok(ResearchedVerification {
            request: enriched,
            decision,
        })
    }

    pub fn new(transport: T) -> Self {
        Self::with_limits(transport, VerifierLimits::default()).expect("default limits are valid")
    }

    pub fn with_limits(transport: T, limits: VerifierLimits) -> Result<Self, TransportError> {
        if limits.timeout.is_zero()
            || limits.timeout > Duration::from_secs(60)
            || !(1..=4).contains(&limits.max_inflight)
        {
            return Err(TransportError::InvalidConfiguration);
        }
        Ok(Self {
            transport,
            limits,
            permits: Arc::new(Semaphore::new(limits.max_inflight)),
        })
    }

    /// At most one model call, no automatic retries, and no mutation or repair
    /// execution. The deadline includes waiting for a concurrency permit.
    pub async fn verify(&self, request: &VerificationRequest) -> VerificationDecision {
        let started = Instant::now();
        let identity = self.transport.identity();
        let mut decision = empty_decision(request, &identity);
        if !matches!(identity.mode.as_str(), "live" | "mock")
            || !identity.model.to_ascii_lowercase().contains("nemotron")
        {
            decision.receipt.error_code = Some("invalid_transport_identity".into());
            decision.summary =
                "The independent verifier requires an explicitly identified Nemotron transport."
                    .into();
            finish_unreviewed(request, &mut decision);
            return finish(decision, started);
        }
        let input = serde_json::to_value(request).expect("typed request serializes");
        if let Some(reason) = invalid_input(request, &input) {
            decision.findings.push(finding(
                LOCAL_CHECKS[0],
                CheckStatus::Blocked,
                reason,
                vec![],
            ));
            finish_unreviewed(request, &mut decision);
            decision.receipt.error_code = Some("invalid_input".into());
            decision.summary =
                "The verifier rejected an invalid or stale experiment envelope.".into();
            return finish(decision, started);
        }

        decision.findings = local_checks(request, &input, &mut decision.repair_requests);
        if decision
            .findings
            .iter()
            .any(|f| f.status != CheckStatus::Pass)
        {
            decision.verdict = if decision
                .findings
                .iter()
                .any(|f| f.status == CheckStatus::Blocked)
            {
                Verdict::Blocked
            } else {
                Verdict::Repair
            };
            decision.summary =
                "Local integrity checks rejected completion; Nemotron was not called.".into();
            finish_unreviewed(request, &mut decision);
            enforce_round_budget(request, &mut decision);
            return finish(decision, started);
        }

        let required = semantic_checks(request);
        let mut evidence_pointers = vec![
            "/goal".to_string(),
            "/plan/assumptions".into(),
            "/plan/scenarios".into(),
            "/conclusions".into(),
        ];
        evidence_pointers.extend(
            request
                .results
                .iter()
                .enumerate()
                .map(|(i, _)| format!("/results/{i}/payload")),
        );
        if let Some(web) = &request.web_research {
            evidence_pointers.extend(
                web.sources
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("/web_research/sources/{i}")),
            );
        }
        let user = json!({
            "schema_version": SCHEMA_VERSION,
            "run_id": request.run_id,
            "plan_hash": request.plan_hash,
            "request_hash": decision.request_hash,
            "repair_round": request.repair_round,
            "evidence_pointers": evidence_pointers,
            "required_checks": required.iter().map(|(id, description)| json!({"id":id,"description":description})).collect::<Vec<_>>(),
            "request": request,
        }).to_string();
        let calls = AtomicU32::new(0);
        let operation = async {
            let _permit = self
                .permits
                .acquire()
                .await
                .map_err(|_| TransportError::Network)?;
            calls.store(1, Ordering::Relaxed);
            self.transport.complete(SYSTEM_PROMPT, &user).await
        };
        let result = tokio::time::timeout(self.limits.timeout, operation).await;
        decision.receipt.model_calls = calls.load(Ordering::Relaxed);
        if decision.receipt.model_calls > 0 {
            decision.receipt.mode = identity.mode;
        }
        let completion = match result {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => return unavailable(request, decision, started, error.code()),
            Err(_) => return unavailable(request, decision, started, "timeout"),
        };
        if completion.model != identity.model {
            return unavailable(request, decision, started, "model_mismatch");
        }
        if completion.text.len() > MAX_RESPONSE_BYTES {
            return unavailable(request, decision, started, "response_too_large");
        }
        let review: ModelReview = match serde_json::from_str(&completion.text) {
            Ok(value) => value,
            Err(_) => return unavailable(request, decision, started, "malformed_review"),
        };
        if let Err(code) = validate_review(request, &input, &review) {
            return unavailable(request, decision, started, code);
        }
        decision.verdict = review.verdict;
        decision.summary = review.summary;
        decision.findings.extend(review.findings);
        decision.repair_requests = review.repair_requests;
        enforce_round_budget(request, &mut decision);
        finish(decision, started)
    }
}

fn empty_decision(
    request: &VerificationRequest,
    identity: &TransportIdentity,
) -> VerificationDecision {
    VerificationDecision {
        schema_version: SCHEMA_VERSION,
        run_id: request.run_id.clone(),
        plan_hash: request.plan_hash.clone(),
        request_hash: request_hash(request),
        repair_round: request.repair_round,
        verdict: Verdict::Blocked,
        summary: String::new(),
        findings: vec![],
        repair_requests: vec![],
        receipt: VerificationReceipt {
            mode: "local".into(),
            provider: identity.provider.clone(),
            model: identity.model.clone(),
            prompt_version: PROMPT_VERSION.into(),
            model_calls: 0,
            elapsed_ms: 0,
            error_code: None,
        },
    }
}

fn finish(mut decision: VerificationDecision, started: Instant) -> VerificationDecision {
    decision.receipt.elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    decision
}

fn unavailable(
    request: &VerificationRequest,
    mut decision: VerificationDecision,
    started: Instant,
    code: &str,
) -> VerificationDecision {
    decision.verdict = Verdict::Blocked;
    decision.receipt.error_code = Some(code.into());
    decision.summary = format!("Independent verification is unavailable ({code}); completion is blocked. No scenario was rerun.");
    for (id, _) in semantic_checks(request) {
        decision.findings.push(finding(
            &id,
            CheckStatus::Blocked,
            "No validated independent review is available.",
            vec![],
        ));
    }
    finish(decision, started)
}

fn enforce_round_budget(request: &VerificationRequest, decision: &mut VerificationDecision) {
    if request.repair_round >= MAX_REPAIR_ROUND && decision.verdict == Verdict::Repair {
        decision.verdict = Verdict::Blocked;
        decision.receipt.error_code = Some("repair_budget_exhausted".into());
        decision.summary = "Verification still requires repairs after the final allowed round. Completion is blocked; no further repairs may execute automatically.".into();
    }
}

fn finding(
    id: &str,
    status: CheckStatus,
    summary: impl Into<String>,
    evidence: Vec<EvidenceRef>,
) -> Finding {
    Finding {
        check_id: id.into(),
        status,
        summary: summary.into(),
        evidence,
    }
}

fn evidence(pointer: impl Into<String>) -> EvidenceRef {
    EvidenceRef {
        pointer: pointer.into(),
    }
}

fn id_valid(id: &str) -> bool {
    (1..=80).contains(&id.len())
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

fn hash_valid(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn unique<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = BTreeSet::new();
    values.into_iter().all(|id| seen.insert(id))
}

fn invalid_input(request: &VerificationRequest, input: &Value) -> Option<&'static str> {
    let plan = &request.plan;
    if input.to_string().len() > 131_072 {
        return Some("The request exceeds the 128 KiB input budget.");
    }
    if request.schema_version != SCHEMA_VERSION {
        return Some("Unsupported schema version.");
    }
    if !id_valid(&request.run_id) || request.goal.trim().is_empty() || request.goal.len() > 8_000 {
        return Some("A stable run ID and a nonempty bounded goal are required.");
    }
    if request.repair_round > MAX_REPAIR_ROUND {
        return Some("Repair round exceeds the two-round limit.");
    }
    if !hash_valid(&request.plan_hash) || request.plan_hash != plan_hash(&request.goal, plan) {
        return Some("Plan hash does not match the immutable goal and plan.");
    }
    let context = &plan.context;
    if !hash_valid(&context.population_hash)
        || !hash_valid(&context.evidence_hash)
        || context.model.trim().is_empty()
        || context.model.len() > 160
        || context.as_of_date.len() != 10
        || chrono::NaiveDate::parse_from_str(&context.as_of_date, "%Y-%m-%d").is_err()
    {
        return Some(
            "Frozen context requires population/evidence hashes, a model, and a valid as-of date.",
        );
    }
    if plan.scenarios.is_empty()
        || plan.scenarios.len() > 32
        || plan.completion_checks.len() > 64
        || plan.assumptions.len() > 64
        || request.results.len() > 64
        || request.deterministic_findings.len() > 128
        || request.conclusions.len() > 32
    {
        return Some("The experiment exceeds a collection budget or has no scenarios.");
    }
    if plan.scenarios.iter().any(|s| {
        !id_valid(&s.id)
            || !id_valid(&s.audience_id)
            || s.description.trim().is_empty()
            || !s.inputs.as_object().is_some_and(|m| !m.is_empty())
    }) || !unique(plan.scenarios.iter().map(|s| s.id.as_str()))
    {
        return Some("Scenario IDs must be unique, described, and assigned to explicit audiences.");
    }
    if plan
        .completion_checks
        .iter()
        .any(|c| !id_valid(&c.id) || c.id.starts_with("p2.") || c.description.trim().is_empty())
        || !unique(plan.completion_checks.iter().map(|c| c.id.as_str()))
    {
        return Some("Completion-check IDs must be unique; the p2. namespace is reserved.");
    }
    if !plan
        .completion_checks
        .iter()
        .any(|c| c.required && c.kind == CheckKind::Deterministic)
    {
        return Some("At least one required deterministic completion check is required.");
    }
    if request.results.iter().any(|r| !id_valid(&r.id))
        || !unique(request.results.iter().map(|r| r.id.as_str()))
    {
        return Some("Result IDs must be unique stable identifiers.");
    }
    None
}

fn local_checks(
    request: &VerificationRequest,
    input: &Value,
    repairs: &mut Vec<RepairRequest>,
) -> Vec<Finding> {
    let mut findings = vec![finding(
        LOCAL_CHECKS[0],
        CheckStatus::Pass,
        "Schema, bounded input, stable IDs, plan hash, round, and frozen context are valid.",
        vec![evidence("/plan"), evidence("/goal")],
    )];
    let scenarios: BTreeMap<_, _> = request
        .plan
        .scenarios
        .iter()
        .map(|s| (s.id.as_str(), s))
        .collect();
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut unknown = false;
    for result in &request.results {
        unknown |= !scenarios.contains_key(result.scenario_id.as_str());
        *counts.entry(&result.scenario_id).or_default() += 1;
    }
    let duplicate = counts.values().any(|n| *n > 1);
    let missing: Vec<_> = request
        .plan
        .scenarios
        .iter()
        .filter(|s| {
            !request
                .results
                .iter()
                .any(|r| r.scenario_id == s.id && r.status == ResultStatus::Complete)
        })
        .map(|s| s.id.clone())
        .collect();
    let coverage = if unknown || duplicate {
        CheckStatus::Blocked
    } else if missing.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Repair
    };
    findings.push(finding(
        LOCAL_CHECKS[1],
        coverage,
        if coverage == CheckStatus::Pass {
            "Every planned scenario has exactly one complete result."
        } else if coverage == CheckStatus::Blocked {
            "Unknown or duplicate scenario results make coverage ambiguous."
        } else {
            "Planned scenarios are missing, pending, or failed."
        },
        vec![evidence("/plan/scenarios"), evidence("/results")],
    ));
    if coverage == CheckStatus::Repair {
        add_repair(
            repairs,
            RepairAction::RerunScenario,
            LOCAL_CHECKS[1],
            missing,
            "Run only the missing, pending, or failed scenarios under the frozen plan.",
        );
    }

    let stale: Vec<_> = request
        .results
        .iter()
        .filter(|r| {
            scenarios.contains_key(r.scenario_id.as_str())
                && (r.context != request.plan.context || r.plan_hash != request.plan_hash)
        })
        .map(|r| r.scenario_id.clone())
        .collect();
    let context_status = if stale.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Repair
    };
    findings.push(finding(
        LOCAL_CHECKS[2],
        context_status,
        if stale.is_empty() {
            "All results match the frozen population, model, date, evidence, seed, and plan hash."
        } else {
            "Results were produced under a different immutable context or plan."
        },
        vec![evidence("/plan/context"), evidence("/results")],
    ));
    if !stale.is_empty() {
        add_repair(
            repairs,
            RepairAction::RerunScenario,
            LOCAL_CHECKS[2],
            stale,
            "Rerun stale scenarios with the exact frozen context; do not relabel old results.",
        );
    }

    let wrong_audience: Vec<_> = request
        .results
        .iter()
        .filter(|r| {
            scenarios
                .get(r.scenario_id.as_str())
                .is_some_and(|s| s.audience_id != r.audience_id)
        })
        .map(|r| r.scenario_id.clone())
        .collect();
    let audience_status = if wrong_audience.is_empty() {
        CheckStatus::Pass
    } else {
        CheckStatus::Repair
    };
    findings.push(finding(
        LOCAL_CHECKS[3],
        audience_status,
        if wrong_audience.is_empty() {
            "Result audience IDs match their corresponding scenarios."
        } else {
            "At least one result uses the wrong planned audience."
        },
        vec![evidence("/plan/scenarios"), evidence("/results")],
    ));
    if !wrong_audience.is_empty() {
        add_repair(
            repairs,
            RepairAction::RerunScenario,
            LOCAL_CHECKS[3],
            wrong_audience,
            "Rerun the affected scenarios for the planned audience.",
        );
    }

    let specs: BTreeMap<_, _> = request
        .plan
        .completion_checks
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();
    let mut deterministic_status = CheckStatus::Pass;
    let mut seen = BTreeSet::new();
    for check in &request.deterministic_findings {
        let valid = specs
            .get(check.check_id.as_str())
            .is_some_and(|s| s.kind == CheckKind::Deterministic)
            && seen.insert(check.check_id.as_str())
            && check
                .scenario_ids
                .iter()
                .all(|id| scenarios.contains_key(id.as_str()))
            && !check.detail.trim().is_empty()
            && !check.evidence.is_empty()
            && check.evidence.iter().all(|e| evidence_valid(input, e))
            && (!check.passed
                || check
                    .evidence
                    .iter()
                    .all(|e| input.pointer(&e.pointer).is_some_and(meaningful)));
        if !valid {
            deterministic_status = CheckStatus::Blocked;
            continue;
        }
        let status = if check.passed {
            CheckStatus::Pass
        } else {
            CheckStatus::Repair
        };
        findings.push(finding(
            &check.check_id,
            status,
            check.detail.clone(),
            check.evidence.clone(),
        ));
        if !check.passed {
            if deterministic_status != CheckStatus::Blocked {
                deterministic_status = CheckStatus::Repair;
            }
            let action = if check.scenario_ids.is_empty() {
                RepairAction::RevisePlan
            } else {
                RepairAction::RerunScenario
            };
            add_repair(repairs, action, &check.check_id, check.scenario_ids.clone(), "Resolve this deterministic failure and recompute its original evidence; a model pass cannot override it.");
        }
    }
    let absent: Vec<_> = request
        .plan
        .completion_checks
        .iter()
        .filter(|s| {
            s.required && s.kind == CheckKind::Deterministic && !seen.contains(s.id.as_str())
        })
        .collect();
    if !absent.is_empty() || request.deterministic_findings.is_empty() {
        deterministic_status = CheckStatus::Blocked;
    }
    for check in absent {
        findings.push(finding(
            &check.id,
            CheckStatus::Blocked,
            "The required deterministic finding is missing or invalid.",
            vec![],
        ));
    }
    findings.push(finding(LOCAL_CHECKS[4], deterministic_status, match deterministic_status {
        CheckStatus::Pass => "Required deterministic findings are present, evidenced, and passing.",
        CheckStatus::Repair => "A deterministic check failed; completion is forbidden.",
        _ => "Deterministic findings are missing, duplicated, unknown, or unsupported by evidence.",
    }, vec![evidence("/plan/completion_checks"), evidence("/deterministic_findings")]));

    let empty = request.results.iter().any(|r| {
        r.status == ResultStatus::Complete && !r.payload.as_object().is_some_and(|m| !m.is_empty())
    });
    let conclusions_missing =
        request.conclusions.is_empty() || request.conclusions.iter().any(|c| c.trim().is_empty());
    let evidence_status = if empty || conclusions_missing {
        CheckStatus::Blocked
    } else {
        CheckStatus::Pass
    };
    findings.push(finding(LOCAL_CHECKS[5], evidence_status, if evidence_status == CheckStatus::Pass {
        "Completed results contain payloads and conclusions are available for review."
    } else { "A completed result payload or a conclusion is missing; independent review has no sufficient evidence." }, vec![evidence("/results"), evidence("/conclusions")]));
    if let Some(web) = &request.web_research {
        let ready = web.is_ready(&request.plan.context.as_of_date);
        findings.push(finding("p2.web_sources", if ready { CheckStatus::Pass } else { CheckStatus::Blocked },
            if ready { "You.com sources have bounded, dated provenance and match the frozen evidence cutoff. Undated pages are background only." }
            else { "Required web research is unavailable, empty, invalid, or outside the evidence cutoff; Nemotron was not called." },
            vec![evidence("/web_research")]));
    }
    findings
}

fn meaningful(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(s) => !s.trim().is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
        _ => true,
    }
}

fn evidence_valid(input: &Value, reference: &EvidenceRef) -> bool {
    let p = &reference.pointer;
    p.len() <= 512
        && [
            "/goal",
            "/plan",
            "/results",
            "/conclusions",
            "/web_research/sources",
        ]
        .iter()
        .any(|root| {
            p == root
                || p.strip_prefix(root)
                    .is_some_and(|tail| tail.starts_with('/'))
        })
        && input.pointer(p).is_some()
}

fn add_repair(
    repairs: &mut Vec<RepairRequest>,
    action: RepairAction,
    check: &str,
    mut scenarios: Vec<String>,
    reason: &str,
) {
    scenarios.sort();
    scenarios.dedup();
    if let Some(existing) = repairs
        .iter_mut()
        .find(|r| r.action == action && r.scenario_ids == scenarios)
    {
        if !existing.check_ids.iter().any(|id| id == check) {
            existing.check_ids.push(check.into());
            existing.check_ids.sort();
            existing.reason.push(' ');
            existing.reason.push_str(reason);
        }
        return;
    }
    repairs.push(RepairRequest {
        id: format!("repair-{}", repairs.len() + 1),
        action,
        check_ids: vec![check.into()],
        scenario_ids: scenarios,
        reason: reason.into(),
    });
}

fn semantic_checks(request: &VerificationRequest) -> Vec<(String, String)> {
    let mut checks: Vec<_> = SEMANTIC_CHECKS.iter().map(|id| (id.to_string(), match *id {
        "p2.goal_alignment" => "The plan and completed scenarios answer the original goal.",
        "p2.explicit_assumptions" => "Consequential assumptions are explicit, specific, and consistent.",
        _ => "Conclusions follow from results and preserve synthetic-data and uncertainty limitations.",
    }.to_string())).collect();
    checks.extend(
        request
            .plan
            .completion_checks
            .iter()
            .filter(|c| c.required && c.kind == CheckKind::Semantic)
            .map(|c| (c.id.clone(), c.description.clone())),
    );
    if request.web_research.is_some() {
        checks.push(("p2.web_grounding".into(), "Check that the retrieved sources are relevant to this experiment's assumptions or external claims and that conclusions represent their evidence faithfully. Cite at least one individual /web_research/sources/N source. Web snippets cannot validate synthetic support percentages or prove real-world causality. Undated snippets are methodological background only; do not use them as evidence of a historical fact or state as of a past date.".into()));
    }
    checks
}

fn finish_unreviewed(request: &VerificationRequest, decision: &mut VerificationDecision) {
    for id in LOCAL_CHECKS
        .iter()
        .map(|s| s.to_string())
        .chain(semantic_checks(request).into_iter().map(|(id, _)| id))
        .chain(
            request
                .plan
                .completion_checks
                .iter()
                .filter(|c| c.required)
                .map(|c| c.id.clone()),
        )
    {
        if !decision.findings.iter().any(|f| f.check_id == id) {
            decision.findings.push(finding(
                &id,
                CheckStatus::NotReviewed,
                "Not reviewed because prerequisite integrity checks failed.",
                vec![],
            ));
        }
    }
    if request.web_research.is_some()
        && !decision
            .findings
            .iter()
            .any(|f| f.check_id == "p2.web_sources")
    {
        decision.findings.push(finding(
            "p2.web_sources",
            CheckStatus::NotReviewed,
            "Not reviewed because prerequisite integrity checks failed.",
            vec![],
        ));
    }
}

fn validate_review(
    request: &VerificationRequest,
    input: &Value,
    review: &ModelReview,
) -> Result<(), &'static str> {
    if review.schema_version != SCHEMA_VERSION
        || review.run_id != request.run_id
        || review.plan_hash != request.plan_hash
        || review.request_hash != request_hash(request)
        || review.repair_round != request.repair_round
    {
        return Err("review_identity_mismatch");
    }
    let required: BTreeSet<_> = semantic_checks(request)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    if review.summary.trim().is_empty()
        || review.summary.len() > 2000
        || review.findings.len() != required.len()
        || review.repair_requests.len() > 16
        || !unique(review.findings.iter().map(|f| f.check_id.as_str()))
        || review
            .findings
            .iter()
            .any(|f| !required.contains(&f.check_id))
    {
        return Err("invalid_review_coverage");
    }
    for finding in &review.findings {
        if finding.summary.trim().is_empty()
            || finding.summary.len() > 2000
            || finding.status == CheckStatus::NotReviewed
            || finding.evidence.len() > 16
            || finding.evidence.iter().any(|e| !evidence_valid(input, e))
            || (finding.status == CheckStatus::Pass
                && finding
                    .evidence
                    .iter()
                    .any(|e| !input.pointer(&e.pointer).is_some_and(meaningful)))
            || (finding.status == CheckStatus::Pass && finding.evidence.is_empty())
        {
            return Err("invalid_review_evidence");
        }
        if finding.check_id == "p2.web_grounding"
            && finding.status == CheckStatus::Pass
            && !finding.evidence.iter().any(|e| {
                e.pointer
                    .strip_prefix("/web_research/sources/")
                    .is_some_and(|tail| {
                        let index = tail.split('/').next().unwrap_or("");
                        index.parse::<usize>().ok().is_some_and(|n| {
                            request
                                .web_research
                                .as_ref()
                                .is_some_and(|web| n < web.sources.len())
                        })
                    })
            })
        {
            return Err("missing_web_source_evidence");
        }
    }
    let expected = if review
        .findings
        .iter()
        .any(|f| f.status == CheckStatus::Blocked)
    {
        Verdict::Blocked
    } else if review
        .findings
        .iter()
        .any(|f| f.status == CheckStatus::Repair)
    {
        Verdict::Repair
    } else {
        Verdict::Pass
    };
    if review.verdict != expected
        || (expected == Verdict::Pass && !review.repair_requests.is_empty())
    {
        return Err("inconsistent_review_verdict");
    }
    if !unique(review.repair_requests.iter().map(|r| r.id.as_str())) {
        return Err("invalid_repair_request");
    }
    for repair in &review.repair_requests {
        if !id_valid(&repair.id)
            || repair.reason.trim().is_empty()
            || repair.reason.len() > 2000
            || repair.check_ids.is_empty()
            || !unique(repair.check_ids.iter().map(String::as_str))
            || !unique(repair.scenario_ids.iter().map(String::as_str))
            || repair.check_ids.iter().any(|id| {
                !review
                    .findings
                    .iter()
                    .any(|f| &f.check_id == id && f.status != CheckStatus::Pass)
            })
            || repair
                .scenario_ids
                .iter()
                .any(|id| !request.plan.scenarios.iter().any(|s| &s.id == id))
            || (repair.action == RepairAction::RerunScenario && repair.scenario_ids.is_empty())
        {
            return Err("invalid_repair_request");
        }
    }
    if review
        .findings
        .iter()
        .filter(|f| f.status != CheckStatus::Pass)
        .any(|f| {
            !review
                .repair_requests
                .iter()
                .any(|r| r.check_ids.contains(&f.check_id))
        })
    {
        return Err("missing_targeted_repair");
    }
    Ok(())
}
