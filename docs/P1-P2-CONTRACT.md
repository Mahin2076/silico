# P1 → P2 integration, schema 0.1

P1 is implemented on `p1`. P2 owns `crates/sim-core/src/verifier/`; P1 has not created or modified it. P1's shared transport lives in `crates/sim-core/src/crusoe.rs` (the legacy model router is unchanged).

## P2 entry point

Expose an HTTP POST endpoint and set `VERIFIER_URL` on the P1 server, for example `http://127.0.0.1:8090/verify`. P1 sends JSON `VerifierRequest` and expects JSON `VerifierResponse`, HTTP 200, within 45 seconds. HTTPS is required except on loopback; redirects are disabled. Optional `VERIFIER_TOKEN` is sent as a bearer token. Both settings are server-only. A direct Rust adapter can replace HTTP after the branches merge without changing these types.

Canonical Rust types: `crates/sim-core/src/experiment/contract.rs`.
JSON schemas: `docs/contracts/verifier-{request,response}.schema.json`.
Examples: `docs/contracts/demo-verifier-request.json` and `demo-verifier-response.json` (explicit fixtures, not Nemotron output).

The request contains `schema_version`, `run_id`, `plan_hash`, `round`, `plan`, `results`, and `deterministic_checks`. Echo all four identity fields exactly. `plan_hash` is opaque SHA-256 assigned by P1; do not recompute it with a different JSON serializer. The plan includes its frozen audience/context hashes, date, model, original goal, scenarios with assumptions, and required checks. No mutable news or resident memory is used by this runner. These are hypothetical assumptions, not researched evidence.

Return exactly one finding per `plan.checks` entry, with its exact scenario IDs, status `pass | fail | unknown`, and nonempty evidence. Preserve the status of deterministic findings: P2 cannot override them. The additional `goal_alignment` check covers whether the scenarios answer the user's goal, assumptions are explicit, and results support only the hypothetical comparison. Missing evidence is `unknown` and prevents completion. Do not treat model estimates as observed ground truth or proven accuracy.

`verdict` is `pass | repair | blocked`. A pass requires every check to pass and an empty repair list. A repair needs a failed/unknown check and at least one repair request:

- `rerun_scenario`: reference known scenario IDs and failed/unknown check IDs; P1 reruns only these scenarios. Include a concrete reason.
- `revise_plan` or `request_clarification`: P1 stops at `needs_input` and displays the verifier summary. The user starts a new run with a revised goal. This creates new hashes and does not reuse old results. Automatic plan rewriting is intentionally not supported in this version.

P1 permits at most two repair rounds. Stale identity fields, missing/duplicate checks, malformed JSON, timeouts, unsupported actions, or contradictory verdicts block the run. The verifier is only called once all deterministic checks pass. With no verifier configured, live runs stop at `awaiting_verifier`; configure P2 and start a new run. No background replay or automatic resume is performed.

## Shared Crusoe transport

`CrusoeClient::new(base, key, model)` and `json(system, &serde_json::Value, &mut Usage)` return parsed JSON. `from_env()` uses `CRUSOE_API_KEY`, `CRUSOE_MODEL`, and optional `CRUSOE_BASE_URL`. P2 should use an explicit Nemotron model ID from the account's model catalog, constructing its own client when different from P1's model. No provider/model fallback occurs. Requests are batched; the new experiment path does not use legacy polls or their partial-coverage thresholds.

P1 limits each run to 48 inference requests, 180,000 reserved text/output tokens, 300 seconds, and two repairs. Reservations conservatively use input UTF-8 bytes plus protocol overhead and 4,096 output tokens per request; they include failed attempts and are an admission estimate, not billing totals. Provider-reported tokens and missing-usage counts are recorded separately. Each verifier call is capped at 45 seconds, at most three calls per run; P2 owns any internal Nemotron budget and should report it in its own logs. Server concurrency is two runs. No keys enter receipts.

## Local API

All run endpoints require `X-Simtra-Workspace` (1–32 ASCII letters/digits/underscore/hyphen). This is local namespace isolation, not authentication. Do not expose this unauthenticated prototype publicly.

- `GET /experiments/config`: loaded cities and live/verifier readiness.
- `POST /experiments`: `{city, goal, mode: "demo" | "live", client_request_id, inject_missing?: boolean}`. Returns 202 and the initial run. Reusing an ID with identical input returns the existing run; different input returns 409. Demo fixes the goal to the lunch-price fixture and does not call external services.
- `GET /experiments`: latest 30 runs in the namespace.
- `GET /experiments/:id` and `/receipt`: durable full run, plan, group weights, raw answers, results, checks, usage, and event timeline.
- `GET /experiments/:id/verifier-request`: current contract input for P2 testing.
- `POST /experiments/:id/cancel`: stop further work. A currently in-flight request may finish; its result cannot mark a cancelled run complete.

SQLite stores runs at `EXPERIMENT_DB` (default `runs/experiments.db`). Reloading the browser restores the selected run. Restarting the server marks active runs `interrupted` and preserves evidence without replaying paid requests. Start a new run to try again.

## Verification commands

```sh
cargo test -p simfrancisco --lib experiment
cargo test -p simfrancisco
node --test frontend/tests/*.test.mjs
```

Tests use loopback mock inference and verifier servers, never live provider keys. They cover full planning/execution/receipt, idempotency, workspace isolation, weighted arithmetic, missing and duplicate archetypes, fixed-context checks, missing-scenario repair, absent verifier, malformed/stale/contradictory verdicts, request/token/repair budgets, cancellation, and restart behavior.
