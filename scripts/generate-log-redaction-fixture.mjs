#!/usr/bin/env node
// Writes crates/noct-core/tests/fixtures/log-redaction.json: the outputs of the redaction in
// electron/logging/Logger.ts for a fixed set of inputs. The vectors are the ones of
// tests/security.test.ts, plus shapes of the same patterns and the limits of the redaction.
// The TypeScript source is only read and compiled into node_modules/.cache; nothing under electron/
// changes. The Rust port is checked against this file by crates/noct-core/tests/log_redaction.rs.
//
// Usage:
//   node scripts/generate-log-redaction-fixture.mjs           write the fixture
//   node scripts/generate-log-redaction-fixture.mjs --check   fail if the fixture is out of date

import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(path.join(root, 'package.json'))
const ts = require('typescript')
const source = path.join(root, 'electron', 'logging', 'Logger.ts')
const buildDirectory = path.join(root, 'node_modules', '.cache', 'noct-log-redaction')
const fixture = path.join(root, 'crates', 'noct-core', 'tests', 'fixtures', 'log-redaction.json')

const TEXT = [
  // tests/security.test.ts
  'Authorization: Bearer segredo-super-secreto',
  'authorization=Basic YWxhZGRpbjpvcGVuc2VzYW1l',
  'token="valor com espaços"',
  '{"api_key":"chave-json","ok":true}',
  // Other shapes of the same patterns
  'sk-abcdefghijklmnopqrstuvwxyz0123456789',
  `ghp_${'a'.repeat(36)}`,
  'AKIAABCDEFGHIJKLMNOP',
  'eyJhbGciOiJIUzI1.eyJzdWIiOiIxMjM0.c2lnbmF0dXJlMTIz',
  // Private key headers. Where the port and TypeScript differ, DIVERGENCES below records it.
  '-----BEGIN PRIVATE KEY-----',
  '-----BEGIN RSA PRIVATE KEY-----',
  '-----BEGIN DSA PRIVATE KEY-----',
  '-----BEGIN EC PRIVATE KEY-----',
  '-----BEGIN OPENSSH PRIVATE KEY-----',
  '-----BEGIN RSA KEY-----',
  '-----BEGIN PUBLIC KEY-----',
  'password: senha-sem-aspas',
  'message="mensagem com dados"',
  "prompt: 'pedido privado'",
  '{"content":"arquivo completo"}',
  'é token="segredo-acentuado"',
  // JavaScript's \b and /i are ASCII: a non-ASCII letter is not a word character, and the long s and
  // the Kelvin sign never fold onto s and k.
  'éToken="segredo-colado"',
  'ſecret="x-segredo-longo"',
  'api_Key="chave-kelvin-longa"',
  'Nenhum dado sensível aqui.',
  // The limit counts UTF-16 units: 4500 emoji are 9000 units, cut at 4000 of them. The cut never
  // splits a surrogate pair here, so the output is well-formed.
  'x'.repeat(9_000),
  '😀'.repeat(3_000),
  '😀'.repeat(4_500),
]

const VALUE = [
  // tests/security.test.ts
  { authorization: 'Bearer segredo', prompt: 'pedido privado', content: 'arquivo completo', error: 'falha com dados privados', nested: { password: 'senha', safe: 'ok' } },
  // Keys are dropped when a sensitive name appears anywhere in them, in any case
  { API_KEY: 'chave', Token: 'x', safe: 'ok' },
  { line: 'x', underline: 'y', ok: 1 },
  // Containers from depth 5 on become [TRUNCATED]; strings are redacted at any depth
  { deep: { a: { b: { c: { d: { e: 'x' } } } } } },
  // Arrays and objects keep their first 50 items
  Array.from({ length: 60 }, (_, index) => index),
  Object.fromEntries(Array.from({ length: 60 }, (_, index) => [`key${index}`, index])),
  { list: ['token="abc-segredo-xyz"', 7, null, true, false] },
  [{ note: 'token="x-segredo-longo"', stdout: 'saida' }],
  // Scalars at the top level
  'Bearer segredo-xyz-123',
  42,
  null,
  true,
]

function loadLogger() {
  const { outputText } = ts.transpileModule(fs.readFileSync(source, 'utf8'), {
    fileName: source,
    compilerOptions: {
      module: ts.ModuleKind.CommonJS,
      target: ts.ScriptTarget.ES2022,
      esModuleInterop: true,
    },
  })
  fs.mkdirSync(buildDirectory, { recursive: true })
  const output = path.join(buildDirectory, 'Logger.js')
  fs.writeFileSync(output, outputText)
  return require(output)
}

// Intentional divergences from TypeScript, decided by the owner. The TypeScript output stays as `output`;
// the Rust port must give `rustOutput`, and crates/noct-core/tests/log_redaction.rs checks it. The TypeScript
// code is not changed by this record: its fix is a separate decision.
const PRIVATE_KEY_DIVERGENCE = {
  rustOutput: '[REDACTED-TOKEN]',
  reason: 'TypeScript needs KEY right after the algorithm, so RSA, DSA, EC and OPENSSH private key headers are not redacted there.',
}
const DIVERGENCES = new Map([
  ['-----BEGIN RSA PRIVATE KEY-----', PRIVATE_KEY_DIVERGENCE],
  ['-----BEGIN DSA PRIVATE KEY-----', PRIVATE_KEY_DIVERGENCE],
  ['-----BEGIN EC PRIVATE KEY-----', PRIVATE_KEY_DIVERGENCE],
  ['-----BEGIN OPENSSH PRIVATE KEY-----', PRIVATE_KEY_DIVERGENCE],
])

function textCase(input, redactLogText) {
  const output = redactLogText(input)
  const divergence = DIVERGENCES.get(input)
  if (!divergence) return { input, output }
  // A recorded divergence that TypeScript no longer has would be a stale record: refuse it.
  if (divergence.rustOutput === output) {
    throw new Error(`${JSON.stringify(input)} no longer diverges from TypeScript: remove it from DIVERGENCES`)
  }
  return { input, output, rustOutput: divergence.rustOutput, divergence: divergence.reason }
}

function generate() {
  const { redactLogText, redactLogValue } = loadLogger()
  const fixtureContent = {
    source: 'electron/logging/Logger.ts',
    text: TEXT.map((input) => textCase(input, redactLogText)),
    value: VALUE.map((input) => ({ input, output: redactLogValue(input) })),
  }
  return `${JSON.stringify(fixtureContent, null, 2)}\n`
}

const content = generate()
if (process.argv.includes('--check')) {
  const current = fs.existsSync(fixture) ? fs.readFileSync(fixture, 'utf8') : null
  if (current !== content) {
    console.error(`${path.relative(root, fixture)} is out of date. Run: node scripts/generate-log-redaction-fixture.mjs`)
    process.exitCode = 1
  } else {
    console.log(`${path.relative(root, fixture)} is up to date.`)
  }
} else {
  fs.mkdirSync(path.dirname(fixture), { recursive: true })
  fs.writeFileSync(fixture, content)
  console.log(`wrote ${path.relative(root, fixture)} (${TEXT.length} text and ${VALUE.length} value vectors)`)
}
