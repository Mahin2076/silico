import { workspaceHeaders, WORKSPACE } from './workspace.js';
export async function mountExperiments(root = document, { getCity = () => null, embedded = false, beforeStart = () => {} } = {}) {
const $ = id => root.querySelector(`#${id}`);
const params = new URLSearchParams(location.search);
// This lab is local by default; it never inherits the legacy production API.
const port = /^\d{1,5}$/.test(params.get('port') || '') ? params.get('port') : '8083';
const base = `http://localhost:${port}`;
const brand = root.querySelector('.brand');
if (brand) brand.href = location.href;
const terminal = new Set(['complete','blocked','cancelled','interrupted','needs_input','awaiting_verifier']);
const fixedGoal = 'Compare a vegetarian takeaway lunch at $8, $10, and $12';
const storageKey = `silico.run.${port}.${WORKSPACE}`;
let current = null, polling = null, ready = null, selected = null;
if ($('city-link')) $('city-link').href = `index.html?backend=local&port=${port}&ws=${WORKSPACE}`;
function node(tag, text, cls) { const e = document.createElement(tag); if (text != null) e.textContent = text; if (cls) e.className = cls; return e; }
async function api(path, body) {
  const response = await fetch(`${base}${path}`, { method: body === undefined ? 'GET' : 'POST', headers: { ...workspaceHeaders(), 'Content-Type': 'application/json' }, ...(body === undefined ? {} : { body: JSON.stringify(body) }), signal: AbortSignal.timeout(15000) });
  const data = await response.json(); if (!response.ok) { const error = new Error(data.error || `Request failed (${response.status})`); error.status = response.status; throw error; } return data;
}
function mode() {
  const demo = $('mode').value === 'demo';
  $('goal').readOnly = demo; $('inject-label').hidden = !demo;
  $('goal').value = demo ? fixedGoal : 'Compare three prices for a vegetarian lunch in San Francisco. What would residents choose?';
  $('mode-note').textContent = demo ? 'A fixed lunch-price fixture, with an explicitly simulated verifier.' : 'Calls your configured Crusoe model. P2 verification is required for completion.';
  availability();
}
function availability() {
  $('start').disabled = !ready || ($('mode').value === 'live' && !ready.live_ready);
  $('connection').hidden = embedded && !!ready && ($('mode').value === 'demo' || ready.live_ready);
  $('connection').textContent = embedded ? (!ready ? 'The experiment runner is unavailable. Please try again.' : !ready.live_ready && $('mode').value === 'live' ? 'Live experiments need an API connection. You can try the sample with “Try demo” below.' : '') : !ready ? `Cannot reach ${base}. Start the Silico backend.` : $('mode').value === 'live' && !ready.live_ready ? 'Live mode needs CRUSOE_API_KEY and CRUSOE_MODEL on the server.' : `Connected · ${base}${ready.verifier_ready ? ' · P2 verifier configured' : ' · live P2 verifier pending'}`;
}
$('mode').addEventListener('change', mode);
function render(r) {
  current = r; $('empty').hidden = true; $('run').hidden = false;
  $('run-mode').textContent = r.mode === 'demo' ? 'DEMO FIXTURE · NO LIVE INFERENCE' : 'LIVE EXPERIMENT · CRUSOE';
  $('status').textContent = r.status === 'complete' ? (r.mode === 'demo' ? (embedded ? 'Complete' : 'Demo verified') : 'Experiment verified') : r.status.replaceAll('_',' ');
  if (embedded) $('run-mode').textContent = `${r.mode === 'demo' ? 'Demo' : 'Live'} · ${({sf:'San Francisco',neu_york:'New York',synth_la:'Los Angeles',cybercago:'Chicago',simami:'Miami'}[r.city] || r.city)}`;
  $('run-goal').textContent = r.goal;
  $('verifier-summary').hidden = !r.verifier;
  const p2 = r.p2_reviews?.at(-1);
  $('verifier-summary').textContent = r.verifier ? `${p2 ? 'Nemotron' : 'Verifier'}: ${r.verifier.summary}` : '';
  $('cancel').hidden = terminal.has(r.status);
  $('export-verifier').disabled = !r.plan;
  const retryableCodes = new Set(['invalid_review_evidence','malformed_review','review_identity_mismatch','invalid_review_coverage','invalid_repair_request','missing_targeted_repair','inconsistent_review_verdict','timeout','network_error','provider_http_error','rate_limited','authentication_failed','malformed_provider_response','model_mismatch','response_too_large']);
  $('retry-verifier').hidden = r.mode !== 'live' || (r.p2_reviews || []).length >= 3 || !(r.status === 'awaiting_verifier' || (['blocked','needs_input'].includes(r.status) && retryableCodes.has(p2?.decision.receipt.error_code)));
  $('retry-verifier').disabled = !ready?.verifier_ready;
  $('stats').replaceChildren(...[`${r.residents.toLocaleString()} residents`,`${r.archetypes.length} archetypes`,`${r.round} / 2 repairs`,`${r.usage.requests + (r.p2_reviews || []).reduce((n, v) => n + v.decision.receipt.model_calls, 0)} model requests`].map(t => node('span',t,'stat')));
  let notice = r.error || (r.mode === 'demo' ? 'Illustrative fixture probabilities and a simulated verifier. This demonstrates execution and repair; it is not a live prediction.' : r.status === 'awaiting_verifier' ? 'Execution passed, but this run is not complete. Connect P2’s verifier and retry verification to reuse these results.' : 'Hypothetical model estimates. Only the stated assumptions are used; no live research or news is included.');
  if (embedded && !r.error) notice = r.mode === 'demo' ? 'Sample data · not a live prediction.' : r.status === 'awaiting_verifier' ? 'Awaiting independent verification.' : 'Model estimates, not observed behavior.';
  $('run-notice').hidden = !notice; $('run-notice').textContent = notice;
  $('scenarios').replaceChildren();
  if (!r.plan) $('scenarios').append(node('p','Preparing the experiment plan…','hint'));
  for (const s of r.plan?.scenarios || []) {
    const card = node('article',null,'scenario'), result = r.results.find(v => v.scenario_id === s.scenario_id);
    card.append(node('h3',s.description),node('p',s.question));
    const assumptions = node('ul'); s.assumptions.forEach(a => assumptions.append(node('li',a))); card.append(assumptions);
    if (result?.status === 'complete') for (const d of (embedded && result.distribution.length === 2 ? result.distribution.slice(0,1) : result.distribution)) {
      const row=node('div',null,'result'), bar=node('div',null,'bar'), fill=node('div',null,'fill');
      fill.style.width=`${Math.max(0,Math.min(100,d.probability*100))}%`; bar.append(fill);
      row.append(node('span',d.option,'label'),bar,node('strong',`${(d.probability*100).toFixed(1)}%`)); card.append(row);
    } else card.append(node('p',result?.error || 'Awaiting a complete scenario result.','hint'));
    if (result) card.append(node('p',`Attempt ${result.attempt} · ${result.answered_archetype_ids.length}/${result.expected_archetype_ids.length} archetypes answered`,'hint'));
    $('scenarios').append(card);
  }
  if (embedded) {
    $('assumptions').replaceChildren();
    for (const s of r.plan?.scenarios || []) {
      const list=node('ul'); s.assumptions.forEach(a=>list.append(node('li',a)));
      $('assumptions').append(node('h4',s.description),list);
    }
  }
  $('checks').replaceChildren();
  const findings=r.verifier?.checks || r.checks;
  if (!findings.length) $('checks').append(node('p','Checks run after all scenarios have been attempted.','hint'));
  findings.forEach(c => { const row=node('div',null,'finding'); row.title=c.evidence; row.append(node('span',c.status.toUpperCase(),`badge ${c.status}`),node('span',c.check_id.replaceAll('_',' '))); $('checks').append(row); });
  $('events').replaceChildren(...r.events.map(e => {const li=node('li',e.message);li.append(node('small',`${e.sequence.toString().padStart(2,'0')} · ${e.stage}`));return li;}));
  $('context').textContent=JSON.stringify({run_id:r.run_id,plan_hash:r.plan_hash,audience_hash:r.plan?.audience_hash,context_hash:r.plan?.context_hash,model:r.plan?.model,context:r.context,usage:r.usage,verification_receipts:(r.p2_reviews || []).map(v=>v.decision.receipt)},null,2);
}
async function history() {
  const runs = await api('/experiments'); $('history').replaceChildren();
  if (!runs.length) $('history').append(node('p','Your receipts will appear here.','hint'));
  runs.slice(0,6).forEach(r => {const b=node('button',r.goal);b.append(node('small',`${r.mode} · ${r.status.replaceAll('_',' ')}`));b.addEventListener('click',()=>select(r.run_id));$('history').append(b);});
}
async function select(id) {
  selected=id; clearTimeout(polling);
  try { localStorage.setItem(storageKey,id); } catch { /* optional persistence */ }
  await poll(id);
}
async function poll(id) {
  try {
    const r=await api(`/experiments/${id}`); if (selected!==id) return; render(r);
    if (!terminal.has(r.status)) polling=setTimeout(()=>poll(id),800); else await history();
  } catch(e) { if(selected!==id)return; if(e.status===404) { selected=null; $('connection').textContent='Saved run not found on this server. Start a new experiment.'; try { localStorage.removeItem(storageKey); } catch {} return; } $('connection').textContent=`Connection interrupted: ${e.message}. Retrying…`; polling=setTimeout(()=>poll(id),3000); }
}
let submitting = false;
async function submit(event) {
  event?.preventDefault();
  if (submitting) return;
  if (!ready || ($('mode').value === 'live' && !ready.live_ready)) { availability(); return; }
  submitting = true; $('start').disabled=true;
  try { beforeStart(); const city = getCity() || $('city').value; $('city').value = city; const r=await api('/experiments',{city,goal:$('goal').value,mode:$('mode').value,inject_missing:$('mode').value==='demo' && $('inject').checked,client_request_id:crypto.randomUUID()});await select(r.run_id);$('run').scrollIntoView({ block: 'start' });await history();}
  catch(e){$('connection').hidden=false;$('connection').textContent=e.message;}
  finally{submitting = false; $('start').disabled=!ready || ($('mode').value==='live'&&!ready.live_ready);}
}
$('experiment-form').addEventListener('submit',submit);
$('retry-verifier').addEventListener('click',async()=>{
  $('retry-verifier').disabled=true;
  try { const r=await api(`/experiments/${current.run_id}/verify`,{}); await select(r.run_id); }
  catch(e) { $('connection').hidden=false; $('connection').textContent=e.message; $('retry-verifier').disabled=false; }
});
$('cancel').addEventListener('click',async()=>{try{render(await api(`/experiments/${current.run_id}/cancel`,{}));await history();}catch(e){$('connection').textContent=e.message;}});
function download(value,name){const url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:'application/json'}));const a=node('a');a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
$('export').addEventListener('click',()=>{if(current)download(current,`silico-${current.run_id.slice(0,12)}.json`);});
$('export-verifier').addEventListener('click',async()=>{try{download(await api(`/experiments/${current.run_id}/verifier-request`),'silico-verifier-request.json');}catch(e){$('connection').textContent=e.message;}});
try {
  ready=await api('/experiments/config');
  const names={sf:'San Francisco',neu_york:'New York City',synth_la:'Los Angeles',cybercago:'Chicago',simami:'Miami'};
  $('city').replaceChildren(...ready.cities.map(id=>{const option=node('option',names[id]||id);option.value=id;return option;}));if(ready.cities.includes('sf'))$('city').value='sf';
  if (embedded) { $('city').disabled = true; $('city').value = getCity() || 'sf'; }
  availability();try { await history(); } catch {} let id;try{id=localStorage.getItem(storageKey);}catch{}if(id)await select(id);
} catch(e){ready=null;availability();}

return {
  async runGoal(goal, city) {
    selected=null; clearTimeout(polling); current=null; $('run').hidden=true; $('empty').hidden=embedded;
    if (embedded) { root.host.scrollTop=0; $('details').open=false; }
    $('mode').value = 'live'; mode(); $('goal').value = goal; $('city').value = city;
    if (!embedded) $('experiment-form').scrollIntoView({ block: 'nearest' });
    await submit();
  },
  async runDemo(city) {
    $('mode').value='demo'; mode(); $('city').value=city;
    if (embedded) { root.host.scrollTop=0; $('details').open=false; }
    await submit();
  },
  syncCity(city) { if (city) $('city').value = city; },
};
}
