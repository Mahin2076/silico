//! Real PUMS + API + prediction engine, with loopback-only model and memory transports.
use axum::{routing::post, Json, Router};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[tokio::test]
async fn paired_api_freezes_memory_preserves_population_and_never_writes_results() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::env::set_current_dir(root).unwrap();
    for key in [
        "NEO4J_URI",
        "HYDRA_DB_KEY",
        "HYDRA_DB_API_KEY",
        "INSFORGE_API_KEY",
        "INSFORGE_ADMIN_KEY",
        "ROCKETRIDE_URL",
        "ROCKETRIDE_WEBHOOK_URL",
        "CRUSOE_API_KEY",
        "SF_PUMS_PATH",
    ] {
        std::env::remove_var(key);
    }
    let models = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(AtomicUsize::new(0));
    let m = models.clone();
    let r = reads.clone();
    let fixture=Router::new().route("/openai/v1/responses",post(move |Json(body):Json<Value>|{let m=m.clone();async move{
        m.fetch_add(1,Ordering::SeqCst);
        let text=body["input"].as_str().unwrap();
        assert!(!text.contains("reference_yes_share"));
        assert!(!text.contains("future-only-record"));
        let profiles=text.split("Resident profiles:\n").nth(1).unwrap().split("\nReturn ONLY").next().unwrap();
        let count=profiles.lines().filter(|line|line.chars().next().is_some_and(|c|c.is_ascii_digit())).count();
        let p=if text.contains("Memory:"){0.8}else{0.2};
        Json(json!({"output_text":serde_json::to_string(&(1..=count).map(|i|json!({"i":i,"p_yes":p,"why":"Transport fixture only"})).collect::<Vec<_>>()).unwrap()}))
    }})).route("/db/neo4j/query/v2",post(move |Json(body):Json<Value>|{let r=r.clone();async move{
        r.fetch_add(1,Ordering::SeqCst);
        let statement=body["statement"].as_str().unwrap();
        assert!(!statement.contains("MERGE") && !statement.contains("CREATE") && !statement.contains("SET "));
        assert_eq!(body["parameters"]["ws"],"comparison-test");
        let rows=if statement.contains("HAPPENED_IN"){
            body["parameters"]["keys"].as_array().unwrap().iter().flat_map(|key|{
                let id=key.as_str().unwrap().rsplit(':').next().unwrap().parse::<u32>().unwrap();
                [json!([id,"event-current","news","loopback-evidence","2026-01-01"]),json!([id,"future","news","future-only-record","2027-01-01"])]
            }).collect::<Vec<_>>()
        }else{vec![]};
        Json(json!({"data":{"values":rows}}))
    }}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let transport = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, fixture).await.unwrap();
    });
    std::env::set_var("MODEL_OFFLINE", "0");
    std::env::set_var("MODEL_API_KEY", "loopback-test");
    std::env::set_var("OPENAI_API_URL", format!("{transport}/openai/v1"));
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("EXPERIMENT_DB", dir.path().join("experiments.db"));
    let mut state = simfrancisco::api::build_state(
        "server_tiles/sf.db",
        None,
        dir.path().join("state.db").to_str().unwrap(),
    )
    .unwrap();
    std::env::set_var("NEO4J_URI", &transport);
    std::env::set_var("NEO4J_DATABASE", "neo4j");
    std::env::set_var("NEO4J_HTTP_API", "query");
    state.engine.memory = simfrancisco::memory::MemoryClient::from_env();
    state.engine.max_clusters = 12;
    state.hydra = None;
    state.engine.hydra = None;
    state.insforge = None;
    state.rocketride = None;
    let app = simfrancisco::api::router(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = reqwest::Client::new();
    let sim:Value=client.post(format!("{base}/simulations")).json(&json!({"city":"sf","n":150,"seed":42,"start_datetime":"2026-10-17T08:00:00Z","tick_seconds":30})).send().await.unwrap().json().await.unwrap();
    let branch = sim["main_branch"].as_str().unwrap();
    let request = json!({"question":"Would you support this policy?","as_of_date":"2026-10-17","model":"gpt-4o","reference_yes_share":0.9});
    let response = client
        .post(format!("{base}/branches/{branch}/memory-comparison"))
        .header("X-Simtra-Workspace", "comparison-test")
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "{}", response.text().await.unwrap());
    // Repeat once against the same frozen population to inspect the receipt.
    let receipt: Value = client
        .post(format!("{base}/branches/{branch}/memory-comparison"))
        .header("X-Simtra-Workspace", "comparison-test")
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!((receipt["off"]["result"]["p_yes"].as_f64().unwrap() - 0.2).abs() < 1e-9);
    assert!((receipt["on"]["result"]["p_yes"].as_f64().unwrap() - 0.8).abs() < 1e-9);
    assert_eq!(
        receipt["off"]["result"]["n_agents"],
        receipt["on"]["result"]["n_agents"]
    );
    assert_eq!(
        receipt["off"]["result"]["n_archetypes"],
        receipt["on"]["result"]["n_archetypes"]
    );
    assert!(receipt["error_reduction"].as_f64().unwrap() > 0.59);
    assert!(receipt["off"]["result"]["memory_test_id"].is_null());
    assert!(receipt["on"]["result"]["memory_test_id"].is_null());
    assert_eq!(reads.load(Ordering::SeqCst), 4); // exactly two recall queries per paired comparison
    assert!(models.load(Ordering::SeqCst) >= 4);
    for answer in receipt["off"]["archetype_answers"].as_array().unwrap() {
        assert_eq!(answer["memory_context"], "");
    }
    for answer in receipt["on"]["archetype_answers"].as_array().unwrap() {
        assert!(answer["memory_context"]
            .as_str()
            .unwrap()
            .contains("loopback-evidence"));
    }
    let bad=client.post(format!("{base}/branches/{branch}/memory-comparison")).json(&json!({"question":"x","as_of_date":"2026-10-17","model":"invented","reference_yes_share":2})).send().await.unwrap();
    assert_eq!(bad.status(), 400);
}
