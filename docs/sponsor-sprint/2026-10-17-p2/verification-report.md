# P2 verification report

Date: 2026-10-17. Branch `p2`, based on
`c31a42c131afb702af086bb799f3601e255eceec`, plus the P2 additions fingerprinted in
`source-manifest.json`. Scope: independent verifier component only.

| Check | Mode | Observed | Evidence |
|---|---|---|---|
| C001 contract and adversarial checks | Local | 20 tests pass; local defects spend no inference; strict output, identity, evidence, repair, deadline and transport validation | `verifier-tests.txt`, `local-cli-check.json` |
| C002 complete evidence | Live Crusoe/Nemotron | Pass, one model call, all required findings, no repairs, no error | `direct-check.json`, `live-complete-final.json` |
| C003 unsupported real-world conclusion | Live Crusoe/Nemotron via OpenShell | Blocked, one model call, two targeted repair requests, no transport/review validation error | `sandbox-check.json`, `live-unsupported-openshell.json` |
| C004 regressions and build | Local | Final workspace: 210 tests pass; example compiles. Optimized `validate` build passes; details in `build-check.json` | `workspace-tests-final.txt`, `p2-build.txt`, `release-build.txt`, `build-check.json` |
| C005 sandbox execution | Live OpenShell | Fixed Python bridge ran inside existing simtra sandbox, selected Nemotron through managed Crusoe inference, no API-key variable in verifier environment | `sandbox-check.json`, `live-unsupported-openshell.json` |

Check times and SHA-256 provenance are recorded in `sprint.json`. C003 and C005
intentionally share one negative live call: one checks semantic rejection, the
other the contained transport. A model verdict of blocked is the expected passing
outcome for this deliberately unsupported conclusion. Only three total live model
calls were used during implementation: one earlier positive candidate, one final
positive, and one final negative sandbox review. Authenticated model-catalog access
is separate from inference. The earlier receipt `live-complete.json` belongs to
`early-complete-input.json`; it does not attest to the strengthened final contract.

## Additional checks

Formatting and `git diff --check` passed. Every previously tracked file is
unchanged, including P1's library/API/model/frontend and Cargo manifests/lockfile.
The repository contains no copies of configured provider credentials. The tested candidate was initially verified locally. It is now being published
on `p2` for pull-request review at the user’s request; no deployment or submission
is included.

Strict Clippy (`-D warnings`) fails on 13 pre-existing issues in untouched backend
files. Normal Clippy also fails because `geo.rs:166` triggers the deny-by-default
`absurd_extreme_comparisons` lint. A scoped check with only that existing lint
allowed completes; it reports 12 existing library warnings and none in P2's
verifier/test/example. See `clippy.txt`, `clippy-scoped.txt`, and `clippy-p2.txt`.
These baseline issues were not fixed as incidental P1 changes.

## What is demonstrated

The real Rust harness sends frozen evidence through each live transport, accepts
only strictly validated Nemotron output, and preserves selected model identity.
The positive request received an evidence-bound pass. The negative review rejected
causal/predictive claims and returned allowed targeted repair actions. Local
coverage/audience failures produce repairs with zero model calls. Explicit fixture
mode says mock and cannot permit live completion. Source and input hashes make
these checks attributable to this local candidate.

## Limits and next integration

P1 must register the module, map real experiment records, retain trusted receipts,
gate completion with `permits_completion(&request)`, and execute at most two repair
rounds. There is no server/frontend workflow integration or deployment here.

Fixtures are software tests, not observed public opinion. P1's weight/probability
arithmetic is attested by its supplied deterministic findings; P2 does not
independently reconstruct population math from aggregate payloads. A semantic
model pass is not scientific validation, proof of causality/predictive accuracy,
or a guarantee against adversarial prompt content. This sandbox check covers
contained inference execution only, not a general research tool chain or prize
eligibility. No full paid prediction/rubric validation was run.

Reproduce offline checks using the verifier README. A new live run must be explicit
and may spend credits. Provider unavailability blocks completion; there is no
silent fixture fallback. Secret values and upstream error bodies are absent from
this packet.

Text build/test logs have trailing whitespace normalized for publication; their
recorded SHA-256 hashes describe those normalized files. Test content is unchanged.
