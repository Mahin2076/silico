use super::*;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

fn request() -> VerificationRequest {
    serde_json::from_str(include_str!("fixtures/complete.json")).unwrap()
}

fn passing_review(envelope: &Value) -> Value {
    json!({
        "schema_version": SCHEMA_VERSION,
        "run_id": envelope["run_id"], "plan_hash": envelope["plan_hash"],
        "request_hash": envelope["request_hash"], "repair_round": envelope["repair_round"],
        "verdict": "pass", "summary": "Fixture review, not live evidence.",
        "claim_assessments": serde_json::from_str::<Value>(include_str!("fixtures/fixture-review.json")).unwrap()["claim_assessments"],
        "repair_assessments": [],
        "findings": envelope["required_checks"].as_array().unwrap().iter().map(|c| json!({
            "check_id": c["id"], "status":"pass", "summary":"Supported by the supplied software fixture.",
            "evidence":[{"pointer":"/results/0/payload"}]
        })).collect::<Vec<_>>(), "repair_requests":[]
    })
}

type Respond = dyn Fn(Value) -> Result<String, TransportError> + Send + Sync;
struct MockTransport {
    calls: Arc<AtomicU32>,
    respond: Box<Respond>,
    delay: Duration,
}

impl MockTransport {
    fn new(f: impl Fn(Value) -> Result<String, TransportError> + Send + Sync + 'static) -> Self {
        Self {
            calls: Arc::new(AtomicU32::new(0)),
            respond: Box::new(f),
            delay: Duration::ZERO,
        }
    }
    fn pass() -> Self {
        Self::new(|v| Ok(passing_review(&v).to_string()))
    }
}

impl VerifierTransport for MockTransport {
    fn identity(&self) -> TransportIdentity {
        TransportIdentity {
            mode: "mock".into(),
            provider: "labeled-fixture".into(),
            model: DEFAULT_MODEL.into(),
        }
    }
    fn complete<'a>(&'a self, system: &'a str, user: &'a str) -> CompletionFuture<'a> {
        Box::pin(async move {
            assert!(system.contains("UNTRUSTED DATA"));
            self.calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.delay).await;
            Ok(Completion {
                text: (self.respond)(serde_json::from_str(user).unwrap())?,
                model: DEFAULT_MODEL.into(),
            })
        })
    }
}

async fn transformed(f: impl Fn(&mut Value) + Send + Sync + 'static) -> VerificationDecision {
    let mock = MockTransport::new(move |envelope| {
        let mut review = passing_review(&envelope);
        f(&mut review);
        Ok(review.to_string())
    });
    Verifier::new(mock).verify(&request()).await
}

fn failure_code(decision: &VerificationDecision, code: &str) {
    assert_eq!(decision.verdict, Verdict::Blocked);
    assert_eq!(decision.receipt.error_code.as_deref(), Some(code));
    assert!(!decision.permits_completion(&request()));
}

#[tokio::test]
async fn complete_fixture_reviews_once_and_cannot_claim_live_completion() {
    let r = request();
    assert_eq!(r.plan_hash, plan_hash(&r.goal, &r.plan));
    let mock = MockTransport::pass();
    let calls = mock.calls.clone();
    let d = Verifier::new(mock).verify(&r).await;
    assert_eq!(d.verdict, Verdict::Pass);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(d.receipt.mode, "mock");
    assert!(
        !d.permits_completion(&r),
        "a fixture verdict is not live verification"
    );
    for check in LOCAL_CHECKS.iter().chain(SEMANTIC_CHECKS.iter()) {
        assert_eq!(
            d.findings.iter().filter(|f| f.check_id == *check).count(),
            1
        );
    }
}

