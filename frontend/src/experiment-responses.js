// Dots sample the saved archetype probabilities; they are not individual interviews.
export function experimentResponses(agents, run, scenarioId) {
  const scenario = run.plan?.scenarios.find(s => s.scenario_id === scenarioId);
  const answers = new Map((run.answers?.[scenarioId] || []).map(a => [a.archetype_id, a]));
  const members = new Map((run.archetypes || []).flatMap(a => a.member_ids.map(id => [String(id), answers.get(a.id)])));
  return agents.map(agent => {
    const answer = members.get(String(agent.id ?? agent.seed));
    if (!scenario || !answer || answer.probabilities.length !== scenario.options.length) return { verdict: null, rationale: null, responseLabel: null };
    let hash = 2166136261;
    for (const c of `${run.run_id}:${agent.id ?? agent.seed}`) hash = Math.imul(hash ^ c.charCodeAt(0), 16777619);
    const draw = (hash >>> 0) / 4294967296;
    let total = 0, choice = answer.probabilities.length - 1;
    for (let i = 0; i < answer.probabilities.length; i++) { total += answer.probabilities[i]; if (draw < total) { choice = i; break; } }
    const estimates = scenario.options.map((option, i) => `${option}: ${(answer.probabilities[i] * 100).toFixed(0)}%`).join(' · ');
    return { verdict: choice === 0 ? 'yes' : 'no', responseLabel: scenario.options[choice], rationale: `${run.mode === 'demo' ? 'Demo fixture' : 'Modeled response'} · ${scenario.description}. ${estimates}. Shared archetype estimate, not an individual interview.` };
  });
}
