import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
const runtime = process.env.PLATO_SCHEMA_RUNTIME;
test('authoritative runtime reports parser and installer errors and clears on correction', {skip: !runtime}, () => {
  const bridge = readFileSync(new URL('../crates/plato-language-server/src/runtime-bridge.mjs', import.meta.url), 'utf8');
  const sources = [
    'model A { fields { name String } }',
    'model A { fields { name UnknownType } }',
    'model A { fields { name } }',
    'model A { fields { name String } }',
  ];
  const result = spawnSync('node', ['--input-type=module', '-e', bridge, runtime], {
    input: sources.map(source => JSON.stringify({ source })).join('\n')+'\n', encoding:'utf8', timeout:10000,
  });
  assert.equal(result.status, 0, result.stderr);
  const output = result.stdout.trim().split('\n').map(line => JSON.parse(line));
  assert.equal(output[0].errors.length, 0);
  assert.ok(output[1].errors.some(error => error.message.includes('UnknownType')));
  assert.ok(output[2].errors.some(error => error.line > 0));
  assert.equal(output[3].errors.length, 0);
  assert.ok(output[3].schema.models.some(model => model.name === 'A'));
});