#[tokio::test]
async fn local_defects_never_spend_a_model_call() {
    let mut cases = Vec::new();
    let mut r = request();
    r.results.pop();
    cases.push(("missing scenario", r, Verdict::Repair));
    let mut r = request();
    r.results[0].status = ResultStatus::Pending;
    cases.push(("pending scenario", r, Verdict::Repair));
    let mut r = request();
    r.results[0].status = ResultStatus::Failed;
    cases.push(("failed scenario", r, Verdict::Repair));
    let mut r = request();
    r.results[0].audience_id = "different-audience".into();
    cases.push(("wrong audience", r, Verdict::Repair));
    let mut r = request();
    r.results[0].context.seed += 1;
    cases.push(("wrong seed", r, Verdict::Repair));
    let mut r = request();
    r.results[0].context.model = "different-model".into();
    cases.push(("wrong model", r, Verdict::Repair));
    let mut r = request();
    r.results[0].context.population_hash = "f".repeat(64);
    cases.push(("wrong population", r, Verdict::Repair));
    let mut r = request();
    r.results[0].context.evidence_hash = "f".repeat(64);
    cases.push(("wrong evidence", r, Verdict::Repair));
    let mut r = request();
    r.results[0].context.as_of_date = "2026-10-16".into();
    cases.push(("wrong date", r, Verdict::Repair));
    let mut r = request();
    r.results[0].plan_hash = "f".repeat(64);
    cases.push(("stale result hash", r, Verdict::Repair));
    let mut r = request();
    r.deterministic_findings[0].passed = false;
    cases.push(("failed deterministic check", r, Verdict::Repair));
    let mut r = request();
    r.deterministic_findings.clear();
    cases.push(("missing deterministic checks", r, Verdict::Blocked));
    let mut r = request();
    r.results[0].payload = Value::Null;
    cases.push(("missing payload", r, Verdict::Blocked));
    let mut r = request();
    r.results[0].payload = json!(true);
    cases.push(("primitive payload", r, Verdict::Blocked));
    let mut r = request();
    r.conclusions.clear();
    cases.push(("missing conclusion", r, Verdict::Blocked));
    let mut r = request();
    r.results[0].scenario_id = "unknown-scenario".into();
    cases.push(("unknown result", r, Verdict::Blocked));
    let mut r = request();
    r.results[1].scenario_id = r.results[0].scenario_id.clone();
    cases.push(("duplicate scenario result", r, Verdict::Blocked));
    let mut r = request();
    r.deterministic_findings[0].evidence[0].pointer = "/results/900/payload".into();
    cases.push(("invented deterministic evidence", r, Verdict::Blocked));
    for (name, r, expected) in cases {
        let mock = MockTransport::pass();
        let calls = mock.calls.clone();
        let d = Verifier::new(mock).verify(&r).await;
        assert_eq!(d.verdict, expected, "{name}");
        assert_eq!(d.receipt.model_calls, 0, "{name}");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "{name}");
        assert!(!d.permits_completion(&r), "{name}");
        for c in r.plan.completion_checks.iter().filter(|c| c.required) {
            assert!(
                d.findings.iter().any(|f| f.check_id == c.id),
                "{name}: {} missing",
                c.id
            );
        }
        for repair in d.repair_requests {
            assert!(
                repair
                    .scenario_ids
                    .iter()
                    .all(|id| r.plan.scenarios.iter().any(|s| &s.id == id)),
                "unknown repair target in {name}"
            );
        }
    }
}

#[tokio::test]
async fn malformed_envelopes_are_blocked_without_inference() {
    let mut cases = Vec::new();
    let mut r = request();
    r.schema_version = 9;
    cases.push(r);
    let mut r = request();
    r.plan_hash = "0".repeat(64);
    cases.push(r);
    let mut r = request();
    r.repair_round = 3;
    cases.push(r);
    let mut r = request();
    r.goal.clear();
    cases.push(r);
    let mut r = request();
    r.goal = "x".repeat(140_000);
    cases.push(r);
    let mut r = request();
    r.results[1].id = r.results[0].id.clone();
    cases.push(r);
    let mut r = request();
    r.plan.scenarios[1].id = r.plan.scenarios[0].id.clone();
    r.plan_hash = plan_hash(&r.goal, &r.plan);
    cases.push(r);
    let mut r = request();
    r.plan.completion_checks[0].id = "p2.spoof".into();
    r.plan_hash = plan_hash(&r.goal, &r.plan);
    cases.push(r);
    let mut r = request();
    r.plan.context.as_of_date = "2026-99-01".into();
    r.plan_hash = plan_hash(&r.goal, &r.plan);
    cases.push(r);
    for r in cases {
        let d = Verifier::new(MockTransport::pass()).verify(&r).await;
        failure_code(&d, "invalid_input");
        assert_eq!(d.receipt.model_calls, 0);
    }
}

