# Public hackathon demo

The static site deploys to Vercel. `/api` is rewritten to a Cloudflare quick tunnel
which exposes only `tools/deploy/demo_gateway.py` on localhost:8090. The Rust
backend on localhost:8086 retains its SQLite state and server-only provider keys.
**This is a temporary judging setup: the Mac, Rust backend, gateway, and tunnel
must remain running.** A quick tunnel URL changes when recreated; update the
rewrite and redeploy. Persistent hosting requires a stateful backend deployment.

The ingress admits at most four new live experiments across all visitors using an
atomic, persistent SQLite ledger outside Git. Every admission reserves $0.35,
including failures; repeat request IDs reuse the same backend run. This allocates
$1.40 in addition to the previous $0.35 reservation, below the user's $2.50 limit.
The allowance assumes the configured GLM Flash and Nemotron models and existing
180k-token P1, 128-KiB verifier input, three-review and one-search caps. It is a
conservative reservation, not a provider invoice or an account-wide dollar cap.
Do not change models/rates or reset the ledger without recalculating the budget.

Public access permits live scenario experiments, free fixture experiments,
Census queries, population loading, and saved receipts. Other paid legacy routes
(polls, A/B, memory comparison, chatter, personal answers) are blocked at ingress.
Public workspaces are separately namespaced from local sessions. Provider keys
are never in the frontend, Vercel static output, or gateway responses.

Run gateway:

```sh
python3 tools/deploy/demo_gateway.py --ledger /absolute/private/path/admissions.db
cloudflared tunnel --url http://127.0.0.1:8090 --no-autoupdate
```

Never tunnel the unrestricted Rust port. A restart must reuse the ledger.

## Resident responses

Each completed experiment scenario has a map preview button. Dots sample saved
archetype probabilities using stable resident IDs; scenario switching reuses the
same draws. Green means the first option; red means another option. Click a
resident to see the chosen label and full group probability distribution. These
are labeled modeled estimates, not fabricated individual quotations. Scenario
cards retain weighted aggregate results; raw dot counts do not replace them.
