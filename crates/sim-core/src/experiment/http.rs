use super::{
    contract::*,
    hash,
    runner::{validate_url, Service},
    store::Store,
};
use crate::{
    api::CityRuntime,
    crusoe::{CrusoeClient, Usage},
};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Duration,
};
type ApiError = (StatusCode, Json<Value>);
fn err(status: StatusCode, message: impl ToString) -> ApiError {
    (status, Json(json!({"error":message.to_string()})))
}
fn internal(_: anyhow::Error) -> ApiError {
    err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Experiment storage unavailable",
    )
}
fn workspace(headers: &HeaderMap) -> Result<String, ApiError> {
    let ws = headers
        .get("x-simtra-workspace")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ws.is_empty()
        || ws.len() > 32
        || !ws
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "A valid X-Simtra-Workspace header is required",
        ));
    }
    Ok(ws.into())
}
pub fn router(cities: Arc<HashMap<String, Arc<CityRuntime>>>) -> Router {
    let init = (|| -> anyhow::Result<Service> {
        let verifier_url = std::env::var("VERIFIER_URL")
            .ok()
            .filter(|v| !v.trim().is_empty());
        if let Some(u) = &verifier_url {
            validate_url(u)?;
        }
        let p2 = if verifier_url.is_none()
            && std::env::var("CRUSOE_API_KEY").is_ok_and(|k| !k.trim().is_empty())
        {
            Some(crate::verifier::Verifier::new(super::p2::Transport(
                Box::new(crate::verifier::CrusoeTransport::from_env()?),
            )))
        } else {
            None
        };
        Ok(Service {
            p2,
            research_client: crate::verifier::YouSearchClient::from_env().ok(),
            cities,
            store: Store::open(
                &std::env::var("EXPERIMENT_DB").unwrap_or_else(|_| "runs/experiments.db".into()),
            )?,
            client: CrusoeClient::from_env()?,
            verifier_url,
            verifier_token: std::env::var("VERIFIER_TOKEN").ok(),
            slots: Arc::new(tokio::sync::Semaphore::new(2)),
        })
    })();
    match init {
        Ok(s) => routes(Arc::new(s)),
        Err(e) => {
            tracing::error!("Experiment initialization failed: {e}");
            Router::new()
                .route("/experiments", get(unavailable).post(unavailable))
                .route("/experiments/config", get(unavailable))
                .route("/experiments/:id", get(unavailable))
        }
    }
}
async fn unavailable() -> ApiError {
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "Experiment service unavailable; inspect server configuration",
    )
}
pub(super) fn routes(service: Arc<Service>) -> Router {
    Router::new()
        .route("/experiments/config", get(config))
        .route("/experiments", post(start).get(list))
        .route("/experiments/:id", get(read))
        .route("/experiments/:id/receipt", get(read))
        .route("/experiments/:id/verifier-request", get(verifier_request))
        .route("/experiments/:id/cancel", post(cancel))
        .route("/experiments/:id/verify", post(retry_verification))
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(service)
}
async fn config(State(s): State<Arc<Service>>) -> Json<Value> {
    let mut cities: Vec<_> = s.cities.keys().cloned().collect();
    cities.sort();
    Json(
        json!({"live_ready":s.client.configured() && std::env::var("MODEL_OFFLINE").as_deref()!=Ok("1"),"verifier_ready":s.p2.is_some() || s.verifier_url.is_some(),"verifier_model":std::env::var("SILICO_VERIFIER_MODEL").unwrap_or_else(|_|crate::verifier::DEFAULT_MODEL.into()),"research_ready":s.p2.is_some() && s.research_client.is_some() && std::env::var("MODEL_OFFLINE").as_deref()!=Ok("1"),"max_search_requests":1,"max_verifier_requests":3,"verifier_input_limit_bytes":131072,"model":s.client.model,"cities":cities,"max_repair_rounds":2,"max_requests":48,"deadline_seconds":300,"token_reservation_budget":180000}),
    )
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Start {
    city: String,
    goal: String,
    mode: String,
    client_request_id: String,
    #[serde(default)]
    inject_missing: bool,
}
async fn start(
    State(s): State<Arc<Service>>,
    headers: HeaderMap,
    Json(mut input): Json<Start>,
) -> Result<(StatusCode, Json<Run>), ApiError> {
    let ws = workspace(&headers)?;
    if !["live", "demo"].contains(&input.mode.as_str())
        || input.goal.trim().is_empty()
        || input.goal.len() > 2000
        || input.client_request_id.is_empty()
        || input.client_request_id.len() > 80
        || !s.cities.contains_key(&input.city)
    {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "Invalid city, goal, mode, or request ID",
        ));
    }
    if input.mode == "live" && input.inject_missing {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "Failure injection is demo-only",
        ));
    }
    if input.mode == "demo" {
        input.goal = "Compare a vegetarian takeaway lunch at $8, $10, and $12".into();
    }
    let id = hash(&(ws.as_str(), input.client_request_id.as_str()));
    let request_hash = hash(&input);
    if let Some(r) = s.store.get(&ws, &id).map_err(internal)? {
        if r.request_hash != request_hash {
            return Err(err(
                StatusCode::CONFLICT,
                "Request ID already used with different input",
            ));
        }
        return Ok((StatusCode::OK, Json(r)));
    }
    if input.mode == "live"
        && (!s.client.configured() || std::env::var("MODEL_OFFLINE").as_deref() == Ok("1"))
    {
        return Err(err(StatusCode::SERVICE_UNAVAILABLE,"Live mode requires server-side CRUSOE_API_KEY and CRUSOE_MODEL, with MODEL_OFFLINE disabled"));
    }
    let permit = s.slots.clone().try_acquire_owned().map_err(|_| {
        err(
            StatusCode::TOO_MANY_REQUESTS,
            "Two experiments are already running; try again after one finishes",
        )
    })?;
    let mut run = Run {
        run_id: id,
        workspace: ws,
        request_hash,
        mode: input.mode,
        status: "planning".into(),
        round: 0,
        goal: input.goal,
        city: input.city,
        plan: None,
        plan_hash: String::new(),
        context: Value::Null,
        archetypes: vec![],
        residents: 0,
        results: vec![],
        answers: BTreeMap::new(),
        checks: vec![],
        verifier: None,
        p2_reviews: vec![],
        research: None,
        events: vec![],
        usage: Usage::default(),
        error: None,
        inject_missing: input.inject_missing,
    };
    run.event(
        "planning",
        "Creating explicit scenarios and freezing the audience",
    );
    if !s.store.insert(&run).map_err(internal)? {
        let r = s
            .store
            .get(&run.workspace, &run.run_id)
            .map_err(internal)?
            .ok_or_else(|| err(StatusCode::CONFLICT, "Concurrent request; retry"))?;
        if r.request_hash != run.request_hash {
            return Err(err(
                StatusCode::CONFLICT,
                "Request ID already used with different input",
            ));
        }
        return Ok((StatusCode::OK, Json(r)));
    }
    let initial = run.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let result = tokio::time::timeout(Duration::from_secs(300), s.execute(&mut run)).await;
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(_) => Some("Five-minute wall-clock budget exhausted; completion refused".into()),
        };
        if let Some(e) = error {
            run.status = "blocked".into();
            run.error = Some(e.clone());
            run.event("blocked", e);
            if let Err(e) = s.save(&run) {
                tracing::info!("Experiment worker stopped: {e}");
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(initial)))
}
async fn list(State(s): State<Arc<Service>>, h: HeaderMap) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(s
        .store
        .list(&workspace(&h)?)
        .map_err(internal)?)))
}
fn load(s: &Service, h: &HeaderMap, id: &str) -> Result<Run, ApiError> {
    s.store
        .get(&workspace(h)?, id)
        .map_err(internal)?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Experiment not found"))
}
async fn read(
    State(s): State<Arc<Service>>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Run>, ApiError> {
    Ok(Json(load(&s, &h, &id)?))
}
async fn verifier_request(
    State(s): State<Arc<Service>>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<crate::verifier::VerificationRequest>, ApiError> {
    let run = load(&s, &h, &id)?;
    Ok(Json(
        super::p2::request(&run).map_err(|e| err(StatusCode::CONFLICT, e))?,
    ))
}
async fn cancel(
    State(s): State<Arc<Service>>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Run>, ApiError> {
    // Serialize cancellation with worker saves so a finished run cannot be overwritten.
    let ws = workspace(&h)?;
    Ok(Json(
        s.store
            .cancel(&ws, &id)
            .map_err(internal)?
            .ok_or_else(|| err(StatusCode::NOT_FOUND, "Experiment not found"))?,
    ))
}

async fn retry_verification(
    State(s): State<Arc<Service>>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Run>), ApiError> {
    if s.p2.is_none() || std::env::var("MODEL_OFFLINE").as_deref() == Ok("1") {
        return Err(err(
            StatusCode::SERVICE_UNAVAILABLE,
            "Live native verification is not configured",
        ));
    }
    let permit = s.slots.clone().try_acquire_owned().map_err(|_| {
        err(
            StatusCode::TOO_MANY_REQUESTS,
            "Two experiments are already running",
        )
    })?;
    let mut run = s
        .store
        .claim_verification(&workspace(&h)?, &id)
        .map_err(|e| err(StatusCode::CONFLICT, e))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "Experiment not found"))?;
    let initial = run.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let result = tokio::time::timeout(Duration::from_secs(300), s.verify_saved(&mut run)).await;
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(_) => Some("Verification retry exceeded its deadline".into()),
        };
        if let Some(e) = error {
            run.status = "blocked".into();
            run.error = Some(e.clone());
            run.event("blocked", e);
            let _ = s.save(&run);
        }
    });
    Ok((StatusCode::ACCEPTED, Json(initial)))
}
