//! Paired evaluations over real PUMS residents. No fixture answers or persistent writes.
use crate::{
    memory::{self, PersonaMemory},
    persona::Population,
    predict::{cluster_agents, Engine, Framing, Poll, PollResult},
};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonRequest {
    pub question: String,
    #[serde(default)]
    pub description: String,
    pub as_of_date: String,
    pub model: String,
    /// Optional observed YES share. Never included in the model prompt.
    pub reference_yes_share: Option<f64>,
}
impl ComparisonRequest {
    pub fn validate(&self) -> Result<()> {
        if self.question.trim().is_empty()
            || self.question.chars().count() > 2000
            || self.description.chars().count() > 4000
        {
            return Err(anyhow!("A question of 1–2000 characters is required"));
        }
        chrono::NaiveDate::parse_from_str(&self.as_of_date, "%Y-%m-%d")
            .map_err(|_| anyhow!("as_of_date must use YYYY-MM-DD"))?;
        if self.model.trim().is_empty()
            || self.model.len() > 100
            || crate::model::Model::parse(&self.model).id() != self.model
        {
            return Err(anyhow!("model is required"));
        }
        if self
            .reference_yes_share
            .is_some_and(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(anyhow!("reference_yes_share must be between 0 and 1"));
        }
        Ok(())
    }
    fn poll(&self) -> Poll {
        Poll {
            question: self.question.trim().into(),
            description: self.description.clone(),
            framing: Framing::Vote,
            as_of_date: self.as_of_date.clone(),
            model: Some(self.model.clone()),
            population: Some("all".into()),
            event: None,
            options: vec![],
        }
    }
}
#[derive(Debug, Serialize)]
pub struct ArchetypeAnswer {
    pub representative_id: u32,
    pub member_ids: Vec<u32>,
    pub p_yes: f64,
    pub rationale: String,
    /// Exact recalled fragment supplied, not a claim that the model cited it.
    pub memory_context: String,
}
pub(crate) struct EvaluationContext {
    pub memory_by_rep: HashMap<usize, String>,
    pub answers: Mutex<Vec<ArchetypeAnswer>>,
}
#[derive(Serialize)]
pub struct ComparisonArm {
    pub result: PollResult,
    pub archetype_answers: Vec<ArchetypeAnswer>,
    pub absolute_error: Option<f64>,
}
#[derive(Serialize)]
pub struct Comparison {
    pub kind: &'static str,
    pub city: String,
    pub population_key: String,
    pub population_hash: String,
    pub snapshot_hash: String,
    pub memory_snapshot: BTreeMap<u32, PersonaMemory>,
    pub reference_yes_share: Option<f64>,
    pub off: ComparisonArm,
    pub on: ComparisonArm,
    pub delta_pp: f64,
    /// Positive means lower error with memory; negative is a regression.
    pub error_reduction: Option<f64>,
    pub limitations: Vec<&'static str>,
}

pub fn frozen_memories(
    mut recalled: HashMap<u32, PersonaMemory>,
    as_of: &str,
) -> BTreeMap<u32, PersonaMemory> {
    // Enforce date filtering even when an adapter returns future records.
    for memory in recalled.values_mut() {
        memory.events.retain(|e| {
            chrono::NaiveDate::parse_from_str(&e.as_of_date, "%Y-%m-%d").is_ok()
                && e.as_of_date.as_str() <= as_of
        });
        memory.tests.retain(|e| {
            chrono::NaiveDate::parse_from_str(&e.as_of_date, "%Y-%m-%d").is_ok()
                && e.as_of_date.as_str() <= as_of
        });
        memory
            .events
            .sort_by(|a, b| a.as_of_date.cmp(&b.as_of_date).then(a.id.cmp(&b.id)));
        memory
            .tests
            .sort_by(|a, b| a.as_of_date.cmp(&b.as_of_date).then(a.id.cmp(&b.id)));
    }
    recalled.into_iter().collect()
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(value)?)))
}
fn absolute_error(prediction: f64, reference: Option<f64>) -> Option<f64> {
    reference.map(|r| (prediction - r).abs())
}

