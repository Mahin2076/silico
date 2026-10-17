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

The work below is planned hackathon development on top of the existing Simtra codebase. Each person works on their own branch; merge both into `main` after integration checks pass.

| Person | Branch | Responsibility | Track / prize target |
| --- | --- | --- | --- |
| **P1 — Mahin Bharathwaj** | `p1` | Experiment planning, execution, checks, and repair loop | Done Means Done |
| **P2 — Aradhya Mishra** | `p2` | Independent Nemotron verifier; optional sandbox integration | Best Use of Nemotron; optional Best Use of NemoClaw & OpenShell |

### Run the P1 experiment lab locally

P1 includes **Experiment mode in the city map’s input box**, with compact results, saved receipts, and a deterministic failure-and-repair demo. Select Experiment, type your goal, and send; use Try demo for a fixture run without API calls. Assumptions and checks expand under Details. The standalone lab remains available for development. From the repository root:

```sh
mkdir -p runs
PORT=8083 TILES_DB=server_tiles/sf.db CACHE_DB=runs/cache.db STATE_DB=runs/state.db cargo run -p simfrancisco --bin server
# In a second terminal:
python3 -m http.server 5176 --bind 127.0.0.1 --directory frontend
```

Open [the city map](http://localhost:5176/index.html?backend=local&port=8083) and select **Experiment**. Demo mode needs no API key. Try demo deliberately omits one scenario so you can see detection → targeted repair → a labeled demo receipt. Live mode uses server-side `CRUSOE_API_KEY` and `CRUSOE_MODEL`; an independent P2 verifier is required before a live run can complete. Set variables in the shell or the ignored `.env` file (which overrides shell values). The new lab always defaults to the local backend.

The integration boundary, schemas, limitations, and test commands are in [P1–P2 contract](docs/P1-P2-CONTRACT.md). Plan revision requests stop for user input and require a new run. Completed execution verifies the workflow, not predictive accuracy.

### P1 — Experiment workflow (`p1`)

- Turn a user request into explicit scenarios, assumptions, and completion checks.
- Run scenarios against the same frozen population, model, date, and evidence context.
- Add the shared Crusoe inference client and provider configuration.
- Check complete archetype coverage, valid probabilities, matching context, and weighted calculations in code.
- Call P2's verifier and execute supported repairs, with a maximum of two repair rounds and bounded inference budgets.
- Show planning, simulation, verification, repair progress, and the final experiment receipt.
- Test that missing scenarios are repaired and incomplete runs never report success.

**Owns:** a new `crates/sim-core/src/experiment/` module, shared provider client in `crusoe.rs`, route integration in `api.rs`, module registration in `lib.rs`, and frontend changes.

### P2 — Nemotron verifier (`p2`)

- Use Nemotron through Crusoe Managed Inference; confirm the exact model ID and account access before live integration.
- Review whether the plan answers the user's request, assumptions are explicit, and conclusions follow from the results.
- Return structured `pass`, `repair`, or `blocked` decisions with check IDs, evidence, and targeted repair requests.
- Validate verifier output and handle timeouts, malformed responses, and provider failures clearly.
- Test deliberately incomplete scenarios, mismatched audiences, and unsupported conclusions.
- After the verifier works, optionally sandbox research/tool execution with NemoClaw & OpenShell for the additional partner prize.

**Owns:** a new `crates/sim-core/src/verifier/` module, verifier prompts, tests, and optional sandbox configuration. Coordinate shared provider requirements with P1; P1 integrates changes to `crusoe.rs`, `api.rs`, `lib.rs`, and the frontend to avoid merge conflicts.

### Shared integration contract

Agree on this contract before implementing either side:

- **P1 sends:** schema version, run ID, plan hash, repair round, original goal, immutable plan/context, scenarios, completion checks, scenario results, and deterministic check findings.
- **P2 returns:** the same run ID, plan hash, and round; a verdict; findings for every required check; evidence; and repair requests referencing stable check/scenario IDs.
- **Allowed repairs:** rerun a scenario, revise the plan, or request clarification. P1 executes these actions; P2 does not mutate runs directly.
- A verifier pass cannot override failed deterministic checks. Missing evidence blocks completion. Plan revisions get a new hash and invalidate affected results.
- P1 can develop against a verifier stub while P2 tests against fixed JSON inputs. Label fixtures and failure injection clearly.

### Branch and merge workflow

Both branches start from the same `main` commit containing this task split.

```sh
# P1
git switch p1

# P2 (after cloning or fetching)
git fetch origin
git switch --track origin/p2
```

Open a pull request from each branch into `main`. Merge the verifier first, then the experiment workflow, resolve integration issues, and run the combined end-to-end check.

**Shared demo:** omit one scenario in a labeled failure-injection test → the checks/verifier reject completion → P1 reruns the missing scenario → verification passes → Silico displays the final receipt. This verifies experiment execution, not real-world predictive accuracy.

## Built by

Mahin Bharathwaj.
