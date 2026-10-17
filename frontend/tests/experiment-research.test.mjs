import test from 'node:test';
import assert from 'node:assert/strict';
import {sourceUrl,researchOutcome} from '../src/experiment-research.js';
test('research links reject executable, credential and local URLs',()=>{
  for(const url of ['javascript:alert(1)','data:text/html,hi','https://user:secret@example.com','http://127.0.0.1/','http://foo.local','http://foo.localhost']) assert.equal(sourceUrl(url),null);
  assert.equal(sourceUrl('https://example.org/evidence'),'https://example.org/evidence');
});
test('finding sources alone is never presented as a successful review',()=>{
  const run={research:{query:{query:'evidence'},web:{status:'ready'},trigger_request_hash:'original'},p2_reviews:[]};
  assert.match(researchOutcome(run),/Awaiting independent recheck/);
  run.p2_reviews=[{request:{web_research:{}},decision:{request_hash:'updated',verdict:'blocked',summary:'Sources do not address the gap'}}];
  assert.match(researchOutcome(run),/blocked.*do not address/);
  run.research.error='Search failed';
  assert.equal(researchOutcome(run),'Search failed');
});