impl Engine {
    pub async fn compare_memory(
        &self,
        pop: &Population,
        req: &ComparisonRequest,
        workspace: &str,
    ) -> Result<Comparison> {
        req.validate()?;
        let memory = self
            .memory
            .as_ref()
            .ok_or_else(|| anyhow!("Memory comparison needs the configured city memory service"))?;
        let clusters = cluster_agents(pop, self.max_clusters);
        if clusters.is_empty() {
            return Err(anyhow!("No resident archetypes available"));
        }
        let ids: Vec<u32> = clusters.iter().map(|c| pop.agents[c.rep_idx].id).collect();
        let population_key = memory::population_key_in(workspace, pop);
        // Read once before either arm. An outage fails instead of masquerading as empty memory.
        let recalled = memory
            .recall(
                workspace,
                &pop.profile.slug,
                &population_key,
                &ids,
                &req.as_of_date,
            )
            .await?;
        let snapshot = frozen_memories(recalled, &req.as_of_date);
        let by_rep: HashMap<usize, String> = clusters
            .iter()
            .filter_map(|c| {
                snapshot
                    .get(&pop.agents[c.rep_idx].id)
                    .map(|m| (c.rep_idx, memory::prompt_fragment(m)))
            })
            .filter(|(_, s)| !s.is_empty())
            .collect();
        if by_rep.is_empty() {
            return Err(anyhow!("No dated memories are available for this population. Add city events in Timeline first."));
        }
        let off_context = EvaluationContext {
            memory_by_rep: HashMap::new(),
            answers: Mutex::new(vec![]),
        };
        let on_context = EvaluationContext {
            memory_by_rep: by_rep,
            answers: Mutex::new(vec![]),
        };
        let poll = req.poll();
        let off = self.run_isolated_poll(pop, &poll, &off_context).await?;
        let on = self.run_isolated_poll(pop, &poll, &on_context).await?;
        let off_error = absolute_error(off.p_yes, req.reference_yes_share);
        let on_error = absolute_error(on.p_yes, req.reference_yes_share);
        let delta_pp = (on.p_yes - off.p_yes) * 100.0;
        let population_hash = digest(
            &pop.agents
                .iter()
                .map(|a| (a.id, &a.persona, a.weight()))
                .collect::<Vec<_>>(),
        )?;
        Ok(Comparison{
            kind:"live_population_memory_comparison",city:pop.profile.slug.clone(),population_key,population_hash,snapshot_hash:digest(&snapshot)?,memory_snapshot:snapshot,reference_yes_share:req.reference_yes_share,
            off:ComparisonArm{result:off,archetype_answers:off_context.answers.into_inner().map_err(|_|anyhow!("trace unavailable"))?,absolute_error:off_error},
            on:ComparisonArm{result:on,archetype_answers:on_context.answers.into_inner().map_err(|_|anyhow!("trace unavailable"))?,absolute_error:on_error},delta_pp,error_reduction:off_error.zip(on_error).map(|(a,b)|a-b),
            limitations:vec!["One paired model comparison; no statistical significance or causal accuracy claim.","Same resident personas, Census weights, date, model, batching and token limits; mutable news and Hydra context disabled in both arms.","Snapshot read once; neither arm writes survey memories or changes the simulation.","Exact prompt caching applies. Model-request counts include cache lookups, not paid-call counts.","Memory context is what was supplied, not proof of model citation. Legacy persona memory does not resolve conflicting or superseded city facts.","Reference share is user-supplied and is withheld from prompts; lower error is meaningful only if that reference is valid."],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_reference() {
        let mut r = ComparisonRequest {
            question: "Test?".into(),
            description: String::new(),
            as_of_date: "2026-10-17".into(),
            model: "gpt-4o".into(),
            reference_yes_share: Some(2.0),
        };
        assert!(r.validate().is_err());
        r.reference_yes_share = None;
        assert!(r.validate().is_ok());
        assert_eq!(absolute_error(0.6, None), None);
        assert!(absolute_error(0.6, Some(0.9)).unwrap() > 0.29);
    }
    #[test]
    fn future_memory_is_removed_and_hash_is_stable() {
        use crate::memory::RecalledEvent;
        let event = |id: &str, date: &str| RecalledEvent {
            id: id.into(),
            kind: "news".into(),
            text: "test".into(),
            as_of_date: date.into(),
        };
        let memory = PersonaMemory {
            events: vec![
                event("future", "2027-01-01"),
                event("b", "2026-01-01"),
                event("a", "2026-01-01"),
            ],
            tests: vec![],
        };
        let snapshot = frozen_memories(HashMap::from([(7, memory)]), "2026-10-17");
        assert_eq!(snapshot[&7].events.len(), 2);
        assert_eq!(snapshot[&7].events[0].id, "a");
        assert_eq!(digest(&snapshot).unwrap(), digest(&snapshot).unwrap());
    }
}
