# Live research-loop verification — 2026-10-17

Run `31d147ef374090ca462d1c83316e39af5cf37b8ad73dac5a3b491f1ff924b67a` finished with `complete` on the local city-map UI.
Goal: Compare $8, $10, and $12 vegetarian lunches in San Francisco. Return simulated purchase interest for each price. Also assess, using cited external methodological research, whether synthetic survey intentions are valid evidence for choosing real-world lunch pricing. Identify what remains uncertain; simulated probabilities alone do not satisfy the external-evidence requirement.

## Observed behavior

- 10,000 seeded SF residents represented by 33 weighted archetypes; three price scenarios; all 12 deterministic checks passed.
- First live Nemotron review: `repair`, because external methodological evidence was missing.
- One You.com search retrieved five source records with dates, URLs, and snippets.
- The first assessment request failed in transport. The run stopped as `blocked` and retained its evidence.
- An explicit retry reused the saved sources and simulation, wrote a cited assessment, and submitted it for independent review.
- The second Nemotron review was rejected for an invalid evidence pointer; it did not authorize completion.
- A final explicit review retry returned a valid `pass`. The source packet, frozen plan, simulation results, and archetype answers were identical before and after recovery.
- Browser verification confirmed “Experiment Verified,” the cited assessment, limitations, source links, and saved repair history on the existing map.

This demonstrates the research → assessment → independent recheck path and failure recovery. It required two explicit retries after provider/format failures; it is not evidence of flawless unattended completion, predictive accuracy, or broad verifier reliability. External snippets do not validate simulated purchase rates. The displayed model assessment remains subject to the limits of semantic model review.

## Usage and budget

- Planner/simulation/assessment: 12 attempted calls, 24,828 recorded input tokens and 3,450 recorded output tokens; 176,217 of 180,000 admission tokens reserved.
- Nemotron: 3 attempted review calls, within the original limit.
- You.com: 1 search, within the original limit.
- A conservative $0.35 reservation covered the entire run and retries against the user's $2.50 total limit. This is an operator reservation, not a provider-enforced account cap or an exact invoice. The failed transport call and verifier token usage are not fully recorded.
- No credentials are included in this report or committed.

## Checks

All 178 application Rust tests and 88 frontend Node tests passed. The release `validate` binary built successfully. The full workspace test remains blocked by missing CMake for `proj-sys` in the separate map-building crate; the city application itself builds and runs.

To reproduce, configure ignored server-side `CRUSOE_API_KEY`, `CRUSOE_MODEL`, and `YOU_API_KEY`, start the backend and frontend as described in README, select Experiment in the map, and submit the goal above. Reproduction makes paid API calls and may yield a different semantic verdict. Do not reset budgets or relax evidence validation to obtain a pass.
