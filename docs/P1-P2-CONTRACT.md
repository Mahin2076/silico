# P1 → P2 integration

P1 calls the merged P2 verifier in process. No second server is needed. Keep
`VERIFIER_URL` empty and configure `CRUSOE_API_KEY`, `CRUSOE_MODEL` (P1), and optional
`SILICO_VERIFIER_MODEL` (P2, default `nvidia/NVIDIA-Nemotron-3-Super-120B-A12B`).
`MODEL_OFFLINE=1` disables live verification. Demo runs remain explicit fixtures.

## Native adapter

`crates/sim-core/src/experiment/p2.rs` maps P1's frozen plan and original receipts to
`crates/sim-core/src/verifier/types.rs` (schema 1). It preserves scenario IDs,
result-specific population/model/date/evidence identities, original result JSON,
raw archetype answers and weights, and the original P1 plan hash. Completion-check
IDs receive deterministic hashes to avoid punctuation collisions. It recomputes
P1 deterministic findings, supplies a descriptive summary of hypothetical outcome
probabilities, and never treats them as observed behavior or validated forecasts.

P2 calculates its own canonical plan hash and exact request hash. P1 records the
request and decision together in `p2_reviews`, preserving earlier repair rounds.
A pass completes only when `VerificationDecision::permits_completion` accepts the
exact current request. Mock reviews, stale results, missing evidence, provider
failures, or invalid reviews cannot complete a live experiment. Old saved receipts
load with an empty review list. Existing awaiting-verifier runs are preserved. Use **Retry verification** to review
the saved results without regenerating the plan or simulations.

Validated rerun requests execute only the named scenarios, retain the frozen plan,
and pass the reason to the next simulation attempt. Plan revisions and
clarifications stop for user input. At most two repair rounds are allowed.
P2's detailed checks/evidence remain in the receipt; P1's existing presentation
rolls their verdict into its semantic gate.

You.com research and OpenShell remain available in the P2 CLI. The map workflow
currently uses direct Crusoe verification of its frozen simulation evidence.

## Optional legacy HTTP entry point

Expose an HTTP POST endpoint and set `VERIFIER_URL` on the P1 server, for example `http://127.0.0.1:8090/verify`. P1 sends JSON `VerifierRequest` and expects JSON `VerifierResponse`, HTTP 200, within 45 seconds. HTTPS is required except on loopback; redirects are disabled. Optional `VERIFIER_TOKEN` is sent as a bearer token. Both settings are server-only. The integrated native adapter is the default when VERIFIER_URL is empty; this override retains the old schema 0.1 contract.

Canonical Rust types: `crates/sim-core/src/experiment/contract.rs`.
JSON schemas: `docs/contracts/verifier-{request,response}.schema.json`.
Examples: `docs/contracts/demo-verifier-request.json` and `demo-verifier-response.json` (explicit fixtures, not Nemotron output).

The request contains `schema_version`, `run_id`, `plan_hash`, `round`, `plan`, `results`, and `deterministic_checks`. Echo all four identity fields exactly. `plan_hash` is opaque SHA-256 assigned by P1; do not recompute it with a different JSON serializer. The plan includes its frozen audience/context hashes, date, model, original goal, scenarios with assumptions, and required checks. No mutable news or resident memory is used by this runner. These are hypothetical assumptions, not researched evidence.

Return exactly one finding per `plan.checks` entry, with its exact scenario IDs, status `pass | fail | unknown`, and nonempty evidence. Preserve the status of deterministic findings: P2 cannot override them. The additional `goal_alignment` check covers whether the scenarios answer the user's goal, assumptions are explicit, and results support only the hypothetical comparison. Missing evidence is `unknown` and prevents completion. Do not treat model estimates as observed ground truth or proven accuracy.

`verdict` is `pass | repair | blocked`. A pass requires every check to pass and an empty repair list. A repair needs a failed/unknown check and at least one repair request:

- `rerun_scenario`: reference known scenario IDs and failed/unknown check IDs; P1 reruns only these scenarios. Include a concrete reason.
- `revise_plan` or `request_clarification`: P1 stops at `needs_input` and displays the verifier summary. The user starts a new run with a revised goal. This creates new hashes and does not reuse old results. Automatic plan rewriting is intentionally not supported in this version.

