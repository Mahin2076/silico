# P2: independent experiment verifier

P2 reviews a frozen experiment and returns `pass`, `repair`, or `blocked`. It never
runs scenarios, changes a plan, or executes model-proposed actions. The live
transport uses `nvidia/NVIDIA-Nemotron-3-Super-120B-A12B` through Crusoe Managed
Inference. An optional OpenShell transport runs the fixed inference bridge in an
existing sandbox while provider credentials remain with the gateway.

This branch contains P2 only. P1's workflow, API, shared model client, `lib.rs`, and
frontend are unchanged. The example and integration test compile the module by
path so it is usable before P1 registers it in the library.

## Run it

From the repository root, with the existing Rust toolchain:

```sh
cargo test -p simfrancisco --test verifier --locked
cargo build -p simfrancisco --example p2_verify --locked

# Incomplete scenario: exit 3, targeted repair, zero model calls.
cargo run -p simfrancisco --example p2_verify -- \
  --input crates/sim-core/src/verifier/fixtures/missing-scenario.json

# Explicit offline fixture: exit 0, receipt.mode=mock. Never live completion.
cargo run -p simfrancisco --example p2_verify -- \
  --input crates/sim-core/src/verifier/fixtures/complete.json \
  --fixture-review crates/sim-core/src/verifier/fixtures/fixture-review.json

# Live: supply CRUSOE_API_KEY through your secret manager/environment first.
cargo run -p simfrancisco --example p2_verify -- \
  --input crates/sim-core/src/verifier/fixtures/complete.json --live
```

`--input -` reads bounded JSON from stdin. `--output path.json` saves the receipt.
`--hash-plan` prints the canonical plan hash without inference. It does not relabel
or reseal results. No mode selected performs local checks and blocks if a semantic
review would require an unconfigured transport. Exit codes: 0 pass, 3 repair,
4 blocked, 2 CLI/input/configuration error. Cargo may reserve its own exit codes.
A mock pass is deliberately useful for contract development, but
`permits_completion` returns false for it.

Optional managed sandbox execution:

```sh
cargo run -p simfrancisco --example p2_verify -- \
  --input crates/sim-core/src/verifier/fixtures/unsupported-conclusion.json \
  --openshell-bin /absolute/path/to/openshell --gateway nemoclaw --sandbox simtra
```

This requires an existing Ready sandbox with `/usr/bin/python3` and permission to
call `https://inference.local/v1/chat/completions`. Configure the gateway's managed
inference route to the same Nemotron model first. P2 does not install runtimes,
create sandboxes, relax policy, or read `CRUSOE_API_KEY` in this mode. The bridge
uses fixed argv and stdin JSON; model output never becomes a command. This is
contained inference execution, not a general research/tool agent.

## P1 handoff

1. Register `pub mod verifier;` in P1's `lib.rs` when integrating. Reuse one
   `Verifier` instance across requests so its concurrency budget is shared.
2. Freeze the original goal and `ExperimentPlan` before executing scenarios. Store
   the exact question, framing, options, stimuli, and audience filters in each
   scenario's nonempty `inputs` object. Supply population/evidence hashes, seed,
   model, and as-of date in `FrozenContext`. Use `plan_hash(goal, plan)`.
3. Attach each completed result to that hash and exact context/audience. Supply
   P1's evidenced deterministic findings plus nonempty result payload objects and
   conclusions. Use the full `complete.json` fixture as the wire-format example.
4. Call `verify(&request).await`. Gate completion with
   **`decision.permits_completion(&request)`**, never a bare verdict or summary.
   Keep decisions server-owned; these receipts are not cryptographic signatures
   and must not be accepted from untrusted clients as proof of verification.
5. P1 executes supported repairs, with at most two repair rounds. Initial review
   uses round 0; rounds 1 and 2 are repairs. A pass at round 2 is allowed; another
   repair becomes blocked. Stable check/scenario IDs make repairs explicit.
6. A revised plan needs a new hash and newly executed results. Do not rewrite old
   result hashes to make stale evidence look current. A result/conclusion change
   also changes `request_hash`, so a previously accepted receipt cannot be reused.

