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
const { parseProbeResponse, parseConfigResponse, parseUartResponse, compareEdges } = vm.runInNewContext(
  pure + '; ({parseProbeResponse, parseConfigResponse, parseUartResponse, compareEdges})'
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

test('configuration reply validates port, clock and four 7-bit fields per pin', () => {
  const frame = Uint8Array.from([0xf0, 0x7d, ...FIXED_IN_TEST, 0x71, 2, 1,
    ...Array(16).fill([3, 0, 0, 0]).flat(), 0xf7]);
  frame[10] = 2; frame[11] = 7; frame[12] = 1; frame[13] = 1;
  const config = parseConfigResponse(frame, 0x7d, 2);
  assert.equal(config.clock, true);
  assert.deepEqual(Array.from(config.pins[0]), [2, 7, 1, 1]);
  assert.equal(parseConfigResponse(frame, 0x7d, 3), null);
  frame[11] = 16;
  assert.equal(parseConfigResponse(frame, 0x7d, 2), null);
});

test('UART snapshot reads 32-bit register values without accepting wrong index', () => {
  const regs = [0x12345678, 0, 0x03a1, 0x28];
  const frame = Uint8Array.from([0xf0, 0x7d, ...FIXED_IN_TEST, 0x72, 1, 1,
    ...regs.flatMap(value => Array.from({length: 5}, (_, shift) => (value >>> (shift * 7)) & 127)), 0xf7]);
  const result = parseUartResponse(frame, 0x7d, 1);
  assert.equal(result.cr1, 0x12345678);
  assert.equal(result.brr, 0x03a1);
  assert.equal(result.isr, 0x28);
  assert.equal(parseUartResponse(frame, 0x7d, 2), null);
  frame[14] = 16;
  assert.equal(parseUartResponse(frame, 0x7d, 1), null);
});

test('browser run needs only Pro Mk3 USB in/out and never sends a CC64', async () => {
  assert.doesNotMatch(html, /id="din-out"/);
  const elements = new Map();
  for (const id of ['connect', 'run', 'stop', 'copy', 'log', 'status', 'pro-in', 'pro-out', 'id', 'control']) {
    elements.set('#' + id, {
      value: id === 'pro-in' ? 'in' : id === 'pro-out' ? 'out' : id === 'id' ? '7D' : '',
      textContent: '', disabled: id === 'run' || id === 'stop' || id === 'copy',
      replaceChildren() {}, add() {}
    });
  }
  let receive;
  let requests = 0;
  let configs = 0;
  let uarts = 0;
  const input = {
    name: 'PRO MK3 (MIDI)', manufacturer: 'Novation', id: 'in', open: async () => {},
    addEventListener(_type, fn) { receive = fn; }, removeEventListener() { receive = null; }
  };
  const output = {
    name: 'PRO MK3 (MIDI)', manufacturer: 'Novation', id: 'out', open: async () => {},
    send(bytes) {
      assert.deepEqual(Array.from(bytes.slice(0, 7)), [0xf0, 0x7d, ...FIXED_IN_TEST]);
      if (bytes[7] === 0x31) {
        assert.deepEqual(Array.from(bytes.slice(8)), [configs++, 0xf7]);
        receive({data: Uint8Array.from([0xf0, 0x7d, ...FIXED_IN_TEST, 0x71, bytes[8], 1,
          ...Array(16).fill([3, 0, 0, 0]).flat(), 0xf7])});
        return;
      }
      if (bytes[7] === 0x32) {
        assert.deepEqual(Array.from(bytes.slice(8)), [uarts % 4, 0xf7]);
        const after = uarts >= 4;
        uarts++;
        const regs = [0, 0, 0, after && bytes[8] === 1 ? 8 : 0];
        receive({data: Uint8Array.from([0xf0, 0x7d, ...FIXED_IN_TEST, 0x72, bytes[8], 1,
          ...regs.flatMap(value => [value, 0, 0, 0, 0]), 0xf7])});
        return;
      }
      assert.deepEqual(Array.from(bytes.slice(7)), [0x30, 0xf7]);
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
  assert.equal(configs, 6);
  assert.equal(uarts, 8);
  assert.match(elements.get('#log').textContent, /UART7: RXNE 0→0, ORE 0→1, FE 0→0/);
  assert.match(elements.get('#log').textContent, /GPIOD2: baseline max 0, active max 2, \+2/);
  assert.equal(elements.get('#copy').disabled, false);
});

const FIXED_IN_TEST = [0x43, 0x46, 0x57, 0x0e, 0x01];
