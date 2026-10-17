// Live comparison rendered on the existing map, with no fixture fallback.
export function previewArchetypes(agents, answers, question) {
  const byId=new Map(answers.flatMap(a=>a.member_ids.map(id=>[String(id),a])));
  return agents.map(agent=>{
    const answer=byId.get(String(agent.id));
    if(!answer)return {verdict:null,rationale:null};
    let hash=2166136261;
    for(const c of `${question}:${agent.id}`)hash=Math.imul(hash^c.charCodeAt(0),16777619);
    return {verdict:(hash>>>0)/4294967296<answer.p_yes?'yes':'no',rationale:answer.rationale};
  });
}
export function comparisonSummary(receipt) {
  const delta=(receipt.on.result.p_yes-receipt.off.result.p_yes)*100;
  if(receipt.reference_yes_share==null)return `${delta>=0?'+':''}${delta.toFixed(1)} pp change in modeled support. No reference outcome supplied; accuracy is unmeasured.`;
  const reduction=(Math.abs(receipt.off.result.p_yes-receipt.reference_yes_share)-Math.abs(receipt.on.result.p_yes-receipt.reference_yes_share))*100;
  return `${Math.abs(reduction).toFixed(1)} pp ${reduction>=0?'lower':'higher'} absolute error against the supplied reference. ${reduction<0?'Memory regressed on this comparison.':'One paired result; no general improvement claim.'}`;
}
export function comparisonError(error) {
  const message=error?.message || "The comparison could not finish.";
  if(/503:.*memory|memory.*not configured/i.test(message))return "City memory is not connected on this server. Connect it before comparing residents.";
  if(/404/.test(message))return "This server needs the memory comparison update. Restart it with the latest Silico code.";
  return message.replace(/^POST .*? → \d+:\s*/, "");
}
export function createMapMemoryComparison({request,getContext,onPreview,returnFocus}) {
  const host=document.createElement('section');host.className='map-experiment-panel';host.hidden=true;host.setAttribute('aria-label','Memory comparison results');document.body.append(host);
  const root=host.attachShadow({mode:'open'});
  root.innerHTML=`<style>
  :host{font:13px/1.5 -apple-system,BlinkMacSystemFont,sans-serif;color:#222}*{box-sizing:border-box}[hidden]{display:none!important}header{position:sticky;top:0;background:#fffffff0;backdrop-filter:blur(20px);display:flex;justify-content:space-between;align-items:center;padding:12px 18px;border-bottom:1px solid #0001}main{padding:14px 18px}h3,p{margin:0 0 10px}button{font:inherit;border:1px solid #0002;background:#f5f5f5;color:#333;border-radius:8px;padding:5px 10px;cursor:pointer}button[aria-pressed=true]{background:#333;color:white}button:focus-visible,summary:focus-visible{outline:2px solid #555;outline-offset:3px}.actions{display:flex;gap:7px;margin:12px 0;flex-wrap:wrap}table{width:100%;border-collapse:collapse;font-size:12px}th,td{text-align:left;border-bottom:1px solid #0001;padding:7px 4px}.hint,small{font-size:11px;color:#666}details{margin-top:12px}summary{cursor:pointer}article{padding:10px 0;border-bottom:1px solid #0001}pre{white-space:pre-wrap;overflow-wrap:anywhere;font:10px/1.5 monospace}#message{white-space:pre-wrap}#dismiss{border:0;background:transparent;font-size:20px;padding:0 6px}
  </style><header><strong>Memory comparison</strong><button id="dismiss" aria-label="Close memory comparison">×</button></header><main><p id="message" role="status" aria-live="polite"></p><div id="result" hidden><p id="question"></p><p id="summary"></p><table><thead><tr><th></th><th>Memory off</th><th>Memory on</th></tr></thead><tbody id="metrics"></tbody></table><div class="actions"><button id="off" aria-pressed="false">Show off on map</button><button id="on" aria-pressed="true">Show on on map</button><button id="download">Receipt</button></div><p class="hint">Dot colors sample each archetype's modeled probability. Census-weighted totals above are authoritative. Rationale bubbles are inherited from the resident's archetype.</p><details><summary>Resident evidence & decisions</summary><div id="answers"></div></details><details><summary>Frozen city memory</summary><pre id="snapshot"></pre></details><details><summary>Method & limitations</summary><div id="method"></div></details></div></main>`;
  const $=id=>root.getElementById(id),node=(tag,text)=>{const el=document.createElement(tag);el.textContent=text;return el;};
  let receipt=null,context=null;
  const size=()=>document.documentElement.style.setProperty('--experiment-dock-top',`${Math.round(document.getElementById('dock').getBoundingClientRect().top)}px`);
  new ResizeObserver(size).observe(document.getElementById('dock'));
  window.addEventListener('resize',size);
  function close(){host.hidden=true;returnFocus();}
  $('dismiss').addEventListener('click',close);
  root.addEventListener('keydown',event=>{event.stopPropagation();if(event.key==='Escape'){event.preventDefault();close();}});
  function preview(mode){if(!receipt)return;$('off').setAttribute('aria-pressed',String(mode==='off'));$('on').setAttribute('aria-pressed',String(mode==='on'));onPreview(receipt[mode].archetype_answers,receipt.off.result.question,context);}
  $('off').addEventListener('click',()=>preview('off'));$('on').addEventListener('click',()=>preview('on'));
  $('download').addEventListener('click',()=>{if(!receipt)return;const url=URL.createObjectURL(new Blob([JSON.stringify(receipt,null,2)],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download=`silico-memory-${receipt.snapshot_hash.slice(0,12)}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);});
  return {close,async run(question,reference,signal){
    context=getContext();receipt=null;host.hidden=false;size();$('result').hidden=true;$('message').textContent='Running two model passes on the same residents. City memory is frozen before either pass…';
    try {
      const data=await request(context.branch,{question,as_of_date:context.asOf,model:context.model,reference_yes_share:reference},signal);
      if(signal.aborted)return;
      receipt=data;$('message').textContent=`${context.city} · ${data.on.result.n_agents.toLocaleString()} residents · live population comparison`;
      $('question').textContent=data.off.result.question;$('summary').textContent=comparisonSummary(data);
      const rows=[['Modeled yes share',...['off','on'].map(m=>`${(data[m].result.p_yes*100).toFixed(1)}%`)],['Archetypes',data.off.result.n_archetypes,data.on.result.n_archetypes],['Model requests',data.off.result.n_llm_calls,data.on.result.n_llm_calls]];
      if(reference!=null)rows.push(['Absolute error',...['off','on'].map(m=>`${(data[m].absolute_error*100).toFixed(1)} pp`)]);
      $('metrics').replaceChildren(...rows.map(row=>{const tr=document.createElement('tr');row.forEach(v=>tr.append(node('td',v)));return tr;}));
      $('answers').replaceChildren();
      for(let i=0;i<data.on.archetype_answers.length;i++){
        const on=data.on.archetype_answers[i],off=data.off.archetype_answers[i],card=document.createElement('article');
        card.append(node('strong',`Representative #${on.representative_id} · ${on.member_ids.length} residents`),node('p',`Off ${(off.p_yes*100).toFixed(1)}%: ${off.rationale}`),node('p',`On ${(on.p_yes*100).toFixed(1)}%: ${on.rationale}`),node('small',on.memory_context||'No memory supplied to this archetype.'));$('answers').append(card);
      }
      $('snapshot').textContent=JSON.stringify({population_hash:data.population_hash,snapshot_hash:data.snapshot_hash,memory:data.memory_snapshot},null,2);
      $('method').replaceChildren(...data.limitations.map(text=>node('p',text)));
      $('result').hidden=false;preview('on');
    }catch(error){$('message').textContent=signal.aborted?'Comparison cancelled. No completed result.':`Comparison unavailable: ${comparisonError(error)}`;throw new Error(comparisonError(error));}
  }};
}
