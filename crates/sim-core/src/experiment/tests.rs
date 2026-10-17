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
        let content=if input.get("archetypes").is_none(){counter.fetch_add(1,Ordering::SeqCst);json!({"scenarios":fixture().plan.unwrap().scenarios})}else{json!({"answers":input["archetypes"].as_array().unwrap().iter().map(|g|json!({"archetype_id":g["id"],"probabilities":[0.7,0.3]})).collect::<Vec<_>>()})};
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
    let req: VerifierRequest = client
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
    assert_eq!(req.deterministic_checks.len(), 12);
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
