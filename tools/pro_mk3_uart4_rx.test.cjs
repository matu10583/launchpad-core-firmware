const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {test} = require('node:test');

const html = fs.readFileSync(__dirname + '/pro_mk3_uart4_rx.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert.ok(script);
new Function(script);
const pure = script.match(/\/\/ BEGIN PURE PROTOCOL([\s\S]*?)\/\/ END PURE PROTOCOL/)?.[1];
assert.ok(pure);
const parse = vm.runInNewContext(pure + '; parseDinResponse');
const frame = (status, bytes, fe=0, ore=0) => Uint8Array.from([
  0xf0,0x7d,0x43,0x46,0x57,0x0e,0x01,0x73,status,bytes.length,fe,ore,
  ...bytes.flatMap(b => [b >> 4,b & 15]),0xf7
]);

test('reconstructs MIDI status and CC64, including the high status bit', () => {
  const result = parse(frame(0,[0xb0,64,127,0xb0,64,0]),0x7d);
  assert.deepEqual(Array.from(result.data),[0xb0,64,127,0xb0,64,0]);
  assert.equal(result.fe,0);
  assert.equal(result.ore,0);
});

test('accepts empty busy reply and error/overflow counters', () => {
  assert.equal(parse(frame(1,[]),0x7d).status,1);
  const result = parse(frame(2,[0xfe],3,4),0x7d);
  assert.equal(result.data[0],0xfe);
  assert.equal(result.fe,3);
  assert.equal(result.ore,4);
});

test('rejects wrong ID, truncated, non-nibble and excess data', () => {
  const valid = frame(0,[0xb0,64,127]);
  assert.equal(parse(valid,0x7c),null);
  assert.equal(parse(valid.slice(0,-1),0x7d),null);
  valid[12] = 16;
  assert.equal(parse(valid,0x7d),null);
  assert.equal(parse(frame(0,Array(33).fill(0)),0x7d),null);
});

test('requests only the opt-in UART4 capture; does not send CC64', () => {
  assert.match(html,/0x33,0xf7/);
  assert.doesNotMatch(html,/\.send\(\[0xb0/);
});
