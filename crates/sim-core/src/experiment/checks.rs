use super::contract::*;
use anyhow::{bail, ensure, Result};
use std::collections::BTreeSet;

pub fn validate_scenarios(scenarios: &[Scenario]) -> Result<()> {
    ensure!(
        (2..=3).contains(&scenarios.len()),
        "Plan must contain two or three scenarios"
    );
    let mut ids = BTreeSet::new();
    let first = &scenarios[0];
    ensure!(
        (2..=5).contains(&first.options.len()),
        "Use two to five explicit outcome options"
    );
    ensure!(
        first
            .options
            .iter()
            .all(|s| !s.trim().is_empty() && s.len() <= 100),
        "Invalid outcome labels"
    );
    ensure!(
        first.options.iter().collect::<BTreeSet<_>>().len() == first.options.len(),
        "Duplicate outcome labels"
    );
    for s in scenarios {
        ensure!(
            !s.scenario_id.is_empty()
                && s.scenario_id.len() <= 40
                && s.scenario_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                && ids.insert(&s.scenario_id),
            "Scenario IDs must be unique"
        );
        ensure!(
            !s.question.trim().is_empty() && s.question.len() <= 500,
            "Question required, max 500 bytes"
        );
        ensure!(
            !s.description.trim().is_empty() && s.description.len() <= 2000,
            "Scenario description required, max 2000 bytes"
        );
        ensure!(
            s.question == first.question && s.options == first.options,
            "All scenarios must use the same outcome question and ordered options"
        );
        ensure!(
            !s.assumptions.is_empty()
                && s.assumptions.len() <= 12
                && s.assumptions
                    .iter()
                    .all(|a| !a.trim().is_empty() && a.len() <= 500),
            "Each scenario needs explicit bounded assumptions"
        );
    }
    Ok(())
}
pub fn distribution_valid(v: &[f64], n: usize) -> bool {
    v.len() == n
        && v.iter().all(|p| p.is_finite() && (0.0..=1.0).contains(p))
        && (v.iter().sum::<f64>() - 1.0).abs() <= 1e-6
}
pub fn aggregate(groups: &[Archetype], answers: &[GroupAnswer], n: usize) -> Result<Vec<f64>> {
    ensure!(
        !groups.is_empty() && answers.len() == groups.len(),
        "Incomplete archetype coverage"
    );
    let mut seen = BTreeSet::new();
    let mut totals = vec![0.0; n];
    let mut total = 0.0;
    for a in answers {
        ensure!(seen.insert(&a.archetype_id), "Duplicate archetype answer");
        let g = groups
            .iter()
            .find(|g| g.id == a.archetype_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown archetype ID"))?;
        ensure!(
            g.weight.is_finite() && g.weight > 0.0 && distribution_valid(&a.probabilities, n),
            "Invalid weight or probability distribution"
        );
        for (t, p) in totals.iter_mut().zip(&a.probabilities) {
            *t += g.weight * p;
        }
        total += g.weight;
    }
    ensure!(total.is_finite() && total > 0.0, "Invalid total weight");
    for t in &mut totals {
        *t /= total;
    }
    Ok(totals)
}
pub fn checks(run: &Run) -> Vec<Finding> {
    let Some(plan) = &run.plan else { return vec![] };
    let expected: BTreeSet<_> = run.archetypes.iter().map(|g| g.id.clone()).collect();
    let mut out = vec![];
    for s in &plan.scenarios {
        let r = run.results.iter().find(|r| r.scenario_id == s.scenario_id);
        let answers = run
            .answers
            .get(&s.scenario_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let agg = aggregate(&run.archetypes, answers, s.options.len());
        let coverage = r
            .map(|r| {
                r.status == "complete"
                    && r.expected_archetype_ids.len() == expected.len()
                    && r.expected_archetype_ids
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        == expected
                    && r.answered_archetype_ids
                        .iter()
                        .cloned()
                        .collect::<BTreeSet<_>>()
                        == expected
                    && r.answered_archetype_ids.len() == expected.len()
            })
            .unwrap_or(false)
            && agg.is_ok();
        let context = r
            .map(|r| {
                super::hash(&run.plan) == run.plan_hash
                    && super::hash(&run.context) == plan.context_hash
                    && r.audience_hash == plan.audience_hash
                    && r.context_hash == plan.context_hash
                    && r.model == plan.model
                    && r.as_of_date == plan.as_of_date
            })
            .unwrap_or(false);
        let probabilities = r
            .map(|r| {
                r.distribution
                    .iter()
                    .map(|x| &x.option)
                    .eq(s.options.iter())
                    && distribution_valid(
                        &r.distribution
                            .iter()
                            .map(|p| p.probability)
                            .collect::<Vec<_>>(),
                        s.options.len(),
                    )
            })
            .unwrap_or(false);
        let arithmetic = r
            .zip(agg.as_ref().ok())
            .map(|(r, agg)| {
                r.distribution.len() == agg.len()
                    && r.distribution
                        .iter()
                        .zip(agg)
                        .all(|(a, b)| (a.probability - b).abs() < 1e-10)
            })
            .unwrap_or(false);
        for (kind, ok, description) in [
            ("coverage", coverage, "Every expected archetype answered"),
            (
                "context",
                context,
                "Audience, evidence, date, and model match the frozen plan",
            ),
            (
                "probabilities",
                probabilities,
                "Ordered outcome probabilities are valid and sum to one",
            ),
            (
                "arithmetic",
                arithmetic,
                "Reported estimates match independently recomputed Census-weighted totals",
            ),
        ] {
            out.push(Finding {
                check_id: format!("{}:{kind}", s.scenario_id),
                status: if ok { "pass" } else { "fail" }.into(),
                scenario_ids: vec![s.scenario_id.clone()],
                evidence: description.into(),
            });
        }
    }
    out
}
pub fn validate_verifier(req: &VerifierRequest, res: &VerifierResponse) -> Result<()> {
    ensure!(
        res.schema_version == req.schema_version
            && res.run_id == req.run_id
            && res.plan_hash == req.plan_hash
            && res.round == req.round,
        "Verifier response belongs to a stale or different run"
    );
    ensure!(
        !res.summary.trim().is_empty() && res.summary.len() <= 4000,
        "Verifier summary required, maximum 4000 bytes"
    );
    ensure!(
        ["pass", "repair", "blocked"].contains(&res.verdict.as_str()),
        "Unknown verdict"
    );
    let required: BTreeSet<_> = req
        .plan
        .checks
        .iter()
        .map(|c| c.check_id.as_str())
        .collect();
    let received: BTreeSet<_> = res.checks.iter().map(|c| c.check_id.as_str()).collect();
    ensure!(
        required == received && received.len() == res.checks.len(),
        "Verifier must return exactly one finding per required check"
    );
    let scenarios: BTreeSet<_> = req
        .plan
        .scenarios
        .iter()
        .map(|s| s.scenario_id.as_str())
        .collect();
    for c in &res.checks {
        ensure!(
            ["pass", "fail", "unknown"].contains(&c.status.as_str())
                && !c.evidence.trim().is_empty()
                && c.evidence.len() <= 3000,
            "Invalid verifier finding"
        );
        ensure!(
            c.scenario_ids
                .iter()
                .all(|id| scenarios.contains(id.as_str())),
            "Unknown scenario in finding"
        );
        let expected = &req
            .plan
            .checks
            .iter()
            .find(|x| x.check_id == c.check_id)
            .unwrap()
            .scenario_ids;
        ensure!(
            c.scenario_ids.iter().collect::<BTreeSet<_>>()
                == expected.iter().collect::<BTreeSet<_>>()
                && c.scenario_ids.len() == expected.len(),
            "Finding must cover the required scenarios"
        );
        if let Some(d) = req
            .deterministic_checks
            .iter()
            .find(|d| d.check_id == c.check_id)
        {
            ensure!(
                d.status == c.status,
                "Verifier cannot override deterministic checks"
            );
        }
    }
    for r in &res.repair_requests {
        ensure!(
            ["rerun_scenario", "revise_plan", "request_clarification"].contains(&r.action.as_str()),
            "Unsupported repair action"
        );
        ensure!(
            !r.reason.trim().is_empty() && r.reason.len() <= 3000,
            "Repair needs a bounded reason"
        );
        ensure!(
            !r.check_ids.is_empty()
                && r.check_ids.iter().all(|id| required.contains(id.as_str())
                    && res
                        .checks
                        .iter()
                        .any(|c| c.check_id == *id && c.status != "pass")),
            "Repair must target failed or unknown checks"
        );
        ensure!(
            r.scenario_ids
                .iter()
                .all(|id| scenarios.contains(id.as_str())),
            "Unknown repair scenario"
        );
        if r.action == "rerun_scenario" {
            ensure!(!r.scenario_ids.is_empty(), "Rerun requires scenario IDs");
        }
    }
    if res.verdict == "pass" {
        ensure!(
            res.repair_requests.is_empty()
                && res.checks.iter().all(|c| c.status == "pass")
                && req.deterministic_checks.iter().all(|c| c.status == "pass"),
            "Passing verdict contradicts checks"
        );
    }
    if res.verdict == "repair"
        && (res.repair_requests.is_empty() || res.checks.iter().all(|c| c.status == "pass"))
    {
        bail!("Repair verdict has no failing checks or repair actions");
    }
    Ok(())
}

/// One shared measurement definition makes comparison consistency structural.
pub fn planned_scenarios(value: serde_json::Value) -> Result<Vec<Scenario>> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum Assumptions {
        List(Vec<String>),
        Text(String),
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Variation {
        scenario_id: String,
        description: String,
        assumptions: Assumptions,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Draft {
        question: String,
        options: Vec<String>,
        scenarios: Vec<Variation>,
    }
    let draft: Draft = serde_json::from_value(value)?;
    let scenarios = draft
        .scenarios
        .into_iter()
        .map(|s| Scenario {
            scenario_id: s.scenario_id,
            description: s.description,
            assumptions: match s.assumptions {
                Assumptions::List(v) => v,
                Assumptions::Text(v) => vec![v],
            },
            question: draft.question.clone(),
            options: draft.options.clone(),
        })
        .collect::<Vec<_>>();
    validate_scenarios(&scenarios)?;
    Ok(scenarios)
}
