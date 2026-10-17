# P2 handoff — Silico

Local branch: `p2`. Base: `c31a42c131afb702af086bb799f3601e255eceec`.
Delivered as a P2-only branch for review into `main`. P1-owned files and
dependencies are unchanged. No deployment is included.

## Delivered

- Independent Nemotron verifier with immutable plan and request binding.
- Local coverage/context/audience/deterministic gates before paid review.
- Strictly validated decisions, evidence paths, and targeted repair requests.
- Bounded direct Crusoe transport and optional OpenShell sandbox transport.
- Adversarial tests, explicit synthetic fixtures, and runnable CLI example.
- Live positive/negative receipts with separate local/mock/live evidence.

Start with [the verifier guide](../crates/sim-core/src/verifier/README.md).
The exact wire contract is [types.rs](../crates/sim-core/src/verifier/types.rs).
The runnable harness is [p2_verify.rs](../crates/sim-core/examples/p2_verify.rs).
Verification results and limits are in the
[verification report](sponsor-sprint/2026-10-17-p2/verification-report.md).

## Remaining P1 integration

P1 registers the verifier module in `lib.rs`, maps its immutable experiment records
to the contract, reuses one verifier instance, gates completion with
`permits_completion(&request)`, and executes bounded repairs. P1 owns actual
probability/weight arithmetic and its deterministic findings. P2 does not execute
repairs or change the server/frontend. That product integration has not been
performed on this branch.

The P2 completion gate accepts a validated live pass only. Mock fixture passes are
explicitly labeled and cannot grant completion. Receipts bind exact request data;
keep them server-owned because they are not signed attestations.