P1 permits at most two repair rounds. Stale identity fields, missing/duplicate checks, malformed JSON, timeouts, unsupported actions, or contradictory verdicts block the run. The verifier is only called once all deterministic checks pass. If neither native nor HTTP verification is configured, live runs stop at `awaiting_verifier`; configure P2 and explicitly retry verification. No background replay or automatic resume is performed.

## Shared Crusoe transport

`CrusoeClient::new(base, key, model)` and `json(system, &serde_json::Value, &mut Usage)` return parsed JSON. `from_env()` uses `CRUSOE_API_KEY`, `CRUSOE_MODEL`, and optional `CRUSOE_BASE_URL`. Native P2 uses its own bounded Crusoe transport and an explicit Nemotron model ID. No provider/model fallback occurs. Requests are batched; the new experiment path does not use legacy polls or their partial-coverage thresholds.

P1 limits each run to 48 inference requests, 180,000 reserved text/output tokens, 300 seconds, and two repairs. Reservations conservatively use input UTF-8 bytes plus protocol overhead and 4,096 output tokens per request; they include failed attempts and are an admission estimate, not billing totals. Provider-reported tokens and missing-usage counts are recorded separately. The native verifier has a separate budget: at most three reviews, one model call per review, 128 KiB request input, 4,096 output tokens per call, a 35-second deadline including its queue, and two concurrent calls across the service. These calls are additional to P1's 48-request/180,000-token reservation budget; each explicitly started execution attempt remains capped at 300 seconds. Native review receipts record call counts and elapsed time; P1 token totals do not include verifier tokens. The UI request count includes both. The legacy HTTP override has a 45-second deadline per review. Server concurrency is two runs. No keys enter receipts.

## Local API

All run endpoints require `X-Simtra-Workspace` (1–32 ASCII letters/digits/underscore/hyphen). This is local namespace isolation, not authentication. Do not expose this unauthenticated prototype publicly.

- `GET /experiments/config`: loaded cities and live/verifier readiness.
- `POST /experiments`: `{city, goal, mode: "demo" | "live", client_request_id, inject_missing?: boolean}`. Returns 202 and the initial run. Reusing an ID with identical input returns the existing run; different input returns 409. Demo fixes the goal to the lunch-price fixture and does not call external services.
- `GET /experiments`: latest 30 runs in the namespace.
- `GET /experiments/:id` and `/receipt`: durable full run, plan, group weights, raw answers, results, checks, usage, event timeline, and p2_reviews (the exact native request and decision for every round).
- `GET /experiments/:id/verifier-request`: native P2 schema-v1 input, directly usable with the p2_verify CLI. This endpoint now returns schema 1 even if the legacy HTTP override is configured.
- `POST /experiments/:id/verify`: explicitly retry native verification for an awaiting-verifier run or a retryable provider/output failure. The server atomically claims the run, validates saved evidence, and retains cumulative inference and three-review budgets. Concurrent retries return 409. No initial scenario is rerun; a validated subsequent repair may target specific scenarios. A full run receipt preserves every review.
- `POST /experiments/:id/cancel`: stop further work. A currently in-flight request may finish; its result cannot mark a cancelled run complete.

SQLite stores runs at `EXPERIMENT_DB` (default `runs/experiments.db`). Reloading the browser restores the selected run. Restarting the server marks active runs `interrupted` and preserves evidence without replaying paid requests. Start a new run after interruption; eligible completed simulation results can be explicitly reverified.

## Verification commands

```sh
cargo test -p simfrancisco --lib experiment
cargo test -p simfrancisco
node --test frontend/tests/*.test.mjs
```

Tests use loopback mock inference and verifier servers, never live provider keys. They cover full planning/execution/receipt, idempotency, workspace isolation, weighted arithmetic, missing and duplicate archetypes, fixed-context checks, missing-scenario repair, absent verifier, malformed/stale/contradictory verdicts, request/token/repair budgets, cancellation, and restart behavior.
