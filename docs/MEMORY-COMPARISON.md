# Memory comparison on the Silico map

The **Memory compare** tab in the city composer runs the existing Census population through the Rust prediction engine twice: memory off, then memory on. This is part of the actual satellite/resident map, not a separate simulation lab. No fixture fallback is shipped.

## Use

Start the Rust backend and serve `frontend/`. Open `index.html?backend=local&port=8086&view=city&city=sf` (adjust the port for your server). Choose **Memory compare**, enter a yes/no question, optionally enter an observed yes share, and submit. Both arms use the displayed city's current population, including resident filters. The mode is binary; use a yes/no question.

The backend needs its existing model-provider credentials and Neo4j persona-memory configuration. Add dated events through Timeline before comparing. Missing/unreachable memory, empty dated recall, model errors, partial coverage, or malformed answers fail clearly; they never produce demo probabilities. The development environment used for this change has no Neo4j service configured, so a paid live comparison has not been claimed.

Results show both weighted yes shares, change in percentage points, model-request counts, and—only when a reference is provided—absolute error and any regression. A reference is user-supplied, not verified by the app, and never enters the model prompt. Without it, a changed answer is not labeled an accuracy improvement.

Use **Show off/on on map** to switch the same resident sprites between arms. Each dot samples its own archetype probability using the same seeded draw in both arms. Rationale bubbles inherit that resident's archetype answer. These colors are illustrative; Census-weighted results are authoritative. Expand the panel for exact archetype answers, member IDs, supplied memory fragments, source event/test IDs, snapshot/population hashes, and limitations. Download the receipt for independent inspection.

## API and isolation

`POST /branches/:bid/memory-comparison`

```json
{
  "question": "Would you support this transit proposal?",
  "description": "",
  "as_of_date": "2026-10-17",
  "model": "claude-sonnet-4-6",
  "reference_yes_share": null
}
```

Use a canonical model ID supported by the existing `ModelClient`. `X-Simtra-Workspace` scopes recall just as it does for normal resident memory. The comparison reads recall once, rechecks dates, sorts records, and freezes the snapshot before either arm. Both arms share the same population, model, batching, token limits, question and date. Mutable news and Hydra retrieval are disabled in both. Neither arm writes survey memories, prediction history, or simulation state. Every archetype must answer with valid indexed probabilities; incomplete pairs never report completion.

This currently consumes the existing persona-memory service. It does not implement Branch 1's conflict resolution, correction/version semantics, or shared research agents. The snapshot's supplied fragments are exposure evidence, not proof that a model cited or correctly used each fact. A future Branch 1 adapter should replace snapshot retrieval while retaining isolation and the paired prediction path.

A single pair does not measure statistical significance, stale-claim rates, duplicated research, or general improvement on unseen tasks. Existing exact-prompt caching applies, so reruns may reuse answers. `n_llm_calls` counts requested batches, including cache hits, and is not an invoice. No per-person model calls were added.

## Validation

- Frontend tests cover resident/archetype mapping, stable map samples, reference-free reporting, regressions, and readable errors.
- Backend unit tests cover validation, future-date filtering, and deterministic snapshot hashing.
- `cargo test -p simfrancisco --test memory_comparison` exercises real PUMS residents through the API and prediction engine with loopback-only model/memory transports. It checks frozen context, workspace scope, equal population coverage, reference isolation and absence of memory writes. The transport probabilities are test fixtures, not live performance evidence.
- Existing frontend and core backend contracts remain applicable. No paid inference is needed for these tests.
