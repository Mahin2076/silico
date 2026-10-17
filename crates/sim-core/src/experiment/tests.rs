use super::{checks::*, contract::*, hash, runner::Service, store::Store};
use crate::crusoe::{CrusoeClient, Usage};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

fn fixture() -> Run {
    let scenarios = (0..3)
        .map(|i| Scenario {
            scenario_id: format!("s{i}"),
            question: "Choose this?".into(),
            description: format!("Offer {i}"),
            options: vec!["Yes".into(), "No".into()],
            assumptions: vec!["Illustrative assumption".into()],
        })
        .collect::<Vec<_>>();
    let mut required = vec![];
    for s in &scenarios {
        for kind in ["coverage", "context", "probabilities", "arithmetic"] {
            required.push(Check {
                check_id: format!("{}:{kind}", s.scenario_id),
                kind: kind.into(),
                requirement: "Must pass".into(),
                scenario_ids: vec![s.scenario_id.clone()],
            });
        }
    }
    required.push(Check {
        check_id: "goal_alignment".into(),
        kind: "semantic".into(),
        requirement: "Goal supported".into(),
        scenario_ids: scenarios.iter().map(|s| s.scenario_id.clone()).collect(),
    });
    let plan = Plan {
        goal: "Compare offers".into(),
        city: "sf".into(),
        simulation_id: "test".into(),
        workspace: "one".into(),
        audience_hash: "audience".into(),
        context_hash: hash(&json!({})),
        as_of_date: "2026-10-17".into(),
        model: "fixture".into(),
        scenarios,
        checks: required,
        max_repair_rounds: 2,
    };
    Run {
        run_id: "test".into(),
        workspace: "one".into(),
        request_hash: "request".into(),
        mode: "demo".into(),
        status: "planning".into(),
        round: 0,
        goal: plan.goal.clone(),
        city: "sf".into(),
        plan_hash: hash(&Some(&plan)),
        plan: Some(plan),
        context: json!({}),
        archetypes: vec![
            Archetype {
                id: "a".into(),
                profile: "A".into(),
                weight: 1.0,
                member_ids: vec![1],
            },
            Archetype {
                id: "b".into(),
                profile: "B".into(),
                weight: 3.0,
                member_ids: vec![2],
            },
        ],
        residents: 2,
        results: vec![],
        answers: BTreeMap::new(),
        checks: vec![],
        verifier: None,
        p2_reviews: vec![],
        research: None,
        events: vec![],
        usage: Usage::default(),
        error: None,
        inject_missing: true,
    }
}
fn service(base: String, verifier_url: Option<String>) -> Service {
    Service {
        cities: Arc::new(HashMap::new()),
        store: Store::open(":memory:").unwrap(),
        client: CrusoeClient::new(base, "fixture-key".into(), "fixture".into()).unwrap(),
        verifier_url,
        verifier_token: None,
        p2: None,
        research_client: None,
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
    }
}
fn passing(req: &VerifierRequest) -> VerifierResponse {
    let mut findings = req.deterministic_checks.clone();
    findings.push(Finding {
        check_id: "goal_alignment".into(),
        status: "pass".into(),
        scenario_ids: req
            .plan
            .scenarios
            .iter()
            .map(|s| s.scenario_id.clone())
            .collect(),
        evidence: "Fixture semantic evidence".into(),
    });
    VerifierResponse {
        schema_version: req.schema_version.clone(),
        run_id: req.run_id.clone(),
        plan_hash: req.plan_hash.clone(),
        round: req.round,
        verdict: "pass".into(),
        checks: findings,
        repair_requests: vec![],
        summary: "Fixture only".into(),
    }
}
async fn serve(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(l, router).await.unwrap() });
    (url, task)
}
#[test]
fn weighted_arithmetic_and_invalid_coverage() {
    let r = fixture();
    let a = vec![
        GroupAnswer {
            archetype_id: "a".into(),
            probabilities: vec![0.2, 0.8],
        },
        GroupAnswer {
            archetype_id: "b".into(),
            probabilities: vec![0.8, 0.2],
        },
    ];
    assert!((aggregate(&r.archetypes, &a, 2).unwrap()[0] - 0.65).abs() < 1e-12);
    assert!(aggregate(&r.archetypes, &a[..1], 2).is_err());
    let mut bad = a.clone();
    bad[1].archetype_id = "a".into();
    assert!(aggregate(&r.archetypes, &bad, 2).is_err());
    bad[1].archetype_id = "unknown".into();
    assert!(aggregate(&r.archetypes, &bad, 2).is_err());
    for v in [
        vec![f64::NAN, 0.0],
        vec![-0.1, 1.1],
        vec![0.5, 0.4],
        vec![1.0],
    ] {
        assert!(!distribution_valid(&v, 2));
    }
}
#[test]
fn plans_require_comparable_explicit_scenarios() {
    let mut p = fixture().plan.unwrap();
    assert!(validate_scenarios(&p.scenarios).is_ok());
    p.scenarios[1].options.reverse();
    assert!(validate_scenarios(&p.scenarios).is_err());
    p.scenarios[1] = p.scenarios[0].clone();
    assert!(validate_scenarios(&p.scenarios).is_err());
}
#[tokio::test]
async fn missing_scenario_is_repaired_with_frozen_identity() {
    let s = service("http://127.0.0.1:1".into(), None);
    let mut r = fixture();
    s.store.insert(&r).unwrap();
    let original = r.plan_hash.clone();
    s.execute(&mut r).await.unwrap();
    assert_eq!(r.status, "complete");
    assert_eq!(r.round, 1);
    assert_eq!(r.usage.requests, 0);
    assert_eq!(r.results.len(), 3);
    assert_eq!(r.plan_hash, original);
    assert_eq!(
        r.results
            .iter()
            .find(|v| v.scenario_id == "s0")
            .unwrap()
            .attempt,
        1
    );
    assert_eq!(
        r.results
            .iter()
            .find(|v| v.scenario_id == "s2")
            .unwrap()
            .attempt,
        2
    );
    assert!(r.events.iter().any(|e| e.stage == "repairing"));
    assert!(r.checks.iter().all(|c| c.status == "pass"));
    r.results[0].audience_hash = "changed".into();
    assert!(checks(&r)
        .iter()
        .any(|c| c.check_id.ends_with(":context") && c.status == "fail"));
    r.results[0].distribution[0].probability += 0.01;
    assert!(checks(&r)
        .iter()
        .any(|c| c.check_id.ends_with(":arithmetic") && c.status == "fail"));
}
#[tokio::test]
async fn verifier_cannot_override_checks_or_accept_stale_results() {
    let s = service("http://127.0.0.1:1".into(), None);
    let mut r = fixture();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    let req = r.verifier_request().unwrap();
    let good = passing(&req);
    assert!(validate_verifier(&req, &good).is_ok());
    let mut v = good.clone();
    v.round += 1;
    assert!(validate_verifier(&req, &v).is_err());
    let mut v = good.clone();
    v.checks.pop();
    assert!(validate_verifier(&req, &v).is_err());
    let mut bad = req.clone();
    bad.deterministic_checks[0].status = "fail".into();
    assert!(validate_verifier(&bad, &good).is_err());
    let mut v = good.clone();
    v.verdict = "repair".into();
    v.checks.last_mut().unwrap().status = "fail".into();
    v.repair_requests.push(Repair {
        action: "execute_shell".into(),
        scenario_ids: vec!["s0".into()],
        check_ids: vec!["goal_alignment".into()],
        reason: "Unsupported".into(),
    });
    assert!(validate_verifier(&req, &v).is_err());
}
#[tokio::test]
async fn live_transport_repair_and_verifier_work_end_to_end() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let router=axum::Router::new().route("/v1/chat/completions",axum::routing::post(move|axum::Json(body):axum::Json<Value>|{let n=count.fetch_add(1,Ordering::SeqCst);async move{
        assert_eq!(body["model"],"fixture");let input:Value=serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        let answers=if n==0 {vec![]} else {input["archetypes"].as_array().unwrap().iter().map(|g|json!({"archetype_id":g["id"],"probabilities":[0.6,0.4]})).collect()};
        axum::Json(json!({"choices":[{"finish_reason":"stop","message":{"content":json!({"answers":answers}).to_string()}}],"usage":{"prompt_tokens":10,"completion_tokens":10}}))
    }})).route("/verify",axum::routing::post(|axum::Json(req):axum::Json<VerifierRequest>|async move{axum::Json(passing(&req))}));
    let (url, task) = serve(router).await;
    let s = service(format!("{url}/v1"), Some(format!("{url}/verify")));
    let mut r = fixture();
    r.mode = "live".into();
    r.inject_missing = false;
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    assert_eq!(r.status, "complete");
    assert_eq!(r.round, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(r.usage.requests, 4);
    assert_eq!(r.usage.input_tokens, 40);
    assert!(r.verifier.is_some());
    task.abort();
}
#[tokio::test]
async fn live_missing_verifier_never_completes_and_retries_are_bounded() {
    let router = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(|axum::Json(body): axum::Json<Value>| async move {
            let input: Value =
                serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            let answers: Vec<_> = input["archetypes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|g| json!({"archetype_id":g["id"],"probabilities":[0.5,0.5]}))
                .collect();
            axum::Json(
                json!({"choices":[{"message":{"content":json!({"answers":answers}).to_string()}}]}),
            )
        }),
    );
    let (url, task) = serve(router).await;
    let s = service(format!("{url}/v1"), None);
    let mut r = fixture();
    r.mode = "live".into();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    assert_eq!(r.status, "awaiting_verifier");
    assert_eq!(r.usage.responses_without_usage, 3);
    task.abort();
    let s = service("http://127.0.0.1:1/v1".into(), None);
    let mut r = fixture();
    r.mode = "live".into();
    s.store.insert(&r).unwrap();
    assert!(s
        .execute(&mut r)
        .await
        .unwrap_err()
        .to_string()
        .contains("Repair budget"));
    assert_eq!(r.round, 2);
    assert_eq!(r.usage.requests, 9);
    assert_ne!(r.status, "complete");
}
#[tokio::test]
async fn cancellation_workspace_isolation_and_restart_are_persistent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runs.db");
    let store = Store::open(path.to_str().unwrap()).unwrap();
    let r = fixture();
    assert!(store.insert(&r).unwrap());
    assert!(!store.insert(&r).unwrap());
    assert!(store.get("two", "test").unwrap().is_none());
    let cancelled = store.cancel("one", "test").unwrap().unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert!(store.save(&r).is_err());
    let mut other = r.clone();
    other.run_id = "running".into();
    store.insert(&other).unwrap();
    drop(store);
    let reopened = Store::open(path.to_str().unwrap()).unwrap();
    assert_eq!(
        reopened.get("one", "test").unwrap().unwrap().status,
        "cancelled"
    );
    assert_eq!(
        reopened.get("one", "running").unwrap().unwrap().status,
        "interrupted"
    );
}
#[tokio::test]
async fn budgets_reject_before_dispatch() {
    let client = CrusoeClient::new(
        "http://127.0.0.1:1/v1".into(),
        "fixture".into(),
        "fixture".into(),
    )
    .unwrap();
    let mut usage = Usage {
        requests: 48,
        ..Usage::default()
    };
    assert!(client
        .json("test", &json!({}), &mut usage)
        .await
        .unwrap_err()
        .to_string()
        .contains("request budget"));
    assert_eq!(usage.requests, 48);
    let mut usage = Usage {
        reserved_tokens: 179_000,
        ..Usage::default()
    };
    assert!(client
        .json("test", &json!({}), &mut usage)
        .await
        .unwrap_err()
        .to_string()
        .contains("token reservation"));
    assert_eq!(usage.requests, 0);
    assert!(CrusoeClient::new("http://example.com/v1".into(), "x".into(), "y".into()).is_err());
}

