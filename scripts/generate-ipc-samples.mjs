#!/usr/bin/env node
// Writes the IPC contract samples of the first slice to crates/noct-core/tests/fixtures/ipc. They are the
// TypeScript side of the parity that crates/noct-core/tests/ipc_contracts.rs checks on the Rust side.
//
// Inputs: the zod schema `settingsSchema` of electron/ipc/registerSettingsIpc.ts decides which candidates
// are valid, and its `parse` output is the expected output. The file is transpiled and loaded here; the
// only stubs are the Electron import of its neighbour safeIpc, which the schema does not use.
//
// Outputs: there is no zod schema for the outputs. The TypeScript compiler decides them: a valid sample
// must type-check against `AppSettings`, `Workspace[]` or `Conversation[]` in shared/types.ts, and an
// invalid sample must not.
//
// Usage:
//   node scripts/generate-ipc-samples.mjs           write the samples
//   node scripts/generate-ipc-samples.mjs --check   fail when the samples on disk are out of date

import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const requireFromRoot = createRequire(path.join(root, 'package.json'))
const ts = requireFromRoot('typescript')
const outputDirectory = path.join(root, 'crates', 'noct-core', 'tests', 'fixtures', 'ipc')
const cacheDirectory = path.join(root, 'node_modules', '.cache', 'noct-ipc-samples')
const settingsFile = path.join(root, 'electron', 'ipc', 'registerSettingsIpc.ts')
const typesFile = path.join(root, 'shared', 'types.ts')

const stubs = new Map([
  // The handler's neighbour imports Electron at run time. The schema does not use it.
  [path.join(root, 'electron', 'ipc', 'safeIpc.ts'), { safeIpcMain: () => undefined }],
])

function transpile(file) {
  return ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    fileName: file,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true },
  }).outputText
}

function loadTypeScriptModule(file, loaded = new Map()) {
  if (stubs.has(file)) return stubs.get(file)
  if (loaded.has(file)) return loaded.get(file).exports
  const module = { exports: {} }
  loaded.set(file, module)
  const localRequire = (specifier) => {
    if (specifier.startsWith('.')) return loadTypeScriptModule(path.resolve(path.dirname(file), `${specifier}.ts`), loaded)
    return requireFromRoot(specifier)
  }
  new Function('require', 'module', 'exports', transpile(file))(localRequire, module, module.exports)
  return module.exports
}

// The candidates decide the coverage: each one is valid or invalid by the schema, not by this list.
const SETTINGS_CANDIDATES = [
  {},
  { theme: 'dark' },
  { theme: 'light' },
  { language: 'pt-BR' },
  { language: 'en' },
  { approvalPolicy: 'untrusted' },
  { approvalPolicy: 'on-request' },
  { diagnosticMode: true },
  { diagnosticMode: false },
  { approvalPolicy: 'untrusted', diagnosticMode: true, theme: 'light', language: 'en' },
  null,
  [],
  'dark',
  42,
  true,
  { theme: 'blue' },
  { language: 'fr' },
  { approvalPolicy: 'always' },
  { diagnosticMode: 'true' },
  { diagnosticMode: 1 },
  { theme: null },
  { diagnosticMode: null },
  { language: null },
  { approvalPolicy: null },
  { theme: 'Dark' },
  { theme: ' dark' },
  { unknown: true },
  { theme: 'dark', extra: 1 },
  { approvalPolicy: ['untrusted'] },
  { diagnosticMode: [] },
  JSON.parse('{"__proto__":{"theme":"dark"}}'),
]

const WORKSPACE_ALPHA = {
  path: '/work/alpha',
  name: 'alpha',
  favorite: true,
  authorized: true,
  availability: 'available',
  createdAt: '2026-01-01T00:00:00.000Z',
  lastOpenedAt: '2026-01-02T00:00:00.000Z',
}

