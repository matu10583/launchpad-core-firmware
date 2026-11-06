const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {test} = require('node:test');

const html = fs.readFileSync(__dirname + '/pro_mk3_din_monitor.html','utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert.ok(script);
new Function(script);
const pure = script.match(/\/\/ BEGIN PURE PROTOCOL([\s\S]*?)\/\/ END PURE PROTOCOL/)?.[1];
const cc64 = vm.runInNewContext(pure + '; cc64');

test('identifies both pedal edges on any MIDI channel', () => {
  assert.deepEqual(Object.values(cc64([0xb0,64,127])),[1,127,true]);
  assert.deepEqual(Object.values(cc64([0xb3,64,0])),[4,0,false]);
  assert.equal(cc64([0x90,64,127]),null);
  assert.equal(cc64([0xb0,63,127]),null);
  assert.equal(cc64([0xb0,64]),null);
});

test('monitor opens only input and never sends MIDI or SysEx', () => {
  assert.match(html,/requestMIDIAccess\(\)/);
  assert.doesNotMatch(script,/\.send\(/);
  assert.doesNotMatch(html,/id="output"/);
});