#[tokio::test]
async fn http_planning_receipt_and_idempotency_contract() {
    let plans = Arc::new(AtomicUsize::new(0));
    let counter = plans.clone();
    let mock=axum::Router::new().route("/v1/chat/completions",axum::routing::post(move|axum::Json(body):axum::Json<Value>|{let counter=counter.clone();async move{
        let input:Value=serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        let content=if input.get("archetypes").is_none(){counter.fetch_add(1,Ordering::SeqCst);json!({"question":"Choose this?","options":["Yes","No"],"scenarios":fixture().plan.unwrap().scenarios.iter().map(|s|json!({"scenario_id":s.scenario_id,"description":s.description,"assumptions":s.assumptions})).collect::<Vec<_>>()})}else{json!({"answers":input["archetypes"].as_array().unwrap().iter().map(|g|json!({"archetype_id":g["id"],"probabilities":[0.7,0.3]})).collect::<Vec<_>>()})};
        axum::Json(json!({"choices":[{"message":{"content":content.to_string()}}],"usage":{"prompt_tokens":10,"completion_tokens":10}}))
    }})).route("/verify",axum::routing::post(|axum::Json(req):axum::Json<VerifierRequest>|async move{axum::Json(passing(&req))}));
    let (model_url, model_task) = serve(mock).await;
    let mut s = service(
        format!("{model_url}/v1"),
        Some(format!("{model_url}/verify")),
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let profile = Arc::new(crate::city::CityProfile::sf());
    let rt = crate::api::CityRuntime {
        records: Arc::new(
            crate::pums::load_csv(
                root.join("data/sf_pums.csv").to_str().unwrap(),
                &profile.pumas,
            )
            .unwrap(),
        ),
        tiles: Arc::new(
            crate::geo::TilesDb::open(root.join("server_tiles/sf.db").to_str().unwrap()).unwrap(),
        ),
        profile,
    };
    s.cities = Arc::new(HashMap::from([("sf".into(), Arc::new(rt))]));
    let (url, server_task) = serve(super::http::routes(Arc::new(s))).await;
    let client = reqwest::Client::new();
    let payload =
        json!({"city":"sf","goal":"Compare offers","mode":"live","client_request_id":"same-key"});
    let response = client
        .post(format!("{url}/experiments"))
        .header("X-Simtra-Workspace", "one")
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let first: Run = response.json().await.unwrap();
    let repeat = client
        .post(format!("{url}/experiments"))
        .header("X-Simtra-Workspace", "one")
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert_eq!(repeat.status(), 200);
    assert_eq!(repeat.json::<Run>().await.unwrap().run_id, first.run_id);
    let mut changed = payload.clone();
    changed["goal"] = json!("Changed goal");
    assert_eq!(
        client
            .post(format!("{url}/experiments"))
            .header("X-Simtra-Workspace", "one")
            .json(&changed)
            .send()
            .await
            .unwrap()
            .status(),
        409
    );
    assert_eq!(
        client
            .get(format!("{url}/experiments/{}", first.run_id))
            .header("X-Simtra-Workspace", "two")
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert_eq!(
        client
            .get(format!("{url}/experiments"))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let mut last = first.clone();
    for _ in 0..100 {
        last = client
            .get(format!("{url}/experiments/{}/receipt", first.run_id))
            .header("X-Simtra-Workspace", "one")
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if last.status == "complete" || last.status == "blocked" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(last.status, "complete", "{:?}", last.error);
    assert_eq!(last.residents, 10000);
    assert_eq!(plans.load(Ordering::SeqCst), 1);
    assert_eq!(last.plan_hash, hash(&last.plan));
    assert_eq!(
        last.plan.as_ref().unwrap().context_hash,
        hash(&last.context)
    );
    let req: crate::verifier::VerificationRequest = client
        .get(format!(
            "{url}/experiments/{}/verifier-request",
            first.run_id
        ))
        .header("X-Simtra-Workspace", "one")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(req.run_id, first.run_id);
    assert_eq!(req.deterministic_findings.len(), 12);
    server_task.abort();
    model_task.abort();
}

#[tokio::test]
async fn semantic_repairs_receive_feedback_and_clarification_stops() {
    let reviews = Arc::new(AtomicUsize::new(0));
    let review_count = reviews.clone();
    let feedbacks = Arc::new(AtomicUsize::new(0));
    let feedback_count = feedbacks.clone();
    let router=axum::Router::new().route("/v1/chat/completions",axum::routing::post(move|axum::Json(body):axum::Json<Value>|{let seen=feedback_count.clone();async move {
        let input:Value=serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        if input["repair_feedback"].as_array().is_some_and(|a|!a.is_empty()) {assert_eq!(input["scenario"]["scenario_id"],"s1");seen.fetch_add(1,Ordering::SeqCst);}
        let answers:Vec<_>=input["archetypes"].as_array().unwrap().iter().map(|g|json!({"archetype_id":g["id"],"probabilities":[0.5,0.5]})).collect();axum::Json(json!({"choices":[{"message":{"content":json!({"answers":answers}).to_string()}}]}))
    }})).route("/verify",axum::routing::post(move|axum::Json(req):axum::Json<VerifierRequest>|{let count=review_count.clone();async move{
        let mut response=passing(&req);if count.fetch_add(1,Ordering::SeqCst)==0 {response.verdict="repair".into();response.checks.last_mut().unwrap().status="unknown".into();response.repair_requests=vec![Repair{action:"rerun_scenario".into(),scenario_ids:vec!["s1".into()],check_ids:vec!["goal_alignment".into()],reason:"Check the estimate against the stated offer assumptions".into()}];}axum::Json(response)
    }})).route("/clarify",axum::routing::post(|axum::Json(req):axum::Json<VerifierRequest>|async move{let mut r=passing(&req);r.verdict="repair".into();r.checks.last_mut().unwrap().status="unknown".into();r.repair_requests=vec![Repair{action:"request_clarification".into(),scenario_ids:vec![],check_ids:vec!["goal_alignment".into()],reason:"Specify the audience".into()}];r.summary="Please specify the audience".into();axum::Json(r)}));
    let (url, task) = serve(router).await;
    let s = service(format!("{url}/v1"), Some(format!("{url}/verify")));
    let mut r = fixture();
    r.mode = "live".into();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    assert_eq!(r.status, "complete");
    assert_eq!(r.round, 1);
    assert_eq!(feedbacks.load(Ordering::SeqCst), 1);
    assert_eq!(reviews.load(Ordering::SeqCst), 2);
    let s = service(format!("{url}/v1"), Some(format!("{url}/clarify")));
    let mut r = fixture();
    r.mode = "live".into();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    assert_eq!(r.status, "needs_input");
    assert_eq!(r.round, 0);
    assert_eq!(r.error.as_deref(), Some("Please specify the audience"));
    task.abort();
}

struct NativeFixture {
    repair: bool,
}
impl crate::verifier::VerifierTransport for NativeFixture {
    fn identity(&self) -> crate::verifier::TransportIdentity {
        crate::verifier::TransportIdentity {
            mode: "mock".into(),
            provider: "adapter-test".into(),
            model: crate::verifier::DEFAULT_MODEL.into(),
        }
    }
    fn complete<'a>(&'a self, _: &'a str, user: &'a str) -> crate::verifier::CompletionFuture<'a> {
        Box::pin(async move {
            let e: Value = serde_json::from_str(user).unwrap();
            let repair = self.repair && e["repair_round"] == 0;
            let findings:Vec<_> = e["required_checks"].as_array().unwrap().iter().map(|c| json!({"check_id":c["id"],"status":if repair && c["id"] == "p2.conclusion_support" {"repair"} else {"pass"},"summary":"Software fixture only","evidence":[{"pointer":"/results/0/payload"}]})).collect();
            let repairs = if repair {
                json!([{"id":"repair-1","action":"rerun_scenario","check_ids":["p2.conclusion_support"],"scenario_ids":["s1"],"reason":"Recheck the estimate using the frozen assumptions"}])
            } else {
                json!([])
            };
            Ok(crate::verifier::Completion {model:crate::verifier::DEFAULT_MODEL.into(),text:json!({"schema_version":1,"run_id":e["run_id"],"plan_hash":e["plan_hash"],"request_hash":e["request_hash"],"repair_round":e["repair_round"],"verdict":if repair {"repair"} else {"pass"},"summary":"Mock adapter review","findings":findings,"repair_requests":repairs}).to_string()})
        })
    }
}
fn native_fixture() -> Run {
    let mut r = fixture();
    r.context = json!({"seed":42});
    let p = r.plan.as_mut().unwrap();
    p.context_hash = hash(&r.context);
    p.audience_hash = hash(&"fixture population");
    r.plan_hash = hash(&r.plan);
    r
}

#[tokio::test]
async fn native_bridge_preserves_evidence_and_enforces_live_completion() {
    let s = service("http://localhost:1/v1".into(), None);
    let mut r = native_fixture();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    let request = super::p2::request(&r).unwrap();
    assert_eq!(request.deterministic_findings.len(), 12);
    assert_eq!(
        request.results[0].payload["receipt"],
        serde_json::to_value(&r.results[0]).unwrap()
    );
    assert_eq!(
        request.results[0].payload["answers"],
        serde_json::to_value(&r.answers[&r.results[0].scenario_id]).unwrap()
    );
    let decision = crate::verifier::Verifier::new(NativeFixture { repair: false })
        .verify(&request)
        .await;
    assert_eq!(
        decision.verdict,
        crate::verifier::Verdict::Pass,
        "{}",
        decision.summary
    );
    let mut review = super::p2::Review { request, decision };
    assert_eq!(super::p2::response(&r, &review).unwrap().verdict, "blocked");
    // Unit-test the successful gate with explicit simulated provenance; no live call.
    review.decision.receipt.mode = "live".into();
    let response = super::p2::response(&r, &review).unwrap();
    assert_eq!(response.verdict, "pass");
    validate_verifier(&r.verifier_request().unwrap(), &response).unwrap();
    review.decision.request_hash = "f".repeat(64);
    assert!(super::p2::response(&r, &review).is_err());
}

#[tokio::test]
async fn native_bridge_recomputes_checks_and_rejects_stale_results() {
    let s = service("http://localhost:1/v1".into(), None);
    let mut r = native_fixture();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    r.results[0].model = "different-model".into();
    let request = super::p2::request(&r).unwrap();
    assert_eq!(request.results[0].context.model, "different-model");
    assert!(request.deterministic_findings.iter().any(|f| !f.passed));
    let decision = crate::verifier::Verifier::new(NativeFixture { repair: false })
        .verify(&request)
        .await;
    assert_ne!(decision.verdict, crate::verifier::Verdict::Pass);
    assert_eq!(decision.receipt.model_calls, 0);
    r.context["seed"] = json!(43);
    assert!(super::p2::request(&r).is_err());
}

#[tokio::test]
async fn native_runner_targets_repair_and_persists_every_review() {
    let feedbacks = Arc::new(AtomicUsize::new(0));
    let seen = feedbacks.clone();
    let router = axum::Router::new().route("/v1/chat/completions", axum::routing::post(move |axum::Json(body):axum::Json<Value>| {
        let seen = seen.clone();
        async move {
            let input:Value = serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
            if input["repair_feedback"].as_array().is_some_and(|a| !a.is_empty()) {
                assert_eq!(input["scenario"]["scenario_id"], "s1");
                seen.fetch_add(1, Ordering::SeqCst);
            }
            let answers:Vec<_> = input["archetypes"].as_array().unwrap().iter().map(|g| json!({"archetype_id":g["id"],"probabilities":[0.5,0.5]})).collect();
            axum::Json(json!({"choices":[{"message":{"content":json!({"answers":answers}).to_string()}}]}))
        }
    }));
    let (url, task) = serve(router).await;
    let mut s = service(format!("{url}/v1"), None);
    s.p2 = Some(crate::verifier::Verifier::new(super::p2::Transport(
        Box::new(NativeFixture { repair: true }),
    )));
    let mut r = native_fixture();
    r.mode = "live".into();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    assert_eq!(r.round, 1);
    assert_eq!(r.usage.requests, 4);
    assert_eq!(feedbacks.load(Ordering::SeqCst), 1);
    assert_eq!(r.p2_reviews.len(), 2);
    assert_eq!(
        r.p2_reviews[0].request.plan_hash,
        r.p2_reviews[1].request.plan_hash
    );
    assert_eq!(
        r.status, "needs_input",
        "mock pass must not complete live run"
    );
    assert_eq!(
        s.store
            .get(&r.workspace, &r.run_id)
            .unwrap()
            .unwrap()
            .p2_reviews
            .len(),
        2
    );
    task.abort();
}

#[test]
fn old_receipts_load_without_native_reviews() {
    let mut value = serde_json::to_value(fixture()).unwrap();
    value.as_object_mut().unwrap().remove("p2_reviews");
    assert!(serde_json::from_value::<Run>(value)
        .unwrap()
        .p2_reviews
        .is_empty());
}

#[test]
fn planner_uses_one_shared_question_and_rejects_per_scenario_overrides() {
    let mut draft = json!({"question":"Would you buy this lunch?","options":["Yes","No"],"scenarios":[{"scenario_id":"price_8","description":"Lunch at $8","assumptions":["Same meal"]},{"scenario_id":"price_10","description":"Lunch at $10","assumptions":["Same meal"]}]});
    let scenarios = planned_scenarios(draft.clone()).unwrap();
    assert_eq!(scenarios[0].question, scenarios[1].question);
    assert_eq!(scenarios[0].options, scenarios[1].options);
    draft["scenarios"][0]["assumptions"] = json!("Same meal");
    assert_eq!(
        planned_scenarios(draft.clone()).unwrap()[0].assumptions,
        vec!["Same meal"]
    );
    draft["scenarios"][0]["question"] = json!("Would you buy at a different price?");
    assert!(planned_scenarios(draft).is_err());
}

#[tokio::test]
async fn verification_retry_is_claimed_once_and_reuses_saved_results() {
    let mut s = service("http://127.0.0.1:1/v1".into(), None);
    let mut r = native_fixture();
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    r.mode = "live".into();
    r.status = "awaiting_verifier".into();
    s.store.save(&r).unwrap();
    let original = serde_json::to_value(&r.results).unwrap();
    let claimed = s
        .store
        .claim_verification(&r.workspace, &r.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.status, "verifying");
    assert!(s.store.claim_verification(&r.workspace, &r.run_id).is_err());
    assert!(s
        .store
        .claim_verification("other-workspace", &r.run_id)
        .unwrap()
        .is_none());
    s.store.save(&r).unwrap();
    s.p2 = Some(crate::verifier::Verifier::new(super::p2::Transport(
        Box::new(NativeFixture { repair: false }),
    )));
    let (url, task) = serve(super::http::routes(Arc::new(s))).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{url}/experiments/{}/verify", r.run_id))
        .header("X-Simtra-Workspace", &r.workspace)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 202);
    let mut last = r.clone();
    for _ in 0..50 {
        last = client
            .get(format!("{url}/experiments/{}", r.run_id))
            .header("X-Simtra-Workspace", &r.workspace)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if last.status == "needs_input" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(last.status, "needs_input"); // mock review cannot complete a live run
    assert_eq!(last.usage.requests, 0, "retry must not rerun simulation");
    assert_eq!(last.p2_reviews.len(), 1);
    assert_eq!(serde_json::to_value(&last.results).unwrap(), original);
    let persisted_review = &last.p2_reviews[0];
    assert_eq!(
        crate::verifier::request_hash(&persisted_review.request),
        persisted_review.decision.request_hash
    );
    last.status = "blocked".into();
    last.p2_reviews[0].decision.receipt.error_code = Some("invalid_review_evidence".into());
    assert!(super::p2::can_retry(&last));
    last.p2_reviews = vec![last.p2_reviews[0].clone(); 3];
    assert!(!super::p2::can_retry(&last));
    task.abort();
}

// Local transport fixtures only: no paid search/model calls in these tests.
struct ResearchFixture {
    repeat: bool,
}
impl crate::verifier::VerifierTransport for ResearchFixture {
    fn identity(&self) -> crate::verifier::TransportIdentity {
        crate::verifier::TransportIdentity {
            mode: "mock".into(),
            provider: "research-fixture".into(),
            model: crate::verifier::DEFAULT_MODEL.into(),
        }
    }
    fn complete<'a>(&'a self, _: &'a str, user: &'a str) -> crate::verifier::CompletionFuture<'a> {
        Box::pin(async move {
            let e: Value = serde_json::from_str(user).unwrap();
            let research = e["request"]["web_research"].is_object();
            let repair = !research || self.repeat;
            let findings:Vec<_>=e["required_checks"].as_array().unwrap().iter().map(|c|json!({
                "check_id":c["id"], "status":if repair && c["id"]=="p2.explicit_assumptions" {"repair"} else {"pass"},
                "summary":"Fixture assessment", "evidence":[{"pointer":if c["id"]=="p2.web_grounding" {"/web_research/sources/0"} else {"/plan/assumptions/0"}}]
            })).collect();
            let repairs = if repair {
                json!([{"id":"research-1","action":"research_evidence","check_ids":["p2.explicit_assumptions"],"scenario_ids":[],"reason":"What evidence supports the stated offer assumptions in San Francisco?"}])
            } else {
                json!([])
            };
            Ok(crate::verifier::Completion{model:crate::verifier::DEFAULT_MODEL.into(),text:json!({
                "schema_version":1,"run_id":e["run_id"],"plan_hash":e["plan_hash"],"request_hash":e["request_hash"],"repair_round":e["repair_round"],"verdict":if repair {"repair"} else {"pass"},"summary":if repair {"External support for the stated assumptions is missing"} else {"Sources address the original gap; simulation remains hypothetical"},"findings":findings,"repair_requests":repairs
            }).to_string()})
        })
    }
}
async fn research_case(kind: &str, repeat: bool) -> (Service, Run, usize) {
    let search_count = Arc::new(AtomicUsize::new(0));
    let count = search_count.clone();
    let empty = kind == "empty";
    let bad_citation = kind == "invalid_assessment";
    let retry_once = kind == "retry_assessment";
    let assessment_calls = Arc::new(AtomicUsize::new(0));
    let (url,task)=serve(axum::Router::new().route("/search",axum::routing::post(move |axum::Json(body):axum::Json<Value>| {
        count.fetch_add(1,Ordering::SeqCst);
        assert!(body["query"].as_str().unwrap().contains("offer assumptions"));
        async move {axum::Json(json!({"results":{"web":if empty {json!([])} else {json!([{"title":"Offer evidence","url":"https://example.org/source","description":"Published evidence about the stated assumptions","page_age":"2026-10-16"}])}}}))}
    })).route("/v1/chat/completions", axum::routing::post(move |axum::Json(body):axum::Json<Value>| {
        let attempt = assessment_calls.fetch_add(1,Ordering::SeqCst);
        async move {
        if retry_once && attempt == 0 { return (axum::http::StatusCode::BAD_GATEWAY, axum::Json(json!({"error":"temporary fixture failure"}))); }
        let input:Value=serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert!(input["objection"].as_str().unwrap().contains("missing"));
        assert_eq!(input["sources"][0]["id"],"web-1");
        let id=if bad_citation {"web-99"} else {"web-1"};
        (axum::http::StatusCode::OK, axum::Json(json!({"choices":[{"message":{"content":json!({"assessment":format!("Evidence addresses the assumptions [{id}], without validating simulated demand."),"source_ids":[id],"limitations":["Software fixture only; no observed purchases."]}).to_string()}}]})))
    }}))).await;
    let mut s = service(format!("{url}/v1"), None);
    let mut r = native_fixture();
    r.inject_missing = false;
    s.store.insert(&r).unwrap();
    s.execute(&mut r).await.unwrap();
    r.mode = "live".into();
    r.status = "awaiting_verifier".into();
    r.verifier = None;
    s.save(&r).unwrap();
    s.p2 = Some(crate::verifier::Verifier::new(super::p2::Transport(
        Box::new(ResearchFixture { repeat }),
    )));
    if kind != "missing" {
        s.research_client = Some(crate::verifier::YouSearchClient::for_test(format!(
            "{url}/search"
        )));
    }
    s.verify_saved(&mut r).await.unwrap();
    if retry_once {
        assert_eq!(r.status, "blocked");
        assert_eq!(r.research.as_ref().unwrap().assessment_attempts, 1);
        assert!(
            super::p2::request(&r).is_err(),
            "Missing assessment cannot be reviewed as complete"
        );
        let results = hash(&r.results);
        let sources = hash(&r.research.as_ref().unwrap().web);
        let mut stale = r.clone();
        stale.research.as_mut().unwrap().query.query = "Changed question".into();
        assert!(!super::p2::can_retry_assessment(&stale));
        r = s
            .store
            .claim_verification(&r.workspace, &r.run_id)
            .unwrap()
            .unwrap();
        assert!(s.store.claim_verification(&r.workspace, &r.run_id).is_err());
        s.verify_saved(&mut r).await.unwrap();
        assert_eq!(hash(&r.results), results);
        assert_eq!(hash(&r.research.as_ref().unwrap().web), sources);
        assert_eq!(r.research.as_ref().unwrap().assessment_attempts, 2);
        assert!(!super::p2::can_retry_assessment(&r));
    }
    task.abort();
    (s, r, search_count.load(Ordering::SeqCst))
}
#[tokio::test]
async fn research_followup_preserves_simulation_and_binds_sources_to_recheck() {
    let (s, r, calls) = research_case("ready", false).await;
    assert_eq!(calls, 1);
    assert_eq!(r.round, 1);
    assert_eq!(r.p2_reviews.len(), 2);
    let before = &r.p2_reviews[0];
    let after = &r.p2_reviews[1];
    assert_eq!(before.request.plan_hash, after.request.plan_hash);
    assert_eq!(
        serde_json::to_value(&before.request.results).unwrap(),
        serde_json::to_value(&after.request.results).unwrap()
    );
    assert_ne!(before.decision.request_hash, after.decision.request_hash);
    assert!(after.request.web_research.is_some());
    assert!(after
        .request
        .conclusions
        .iter()
        .any(|c| c.contains("[web-1]")));
    assert_eq!(
        r.usage.requests, 1,
        "Only the synthesis call runs after the saved simulation"
    );
    assert_eq!(
        after.decision.verdict,
        crate::verifier::Verdict::Pass,
        "{}",
        after.decision.summary
    );
    assert_ne!(
        r.status, "complete",
        "mock evidence cannot complete a live run"
    );
    assert_eq!(
        r.research.as_ref().unwrap().trigger_request_hash,
        before.decision.request_hash
    );
    assert!(s
        .store
        .get(&r.workspace, &r.run_id)
        .unwrap()
        .unwrap()
        .research
        .is_some());
    // Exercise the production completion binding with explicit synthetic live labels.
    let mut live = r.clone();
    live.research.as_mut().unwrap().web.as_mut().unwrap().mode = "live".into();
    let mut review = after.clone();
    review.request = super::p2::request(&live).unwrap();
    review.decision.request_hash = crate::verifier::request_hash(&review.request);
    review.decision.receipt.mode = "live".into();
    assert_eq!(super::p2::response(&live, &review).unwrap().verdict, "pass");
    live.research
        .as_mut()
        .unwrap()
        .web
        .as_mut()
        .unwrap()
        .sources[0]
        .snippets[0] = "Changed evidence".into();
    assert!(super::p2::response(&live, &review).is_err());
    live.research.as_mut().unwrap().query.query = "Unrelated query".into();
    assert!(super::p2::request(&live).is_err());
}
#[tokio::test]
async fn research_failures_and_loops_cannot_pass_or_repeat_search() {
    for kind in ["empty", "missing", "invalid_assessment"] {
        let (_, r, calls) = research_case(kind, false).await;
        assert_eq!(r.status, "blocked");
        assert!(r.research.unwrap().error.is_some());
        assert_eq!(r.p2_reviews.len(), 1);
        assert_eq!(calls, if kind == "missing" { 0 } else { 1 });
    }
    let (_, r, calls) = research_case("ready", true).await;
    assert_eq!(r.status, "blocked");
    assert_eq!(calls, 1);
    assert_eq!(r.p2_reviews.len(), 2);
    assert!(r.error.unwrap().contains("One-search budget exhausted"));
}