#[tokio::test]
async fn plan_revisions_invalidate_previously_completed_results() {
    let mut r = request();
    r.plan.scenarios[0].inputs["stimulus"] = json!("A different exact message.");
    assert_ne!(r.plan_hash, plan_hash(&r.goal, &r.plan));
    r.plan_hash = plan_hash(&r.goal, &r.plan);
    let d = Verifier::new(MockTransport::pass()).verify(&r).await;
    assert_eq!(d.verdict, Verdict::Repair);
    assert_eq!(d.receipt.model_calls, 0);
    assert!(d
        .repair_requests
        .iter()
        .any(|x| x.action == RepairAction::RerunScenario && x.scenario_ids.len() == 2));
}

#[tokio::test]
async fn related_local_failures_share_one_targeted_rerun() {
    let mut r = request();
    r.results[0].context.seed += 1;
    r.results[0].audience_id = "wrong-audience".into();
    let d = Verifier::new(MockTransport::pass()).verify(&r).await;
    assert_eq!(d.verdict, Verdict::Repair);
    assert_eq!(d.receipt.model_calls, 0);
    assert_eq!(d.repair_requests.len(), 1);
    assert_eq!(d.repair_requests[0].scenario_ids, ["message-a"]);
    assert_eq!(d.repair_requests[0].check_ids.len(), 2);
}

#[tokio::test]
async fn model_identity_must_echo_every_binding() {
    for field in [
        "run_id",
        "plan_hash",
        "request_hash",
        "repair_round",
        "schema_version",
    ] {
        let d = transformed(move |v| {
            v[field] = if field == "repair_round" || field == "schema_version" {
                json!(77)
            } else {
                json!("stale")
            }
        })
        .await;
        failure_code(&d, "review_identity_mismatch");
    }
}

#[tokio::test]
async fn missing_duplicated_and_invented_review_checks_are_blocked() {
    failure_code(
        &transformed(|v| {
            v["findings"].as_array_mut().unwrap().pop();
        })
        .await,
        "invalid_review_coverage",
    );
    failure_code(
        &transformed(|v| {
            v["findings"][1] = v["findings"][0].clone();
        })
        .await,
        "invalid_review_coverage",
    );
    failure_code(
        &transformed(|v| {
            v["findings"][0]["check_id"] = json!("p2.deterministic_checks");
        })
        .await,
        "invalid_review_coverage",
    );
}

#[tokio::test]
async fn evidence_must_exist_and_cannot_cite_its_own_verdict() {
    for pointer in [
        "",
        "/",
        "/results/999/payload",
        "/deterministic_findings/0/passed",
        "/plan_hash",
        "/resultsX",
    ] {
        failure_code(
            &transformed(move |v| {
                v["findings"][0]["evidence"][0]["pointer"] = json!(pointer);
            })
            .await,
            "invalid_review_evidence",
        );
    }
    failure_code(
        &transformed(|v| {
            v["findings"][0]["evidence"] = json!([]);
        })
        .await,
        "invalid_review_evidence",
    );
}

#[tokio::test]
async fn malformed_output_never_becomes_a_pass() {
    for text in [
        "not json",
        "```json\n{}\n```",
        "{\"verdict\":\"pass\"}",
        "{",
        "[]",
    ] {
        let d = Verifier::new(MockTransport::new(move |_| Ok(text.into())))
            .verify(&request())
            .await;
        failure_code(&d, "malformed_review");
    }
    failure_code(
        &transformed(|v| {
            v["execute_shell"] = json!("do not execute me");
        })
        .await,
        "malformed_review",
    );
    let d = Verifier::new(MockTransport::new(|_| Ok("x".repeat(65_537))))
        .verify(&request())
        .await;
    failure_code(&d, "response_too_large");
}

fn make_repair(v: &mut Value) {
    v["verdict"] = json!("repair");
    v["findings"][0]["status"] = json!("repair");
    v["repair_requests"] = json!([{"id":"fix-goal","action":"revise_plan","check_ids":["p2.goal_alignment"],"scenario_ids":["message-b"],"reason":"Align the comparison with the requested goal."}]);
}

