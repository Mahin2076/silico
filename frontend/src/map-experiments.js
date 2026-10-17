import { mountExperiments } from './experiments.js';

// A compact result card above the existing composer. The map stays mounted.
export function createMapExperiments({ getCity, validateAudience, returnFocus }) {
  const host = document.createElement('section');
  host.id = 'map-experiment-panel';
  host.className = 'map-experiment-panel';
  host.hidden = true;
  host.setAttribute('role', 'region');
  host.setAttribute('aria-label', 'Experiment results on the map');
  document.body.append(host);
  const root = host.attachShadow({ mode: 'open' });
  let loading;
  const dock = document.getElementById('dock');
  const size = () => document.documentElement.style.setProperty('--experiment-dock-top', `${Math.round(dock.getBoundingClientRect().top)}px`);
  new ResizeObserver(size).observe(dock);
  window.addEventListener('resize',size);
  function close() {
    host.hidden = true;
    document.body.classList.remove('experiment-panel-open');
    document.getElementById('experiment-results').setAttribute('aria-expanded', 'false');
    returnFocus();
  }
  async function load() {
    // This is a static local template; all run/model content is rendered as text.
    root.innerHTML = `
      <link rel="stylesheet" href="${new URL('../map-experiments.css', import.meta.url).href}">
      <header><strong>Experiment</strong><button id="dismiss" aria-label="Close experiment results">×</button></header>
      <main>
        <p id="connection" role="status"></p>
        <form id="experiment-form" hidden>
          <select id="mode"><option value="demo">Demo</option><option value="live">Live</option></select>
          <p id="mode-note"></p><select id="city"></select><textarea id="goal"></textarea>
          <label id="inject-label"><input id="inject" type="checkbox" checked></label><button id="start">Run</button>
        </form>
        <div id="empty"><p>Type your goal in the map’s input box, or try the sample.</p></div>
        <div id="run" hidden>
          <div class="status-row"><span id="run-mode" class="badge"></span><h2 id="status" aria-live="polite"></h2><button id="cancel" hidden>Cancel</button></div>
          <p id="run-goal"></p><p id="run-notice" class="notice" hidden></p>
          <div id="scenarios"></div>
        </div>
        <details id="details">
          <summary>Details & saved runs</summary>
          <div id="stats"></div><p id="verifier-summary" hidden></p>
          <h3>Assumptions</h3><div id="assumptions"></div>
          <h3>Completion checks</h3><div id="checks"></div>
          <h3>Run history</h3><ol id="events"></ol>
          <div class="actions"><button id="retry-verifier" class="secondary" hidden>Retry verification</button><button id="export">Download receipt</button><button id="export-verifier" disabled>Verifier input</button></div>
          <details><summary>Context</summary><pre id="context"></pre></details>
          <h3>Saved runs</h3><div id="history"></div>
        </details>
      </main>`;
    root.getElementById('dismiss').addEventListener('click',close);
    root.addEventListener('keydown',event => {
      event.stopPropagation();
      if(event.key==='Escape'){event.preventDefault();close();}
    });
    return mountExperiments(root,{embedded:true,getCity,beforeStart:validateAudience});
  }
  async function open() {
    host.hidden = false;
    document.body.classList.add('experiment-panel-open');
    document.getElementById('experiment-results').setAttribute('aria-expanded','true');
    size();
    if(!loading) loading=load().catch(error=>{loading=null;close();throw error;});
    const panel=await loading; panel.syncCity(getCity()); return panel;
  }
  return {
    open, close,
    async runGoal(goal){validateAudience();const city=getCity();const panel=await open();await panel.runGoal(goal,city);},
    async demo(){validateAudience();const panel=await open();await panel.runDemo(getCity());},
  };
}
