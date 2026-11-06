// Run with: node --test tools/pro_mk3_din_probe.test.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const { test } = require('node:test');

const html = fs.readFileSync(__dirname + '/pro_mk3_din_probe.html', 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)?.[1];
assert.ok(script, 'inline script is present');
new Function(script); // Syntax check the entire offline page.
const pure = script.match(/\/\/ BEGIN PURE PROTOCOL([\s\S]*?)\/\/ END PURE PROTOCOL/)?.[1];
assert.ok(pure, 'protocol parser is present');
const { parseProbeResponse, compareEdges } = vm.runInNewContext(
  pure + '; ({parseProbeResponse, compareEdges})'
);

test('accepts only Pro Mk3 diagnostic SysEx of the right ID, length and version', () => {
  const prefix = [0xf0, 0x7d, 0x43, 0x46, 0x57, 0x0e, 0x01, 0x70, 0x01, 0x28];
  const frame = Uint8Array.from([...prefix, ...Array(96).fill(0), 0xf7]);
  frame[10 + 3 * 16 + 2] = 7;
  assert.equal(parseProbeResponse(frame, 0x7d)[50], 7);
  assert.equal(parseProbeResponse(frame, 0x7c), null);
  assert.equal(parseProbeResponse(frame.slice(0, -1), 0x7d), null);
  frame[6] = 2;
  assert.equal(parseProbeResponse(frame, 0x7d), null);
  frame[6] = 1;
  frame[10] = 128;
  assert.equal(parseProbeResponse(frame, 0x7d), null);
});

test('reports pin-specific increases in descending order without treating zero as proof', () => {
  const baseline = Array(96).fill(0);
  const active = Array(96).fill(0);
  baseline[50] = 3; active[50] = 10; // GPIOD2
  active[16 + 1] = 2; // GPIOB1
  baseline[0] = 9; active[0] = 1; // decrease ignored
  const changes = compareEdges(baseline, active);
  assert.equal(changes[0].pin, 'GPIOD2');
  assert.equal(changes[0].delta, 7);
  assert.equal(changes[1].pin, 'GPIOB1');
  assert.equal(changes.length, 2);
  assert.throws(() => compareEdges([], []));
});

test('browser run needs only Pro Mk3 USB in/out and never sends a CC64', async () => {
  assert.doesNotMatch(html, /id="din-out"/);
  const elements = new Map();
  for (const id of ['connect', 'run', 'stop', 'copy', 'log', 'status', 'pro-in', 'pro-out', 'id']) {
    elements.set('#' + id, {
      value: id === 'pro-in' ? 'in' : id === 'pro-out' ? 'out' : id === 'id' ? '7D' : '',
      textContent: '', disabled: id === 'run' || id === 'stop' || id === 'copy',
      replaceChildren() {}, add() {}
    });
  }
  let receive;
  let requests = 0;
  const input = {
    name: 'PRO MK3 (MIDI)', manufacturer: 'Novation', id: 'in', open: async () => {},
    addEventListener(_type, fn) { receive = fn; }, removeEventListener() { receive = null; }
  };
  const output = {
    name: 'PRO MK3 (MIDI)', manufacturer: 'Novation', id: 'out', open: async () => {},
    send(bytes) {
      assert.deepEqual(Array.from(bytes), [0xf0, 0x7d, 0x43, 0x46, 0x57, 0x0e, 1, 0x30, 0xf7]);
      requests++;
      const edge = Array(96).fill(0);
      if (requests > 3) edge[50] = 2;
      receive({data: Uint8Array.from([0xf0, 0x7d, ...FIXED_IN_TEST, 0x70, 1, 0x28, ...edge, 0xf7])});
    }
  };
  const access = {inputs: new Map([['in', input]]), outputs: new Map([['out', output]])};
  let time = 0;
  const context = {
    document: {querySelector: selector => elements.get(selector)},
    navigator: {requestMIDIAccess: async () => access}, window: {isSecureContext: true},
    Option: class { constructor() {} },
    performance: {now: () => (time += 2500)},
    setTimeout: (fn, ms) => { if (ms < 2000) queueMicrotask(fn); return 1; },
    clearTimeout: () => {},
    console
  };
  vm.runInNewContext(script, context);
  await elements.get('#connect').onclick();
  await elements.get('#run').onclick();
  assert.ok(requests >= 4);
  assert.match(elements.get('#log').textContent, /GPIOD2: baseline max 0, active max 2, \+2/);
  assert.equal(elements.get('#copy').disabled, false);
});

const FIXED_IN_TEST = [0x43, 0x46, 0x57, 0x0e, 0x01];
