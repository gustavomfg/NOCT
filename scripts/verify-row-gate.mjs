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

  // Provider configurations keep only a credential reference. The secret itself lives in
  // the operating system store, so the seed uses a fixed reference and no secret.
  const localProvider = db.providerConfigurations.create({
    providerType: 'chat-completions',
    displayName: '  Local  ',
    source: 'local',
    baseUrl: ' http://127.0.0.1:11434 ',
    enabled: true,
    requiresAuthentication: false,
    timeoutMs: 30000,
  })
  db.providerConfigurations.create({
    providerType: 'chat-completions',
    displayName: 'Remoto',
    source: 'remote',
    baseUrl: 'https://api.example.test/v1',
    enabled: false,
    requiresAuthentication: true,
    timeoutMs: 120000,
  }, '6f9619ff-8b86-4d11-b42d-00c04fc964ff')
  db.providerConfigurations.setCredentialReference(localProvider.id, '0f3e5b8a-2c4d-4e6f-8a9b-1c2d3e4f5a6b')

  const workspaces = db.listWorkspaces()
  const conversations = db.listConversations()
  const providers = db.providerConfigurations.list()
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
    providerConfigurations: providers,
    providerConfigurationById: Object.fromEntries(providers.map((row) => [row.id, db.providerConfigurations.get(row.id)])),
    providerCredentialReferences: db.providerConfigurations.listCredentialReferences().sort(),
  }
  db.close()
  return expected
}

// Startup reconciliation writes rows that no ported repository owns yet, so its fixture is
// inserted with SQL. The TypeScript repository reconciles a copy of that database, and Rust
// must reach the same state from the copy saved before reconciliation. Timestamps are left
// out of the snapshot because each run records its own clock.
const RECONCILE_TABLES = [
  ['executions', 'SELECT id, status, decision, error FROM executions ORDER BY id'],
  ['checkpoints', 'SELECT id, status, error FROM checkpoints ORDER BY id'],
  ['changes', 'SELECT id, status FROM changes ORDER BY id'],
  ['change_sets', 'SELECT id, status FROM change_sets ORDER BY id'],
  ['change_decision_operations', 'SELECT id, status, error FROM change_decision_operations ORDER BY id'],
  ['execution_commands', 'SELECT id, status, output_summary FROM execution_commands ORDER BY id'],
  ['semantic_units', 'SELECT id, status FROM semantic_units ORDER BY id'],
  ['project_index_runs', 'SELECT id, status, error FROM project_index_runs ORDER BY id'],
  ['semantic_index_runs', 'SELECT id, status, error FROM semantic_index_runs ORDER BY id'],
  ['interrupted_operations', 'SELECT id, workspace, execution_id, kind, source_id, original_status, message FROM interrupted_operations ORDER BY id'],
]