Minimal integration (after P1 registers the module):

```rust,ignore
use simfrancisco::verifier::{CrusoeTransport, Verifier, VerificationRequest};
let verifier = Verifier::new(CrusoeTransport::from_env()?);
let request: VerificationRequest = serde_json::from_slice(&immutable_payload)?;
let decision = verifier.verify(&request).await;
if decision.permits_completion(&request) {
    // P1 may mark this exact immutable request complete.
} else {
    // P1 presents the findings and applies only supported, bounded repairs.
}
```

The canonical hash sorts JSON object keys recursively and retains array order.
It binds schema version, original goal, context, assumptions, scenarios including
exact inputs, and completion checks. `request_hash` additionally binds all result
payloads, deterministic findings, conclusions, run ID, and repair round. Cross-
language callers should use Rust's hash helpers as the authority, particularly
for floating-point JSON serialization. IDs use ASCII letters, digits, `.`, `_`,
and `-`; `p2.` is reserved for built-in checks.

P1 may implement `VerifierTransport` using its shared provider client. It must
truthfully identify live/mock mode, enforce bounded network work, and return the
actual selected model identity. Built-in live transports have no fixture fallback.
No new dependencies, credentials, runtime `.env`, or shared provider edits are
included in this branch. Direct mode reads `CRUSOE_API_KEY` and optional
`SILICO_VERIFIER_MODEL`; keys are never put in prompts, receipts, or logs.

## What gets checked

Local gates cover input integrity, exactly one complete result per scenario,
plan/context consistency, audience consistency, required deterministic checks,
and evidence availability. Missing/stale results yield targeted reruns; ambiguous
or missing evidence blocks. Related local failures for the same scenario targets
share a repair. Every required check receives a finding, including `not_reviewed`
when local failure or provider unavailability prevents semantic review.

Nemotron reviews goal alignment, explicit assumptions, conclusion support, and
required custom semantic checks. Output must echo run/plan/request/round identity,
contain every required check exactly once, cite existing RFC 6901 evidence paths,
and use only `rerun_scenario`, `revise_plan`, or `request_clarification` actions.
Unknown fields, invented references, contradictory verdicts, unsupported repairs,
and malformed/truncated output block completion. Model findings cannot override a
failed deterministic gate. Prompt content is treated as untrusted evidence.

P1 owns probability validation and weighted aggregation; P2 validates the supplied
deterministic findings and their references. It does not independently recompute
PUMS weights from aggregate fixture flags. Hashes establish consistency, not the
truth of caller-supplied evidence. Semantic review remains probabilistic and is not
a proof against prompt injection or a validation of real-world predictive accuracy.

## Budgets and failure behavior

- One model call per review, no retries or fallback. At most three reviews for a
  run with initial review plus two repair rounds; P1 must enforce that lifecycle.
- Default two concurrent calls per verifier instance; configurable 1–4.
- Default 35-second deadline, including queue wait; configurable up to 60 seconds.
- Direct network timeout 30 seconds, connect timeout 10 seconds; redirects disabled.
  Sandbox remote timeout 30 seconds, bridge HTTP timeout 25 seconds.
- Request 128 KiB; provider response 64 KiB; model output 4,096 tokens, temperature
  zero, thinking disabled. Up to 32 scenarios, 64 results/checks, 128 deterministic
  findings, 64 assumptions, and 32 conclusions.
- Timeout, authentication/rate-limit/HTTP errors, wrong-model responses, oversized
  output, invalid JSON, or unavailable sandbox return a sanitized blocked receipt.
  Raw upstream/subprocess error bodies and credentials are never forwarded.

## Verification evidence

See `docs/sponsor-sprint/2026-10-17-p2/verification-report.md` for local regression,
live positive/negative checks, sandbox execution, and SHA-256 evidence provenance.
Fixtures are synthetic software tests, not live population experiments or evidence
of scientific validity. No P1 end-to-end product integration or deployment is
claimed by this P2 handoff.
