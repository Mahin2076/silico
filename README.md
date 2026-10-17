# simtra

**simulate and predict a city before the real world reacts**

## The idea

What if you could see how people might respond to a product, message, policy, or event before putting it into the real world?

Simtra creates synthetic versions of real cities that you can ask questions. Each city is filled with simulated residents sampled from real US Census data, so the population reflects the city's actual mix of ages, incomes, jobs, education levels, households, and backgrounds.

Instead of getting one generic AI answer, Simtra asks different types of residents and combines their responses using Census population weights. The result is an early signal of how the whole city — and different groups inside it — might react.

Simtra currently covers San Francisco, New York City, Los Angeles, Chicago, and Miami.

## What you can test

- Ask whether residents would support an idea, candidate, policy, or product.
- Compare two versions of a message with an A/B test.
- See how planned marketing copy could shift support.
- Break results down by age, income, education, race, gender, and other groups.
- Read sample reasons from individual synthetic residents.

The pixel-art city makes the result visible: residents across the map react as the prediction is revealed.

## How it works

Every synthetic resident gets a seeded persona, routine, values, location, and demographic profile. When a question is asked, similar residents are grouped together, their likely responses are modeled, and the answers are combined using real Census survey weights.

This is meant to give people a fast signal before launch. It is not a replacement for real surveys, experiments, or talking to actual people.

## Proof of concept

For historical San Francisco tests, Simtra used a model cutoff from before the events so it could not simply recall the outcomes:

- **2024 presidential vote:** 83.8% actual Democratic share, 81.3% predicted.
- **March 2024 Proposition A:** 70.38% actual yes, 70% predicted.

## The goal

Most decisions depend on guessing how people will react. Simtra makes that guess easier to explore before time, money, or trust is spent in the real world.

## Hackathon work split

The next iteration focuses on **Done Means Done**: a one-line goal becomes explicit completion checks, an experiment, independent review, targeted follow-up work, and evidence that each failure was resolved. **Best Use of Nemotron** is the partner-prize target. OpenShell/NemoClaw and web research are secondary to completing this loop.

The basic experiment runner and native P2 integration already exist. The unchecked tasks below are **planned additions, not claims of completed functionality**. Credit the existing Simtra map and population engine; the demo must clearly identify the team's hackathon additions. Each person works on their own branch; merge after integration checks pass.

| Person | Branch | Responsibility | Track / prize target |
| --- | --- | --- | --- |
| **P1 — Mahin Bharathwaj** | `p1` | Experiment UX, follow-up execution, stopping rules, and repair history | Done Means Done |
| **P2 — Aradhya Mishra** | `p2` | Claim-level Nemotron review, repair validation, and false-pass evaluation | Done Means Done + Best Use of Nemotron |

### Run the P1 experiment lab locally

P1 includes **Experiment mode in the city map’s input box**, with compact results, saved receipts, and a deterministic failure-and-repair demo. Select Experiment, type your goal, and send; use Try demo for a fixture run without API calls. Assumptions and checks expand under Details. The standalone lab remains available for development. From the repository root:

```sh
mkdir -p runs
PORT=8083 TILES_DB=server_tiles/sf.db CACHE_DB=runs/cache.db STATE_DB=runs/state.db cargo run -p simfrancisco --bin server
# In a second terminal:
python3 -m http.server 5176 --bind 127.0.0.1 --directory frontend
```

