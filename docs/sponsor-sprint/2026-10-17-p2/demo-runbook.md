# P2 demonstration — 60 seconds

This demonstrates the verifier component; P1's product UI is not integrated here.
Use the commands in the verifier README from the repository root.

| Time | Action | Expected result |
|---|---|---|
| 0–15s | Run `missing-scenario.json` with no model mode | Repair targets message-b; local mode; zero model calls |
| 15–30s | Show `live-complete-final.json` beside the exact complete fixture | Live Crusoe/Nemotron pass; plan/request hashes, all required findings and evidence |
| 30–45s | Show `live-unsupported-openshell.json` and its input | Unsupported real-world claim blocked, evidence pointers and targeted repairs |
| 45–60s | Show test summary and P1 handoff | 20 verifier tests; immutable completion gate; no automatic repair execution |

Saved live receipts are recordings of checks performed on 2026-10-17, not a new
live request. For an on-stage live run, explicitly use `--live` or the OpenShell
mode and allow the configured deadline. Do not perform repeated paid calls as
rehearsal. An unavailable provider yields blocked, never an offline-looking pass.

Fallback: `--fixture-review .../fixture-review.json` is explicitly mock. Its receipt
says mock and `permits_completion` rejects it as live evidence. Keep that label
visible. Do not present these fixtures as observed public opinion or validated
population prediction. No prize eligibility or deployment readiness is implied.
