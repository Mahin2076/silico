import test from 'node:test';
import assert from 'node:assert/strict';
import { planQuestion } from '../src/question-plan.js';

test('user questions select the existing runner without a model call', () => {
  for (const [question, mode] of [
    ['Would people buy this lunch?', 'predict'],
    ['Would $8, $10, or $12 work better?', 'experiment'],
    ['Compare free transit versus a $2 fare', 'experiment'],
    ['Which ad would work better?', 'ab'],
    ['Does remembering yesterday’s event change their response?', 'memory'],
    ['Compare memory on vs off: Would residents support the transit proposal?', 'memory'],
    ['Show the age distribution', 'data'],
    ['How many adults live here?', 'data'],
    ['Compare population by age', 'data'],
    ['What if transit became free?', 'experiment'],
    ['What percentage of residents would buy lunch?', 'predict'],
    ['Which candidate would residents vote for?', 'predict'],
    ['Would residents respond to this message?', 'predict'],
  ]) assert.equal(planQuestion(question).mode, mode, question);
});
test('quoted or labeled message variants retain exact content; absent ads are not invented', () => {
  assert.deepEqual(planQuestion('Which ad works better: “Fresh lunch for $8” or “Lunch made nearby”?').variants, ['Fresh lunch for $8','Lunch made nearby']);
  assert.deepEqual(planQuestion('Compare messages. A: Take the train.\nB: Ride together.').variants, ['Take the train.','Ride together.']);
  assert.equal(planQuestion('Which ad would work better?').variants, null);
  assert.ok(planQuestion('Compare ads "a", "b", and "c"').clarification);
});
test('memory comparison keeps the outcome separate and unsupported combinations ask for clarification', () => {
  assert.equal(planQuestion('Compare memory on vs off: Would residents support transit?').outcome, 'Would residents support transit?');
  assert.equal(planQuestion('Does remembering yesterday change their response?').outcome, '');
  assert.ok(planQuestion('Compare memory on and off at $8 and $12').clarification);
  assert.ok(planQuestion('Compare $8, $10, $12, and $14 lunches').clarification);
  assert.ok(planQuestion('Compare four prices').clarification);
});
test('plans describe capabilities without promising verification of legacy prediction or A/B', () => {
  assert.match(planQuestion('Compare three lunch prices').description, /independently verify/);
  assert.doesNotMatch(planQuestion('Which ad works better?').description, /independently verify/);
  assert.match(planQuestion('Compare memory on vs off').description, /Accuracy needs an observed reference/);
  assert.equal(planQuestion('').label, 'Ask anything');
});
