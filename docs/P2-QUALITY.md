# P2 verification quality

Local changes on top of merged main `df94089`. This strengthens the existing
reviewer; it does not add agents, integrations, UI, or experiment types.

## Claims need specific support

The v4 evidence prompt requires one `claim_assessments` entry per conclusion,
including rejected conclusions. Each assessment identifies its kind and status,
cites exact primitive source values, and explains why those values support or
contradict the complete conclusion. A citation to a broad payload, title, URL,
completion flag, attempt number or explanation is not claim evidence.

P2 checks quoted values against the immutable request. Passing comparisons need
both numeric sources, a correct direction and a checked difference in raw units.
Recognizable probability percentages, percentage-point magnitudes, English-number
forms, and decimal fractions must agree with the cited quantities. Printed
precision permits legitimate rounding, including P1's four-decimal percentages.
A synthetic-fixture label never excuses incorrect numbers.

The model receives a bounded catalog of exact available fields and values to
reduce fabricated paths. It still must explain relevance; source existence and
arithmetic checks are not general proofs of natural-language entailment. Nuanced
causal claims, relevance, attribution of values to subjects, and unrecognized
number wording still require semantic judgment. No scientific accuracy claim is
made from these software checks.

## Repairs are checked against the original failure

The native adapter carries `repair_context` from the actual preceding repair
review, bound to the same run, plan and immediately previous round. It records the
original complaint and hashes of the scientific evidence for each target. The
current review is never mistaken for its own history when stored receipts are
reconstructed.

A requested rerun cannot clear its complaint with a new attempt counter, timestamp,
rewritten explanation, pass flag or paraphrased conclusion. Substantive new
scientific evidence is necessary, then a required `p2.repair_resolution` assessment
must explain how it addresses each original complaint and cite the relevant change.
All original checks must pass. A changed answer by itself is not sufficient.
Non-rerun repairs can address an overclaim through a supported conclusion revision.
Record arrays with unique `archetype_id`, `id`, or `option` keys are canonicalized,
so reordering identical records or changing metadata cannot fake progress. Ordered
primitive probability vectors retain their order and meaning.

This is a conservative progress gate, not an instruction to alter estimates. A
rerun that faithfully reproduces unchanged evidence must not be forced to change
probabilities. The unresolved complaint stays blocked for clarification rather
than pretending a repair happened. P2 never mutates the results or runs repairs.

The P1 change is limited to its existing native P2 adapter and adapter tests; its
planner, inference client, runner, frontend and scientific arithmetic are unchanged.
Existing explicit retry budgets remain. Malformed model assessments can be retried
within the existing budget; stale history and unchanged evidence cannot be retried
as if they were transient provider errors.

## Compatibility and evaluation

`VerificationRequest.repair_context` is optional and omitted when absent, preserving
initial request hashes. Model responses now require `claim_assessments`; repaired
reviews also require complete `repair_assessments`. Older saved decisions remain
readable with empty assessment lists but cannot grant completion under the new
gate. This does not retroactively rewrite previously completed run records.

Run the standard verifier and workspace tests. For a bounded live model evaluation:

```sh
python3 tools/evaluate_p2.py --binary /absolute/path/to/p2_verify \
  --output-dir /path/to/evidence --live
```

Supply `CRUSOE_API_KEY` through your environment. The evaluator runs eight labeled
synthetic cases with at most two concurrent calls and no automatic retries. It
reports false accepts, valid accepts, validated reviews, protocol/provider failures
and latency separately. The irrelevant web source is an explicit mock fixture,
not a live search and not eligible to grant live completion. A blocked malformed
response is safe but is not counted as a successful semantic review. This small
suite is a regression check, not a broad reliability or predictive-validity study.
