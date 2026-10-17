import test from 'node:test';
import assert from 'node:assert/strict';
import {previewArchetypes,comparisonSummary} from '../src/map-memory-comparison.js';
test('map preview uses the matching resident archetype, not random sample rationales',()=>{
  const agents=[{id:1},{id:2},{id:3}];
  const answers=[{member_ids:[2],p_yes:1,rationale:'Group two'},{member_ids:[1],p_yes:0,rationale:'Group one'}];
  assert.deepEqual(previewArchetypes(agents,answers,'test'),[{verdict:'no',rationale:'Group one'},{verdict:'yes',rationale:'Group two'},{verdict:null,rationale:null}]);
  assert.deepEqual(previewArchetypes(agents,answers,'test'),previewArchetypes(agents,answers,'test'));
});
test('no reference means no accuracy claim; worse results are labeled as regression',()=>{
  const result={off:{result:{p_yes:0.2}},on:{result:{p_yes:0.8}},reference_yes_share:null};
  assert.match(comparisonSummary(result),/accuracy is unmeasured/);
  result.reference_yes_share=0.1;
  assert.match(comparisonSummary(result),/regressed/);
  result.reference_yes_share=0.9;
  assert.match(comparisonSummary(result),/lower/);
});
test('connection errors explain the action without exposing endpoint details',async()=>{
  const {comparisonError}=await import('../src/map-memory-comparison.js');
  assert.match(comparisonError(new Error('POST /branches/id/memory-comparison → 503: persona memory (Neo4j) is not configured')),/City memory is not connected/);
  assert.doesNotMatch(comparisonError(new Error('POST /branches/id/memory-comparison → 502: No dated memories are available.')),/branches/);
});
