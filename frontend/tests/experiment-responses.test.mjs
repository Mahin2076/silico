import test from 'node:test';
import assert from 'node:assert/strict';
import { experimentResponses } from '../src/experiment-responses.js';
test('scenario dots use exact resident membership and saved probabilities, never invented quotes', () => {
  const run = {run_id:'abc',mode:'live',plan:{scenarios:[{scenario_id:'a',description:'$8 lunch',options:['Buy','Skip']},{scenario_id:'b',description:'$12 lunch',options:['Buy','Skip']}]},archetypes:[{id:'g1',member_ids:[7]},{id:'g2',member_ids:[9]}],answers:{a:[{archetype_id:'g1',probabilities:[1,0]},{archetype_id:'g2',probabilities:[0,1]}],b:[{archetype_id:'g1',probabilities:[0,1]}]}};
  const agents=[{id:9},{id:7},{id:999}];
  const a=experimentResponses(agents,run,'a');
  assert.deepEqual(a.map(x=>x.verdict),['no','yes',null]);
  assert.match(a[1].rationale,/Buy: 100%/);
  assert.match(a[1].rationale,/not an individual interview/);
  assert.deepEqual(experimentResponses(agents,run,'a'),a);
  assert.deepEqual(experimentResponses(agents,run,'b').map(x=>x.verdict),[null,'no',null]);
});