const OUTPUT_SAMPLES = {
  'settings-get': {
    type: 'AppSettings',
    valid: [
      { approvalPolicy: 'on-request', diagnosticMode: false, theme: 'dark', language: 'pt-BR' },
      { approvalPolicy: 'untrusted', diagnosticMode: true, theme: 'light', language: 'en', pandocVersion: '3.1.2' },
      { approvalPolicy: 'on-request' },
    ],
    invalid: [
      { approvalPolicy: 'always' },
      { theme: 'dark' },
      { approvalPolicy: 'on-request', language: 'fr' },
      { approvalPolicy: 'on-request', diagnosticMode: 'true' },
      { approvalPolicy: 'on-request', theme: null },
      { approvalPolicy: 'on-request', pandocVersion: 3 },
    ],
  },
  // `workspaces_list` returns a page (`WorkspacePage` in noct-core). The page type is written here, because the
  // shared types have the list only; the items are checked against `Workspace` as before.
  'workspaces-list': {
    type: '{ items: Workspace[]; hasMore: boolean; nextCursor: { favorite: boolean; lastOpenedAt: string; path: string } | null }',
    valid: [
      { items: [], hasMore: false, nextCursor: null },
      { items: [WORKSPACE_ALPHA], hasMore: false, nextCursor: null },
      {
        items: [
          {
            path: '/work/beta',
            name: 'beta',
            favorite: false,
            authorized: false,
            availability: 'missing',
            availabilityMessage: 'A pasta não existe.',
            createdAt: '2026-01-01T00:00:00.000Z',
            lastOpenedAt: '2026-01-02T00:00:00.000Z',
          },
        ],
        hasMore: true,
        nextCursor: { favorite: false, lastOpenedAt: '2026-01-02T00:00:00.000Z', path: '/work/beta' },
      },
    ],
    invalid: [
      { items: [{ ...WORKSPACE_ALPHA, availability: 'online' }], hasMore: false, nextCursor: null },
      { items: [{ path: '/work/alpha', name: 'alpha', favorite: true, availability: 'available', createdAt: '2026-01-01T00:00:00.000Z', lastOpenedAt: '2026-01-02T00:00:00.000Z' }], hasMore: false, nextCursor: null },
      { items: [{ ...WORKSPACE_ALPHA, availabilityMessage: null }], hasMore: false, nextCursor: null },
      { items: [{ ...WORKSPACE_ALPHA, favorite: 'yes' }], hasMore: false, nextCursor: null },
      { items: [], hasMore: 'no', nextCursor: null },
      { items: [] },
      { items: [], hasMore: false },
      { items: [], hasMore: false, nextCursor: null, offset: 0 },
      { items: [], hasMore: true, nextCursor: { favorite: false, lastOpenedAt: '2026-01-02T00:00:00.000Z' } },
      { items: [], hasMore: true, nextCursor: { favorite: 'no', lastOpenedAt: '2026-01-02T00:00:00.000Z', path: '/work/beta' } },
      { items: [], hasMore: true, nextCursor: { favorite: false, lastOpenedAt: '2026-01-02T00:00:00.000Z', path: '/work/beta', id: 'x' } },
    ],
  },
  'conversations-list': {
    type: 'Conversation[]',
    valid: [
      [],
      [{ id: '7d1a4c2e-6b1f-4f7e-9a55-1c2d3e4f5a6b', title: 'Revisão', workspace: '/work/alpha', createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-02T00:00:00.000Z' }],
    ],
    invalid: [
      [{ id: '7d1a4c2e-6b1f-4f7e-9a55-1c2d3e4f5a6b', title: 'Revisão', workspace: '/work/alpha' }],
      [{ id: 1, title: 'Revisão', workspace: '/work/alpha', createdAt: '2026-01-01T00:00:00.000Z', updatedAt: '2026-01-02T00:00:00.000Z' }],
    ],
  },
}

/** Type-checks every output sample in one program. Returns the diagnostics per sample, by its key. */
function checkOutputs() {
  fs.mkdirSync(cacheDirectory, { recursive: true })
  const samplesFile = path.join(cacheDirectory, 'samples.ts')
  const typesImport = path.relative(cacheDirectory, typesFile).replace(/\\/g, '/').replace(/\.ts$/, '')
  const lines = [`import type { AppSettings, Conversation, Workspace } from '${typesImport.startsWith('.') ? typesImport : `./${typesImport}`}'`]
  const owners = new Map()
  for (const [group, sample] of Object.entries(OUTPUT_SAMPLES)) {
    for (const [kind, values] of [['valid', sample.valid], ['invalid', sample.invalid]]) {
      values.forEach((value, index) => {
        lines.push(`const sample${lines.length}: ${sample.type} = ${JSON.stringify(value)}`)
        owners.set(lines.length, `${group}:${kind}:${index}`)
      })
    }
  }
  fs.writeFileSync(samplesFile, `${lines.join('\n')}\n`)

  const program = ts.createProgram([samplesFile], { strict: true, noEmit: true, target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext })
  const failures = new Map()
  for (const diagnostic of ts.getPreEmitDiagnostics(program)) {
    if (!diagnostic.file || path.resolve(diagnostic.file.fileName) !== samplesFile) continue
    const line = diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start).line + 1
    const owner = owners.get(line)
    if (owner) failures.set(owner, (failures.get(owner) ?? 0) + 1)
  }
  return failures
}

function outputFixture(group, sample, failures) {
  const valid = sample.valid.map((value, index) => {
    if (failures.has(`${group}:valid:${index}`)) throw new Error(`${group} valid sample ${index} does not type-check`)
    return value
  })
  const invalid = sample.invalid.map((value, index) => {
    if (!failures.has(`${group}:invalid:${index}`)) throw new Error(`${group} invalid sample ${index} is accepted by ${sample.type}`)
    return value
  })
  return { source: `shared/types.ts (${sample.type})`, valid, invalid }
}

function settingsFixture() {
  const { settingsSchema } = loadTypeScriptModule(settingsFile)
  if (!settingsSchema) throw new Error('registerSettingsIpc.ts must export settingsSchema')
  const valid = []
  const invalid = []
  for (const input of SETTINGS_CANDIDATES) {
    const result = settingsSchema.safeParse(input)
    if (result.success) valid.push({ input, output: result.data })
    else invalid.push({ input })
  }
  if (valid.length === 0 || invalid.length === 0) throw new Error('the settings candidates must cover both outcomes')
  return { source: 'electron/ipc/registerSettingsIpc.ts (settingsSchema, zod)', valid, invalid }
}

function generate() {
  const failures = checkOutputs()
  const files = new Map([['settings-set.json', settingsFixture()]])
  files.set('settings-get.json', outputFixture('settings-get', OUTPUT_SAMPLES['settings-get'], failures))
  files.set('workspaces-list.json', outputFixture('workspaces-list', OUTPUT_SAMPLES['workspaces-list'], failures))
  files.set('conversations-list.json', outputFixture('conversations-list', OUTPUT_SAMPLES['conversations-list'], failures))
  return files
}

const files = generate()
const check = process.argv.includes('--check')
let stale = false
for (const [name, content] of files) {
  const file = path.join(outputDirectory, name)
  const text = `${JSON.stringify(content, null, 2)}\n`
  if (check) {
    if (!fs.existsSync(file) || fs.readFileSync(file, 'utf8') !== text) {
      console.error(`${path.relative(root, file)} is out of date. Run: node scripts/generate-ipc-samples.mjs`)
      stale = true
    }
  } else {
    fs.mkdirSync(outputDirectory, { recursive: true })
    fs.writeFileSync(file, text)
    console.log(`wrote ${path.relative(root, file)} (${content.valid.length} valid, ${content.invalid.length} invalid)`)
  }
}
if (check && !stale) console.log('IPC samples are up to date.')
process.exitCode = stale ? 1 : 0
