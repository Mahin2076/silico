import test from 'node:test';
import assert from 'node:assert/strict';
import { loadResidents } from '../src/resident-loader.js';
test('loads every resident in order despite out-of-order replies; at most four requests overlap', async () => {
  let active=0, peak=0; const progress=[];
  const result=await loadResidents(async (offset, limit) => {
    active++; peak=Math.max(peak,active);
    await new Promise(resolve=>setTimeout(resolve, 5-offset%4));
    active--;
    return {total_matched:10250,agents:Array.from({length:Math.min(limit,10250-offset)},(_,i)=>({id:offset+i,pums_weight:i+1}))};
  }, (count,total)=>progress.push([count,total]));
  assert.equal(result.length,10250);
  assert.deepEqual(result.map(a=>a.id),Array.from({length:10250},(_,i)=>i));
  assert.equal(peak,4);
  assert.equal(result[0].pums_weight,1);
  assert.deepEqual(progress.at(-1),[10250,10250]);
  assert.ok(progress.every((p,i)=>!i || p[0]>progress[i-1][0]));
});
test('refuses incomplete or changing populations rather than returning a misleading partial sample', async () => {
  await assert.rejects(loadResidents(async offset=>({total_matched:1200,agents:Array(offset?100:1000).fill({})})),/incomplete/);
  await assert.rejects(loadResidents(async offset=>({total_matched:offset?1300:1200,agents:Array(offset?200:1000).fill({})})),/changed/);
  await assert.rejects(loadResidents(async()=>({total_matched:60000,agents:[]})),/exceeds/);
  assert.deepEqual(await loadResidents(async()=>({total_matched:0,agents:[]})),[]);
});
