// Local routing costs no API calls. Preserve the user's words; do not invent variants.
const comparison = /\b(compare|comparison|versus|vs\.?|better|best|a\s*\/\s*b|test (?:two|three|different))\b/i;
const messages = /\b(ads?|advertisements?|messages?|slogans?|headlines?|copies|copy|creatives?)\b/i;
const memory = /\b(memory\s*(?:on|off|comparison)|remember(?:ing)?|recall(?:ing)?)\b|\bwith(?:out)?\s+(?:city\s+)?memory\b/i;
const factual = /\b(distribution|population(?: counts)? by|what share of|what percent(?:age)? of|how many)\b/i;
const hypothetical = /\b(would|could|might|if|buy|support|prefer|choose)\b/i;

export function planQuestion(value) {
  const question = String(value || '').trim();
  if (!question) return { mode: 'predict', label: 'Ask anything', description: 'Ask a question. I’ll choose the method and show the plan here.' };
  const prices = [...new Set(question.match(/\$\s*\d+(?:\.\d+)?/g) || [])];
  if (memory.test(question)) {
    const outcome = question.match(/:\s*((?:would|will|do|does|are|is|should|can)\b[\s\S]+)/i)?.[1]?.trim() || '';
    const mixed = prices.length > 1 || (messages.test(question) && (comparison.test(question) || /\bwhich\b/i.test(question)));
    return { mode: 'memory', label: 'Memory comparison', outcome,
      clarification: mixed ? 'Compare memory for one decision at a time. Ask a single yes/no outcome question after a colon.' : '',
      description: 'I’ll ask the same residents the same yes/no question with memory off and on, then compare the weighted results. Accuracy needs an observed reference.' };
  }
  if (messages.test(question) && (comparison.test(question) || /\bwhich\b/i.test(question))) {
    const labeled = question.match(/(?:^|\n|\s)A:\s*([\s\S]+?)\s+B:\s*([\s\S]+)$/i);
    const quoted = [...question.matchAll(/[“"]([^”"]+)[”"]/g)].map(m => m[1]);
    const variants = labeled ? [labeled[1].trim(), labeled[2].trim()] : quoted.length === 2 ? quoted : null;
    return { mode: 'ab', label: 'Message comparison', variants,
      clarification: quoted.length > 2 && !labeled ? 'This comparison supports two messages. Keep two quoted variants, or enter them below.' : '',
      description: 'I’ll compare two messages with the same residents and question, then show the weighted difference. These are modeled responses.' };
  }
  if (factual.test(question) && !hypothetical.test(question)) return { mode: 'data', label: 'Population data', description: 'I’ll query the Census data and show the source and denominator. No opinion simulation is needed.' };
  if (comparison.test(question) || prices.length > 1 || /\b(experiment|scenarios?|what if|what happens if)\b/i.test(question)) {
    return { mode: 'experiment', label: 'Verified experiment',
      clarification: prices.length > 3 || /\b(?:four|five|six|[4-9])\s+(?:prices|scenarios|options)\b/i.test(question) ? 'One experiment supports two or three scenarios. Narrow this comparison before running it.' : '',
      description: 'I’ll define two or three scenarios and completion checks, compare the same whole-city population, then independently verify the results and follow up on missing evidence.' };
  }
  return { mode: 'predict', label: 'Single prediction', description: 'I’ll ask the selected residents one question and aggregate their modeled responses using Census weights.' };
}