#[tokio::test]
async fn actionable_review_produces_only_targeted_supported_repairs() {
    let d = transformed(make_repair).await;
    assert_eq!(d.verdict, Verdict::Repair);
    assert_eq!(d.repair_requests[0].scenario_ids, vec!["message-b"]);
    assert!(!d.permits_completion(&request()));
    failure_code(
        &transformed(|v| {
            make_repair(v);
            v["repair_requests"][0]["scenario_ids"] = json!(["invented"]);
        })
        .await,
        "invalid_repair_request",
    );
    failure_code(
        &transformed(|v| {
            make_repair(v);
            v["repair_requests"][0]["check_ids"] = json!(["p2.explicit_assumptions"]);
        })
        .await,
        "invalid_repair_request",
    );
    failure_code(
        &transformed(|v| {
            make_repair(v);
            v["repair_requests"] = json!([]);
        })
        .await,
        "missing_targeted_repair",
    );
    failure_code(
        &transformed(|v| {
            make_repair(v);
            v["repair_requests"][0]["action"] = json!("execute_code");
        })
        .await,
        "malformed_review",
    );
    failure_code(
        &transformed(|v| {
            make_repair(v);
            v["repair_requests"][0]["action"] = json!("rerun_scenario");
            v["repair_requests"][0]["scenario_ids"] = json!([]);
        })
        .await,
        "invalid_repair_request",
    );
}

#[tokio::test]
async fn verdict_cannot_contradict_findings_or_pending_repairs() {
    failure_code(
        &transformed(|v| {
            v["verdict"] = json!("repair");
        })
        .await,
        "inconsistent_review_verdict",
    );
    failure_code(
        &transformed(|v| {
            make_repair(v);
            v["verdict"] = json!("pass");
        })
        .await,
        "inconsistent_review_verdict",
    );
    failure_code(
        &transformed(|v| {
            v["findings"][0]["status"] = json!("not_reviewed");
        })
        .await,
        "invalid_review_evidence",
    );
}

