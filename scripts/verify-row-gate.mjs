// Row-level gate of phase 2: the Rust persistence layer must read the rows the
// TypeScript persistence layer writes, at schema 32.
//
//   node scripts/verify-row-gate.mjs
//
// The TypeScript repositories (electron/database, shared/) are compiled to CommonJS
// under node_modules/.cache and run to seed a database and record the rows they return.
// Both go to a temporary directory, which is removed afterwards. No database is committed,
// as AGENTS.md requires. Then `crates/noct-db/tests/row_gate.rs` opens the same file in
// Rust and compares every row.
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'

const root = path.resolve(import.meta.dirname, '..')
const ts = createRequire(path.join(root, 'package.json'))('typescript')
const buildDirectory = path.join(root, 'node_modules', '.cache', 'noct-row-gate')

function compileTree(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const source = path.join(directory, entry.name)
    if (entry.isDirectory()) {
      compileTree(source)
      continue
    }
    if (!entry.name.endsWith('.ts') || entry.name.endsWith('.d.ts')) continue
    const output = path.join(buildDirectory, path.relative(root, source)).replace(/\.ts$/, '.js')
    const { outputText } = ts.transpileModule(fs.readFileSync(source, 'utf8'), {
      fileName: source,
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
        esModuleInterop: true,
      },
    })
    fs.mkdirSync(path.dirname(output), { recursive: true })
    fs.writeFileSync(output, outputText)
  }
}

// Fixed instants keep the seed reproducible. The expected rows come from the TypeScript
// repositories reading that same file, so they hold whatever the application stores.
const at = (second) => new Date(Date.UTC(2026, 0, 1, 12, 0, second)).toISOString()

function seed(LocalDatabase, directory) {
  const db = new LocalDatabase(directory)
  db.importData({
    workspaces: [
      { path: '/work/alpha', name: 'alpha', favorite: 1, authorized: 1, created_at: at(0), last_opened_at: at(10) },
      { path: '/work/beta', name: 'beta', favorite: 0, authorized: 1, created_at: at(1), last_opened_at: at(20) },
    ],
    conversations: [
      { id: 'conv-1', title: 'Primeira conversa', workspace: '/work/alpha', created_at: at(2), updated_at: at(30) },
      { id: 'conv-2', title: 'Conversa com acentuação ✓', workspace: '/work/beta', created_at: at(3), updated_at: at(31) },
    ],
    messages: [
      { id: 'msg-1', conversation_id: 'conv-1', role: 'user', content: 'Olá, mundo — 😀', metadata: null, created_at: at(4) },
      { id: 'msg-2', conversation_id: 'conv-1', role: 'assistant', content: 'Resposta', metadata: '{"model":"m-1","tokens":12}', created_at: at(5) },
      { id: 'msg-3', conversation_id: 'conv-2', role: 'system', content: 'Sistema', metadata: null, created_at: at(6) },
    ],
    artifacts: [
      { id: 'art-1', conversation_id: 'conv-1', workspace: '/work/alpha', type: 'document', title: 'Plano', file_path: 'docs/plano.md', content: '# Plano', metadata: '{"version":2,"draft":false}', created_at: at(7), updated_at: at(8) },
      { id: 'art-2', conversation_id: 'conv-1', workspace: '/work/alpha', type: 'code', title: 'Sem caminho', file_path: null, content: null, metadata: null, created_at: at(9), updated_at: at(9) },
    ],
    memories: [
      { workspace: '/work/alpha', content: 'Usar Vitest.', updated_at: at(40) },
    ],
    settings: { theme: 'light', language: 'en', approvalPolicy: 'untrusted' },
  })
  // Catalog and bindings are written through the TypeScript repositories, so their stored
  // text is what the application writes, including zod's trimming and key order.
  db.modelCatalog.replaceProviderModels('provider-1', [
    { providerId: ' provider-1 ', modelId: 'model-b', displayName: '  Modelo B  ', source: 'local', capabilities: ['chat', 'streaming'], contextWindow: 8192, availability: 'available' },
    {
      providerId: 'provider-1',
      modelId: 'model-a',
      displayName: 'Modelo A',
      family: 'alpha',
      source: 'remote',
      capabilities: ['chat', 'tool-calling', 'vision'],
      maxOutputTokens: 4096,
      pricing: { currency: 'USD', inputPerMillionTokens: 2, outputPerMillionTokens: 0.000001, effectiveAt: ' 2026-01-01 ' },
      availability: 'available',
    },
  ])
  db.workspaceModelBindings.set({
    workspaceId: '/work/alpha',
    defaultBinding: { providerId: 'provider-1', modelId: 'model-a' },
    embeddingBinding: { providerId: 'provider-1', modelId: 'model-b' },
    remoteEmbeddingAllowed: false,
  })

  const workspaces = db.listWorkspaces()
  const conversations = db.listConversations()
  const expected = {
    modelCatalog: db.modelCatalog.list(),
    workspaceModelBindings: db.workspaceModelBindings.list(),
    settings: db.getSettings(),
    workspaces,
    conversations,
    messages: Object.fromEntries(conversations.map((row) => [row.id, db.listMessages(row.id)])),
    recentMessages: Object.fromEntries(conversations.map((row) => [row.id, db.listRecentMessages(row.id, 200)])),
    artifacts: Object.fromEntries(conversations.map((row) => [row.id, db.listArtifacts(row.id)])),
    workspaceMemory: Object.fromEntries(workspaces.map((row) => [row.path, db.getWorkspaceMemory(row.path)])),
  }
  db.close()
  return expected
}

compileTree(path.join(root, 'electron'))
compileTree(path.join(root, 'shared'))
const { LocalDatabase } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/Database.js')

const gateDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'nocturne-row-gate-'))
try {
  const expected = seed(LocalDatabase, gateDirectory)
  for (const suffix of ['-wal', '-shm']) fs.rmSync(path.join(gateDirectory, `nocturne.db${suffix}`), { force: true })
  fs.writeFileSync(path.join(gateDirectory, 'expected.json'), `${JSON.stringify(expected, null, 2)}\n`)
  const run = spawnSync(
    'cargo',
    ['test', '-p', 'noct-db', '--test', 'row_gate', '--', '--ignored', '--nocapture'],
    {
      cwd: root,
      stdio: 'inherit',
      env: { ...process.env, NOCT_ROW_GATE_DIR: gateDirectory },
    },
  )
  process.exitCode = run.status ?? 1
} finally {
  fs.rmSync(gateDirectory, { recursive: true, force: true })
}
