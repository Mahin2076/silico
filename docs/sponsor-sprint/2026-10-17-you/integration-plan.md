# You.com evidence integration

Branch `p2`, extends commit `3fee46977a8ce1c2d34112120bbd5dfe483e28ee`.
User outcome: a reviewer can inspect which independent sources were used to assess
an experiment's assumptions, limitations, and external claims.

P2 runs deterministic checks first, then one explicit You.com Search request
(POST /v1/search, X-API-Key, count 5, strict operators, optional domain allowlist,
as-of date range, no page crawling). Bounded sources become an additive immutable
`web_research` attachment. Nemotron must review a required `p2.web_grounding`
check and cite an actual individual source before passing. The full source packet
participates in request_hash without modifying the original plan or results.

C001–C004 were declared before execution. C005 was declared before the final live
negative check, which reuses the frozen live sources without another search.
Failure/empty/invalid search blocks before model inference. No retries, fabricated
sources, silently omitted research, or automatic execution of repairs.

The shared P1 provider, library registration, workflow, frontend, dependencies,
and scientific rubric remain untouched. Live search is rejected for historical
as-of dates; only contemporaneous source packets may be reviewed for those dates.
