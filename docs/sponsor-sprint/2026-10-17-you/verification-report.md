# You.com + Nemotron verification

Date: 2026-10-17. P2 extends commit
`3fee46977a8ce1c2d34112120bbd5dfe483e28ee`; final source fingerprints are in
`source-manifest.json`. No P1-owned workflow/API/model/library/frontend or Cargo
manifest/lockfile changes.

## Demonstrated behavior

The actual Rust CLI used the configured You.com key to retrieve five AAPOR sources
for an explicit query about synthetic-survey limitations. Nemotron through Crusoe
returned a validated pass and cited `/web_research/sources/4` alongside the
assumption and conclusion it reviewed. The exact source titles, URLs, excerpts,
query, cutoff, retrieval time and live/mock provenance are part of the immutable
request hash. This is a real evidence review, not search results merely displayed
alongside an unrelated verdict.

A final negative live check reused that frozen source packet with a deliberately
unsupported causal/predictive conclusion. Nemotron returned a validated blocked
review and a targeted repair covering conclusion support, the caveat, and web
grounding. It did not claim that the simulated percentages prove real-world
behavior. Source reuse avoided a second search. This packet used one live You.com
search and two live model calls, with no automatic retries.

## Checks

| Check | Mode | Evidence and observed result |
|---|---|---|
| C001 | Local | `verifier-tests.txt`: 29 tests pass, including 9 research groups; HTTP contract, source/date/domain/cap filtering, failures, immutable binding, mocks, source citations, historical leakage guard |
| C002 | Live You.com | `live-research-review.json`, `live-check.json`: ready, one search, five AAPOR sources, no search error |
| C003 | Live You.com + Nemotron | Same receipts: pass, one model call, source-grounding evidence validated, no review error |
| C004 | Local | Final workspace: 219 tests pass; release and example builds pass. Details in `build-check.json`; formatting and protected-file diff checks pass |
| C005 | Live Nemotron | `live-unsupported-input.json`, `live-unsupported-review.json`, `negative-check.json`: blocked with one targeted repair, no provider/review-validation error |

The positive live request preceded the final Rust-1.80 compatibility and historical
cutoff hardening. Those preserve this current-date request's wire behavior and are
covered by final local tests. The final negative check used the hardened code.
Final workspace results and evidence SHA-256 hashes are recorded in `sprint.json`.
Text logs have only trailing whitespace normalized for Git publication.

## Limits

Search snippets are untrusted excerpts, not crawled or independently verified full
pages. Known post-cutoff publication dates and unsafe/duplicate/domain-mismatched
URLs are excluded. Undated snippets are methodological background only, not proof
of historical facts. Fresh searches for past as-of dates are blocked before any
API call; only contemporaneously recorded evidence may be reviewed for those.
The search cutoff parameter alone is not proof of historical availability.

No model pass establishes population predictive validity. P1 still owns weighted
arithmetic, experiment execution, module registration, bounded repairs and product
integration. Gate against the returned `ResearchedVerification` wrapper's enriched
request. Mock search cannot permit completion even with a live model receipt.

The earlier commit's fresh retest encountered a timeout and invalid evidence.
These new live checks passed, but they do not establish provider reliability under
all loads. The stricter evidence validator remains in place; unavailable or invalid
reviews still block. The prompt now explicitly clarifies that evidence paths refer
to the nested request, never an outer `/request` prefix.

Strict Clippy remains blocked by existing backend diagnostics documented in the
original packet. The scoped check with the existing `geo.rs` deny lint allowed
passes with 12 existing library warnings and no P2 diagnostics. No deployment or
submission was performed. The existing P2 PR is updated for review, not merged.