Open [the city map](http://localhost:5176/index.html?backend=local&port=8083) and select **Experiment**. Demo mode needs no API key. Try demo deliberately omits one scenario so you can see detection → targeted repair → a labeled demo receipt. Live mode uses server-side `CRUSOE_API_KEY` and `CRUSOE_MODEL`; the integrated Nemotron P2 verifier runs automatically and must pass before completion. Keep `VERIFIER_URL` empty for native P2; optionally set `SILICO_VERIFIER_MODEL` to another supported Nemotron ID. Full review evidence is saved with each receipt. Set variables in the shell or the ignored `.env` file (which overrides shell values). The new lab always defaults to the local backend.

The integration boundary, schemas, limitations, and test commands are in [P1–P2 contract](docs/P1-P2-CONTRACT.md). Plan revision requests stop for user input and require a new run. Completed execution verifies the workflow, not predictive accuracy.

### P1 — Execute and explain the agent loop (`p1`)

**Owner: Mahin.** Implement in this order:

- [ ] **Definition of done:** show 3–5 concrete, goal-specific checks before execution begins, alongside assumptions. Distinguish deterministic checks from semantic review; show pending, passed, failed, and unknown states.
- [ ] **Targeted follow-up execution:** translate P2's structured complaints into supported tasks. Start with named scenario reruns and deterministic calculations over saved results, such as expected revenue = price × modeled purchase probability for the same audience. Include units, input evidence IDs, and a new result ID. Do not execute arbitrary model-generated code or rerun every scenario.
- [ ] **Repair history:** persist each failed claim/check, original complaint, requested task, previous evidence, new evidence, and subsequent P2 decision. Display a compact “Plan → Run → Failed check → Repair → Verified” history with expandable evidence.
- [ ] **No-progress stopping:** stop automatic retries when the same unresolved check and reason recur with unchanged relevant evidence or no new executable task. Rewording is not progress. Preserve the two-repair ceiling and existing request/time budgets; record why the run stopped and what remains unresolved.
- [ ] **Simplify the main UI:** make Experiment the default, move Predict/A/B into advanced controls, keep timeline/news secondary, and move raw JSON/provider/export details under Details. Keep the map as context; pause further geography polish.
- [ ] **Integration checks:** exercise failure → targeted task → re-review → completion, unchanged repair → stop, exhausted budget, invalid task, and stale review. A provider failure or unresolved check must never appear as success.

**Owns:** `crates/sim-core/src/experiment/` (including the P2 adapter, execution, storage, and tests), `frontend/`, and this README. P1 also owns any required shared edits to `crusoe.rs`, `api.rs`, and `lib.rs`. Preserve population weighting, frozen context, and existing cost controls.

**Done when:** a clearly labeled failure-injection demo resolves a missing calculation or scenario without manual intervention, shows the exact repair evidence, and stops honestly when a repeated failure cannot be resolved. P1 uses a stub while P2 develops; fixture output must never count as live verification.

### P2 — Prove completion and reject false passes (`p2`)

**Owner: Aradhya.** Continue the verifier improvements already underway:

- [ ] **Claim-level review:** identify the exact conclusion, the original result/source supporting it, and why that evidence supports its direction and magnitude. Relevant-looking citations alone do not pass. Synthetic estimates must not be described as observed behavior or validated forecasts.
- [ ] **Actionable repair requests:** return a stable failed check/claim reference, concrete complaint, supported task type, affected scenario/result IDs, and the evidence or condition needed to resolve it. Begin with named reruns and missing calculations; unsupported work must block or request clarification.
- [ ] **Verify the repair itself:** compare the previous complaint and evidence with the new result. Explicitly decide resolved, unresolved, or insufficient evidence. Successful execution, a rewritten conclusion, or a different answer is not proof of repair.
- [ ] **False-pass evaluation:** test convincing incorrect conclusions, irrelevant citations, reversed comparisons, missing calculations, and unchanged repairs, plus valid controls that should pass. Report false passes / invalid cases, valid-case acceptance, review latency, and successful completion separately. Keep tuning fixtures separate from held-out evaluation cases and distinguish mock from live results.
- [ ] **Maintain strict completion gates:** reject stale request/plan identities, missing evidence, contradictory verdicts, malformed output, and provider failures. A semantic pass must never override failed deterministic checks.

**Owns:** `crates/sim-core/src/verifier/`, including `types.rs`, prompts, review logic, fixtures, tests, and verifier evaluation documentation. P2 owns native verifier schema changes; send P1 the schema and fixture examples before integration. Leave frontend, experiment execution, and shared routing/provider files to P1.

**Done when:** the targeted invalid fixture cases are rejected, valid controls can pass, a repaired case passes only after the original complaint is resolved, and an unchanged repair remains rejected. Publish measured results with case counts; do not claim general reliability from a small test set.

### Shared integration contract — agree before changing the schema

The [P1–P2 contract](docs/P1-P2-CONTRACT.md) describes the **currently implemented** interface. New task types and repair-history fields below are proposed extensions; they are not available yet.

1. **P2 defines the native schema and example fixtures first.** Preserve run ID, plan hash, exact request hash, repair round, stable check/scenario/result IDs, and evidence pointers. Coordinate schema versioning and old-receipt handling explicitly.
2. **P1 sends** the original goal, frozen plan/context, completion checks, original results, deterministic findings, and a proposed repair-history attachment containing previous complaints, tasks, and evidence. P2 must inspect underlying evidence rather than trusting prior verdicts.
3. **P2 returns** the existing `pass` / `repair` / `blocked` verdict, findings for every required check, and structured repair requests. Add exact-claim evidence and resolution conditions; define a versioned calculation-task payload together. P2 requests work; P1 validates and executes it.
4. **P1 executes only allowlisted tasks** within existing budgets, stores new evidence with provenance, and asks P2 to review the updated request. Do not edit or overwrite earlier evidence. Missing or unresolved evidence blocks completion.
5. **Keep the objective fixed.** For this iteration, automatic repairs use the frozen plan and context. Changes to goals, audiences, consequential assumptions, or required checks still stop for clarification/new planning. Automatic plan rewriting is stretch work, after the core loop is proven.
6. **P1 updates the integration document** when both sides support the extension. P2 owns native types; P1 owns the adapter and any legacy HTTP compatibility. Unsupported versions/actions fail clearly rather than silently weakening verification.

### Branch and merge workflow

Continue on `p1` and `p2`; do not discard existing local work or recreate the branches. Agree on a common integration baseline and record it in the PRs. Fetch and incorporate current `main` after committing or safely stashing local work. Do not reset or force-push either teammate's branch.

- **First handoff:** P2 supplies schema changes plus invalid, repaired, and unchanged-repair fixtures. P1 builds the executor/UI against those fixtures.
- **Parallel work:** P1 implements the tasks above; P2 implements semantic review and evaluation. Keep to file ownership to minimize conflicts.
- **Integration:** open PRs into `main`, review P2's contract changes first, then integrate P1's adapter/executor. Coordinate any shared-file changes; merging either PR must leave the existing flow working.
- **Final check:** run offline contract/runner/verifier tests, frontend checks, and the combined failure-and-repair demo. Run a deliberate live smoke test separately and report its actual outcome. Do not use a fixture receipt as proof of live inference.

**Shared demo:** ask which lunch price maximizes modeled revenue → expose checks and assumptions → deliberately omit the revenue calculation in a labeled test → P2 identifies the missing evidence → P1 calculates revenue from the saved scenario results → P2 checks the arithmetic and original complaint → show the verified receipt. A second unchanged-repair case should stop with an unresolved explanation. Keep the existing missing-scenario demo as a fallback until calculation tasks are implemented.

Verification confirms workflow completion and evidence support, not real-world predictive accuracy. Avoid claiming Shared Brain or Version N+1 unless shared memory or measured cross-run self-improvement is actually built and demonstrated.

## Built by

Mahin Bharathwaj.