#[tokio::test]
async fn provider_failures_are_explicit_and_have_no_fallback_or_retries() {
    for error in [
        TransportError::HttpStatus(401),
        TransportError::HttpStatus(429),
        TransportError::HttpStatus(503),
        TransportError::Network,
        TransportError::ModelMismatch,
        TransportError::MalformedResponse,
    ] {
        let mock = MockTransport::new(move |_| Err(error));
        let calls = mock.calls.clone();
        let d = Verifier::new(mock).verify(&request()).await;
        failure_code(&d, error.code());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn timeout_cancels_a_slow_transport() {
    let mut mock = MockTransport::pass();
    mock.delay = Duration::from_secs(10);
    let verifier = Verifier::with_limits(
        mock,
        VerifierLimits {
            timeout: Duration::from_millis(10),
            max_inflight: 1,
        },
    )
    .unwrap();
    failure_code(&verifier.verify(&request()).await, "timeout");
}

#[tokio::test]
async fn concurrency_queue_is_included_in_deadline() {
    let mut mock = MockTransport::pass();
    mock.delay = Duration::from_millis(150);
    let verifier = Verifier::with_limits(
        mock,
        VerifierLimits {
            timeout: Duration::from_millis(250),
            max_inflight: 1,
        },
    )
    .unwrap();
    let r = request();
    let (first, second) = tokio::join!(verifier.verify(&r), verifier.verify(&r));
    assert_eq!(first.verdict, Verdict::Pass);
    failure_code(&second, "timeout");
}

#[tokio::test]
async fn exhausted_repairs_block_but_a_final_round_can_still_pass() {
    let mut r = request();
    r.repair_round = 2;
    r.results.pop();
    failure_code(
        &Verifier::new(MockTransport::pass()).verify(&r).await,
        "repair_budget_exhausted",
    );
    let mut r = request();
    r.repair_round = 2;
    assert_eq!(
        Verifier::new(MockTransport::pass())
            .verify(&r)
            .await
            .verdict,
        Verdict::Pass
    );
}

#[tokio::test]
async fn receipt_cannot_be_reused_for_changed_results() {
    let r = request();
    let mut d = Verifier::new(MockTransport::pass()).verify(&r).await;
    // Exercise binding independently of fixture-vs-live policy. This is NOT a live call.
    d.receipt.mode = "live".into();
    assert!(d.permits_completion(&r));
    let mut changed = r.clone();
    changed.results[0].payload["weighted_yes_share"] = json!(0.01);
    assert_ne!(request_hash(&r), request_hash(&changed));
    assert!(!d.permits_completion(&changed));
    d.findings.pop();
    assert!(!d.permits_completion(&r));
}

#[test]
fn configuration_does_not_leak_keys_or_accept_an_unrelated_model() {
    assert_eq!(
        CrusoeTransport::new("".into(), DEFAULT_MODEL.into()).unwrap_err(),
        TransportError::InvalidConfiguration
    );
    assert_eq!(
        CrusoeTransport::new("fixture".into(), "gpt-4o".into()).unwrap_err(),
        TransportError::InvalidConfiguration
    );
    let transport =
        CrusoeTransport::new("private-fixture-key".into(), DEFAULT_MODEL.into()).unwrap();
    assert!(!format!("{transport:?}").contains("private-fixture-key"));
    assert!(Verifier::with_limits(
        MockTransport::pass(),
        VerifierLimits {
            timeout: Duration::from_secs(99),
            max_inflight: 1
        }
    )
    .is_err());
    assert!(Verifier::with_limits(
        MockTransport::pass(),
        VerifierLimits {
            timeout: Duration::from_secs(1),
            max_inflight: 0
        }
    )
    .is_err());
}

async fn http_transport(
    status: u16,
    body: String,
) -> (CrusoeTransport, tokio::task::JoinHandle<()>) {
    let app = axum::Router::new().route(
        "/chat/completions",
        axum::routing::post(
            move |headers: axum::http::HeaderMap, axum::Json(payload): axum::Json<Value>| {
                let body = body.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer fixture-key-not-a-secret");
                    assert_eq!(payload["max_tokens"], 4096);
                    assert_eq!(payload["chat_template_kwargs"]["enable_thinking"], false);
                    (axum::http::StatusCode::from_u16(status).unwrap(), body)
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/chat/completions", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (transport::CrusoeTransport::for_test(endpoint), task)
}

#[tokio::test]
async fn real_http_adapter_checks_status_size_model_and_finish_reason() {
    for (status,body,expected) in [
        (401,"DO NOT ECHO private upstream body".into(),TransportError::HttpStatus(401)),
        (429,"rate limited".into(),TransportError::HttpStatus(429)),
        (503,"unavailable".into(),TransportError::HttpStatus(503)),
        (302,"redirect".into(),TransportError::HttpStatus(302)),
        (200,"x".repeat(65_537),TransportError::ResponseTooLarge),
        (200,"{}".into(),TransportError::MalformedResponse),
        (200,json!({"model":"unexpected-model"}).to_string(),TransportError::ModelMismatch),
        (200,json!({"model":DEFAULT_MODEL,"choices":[{"finish_reason":"length","message":{"content":"{}"}}]}).to_string(),TransportError::MalformedResponse),
    ] {
        let (transport,task)=http_transport(status,body).await;
        let result=transport.complete("system","user").await;
        assert_eq!(result.err().unwrap(),expected); task.abort();
    }
    let body=json!({"model":DEFAULT_MODEL,"choices":[{"finish_reason":"stop","message":{"content":"{\"ok\":true}"}}]}).to_string();
    let (transport, task) = http_transport(200, body).await;
    assert_eq!(
        transport.complete("system", "user").await.unwrap().text,
        "{\"ok\":true}"
    );
    task.abort();
}

#[cfg(unix)]
fn sandbox_executable(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fixture-openshell");
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\ncat > /dev/null\nprintf '%s' '{}'\n",
            body.replace('\'', "'\\''")
        ),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    (dir, path)
}

#[cfg(unix)]
#[tokio::test]
async fn sandbox_adapter_reads_bounded_sanitized_subprocess_output() {
    for (body,expected) in [
        (json!({"p2_bridge_error":"http","status":403}).to_string(),Some(TransportError::HttpStatus(403))),
        (json!({"p2_bridge_error":"timeout"}).to_string(),Some(TransportError::Timeout)),
        ("x".repeat(65_537),Some(TransportError::ResponseTooLarge)),
        ("not-json".into(),Some(TransportError::MalformedResponse)),
        (json!({"model":DEFAULT_MODEL,"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]}).to_string(),None),
    ] {
        let (_dir,path)=sandbox_executable(&body);
        let transport=OpenShellTransport::for_test(path);
        assert_eq!(transport.identity().mode,"mock");
        let result=transport.complete("system","user").await;
        if let Some(error)=expected {assert_eq!(result.err().unwrap(),error);}
        else {assert_eq!(result.unwrap().text,"{}");}
    }
}

#[test]
fn sandbox_config_accepts_only_explicit_executables_and_stable_targets() {
    assert!(OpenShellTransport::new(
        "openshell".into(),
        "gateway".into(),
        "simtra".into(),
        DEFAULT_MODEL.into()
    )
    .is_err());
    assert!(OpenShellTransport::new(
        "/bin/false".into(),
        "gateway".into(),
        "simtra; rm".into(),
        DEFAULT_MODEL.into()
    )
    .is_err());
}
