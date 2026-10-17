// External source content is always text; links accept only public HTTP(S) URLs.
export function sourceUrl(value) {
  try {
    const u = new URL(value);
    if (!['https:', 'http:'].includes(u.protocol) || u.username || u.password) return null;
    if (!u.hostname.includes('.') || u.hostname.endsWith('.local') || u.hostname.endsWith('.localhost') || /^[\d.]+$/.test(u.hostname)) return null;
    return u.href;
  } catch { return null; }
}
export function researchOutcome(run) {
  const research = run.research;
  if (!research) return '';
  if (research.error) return research.error;
  const review = run.p2_reviews?.at(-1);
  if (review?.request.web_research && review.decision.request_hash !== research.trigger_request_hash) {
    return `Nemotron recheck: ${review.decision.verdict}. ${review.decision.summary}`;
  }
  return research.web?.status === 'ready' ? 'Sources saved. Awaiting independent recheck.' : 'Searching for evidence requested by the verifier…';
}
export function renderResearch(host, run) {
  if (!host) return;
  host.hidden = !run.research;
  host.replaceChildren();
  if (!run.research) return;
  const r = run.research, doc = host.ownerDocument;
  const node = (tag, text) => { const e = doc.createElement(tag); e.textContent = text; return e; };
  host.append(node('h3', 'Evidence follow-up · You.com'), node('p', `Objection: ${r.objection}`), node('p', `Search: ${r.query.query}`), node('p', researchOutcome(run)));
  for (const source of r.web?.sources || []) {
    const details = node('details', ''), summary = node('summary', ''), url = sourceUrl(source.url);
    const title = node(url ? 'a' : 'span', source.title);
    if (url) { title.href = url; title.target = '_blank'; title.rel = 'noopener noreferrer'; }
    summary.append(title); details.append(summary);
    details.append(node('small', `${source.publication_date || 'Undated · background only'} · retrieved ${r.web.fetched_at}`));
    source.snippets.forEach(text => details.append(node('p', text)));
    host.append(details);
  }
  host.append(node('small', 'Sources are excerpts reviewed by Nemotron. They do not validate simulated purchase rates.'));
}
