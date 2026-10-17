# P2 integration scope

Repository: Mahin2076/silico; local branch `p2`; base c31a42c131afb702af086bb799f3601e255eceec.
Scope: independent experiment verification only. P1 owns workflow execution and shared files.

| Integration | Minimum behavior | Acceptance |
|---|---|---|
| Crusoe Managed Inference / Nemotron | One bounded semantic review after local gates, validated structured response | Live complete fixture passes; unsupported conclusion rejected |
| NemoClaw / OpenShell | Fixed bridge inside existing sandbox using managed inference | Rust harness gets validated negative review without a provider key in its environment |

Implementation order: immutable contract and local gates; strict model review validation;
direct provider adapter; adversarial tests; bounded live fixture; optional sandbox
adapter and its live negative check. No retries or fixture fallback in live modes.

Checks C001–C005 were declared before their execution in `sprint.json`. Fixtures are
software-only synthetic data. Model selection was confirmed through the account's
live model catalog. No population predictions or scientific validation were run.
No P1 shared files, credentials, manifests, or deployment config were changed.
The user subsequently authorized publishing P2 and opening a pull request. Full integration instructions are in the verifier README.
