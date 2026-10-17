//! Narrow, reproducible support receipts. Exact citations and comparison arithmetic
//! are checked locally; semantic entailment remains the model reviewer's task.
use super::{CheckStatus, VerificationRequest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    ModeledResult,
    Comparison,
    Limitation,
    ExternalFact,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimEvidence {
    pub pointer: String,
    /// Exact source value, not a paraphrase or a percentage conversion.
    pub observed: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonRelation {
    Greater,
    Less,
    Equal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonProof {
    pub left_pointer: String,
    pub right_pointer: String,
    pub relation: ComparisonRelation,
    /// Raw source units: left minus right; 0.65 - 0.60 is 0.05, not 5.
    pub difference: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimAssessment {
    pub conclusion_index: usize,
    pub kind: ClaimKind,
    pub status: CheckStatus,
    pub evidence: Vec<ClaimEvidence>,
    pub reasoning: String,
    pub comparison: Option<ComparisonProof>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Result,
    Assumption,
    Snippet,
}

fn source_kind(pointer: &str, value: &Value) -> Option<SourceKind> {
    let tokens: Vec<_> = pointer.strip_prefix('/')?.split('/').collect();
    let index = |s: &str| s.parse::<usize>().is_ok_and(|i| i.to_string() == s);
    let text = || value.as_str().is_some_and(|s| !s.trim().is_empty());
    if tokens.len() == 3
        && tokens[0] == "plan"
        && tokens[1] == "assumptions"
        && index(tokens[2])
        && text()
    {
        return Some(SourceKind::Assumption);
    }
    if tokens.len() == 5
        && tokens[0] == "web_research"
        && tokens[1] == "sources"
        && index(tokens[2])
        && tokens[3] == "snippets"
        && index(tokens[4])
        && text()
    {
        return Some(SourceKind::Snippet);
    }
    if tokens.len() >= 4
        && tokens[0] == "results"
        && index(tokens[1])
        && tokens[2] == "payload"
        && tokens[3..].iter().all(|token| {
            let name = token
                .replace("~1", "/")
                .replace("~0", "~")
                .to_ascii_lowercase();
            !matches!(
                name.as_str(),
                "id" | "status"
                    | "passed"
                    | "pass"
                    | "valid"
                    | "complete"
                    | "fixture"
                    | "metadata"
                    | "context"
                    | "plan_hash"
                    | "model"
                    | "seed"
                    | "audience_id"
                    | "scenario_id"
                    | "attempt"
                    | "attempt_id"
                    | "attempt_number"
                    | "timestamp"
                    | "created_at"
                    | "updated_at"
                    | "elapsed_ms"
                    | "summary"
                    | "rationale"
                    | "explanation"
                    | "notes"
            ) && !name.ends_with("_valid")
                && !name.ends_with("_checked")
                && !name.starts_with("checked_")
                && !name.contains("checked_by")
        })
        && (value.as_f64().is_some_and(f64::is_finite) || text())
    {
        return Some(SourceKind::Result);
    }
    None
}

/// Exact existing citation candidates, balanced across results. This is a prompt
/// aid, not an alternate source of truth: validation still resolves the request.
pub(crate) fn evidence_catalog(request: &VerificationRequest) -> Vec<ClaimEvidence> {
    fn walk(
        value: &Value,
        pointer: String,
        depth: usize,
        visited: &mut usize,
        out: &mut Vec<ClaimEvidence>,
    ) {
        if depth > 12 || *visited >= 1024 || out.len() >= 8 {
            return;
        }
        *visited += 1;
        if source_kind(&pointer, value).is_some() {
            if !out.iter().any(|e| e.pointer == pointer) {
                out.push(ClaimEvidence {
                    pointer,
                    observed: value.clone(),
                });
            }
            return;
        }
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter().take(128) {
                    walk(
                        child,
                        format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1")),
                        depth + 1,
                        visited,
                        out,
                    );
                    if out.len() >= 8 || *visited >= 1024 {
                        break;
                    }
                }
            }
            Value::Array(values) => {
                for (index, child) in values.iter().take(128).enumerate() {
                    walk(child, format!("{pointer}/{index}"), depth + 1, visited, out);
                    if out.len() >= 8 || *visited >= 1024 {
                        break;
                    }
                }
            }
            _ => (),
        }
    }
    let candidates: Vec<_> = request
        .results
        .iter()
        .take(64)
        .enumerate()
        .map(|(index, result)| {
            let root = format!("/results/{index}/payload");
            let mut out = Vec::new();
            let mut visited = 0;
            for path in ["/weighted_yes_share", "/receipt/distribution"] {
                if let Some(value) = result.payload.pointer(path) {
                    walk(
                        value,
                        format!("{root}{path}"),
                        3 + path.matches('/').count(),
                        &mut visited,
                        &mut out,
                    );
                }
            }
            walk(&result.payload, root, 3, &mut visited, &mut out);
            out
        })
        .collect();
    let mut catalog = Vec::new();
    let mut bytes = 2;
    let mut append = |evidence: ClaimEvidence| {
        if catalog.len() >= 128 {
            return;
        }
        let Ok(serialized) = serde_json::to_vec(&evidence) else {
            return;
        };
        let added = serialized.len() + usize::from(!catalog.is_empty());
        if bytes + added <= 16_384 {
            bytes += added;
            catalog.push(evidence);
        }
    };
    // Round-robin rather than allowing a large first receipt to consume the list.
    for round in 0..8 {
        for result in &candidates {
            if let Some(evidence) = result.get(round) {
                append(evidence.clone());
            }
        }
    }
    for (index, assumption) in request.plan.assumptions.iter().take(64).enumerate() {
        if !assumption.trim().is_empty() {
            append(ClaimEvidence {
                pointer: format!("/plan/assumptions/{index}"),
                observed: Value::String(assumption.clone()),
            });
        }
    }
    if let Some(web) = &request.web_research {
        for (index, source) in web.sources.iter().take(5).enumerate() {
            for (snippet_index, snippet) in source.snippets.iter().take(2).enumerate() {
                if !snippet.trim().is_empty() {
                    append(ClaimEvidence {
                        pointer: format!("/web_research/sources/{index}/snippets/{snippet_index}"),
                        observed: Value::String(snippet.clone()),
                    });
                }
            }
        }
    }
    catalog
}

pub(crate) fn validate_claims(
    request: &VerificationRequest,
    assessments: &[ClaimAssessment],
    support_status: CheckStatus,
) -> Result<(), &'static str> {
    if assessments.is_empty()
        || assessments.len() != request.conclusions.len()
        || assessments.len() > 32
    {
        return Err("claim_coverage");
    }
    let document = serde_json::to_value(request).map_err(|_| "claim_document")?;
    let mut indices = BTreeSet::new();
    for claim in assessments {
        if claim.conclusion_index >= request.conclusions.len()
            || !indices.insert(claim.conclusion_index)
            || claim.status == CheckStatus::NotReviewed
            || claim.reasoning.trim().is_empty()
            || claim.reasoning.len() > 2400
            || claim.evidence.len() > 12
        {
            return Err("claim_shape");
        }
        let mut pointers = BTreeSet::new();
        let mut kinds = Vec::new();
        for evidence in &claim.evidence {
            if evidence.pointer.len() > 1024 || !pointers.insert(evidence.pointer.as_str()) {
                return Err("claim_evidence");
            }
            let actual = document
                .pointer(&evidence.pointer)
                .ok_or("claim_evidence")?;
            if actual != &evidence.observed {
                return Err("claim_observed_mismatch");
            }
            kinds.push(source_kind(&evidence.pointer, actual).ok_or("claim_evidence_kind")?);
        }
        if claim.status == CheckStatus::Pass {
            let valid_support = match claim.kind {
                ClaimKind::ModeledResult | ClaimKind::Comparison => claim
                    .evidence
                    .iter()
                    .zip(&kinds)
                    .any(|(e, kind)| *kind == SourceKind::Result && e.observed.is_number()),
                ClaimKind::Limitation => kinds.contains(&SourceKind::Assumption),
                ClaimKind::ExternalFact => kinds.contains(&SourceKind::Snippet),
            };
            if !valid_support {
                return Err("claim_support_missing");
            }
        }
        if claim.kind != ClaimKind::Comparison && claim.comparison.is_some() {
            return Err("claim_unexpected_comparison");
        }
        if claim.kind == ClaimKind::Comparison
            && claim.status == CheckStatus::Pass
            && claim.comparison.is_none()
        {
            return Err("claim_comparison_missing");
        }
        if let Some(proof) = &claim.comparison {
            if proof.left_pointer == proof.right_pointer || !proof.difference.is_finite() {
                return Err("claim_comparison_invalid");
            }
            let number = |pointer: &str| -> Option<f64> {
                claim
                    .evidence
                    .iter()
                    .zip(&kinds)
                    .find(|(e, kind)| e.pointer == pointer && **kind == SourceKind::Result)
                    .and_then(|(e, _)| e.observed.as_f64())
                    .filter(|n| n.is_finite())
            };
            let left = number(&proof.left_pointer).ok_or("claim_comparison_evidence")?;
            let right = number(&proof.right_pointer).ok_or("claim_comparison_evidence")?;
            let relation = if left > right {
                ComparisonRelation::Greater
            } else if left < right {
                ComparisonRelation::Less
            } else {
                ComparisonRelation::Equal
            };
            let difference = left - right;
            let tolerance = 1e-9 * difference.abs().max(1.0);
            if !difference.is_finite()
                || proof.relation != relation
                || (proof.difference - difference).abs() > tolerance
            {
                return Err("claim_comparison_mismatch");
            }
        }
        if claim.status == CheckStatus::Pass
            && matches!(claim.kind, ClaimKind::ModeledResult | ClaimKind::Comparison)
            && !numeric_claim_supported(&request.conclusions[claim.conclusion_index], claim)
        {
            return Err("claim_numeric_mismatch");
        }
    }
    let expected = if assessments.iter().any(|c| c.status == CheckStatus::Blocked) {
        CheckStatus::Blocked
    } else if assessments.iter().any(|c| c.status == CheckStatus::Repair) {
        CheckStatus::Repair
    } else {
        CheckStatus::Pass
    };
    if support_status != expected {
        return Err("claim_status_mismatch");
    }
    Ok(())
}

// This checks recognizable probability notation, not arbitrary natural-language
// entailment. Topic relevance, population identity and causal reasoning still need
// semantic review. Printed precision determines the permitted rounding interval.
fn numeric_claim_supported(text: &str, claim: &ClaimAssessment) -> bool {
    let chars: Vec<_> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut cursor = 0;
    while cursor < chars.len() {
        if chars[cursor].is_ascii_alphanumeric()
            || (matches!(chars[cursor], '-' | '.')
                && chars.get(cursor + 1).is_some_and(char::is_ascii_digit))
        {
            let start = cursor;
            cursor += 1;
            while cursor < chars.len()
                && (chars[cursor].is_ascii_alphanumeric()
                    || (chars[cursor] == '.'
                        && cursor > start
                        && chars[cursor - 1].is_ascii_digit()
                        && chars.get(cursor + 1).is_some_and(char::is_ascii_digit)))
            {
                cursor += 1;
            }
            tokens.push(
                chars[start..cursor]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase(),
            );
        } else {
            if chars[cursor] == '%' {
                tokens.push("%".into());
            }
            cursor += 1;
        }
        if tokens.len() > 4096 {
            return false;
        }
    }
    let probabilities: Vec<_> = claim
        .evidence
        .iter()
        .filter(|e| e.pointer.starts_with("/results/") && e.pointer.contains("/payload/"))
        .filter_map(|e| e.observed.as_f64())
        .filter(|n| (0.0..=1.0).contains(n))
        .collect();
    let delta = claim.comparison.as_ref().map(|p| p.difference.abs());
    let relative = claim.comparison.as_ref().and_then(|p| {
        claim
            .evidence
            .iter()
            .find(|e| e.pointer == p.right_pointer)
            .and_then(|e| e.observed.as_f64())
            .filter(|n| *n != 0.0)
            .map(|right| (p.difference / right).abs())
    });
    let mut index = 0;
    while index < tokens.len() {
        let Some((number, consumed, decimals, decimal_literal)) = read_number(&tokens[index..])
        else {
            index += 1;
            continue;
        };
        let next = index + consumed;
        let is_percent = tokens
            .get(next)
            .is_some_and(|t| matches!(t.as_str(), "%" | "percent" | "percentage"));
        let is_points = tokens.get(next).is_some_and(|t| t == "pp")
            || (is_percent
                && tokens
                    .get(next + 1)
                    .is_some_and(|t| matches!(t.as_str(), "point" | "points")));
        let tolerance = 0.5 * 10f64.powi(-(decimals as i32)) + 1e-10;
        let close =
            |expected: f64, scale: f64| (number / scale - expected).abs() <= tolerance / scale;
        if is_points {
            if !delta.is_some_and(|d| close(d, 100.0)) {
                return false;
            }
        } else if is_percent {
            let start = index.saturating_sub(5);
            let end = (next + 7).min(tokens.len());
            let relative_context = tokens[start..end].iter().any(|t| {
                matches!(
                    t.as_str(),
                    "relative" | "increase" | "decrease" | "improvement" | "lift"
                )
            });
            if !probabilities.iter().any(|p| close(*p, 100.0))
                && !(relative_context && relative.is_some_and(|r| close(r, 100.0)))
            {
                return false;
            }
        } else if decimal_literal
            && (0.0..=1.0).contains(&number)
            && !probabilities.iter().any(|p| close(*p, 1.0))
            && !delta.is_some_and(|d| close(d, 1.0))
        {
            return false;
        }
        index = next.max(index + 1);
    }
    true
}

fn read_number(tokens: &[String]) -> Option<(f64, usize, usize, bool)> {
    let first = tokens.first()?;
    if let Ok(number) = first.parse::<f64>() {
        if !number.is_finite() {
            return None;
        }
        let decimals = first
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len());
        return Some((number, 1, decimals, first.contains('.')));
    }
    fn small(word: &str) -> Option<u32> {
        Some(match word {
            "zero" => 0,
            "one" => 1,
            "two" => 2,
            "three" => 3,
            "four" => 4,
            "five" => 5,
            "six" => 6,
            "seven" => 7,
            "eight" => 8,
            "nine" => 9,
            "ten" => 10,
            "eleven" => 11,
            "twelve" => 12,
            "thirteen" => 13,
            "fourteen" => 14,
            "fifteen" => 15,
            "sixteen" => 16,
            "seventeen" => 17,
            "eighteen" => 18,
            "nineteen" => 19,
            "twenty" => 20,
            "thirty" => 30,
            "forty" => 40,
            "fifty" => 50,
            "sixty" => 60,
            "seventy" => 70,
            "eighty" => 80,
            "ninety" => 90,
            _ => return None,
        })
    }
    let mut number = small(first)?;
    let mut consumed = 1;
    if (1..=9).contains(&number) && tokens.get(consumed).is_some_and(|t| t == "hundred") {
        number *= 100;
        consumed += 1;
        if tokens.get(consumed).is_some_and(|t| t == "and") {
            consumed += 1;
        }
        if let Some(remainder) = tokens.get(consumed).and_then(|t| small(t)) {
            number += remainder;
            consumed += 1;
            if remainder >= 20 {
                if let Some(ones) = tokens
                    .get(consumed)
                    .and_then(|t| small(t))
                    .filter(|n| (1..=9).contains(n))
                {
                    number += ones;
                    consumed += 1;
                }
            }
        }
    } else if number >= 20 {
        if let Some(ones) = tokens
            .get(consumed)
            .and_then(|t| small(t))
            .filter(|n| (1..=9).contains(n))
        {
            number += ones;
            consumed += 1;
        }
    }
    Some((number as f64, consumed, 0, false))
}