#[test]
fn simulation_prompt_omits_other_scenarios_without_changing_frozen_evidence() {
    let original = json!({"seed":42,"as_of_date":"2026-10-17","news":[],"evidence_policy":"Explicit assumptions","scenarios":[{"price":8},{"price":12}]});
    let before = hash(&original);
    let context = super::runner::simulation_context(&original);
    assert!(context.get("scenarios").is_none());
    for key in ["seed", "as_of_date", "news", "evidence_policy"] {
        assert_eq!(context[key], original[key]);
    }
    assert_eq!(hash(&original), before);
}
#[tokio::test]
async fn exhausted_admission_budget_stops_without_repair_loops_or_dispatch() {
    for tokens in [true, false] {
        let s = service("http://127.0.0.1:1/v1".into(), None);
        let mut r = fixture();
        r.mode = "live".into();
        if tokens {
            r.usage.reserved_tokens = 180_000;
        } else {
            r.usage.requests = 48;
        }
        s.store.insert(&r).unwrap();
        let error = s.execute(&mut r).await.unwrap_err();
        assert!(error
            .downcast_ref::<crate::crusoe::BudgetExhausted>()
            .is_some());
        assert_eq!(r.round, 0);
        assert!(!r.events.iter().any(|e| e.stage == "repairing"));
        assert_eq!(r.usage.requests, if tokens { 0 } else { 48 });
    }
}

#[tokio::test]
async fn assessment_retry_reuses_sources_and_results_without_repeating_search() {
    let (_, r, searches) = research_case("retry_assessment", false).await;
    assert_eq!(searches, 1);
    assert_eq!(r.usage.requests, 2);
    assert_eq!(r.round, 1);
    assert_eq!(r.p2_reviews.len(), 2);
    assert!(r.research.as_ref().unwrap().assessment.is_some());
    assert_ne!(
        r.status, "complete",
        "Mock review must still not complete a live run"
    );
}