function reconcileParity(BetterSqlite3, OperationRecoveryRepository, directory) {
  const database = new BetterSqlite3(path.join(directory, 'nocturne.db'))
  database.pragma('foreign_keys = ON')
  const at = '2026-01-01T00:00:00.000Z'
  const insert = (sql, ...values) => database.prepare(sql).run(...values)
  insert("INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) VALUES('exec-1','/work/alpha','conv-1','Tarefa','build','running','pending',?)", at)
  insert("INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) VALUES('exec-2','/work/alpha','conv-1','Tarefa','build','completed','accepted',?)", at)
  insert("INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) VALUES('exec-3','/work/alpha','conv-1','Tarefa','build','completed','pending',?)", at)
  insert("INSERT INTO executions(id,workspace,conversation_id,prompt,mode,status,decision,started_at) VALUES('exec-4','/work/alpha','conv-1','Tarefa','build','completed','pending',?)", at)
  insert("INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path) VALUES('ck-capturing','exec-2','/work/alpha','before','capturing',?,'/work/alpha')", at)
  for (const set of ['set-1', 'set-2']) {
    const execution = set === 'set-1' ? 'exec-3' : 'exec-4'
    insert(`INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path) VALUES('${set}-before','${execution}','/work/alpha','before','ready',?,'/work/alpha')`, at)
    insert(`INSERT INTO checkpoints(id,execution_id,workspace,phase,status,captured_at,root_path) VALUES('${set}-after','${execution}','/work/alpha','after','ready',?,'/work/alpha')`, at)
    insert(`INSERT INTO change_sets(id,execution_id,before_checkpoint_id,after_checkpoint_id,status,created_at,updated_at) VALUES('${set}','${execution}','${set}-before','${set}-after','pending',?,?)`, at, at)
  }
  insert("INSERT INTO changes(id,change_set_id,execution_id,relative_path,operation,origin,status,validation_status,created_at,updated_at) VALUES('chg-1','set-1','exec-3','src/a.ts','modify','agent-file-change','pending','unknown',?,?)", at, at)
  insert("INSERT INTO change_decision_operations(id,execution_id,change_id,decision,status,started_at) VALUES('op-1','exec-3','chg-1','accepted','running',?)", at)
  insert("INSERT INTO execution_commands(id,execution_id,command,args_json,source,status,output_summary,started_at) VALUES('cmd-1','exec-3','npm test','[]','validation','running','',?)", at)
  insert("INSERT INTO project_index_runs(id,workspace,index_version,kind,status,phase,started_at,updated_at) VALUES('pix-1','/work/alpha',1,'initial','running','hashing',?,?)", at, at)
  insert("INSERT INTO semantic_index_runs(id,workspace,index_version,kind,status,started_at,updated_at) VALUES('six-1','/work/alpha',1,'initial','running',?,?)", at, at)
  insert("INSERT INTO project_index_files(workspace,relative_path,classification,language,extension,size,mtime_ms,ctime_ms,mode,state,excluded,discovered_at) VALUES('/work/alpha','src/a.ts','source','typescript','.ts',10,1,1,420,'indexed',0,?)", at)
  insert("INSERT INTO semantic_units(id,workspace,relative_path,kind,start_line,start_column,end_line,end_column,source_hash,chunk_hash,normalized_text,chunk_strategy_version,status,created_at,updated_at) VALUES('unit-1','/work/alpha','src/a.ts','text',1,1,1,1,?,?,'texto','v1','processing',?,?)", 'a'.repeat(64), 'a'.repeat(64), at, at)

  database.exec(`VACUUM INTO '${path.join(directory, 'reconcile-before.db')}'`)
  const repository = new OperationRecoveryRepository(database, {
    run: (_operation, callback) => database.transaction(callback)(),
  })
  repository.reconcile()
  const snapshot = Object.fromEntries(
    RECONCILE_TABLES.map(([table, sql]) => [table, database.prepare(sql).raw().all()]),
  )
  database.close()
  return snapshot
}

compileTree(path.join(root, 'electron'))
compileTree(path.join(root, 'shared'))
const { LocalDatabase } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/Database.js')
const { OperationRecoveryRepository } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/OperationRecoveryRepository.js')
const BetterSqlite3 = createRequire(path.join(root, 'package.json'))('better-sqlite3')

const gateDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'nocturne-row-gate-'))
try {
  const expected = seed(LocalDatabase, gateDirectory)
  const reconciled = reconcileParity(BetterSqlite3, OperationRecoveryRepository, gateDirectory)
  for (const suffix of ['-wal', '-shm']) fs.rmSync(path.join(gateDirectory, `nocturne.db${suffix}`), { force: true })
  fs.writeFileSync(path.join(gateDirectory, 'expected.json'), `${JSON.stringify(expected, null, 2)}\n`)
  fs.writeFileSync(path.join(gateDirectory, 'reconcile-expected.json'), `${JSON.stringify(reconciled, null, 2)}\n`)
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
