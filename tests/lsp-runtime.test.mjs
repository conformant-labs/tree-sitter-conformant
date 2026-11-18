import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const runtime = process.env.PLATO_SCHEMA_RUNTIME;
test('LSP diagnostics, rename and seed formatting use the actual schema runtime', { skip: !runtime, timeout: 30000 }, async () => {
  const child = spawn(resolve('target/debug/plato-language-server'));
  let buffer = Buffer.alloc(0), nextId = 1;
  const pending = new Map(), notifications = [];
  child.stdout.on('data', chunk => {
    buffer = Buffer.concat([buffer, chunk]);
    while (true) {
      const end = buffer.indexOf('\r\n\r\n');
      if (end < 0) return;
      const length = Number(/Content-Length: (\d+)/i.exec(buffer.subarray(0,end).toString())[1]);
      if (buffer.length < end+4+length) return;
      const message = JSON.parse(buffer.subarray(end+4,end+4+length));
      buffer = buffer.subarray(end+4+length);
      if (message.id) { pending.get(message.id)?.(message); pending.delete(message.id); }
      else notifications.push(message);
    }
  });
  const send = message => {
    const body = JSON.stringify({jsonrpc:'2.0', ...message});
    child.stdin.write('Content-Length: '+Buffer.byteLength(body)+'\r\n\r\n'+body);
  };
  const request = (method, params) => new Promise(resolve => {
    const id = nextId++; pending.set(id, resolve); send({id,method,params});
  });
  const notify = (method, params) => send({method,params});
  const uri = 'file:///tmp/plato-lsp-test.plato', textDocument = {uri};
  let version = 0;
  const open = async text => {
    notify('textDocument/didOpen',{textDocument:{uri,languageId:'plato',version:++version,text}});
    await request('textDocument/foldingRange',{textDocument});
    return notifications.filter(n => n.method === 'textDocument/publishDiagnostics').at(-1).params.diagnostics;
  };
  try {
    await request('initialize',{capabilities:{},initializationOptions:{schemaRuntimePath:runtime}});
    notify('initialized',{});
    assert.ok((await open('model A { fields { value UnknownType } }')).some(d => d.severity === 1));
    assert.deepEqual(await open('model A { fields { value String } }'), []);
    const rename = await request('textDocument/rename',{textDocument,position:{line:0,character:22},newName:'name'});
    assert.ok(rename.result.documentChanges[0].edits.length > 0, JSON.stringify(rename));
    const {parsePlatoWithErrors} = await import(pathToFileURL(resolve(runtime,'src/parse.ts')));
    const seedRoot = resolve(runtime,'../plato-seeds');
    const files = (await readdir(seedRoot,{recursive:true})).filter(p => p.endsWith('.plato'));
    assert.ok(files.length > 0);
    for (const file of files) {
      const source = await readFile(resolve(seedRoot,file),'utf8');
      await open(source);
      const response = await request('textDocument/formatting',{textDocument,options:{tabSize:2,insertSpaces:true}});
      assert.ok(!response.error, file+': '+JSON.stringify(response.error));
      const formatted = response.result[0]?.newText ?? source;
      assert.deepEqual(parsePlatoWithErrors(formatted),parsePlatoWithErrors(source),file);
      await open(formatted);
      const second = await request('textDocument/formatting',{textDocument,options:{tabSize:2,insertSpaces:true}});
      assert.deepEqual(second.result,[],file+' must be idempotent');
    }
    await request('shutdown',null); notify('exit',null);
  } finally { child.stdin.end(); child.kill(); }
});
