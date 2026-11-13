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

// Suggestion fixtures. The titles include compatibility forms, combining marks, final sigma,
// a dotted capital I, controls and emoji, so the identity is checked on its hard cases too.
const IDENTITY_TITLES = [
  'Ação: revisar  ‘Config’',
  '  AÇÃO — revisar\tconfig!  ',
  'ﬁle e ação',
  'v2 é/ok',
  '!!! ---',
  'a\u0007\tb c  ',
  'Ｆｕｌｌｗｉｄｔｈ ＡＢＣ',
  'Ǆ ǅ ǆ',
  'İstanbul',
  'ΣΊΣΥΦΟΣ',
  'ȩ́x',
  '🙂 emoji 😀 x',
  '  ',
  'x​zero-width',
  '',
]

function suggestionInput(title, severity, extra = {}) {
  return {
    title,
    description: 'Descrição do problema.',
    reasoning: 'Motivo da conclusão.',
    category: 'bug',
    severity,
    affectedFiles: ['src/a.ts'],
    proposedChanges: 'Ajustar a leitura.',
    expectedBenefits: ['Menos falhas.'],
    complexity: 'low',
    risk: 'medium',
    ...extra,
  }
}

// The review the parity scenario runs on a copy of the seeded database. It persists the
// accepted suggestion with a new severity, and adds one. The open suggestion it no longer
// finds is resolved.
const SUGGESTION_REVIEW = [
  suggestionInput('revisar CONFIGURAÇÃO', 'critical'),
  suggestionInput('Documentar API', 'low', {
    category: 'documentation',
    evidence: [{ source: 'documentação', detail: 'Sem seção.', location: 'docs/api.md' }],
  }),
]

// Contents the credential filter of a brain memory answers for. They cover the thresholds of
// the patterns, UTF-16 counting, JavaScript whitespace, and the case folding the `/i` flag does
// and does not do. Rust must answer each one as the TypeScript filter does.
const BRAIN_SAFETY_CASES = [
  'Usar SQLite local para persistência.',
  '',
  'Authorization: Bearer abcdefgh',
  'authorization:abcdefgh',
  'authorization: short',
  'api_key = 123456789012',
  'api-key=12345678901',
  'senha=abcdefghijkl',
  'senha=abcdefghijk',
  'sk-abcdefghijklmnop',
  'sk-abcdefghijklmno',
  'GHP_abcdefghijklmnopqrst',
  'ghp_abcdefghijklmnopqrs',
  'token=😀😀😀😀😀😀',
  'token=😀😀😀😀😀',
  'password: 漢漢漢漢漢漢漢漢漢漢漢漢',
  'password: 漢漢漢漢漢漢漢漢漢漢漢',
  'token = aaaaaaaaaaaa',
  'token﻿=﻿aaaaaaaaaaaa',
  'token = aaaaaaaaaaaa',
  'token\u0085=aaaaaaaaaaaaaaaa',
  'token᠎=aaaaaaaaaaaa',
  'token\u000B=\u000Caaaaaaaaaaaa',
  'token=​aaaaaaaaaaaa',
  'secret: 　abcdefghijkl',
  'apiKey=123456789012',
  'ſk-abcdefghijklmnop',
  'SK-abcdefghijklmnop',
  'bearer token only',
]

// The TypeScript symbol, import and export SELECTs also return their position columns next to the
// `location` object. The contract has only `location`, so these aliases are removed on the
// TypeScript side. They are an implementation detail that goes away with the TypeScript repository.
const LOCATION_ALIASES = ['startLine', 'startColumn', 'endLine', 'endColumn']
const contractOnly = (item) => Object.fromEntries(Object.entries(item).filter(([key]) => !LOCATION_ALIASES.includes(key)))

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

  // Executions and checkpoints are written through their repositories. A completed capture
  // also records its evidence, so the rows Rust reads are the ones the application stores.
  db.executions.create({ id: 'exec-g1', workspace: '/work/alpha', conversationId: 'conv-1', prompt: 'Corrigir o teste', mode: 'build', status: 'running', decision: 'pending', retryOf: null, startedAt: at(60), finishedAt: null, error: null })
  db.executions.update({ id: 'exec-g1', workspace: '/work/alpha', conversationId: 'conv-1', prompt: 'Corrigir o teste', mode: 'build', status: 'completed', decision: 'pending', retryOf: null, startedAt: at(60), finishedAt: at(61), error: null })
  db.executions.create({ id: 'exec-g2', workspace: '/work/alpha', conversationId: 'conv-1', prompt: 'Revisar', mode: 'review', status: 'failed', decision: 'pending', retryOf: 'exec-g1', startedAt: at(62), finishedAt: at(63), error: 'Falhou.' })
  const capturing = { id: 'ck-g1', executionId: 'exec-g1', workspace: '/work/alpha', phase: 'before', status: 'capturing', capturedAt: at(64), rootPath: '/work/alpha', error: null }
  db.checkpoints.save(capturing)
  db.checkpoints.completeCapture({ ...capturing, status: 'ready' }, [
    { id: 'cf-g1', checkpointId: 'ck-g1', relativePath: 'src/a.ts', exists: true, kind: 'file', size: 12, mode: 420, hash: 'a'.repeat(64), contentPath: null },
    { id: 'cf-g2', checkpointId: 'ck-g1', relativePath: 'src/gone.ts', exists: false, kind: 'missing', size: null, mode: null, hash: null, contentPath: null },
  ])

  // Change sets, file changes and hunks are written through their repository, including a
  // decision that records evidence and completes its operation.
  db.checkpoints.save({ id: 'ck-g2', executionId: 'exec-g1', workspace: '/work/alpha', phase: 'after', status: 'ready', capturedAt: at(66), rootPath: '/work/alpha', error: null })
  const changeSet = { id: 'cs-g1', executionId: 'exec-g1', beforeCheckpointId: 'ck-g1', afterCheckpointId: 'ck-g2', status: 'pending', createdAt: at(67), updatedAt: at(67) }
  const modified = { id: 'chg-g1', executionId: 'exec-g1', changeSetId: 'cs-g1', checkpointId: 'ck-g2', relativePath: 'src/a.ts', originalPath: null, operation: 'modify', origin: 'agent-file-change', beforeHash: 'a'.repeat(64), afterHash: 'b'.repeat(64), beforeSize: 12, afterSize: 14, status: 'pending', validationStatus: 'unknown', policy: 'allowed', policyReason: null, createdAt: at(67), updatedAt: at(67) }
  const created = { id: 'chg-g2', executionId: 'exec-g1', changeSetId: 'cs-g1', checkpointId: 'ck-g2', relativePath: 'src/new.ts', originalPath: null, operation: 'create', origin: 'agent-file-change', beforeHash: null, afterHash: 'c'.repeat(64), beforeSize: null, afterSize: 3, status: 'pending', validationStatus: 'unknown', policy: 'requires-approval', policyReason: 'Arquivo novo.', createdAt: at(67), updatedAt: at(67) }
  db.changeSets.save(changeSet, [modified, created])
  const operation = db.changeSets.reserveDecision('exec-g1', 'chg-g1', 'accepted')
  db.changeSets.saveDecision({ ...changeSet, status: 'partially-accepted', updatedAt: at(68) }, { ...modified, status: 'accepted', validationStatus: 'pending', afterSize: 14, updatedAt: at(68) }, operation)
  const hunk = { id: 'hk-g1', changeId: 'chg-g1', sequence: 1, baseHash: 'a'.repeat(64), originalPatch: '@@ -1 +1 @@', finalPatch: '@@ -1 +1 @@', status: 'pending', startLine: 1, endLine: 1, decisionAt: null }
  db.changeSets.createHunk(hunk)
  db.changeSets.createHunk({ ...hunk, id: 'hk-g2', sequence: 2, startLine: 4, endLine: 6 })
  db.changeSets.updateHunk({ ...hunk, finalPatch: '@@ -1 +1,2 @@', status: 'edited', decisionAt: at(69) })

  // Validation runs are written through their repository. Creating one also records its
  // evidence, linked to the ready checkpoints of its execution; saving a stored run updates it.
  db.validation.create({ id: 'val-g1', workspace: '/work/alpha', executionId: 'exec-g1', kind: 'typecheck', command: 'npm', args: ['run', 'typecheck'], status: 'running', exitCode: null, durationMs: null, outputSummary: '', artifacts: [], startedAt: at(70), completedAt: null, error: null })
  db.validation.update({ id: 'val-g1', workspace: '/work/alpha', executionId: 'exec-g1', kind: 'typecheck', command: 'npm', args: ['run', 'typecheck'], status: 'passed', exitCode: 0, durationMs: 1200, outputSummary: 'Sem erros.', artifacts: [{ path: 'dist/report.json', kind: 'report', size: 42 }, { path: 'dist/empty.log', kind: 'log', size: null }], startedAt: at(70), completedAt: at(71), error: null })
  db.validation.create({ id: 'val-g2', workspace: '/work/alpha', kind: 'test', command: 'npx', args: ['vitest', 'run', 'acentuação ✓'], status: 'failed', exitCode: 1, durationMs: 800, outputSummary: 'Um teste falhou.', artifacts: [], startedAt: at(72), completedAt: at(73), error: 'Falhou.' })
  db.validation.save({ id: 'val-g3', workspace: '/work/beta', kind: 'lint', command: 'npm', args: ['run', 'lint'], status: 'queued', exitCode: null, durationMs: null, outputSummary: '', artifacts: [], startedAt: at(74), completedAt: null, error: null })
  db.validation.save({ id: 'val-g3', workspace: '/work/beta', kind: 'lint', command: 'npm', args: ['run', 'lint'], status: 'cancelled', exitCode: null, durationMs: 4, outputSummary: 'Cancelado.', artifacts: [], startedAt: at(74), completedAt: at(75), error: null })

  // Execution evidence is written through its repository: commands with bounded arguments,
  // errors, and validation links, which are recorded once for each validation, change and phase.
  db.executionEvidence.createCommand({ id: 'cmd-g1', executionId: 'exec-g1', command: 'npm', args: ['run', 'typecheck'], source: 'validation', status: 'running', exitCode: null, durationMs: null, outputSummary: '', startedAt: at(76), finishedAt: null })
  db.executionEvidence.saveCommand({ id: 'cmd-g1', executionId: 'exec-g1', command: 'npm', args: ['run', 'typecheck'], source: 'validation', status: 'passed', exitCode: 0, durationMs: 1200, outputSummary: 'Sem erros.', startedAt: at(76), finishedAt: at(77) })
  db.executionEvidence.createCommand({ id: 'cmd-g2', executionId: 'exec-g1', command: 'ls', args: Array.from({ length: 101 }, (_, index) => (index === 0 ? 'x'.repeat(4_100) : `arg-${index}`)), source: 'agent', status: 'cancelled', exitCode: null, durationMs: 5, outputSummary: 'Cancelado.', startedAt: at(78), finishedAt: at(79) })
  db.executionEvidence.createError({ id: 'err-g1', executionId: 'exec-g1', stage: 'validation', message: 'Validação falhou: ✓', path: 'src/a.ts', createdAt: at(80) })
  db.executionEvidence.createError({ id: 'err-g2', executionId: 'exec-g1', stage: 'persistence', message: 'Persistência interrompida.', path: null, createdAt: at(81) })
  db.executionEvidence.linkValidation({ executionId: 'exec-g1', changeId: 'chg-g1', validationId: 'val-g1', phase: 'proposed' })
  db.executionEvidence.linkValidation({ executionId: 'exec-g1', changeId: 'chg-g1', validationId: 'val-g1', phase: 'proposed' })
  db.executionEvidence.linkValidation({ executionId: 'exec-g1', changeId: null, validationId: 'val-g2', phase: 'before' })
  db.executionEvidence.linkValidation({ executionId: 'exec-g1', changeId: 'chg-g2', validationId: 'val-g1', phase: 'after-decision' })

  // Review suggestions are written through their repository: an add, status changes with a
  // result, and two reviews that persist, resolve and update suggestions by identity.
  const suggestionIds = new Set()
  const first = db.suggestions.add('conv-1', '/work/alpha', suggestionInput('Revisar configuração', 'medium', {
    evidence: [{ source: 'arquivo', detail: 'Leitura direta.', location: 'src/a.ts:12' }, { source: 'teste', detail: 'Sem cobertura.' }],
    confidence: 85,
  }))
  suggestionIds.add(first.id)
  db.suggestions.setStatus(first.id, 'in-analysis')
  db.suggestions.setStatus(first.id, 'accepted', 'Aceita pelo gate. ✓')
  const firstReview = db.suggestions.reconcile('conv-1', '/work/alpha', [suggestionInput('Remover cache', 'low'), suggestionInput('Nova análise', 'info')], true)
  const secondReview = db.suggestions.reconcile('conv-1', '/work/alpha', [suggestionInput('REVISAR CONFIGURAÇÃO', 'high'), suggestionInput('Verificar testes', 'critical', { category: 'testing' })], true)
  for (const item of [...firstReview.suggestions, ...secondReview.suggestions, ...secondReview.comparison.resolvedSuggestions]) suggestionIds.add(item.id)

  // Brain memories are written through their repository: a workspace memory and a conversation
  // memory, agent candidates in which one repeats another, an approval, an edit, a return to use,
  // an outdated and an archived memory, a deletion, and a memory of another workspace. Every change
  // is recorded in the history. The repository stamps the wall clock, so the update times are set
  // to fixed values afterwards: the orders the pages and retrieval return must not depend on the run.
  const brainCaught = (action) => {
    try {
      action()
      return null
    } catch (error) {
      return error.message
    }
  }
  const workspaceBrain = db.createBrainMemory('/work/alpha', { kind: 'decision', scope: 'workspace', content: '  Usar SQLite local para persistência.  ', confidence: 90, sourceType: 'manual', status: 'active' })
  const conversationBrain = db.createBrainMemory('/work/alpha', { kind: 'preference', scope: 'conversation', conversationId: 'conv-1', content: 'Prefere respostas em português.', sourceType: 'message' })
  const [electronBrain, constraintBrain] = db.createBrainMemoryCandidates('/work/alpha', 'conv-1', [
    { kind: 'fact', scope: 'workspace', content: 'O projeto usa Electron.', confidence: 80 },
    { kind: 'constraint', scope: 'conversation', content: 'Não alterar o schema sem migração.', confidence: 75 },
    { kind: 'fact', scope: 'workspace', content: '  o PROJETO usa electron.  ', confidence: 60 },
  ])
  const removedBrain = db.createBrainMemory('/work/alpha', { kind: 'learning', scope: 'workspace', content: 'Memória para remover.', sourceType: 'agent' })
  const betaBrain = db.createBrainMemory('/work/beta', { kind: 'fact', scope: 'workspace', content: 'Projeto beta usa Go.', status: 'active' })
  db.updateBrainMemory(electronBrain.id, '/work/alpha', { status: 'active' })
  db.updateBrainMemory(workspaceBrain.id, '/work/alpha', { content: 'Usar SQLite local para persistência e migrações.', confidence: 95 })
  db.updateBrainMemory(conversationBrain.id, '/work/alpha', { status: 'archived' })
  db.updateBrainMemory(conversationBrain.id, '/work/alpha', { status: 'active' })
  db.updateBrainMemory(electronBrain.id, '/work/alpha', { status: 'outdated' })
  db.updateBrainMemory(constraintBrain.id, '/work/alpha', { status: 'archived' })
  db.deleteBrainMemory(removedBrain.id, '/work/alpha')
  db.markBrainMemoriesUsed([workspaceBrain.id, workspaceBrain.id, conversationBrain.id])
  const clock = new BetterSqlite3(path.join(directory, 'nocturne.db'))
  const stampUpdate = clock.prepare('UPDATE brain_memories SET updated_at = ? WHERE id = ?')
  for (const [index, memory] of [workspaceBrain, conversationBrain, electronBrain, constraintBrain, betaBrain].entries()) stampUpdate.run(at(400 + index), memory.id)
  clock.close()

  // The project index is written through its repository: runs that end and so become evidence,
  // files with their analyses, a file that goes, a change under a directory, exclusions, stack
  // evidence and relation hashes. Every timestamp is fixed, so the rows do not depend on the run.
  const digest = (character) => character.repeat(64)
  const indexFile = (relativePath, values) => ({
    workspace: '/work/alpha', relativePath, classification: 'source', language: 'TypeScript', extension: '.ts',
    size: 120, mtimeMs: 1000, ctimeMs: 1000, mode: 420, observedHash: digest('0'), analyzedHash: digest('0'),
    state: 'indexed', excluded: false, exclusionReason: null, parserId: 'typescript', parserVersion: '5',
    error: null, discoveredAt: at(500), analyzedAt: at(501), ...values,
  })
  const indexSymbol = (id, relativePath, values) => ({
    id, workspace: '/work/alpha', relativePath, analyzedHash: digest('a'), kind: 'function', name: id, qualifiedName: null,
    scope: null, signature: null, location: { startLine: 1, startColumn: 1, endLine: 1, endColumn: 10 },
    exported: false, parserId: 'typescript', parserVersion: '5', ...values,
  })
  const location = (startLine) => ({ startLine, startColumn: 1, endLine: startLine, endColumn: 20 })
  const app = indexFile('src/app.ts', { analyzedHash: digest('a'), observedHash: digest('a') })
  const util = indexFile('src/util.ts', { analyzedHash: digest('b'), observedHash: digest('b') })
  const old = indexFile('src/old.ts', { analyzedHash: digest('c'), observedHash: digest('c') })
  const gone = indexFile('src/gone.ts', { analyzedHash: digest('e'), observedHash: digest('e') })
  const readme = indexFile('README.md', { classification: 'documentation', language: 'Markdown', extension: '.md', state: 'unsupported', analyzedHash: null, parserId: null, parserVersion: null })
  const dist = indexFile('dist/bundle.js', { language: 'JavaScript', extension: '.js', state: 'excluded', excluded: true, exclusionReason: 'build-output', analyzedHash: null, parserId: null, parserVersion: null })
  const broken = indexFile('src/broken.ts', { state: 'failed', error: 'Erro de sintaxe na linha 3.', analyzedHash: null, observedHash: digest('f') })
  const run = { id: 'pix-g1', workspace: '/work/alpha', indexVersion: 2, kind: 'initial', status: 'running', phase: 'parsing', totalFiles: 7, processedFiles: 2, failedFiles: 0, unsupportedFiles: 1, pendingFiles: 4, startedAt: at(510), updatedAt: at(510), completedAt: null, error: null }
  db.projectIndex.createRun(run)
  db.projectIndex.saveFileAnalysis({
    file: app,
    symbols: [
      indexSymbol('sym-main', 'src/app.ts', { analyzedHash: digest('a'), kind: 'function', name: 'main', signature: 'main(): void', location: { startLine: 3, startColumn: 1, endLine: 5, endColumn: 2 }, exported: true }),
      indexSymbol('sym-app', 'src/app.ts', { analyzedHash: digest('a'), kind: 'class', name: 'App', qualifiedName: 'App', location: { startLine: 7, startColumn: 1, endLine: 20, endColumn: 2 } }),
      indexSymbol('sym-run', 'src/app.ts', { analyzedHash: digest('a'), kind: 'method', name: 'run', qualifiedName: 'App.run', scope: 'App', location: { startLine: 9, startColumn: 3, endLine: 12, endColumn: 4 } }),
    ],
    imports: [
      { id: 'imp-util', workspace: '/work/alpha', sourcePath: 'src/app.ts', sourceHash: digest('a'), specifier: './util', targetPath: 'src/util.ts', targetHash: null, kind: 'named', importedNames: ['helper'], location: location(1), resolution: 'local' },
      { id: 'imp-react', workspace: '/work/alpha', sourcePath: 'src/app.ts', sourceHash: digest('a'), specifier: 'react', targetPath: null, targetHash: null, kind: 'default', importedNames: ['React'], location: location(2), resolution: 'external' },
      { id: 'imp-missing', workspace: '/work/alpha', sourcePath: 'src/app.ts', sourceHash: digest('a'), specifier: './missing', targetPath: null, targetHash: null, kind: 'side-effect', importedNames: [], location: location(3), resolution: 'unresolved' },
    ],
    exports: [
      { id: 'exp-main', workspace: '/work/alpha', sourcePath: 'src/app.ts', sourceHash: digest('a'), name: 'main', kind: 'named', targetPath: null, targetHash: null, location: location(3) },
      { id: 'exp-default', workspace: '/work/alpha', sourcePath: 'src/app.ts', sourceHash: digest('a'), name: 'default', kind: 'default', targetPath: null, targetHash: null, location: location(14) },
    ],
  })
  db.projectIndex.saveFileAnalysis({
    file: util,
    symbols: [indexSymbol('sym-helper', 'src/util.ts', { analyzedHash: digest('b'), kind: 'function', name: 'helper', exported: true, location: { startLine: 1, startColumn: 1, endLine: 4, endColumn: 2 } })],
    imports: [],
    exports: [{ id: 'exp-helper', workspace: '/work/alpha', sourcePath: 'src/util.ts', sourceHash: digest('b'), name: 'helper', kind: 'named', targetPath: null, targetHash: null, location: location(1) }],
  })
  db.projectIndex.saveFileAnalysis({
    file: old,
    symbols: [indexSymbol('sym-legacy', 'src/old.ts', { analyzedHash: digest('c'), kind: 'function', name: 'legacy' })],
    imports: [{ id: 'imp-old', workspace: '/work/alpha', sourcePath: 'src/old.ts', sourceHash: digest('c'), specifier: './util', targetPath: 'src/util.ts', targetHash: null, kind: 'named', importedNames: ['helper'], location: location(1), resolution: 'local' }],
    exports: [],
  })
  db.projectIndex.upsertFile(gone)
  db.projectIndex.markFileState(broken)
  db.projectIndex.upsertFile(readme)
  db.projectIndex.upsertFile(dist)
  db.projectIndex.updateRun({ ...run, status: 'completed', phase: 'completed', processedFiles: 6, pendingFiles: 0, updatedAt: at(520), completedAt: at(520) })
  db.projectIndex.removeMissingFiles('/work/alpha', ['src/app.ts', 'src/util.ts', 'src/broken.ts', 'src/gone.ts', 'README.md', 'dist/bundle.js'])
  db.projectIndex.removeMissingForChange('/work/alpha', ['./src/'], ['src/app.ts', 'src/util.ts', 'src/broken.ts'])
  db.projectIndex.createRun({ id: 'pix-g2', workspace: '/work/alpha', indexVersion: 2, kind: 'incremental', status: 'queued', phase: 'discovering', totalFiles: 0, processedFiles: 0, failedFiles: 0, unsupportedFiles: 0, pendingFiles: 0, startedAt: at(530), updatedAt: at(530), completedAt: null, error: null })
  db.projectIndex.updateRun({ id: 'pix-g2', workspace: '/work/alpha', indexVersion: 2, kind: 'incremental', status: 'failed', phase: 'persisting', totalFiles: 3, processedFiles: 2, failedFiles: 1, unsupportedFiles: 0, pendingFiles: 0, startedAt: at(530), updatedAt: at(531), completedAt: at(531), error: 'Falha de leitura.' })
  db.projectIndex.createRun({ id: 'pix-b1', workspace: '/work/beta', indexVersion: 2, kind: 'manual', status: 'queued', phase: 'discovering', totalFiles: 0, processedFiles: 0, failedFiles: 0, unsupportedFiles: 0, pendingFiles: 0, startedAt: at(532), updatedAt: at(532), completedAt: null, error: null })
  db.projectIndex.replaceStackEvidence('/work/alpha', [
    { id: 'stk-lang', workspace: '/work/alpha', category: 'language', value: 'TypeScript', confidence: 90, sourcePath: 'package.json', sourceHash: digest('d'), sourceLine: null, reason: 'Dependências de tipos.', detectedAt: at(540) },
    { id: 'stk-runtime', workspace: '/work/alpha', category: 'runtime', value: 'Node.js', confidence: 80, sourcePath: 'package.json', sourceHash: digest('d'), sourceLine: 3, reason: 'Campo engines.', detectedAt: at(541) },
    { id: 'stk-framework', workspace: '/work/alpha', category: 'framework', value: 'React', confidence: 85, sourcePath: 'package.json', sourceHash: digest('d'), sourceLine: 8, reason: 'Dependência react.', detectedAt: at(542) },
    { id: 'stk-test-script', workspace: '/work/alpha', category: 'script', value: 'test=vitest run', confidence: 100, sourcePath: 'package.json', sourceHash: digest('d'), sourceLine: 10, reason: 'Script de testes.', detectedAt: at(543) },
    { id: 'stk-build-script', workspace: '/work/alpha', category: 'script', value: 'build=vite build', confidence: 100, sourcePath: 'package.json', sourceHash: digest('d'), sourceLine: 11, reason: 'Script de build.', detectedAt: at(544) },
    { id: 'stk-bare-script', workspace: '/work/alpha', category: 'script', value: 'lint', confidence: 100, sourcePath: 'package.json', sourceHash: digest('d'), sourceLine: 12, reason: 'Script sem comando.', detectedAt: at(545) },
    { id: 'stk-test', workspace: '/work/alpha', category: 'test', value: 'Vitest', confidence: 70, sourcePath: 'vitest.config.ts', sourceHash: digest('g'), sourceLine: null, reason: 'Configuração do teste.', detectedAt: at(546) },
    { id: 'stk-pm', workspace: '/work/alpha', category: 'package-manager', value: 'npm', confidence: 95, sourcePath: 'package-lock.json', sourceHash: digest('h'), sourceLine: null, reason: 'Lockfile presente.', detectedAt: at(547) },
    { id: 'stk-convention', workspace: '/work/alpha', category: 'convention', value: 'ESLint', confidence: 60, sourcePath: 'eslint.config.js', sourceHash: digest('i'), sourceLine: null, reason: 'Configuração de lint.', detectedAt: at(548) },
  ])
  db.projectIndex.replaceExclusions('/work/alpha', [{ relativePath: 'dist/bundle.js', reason: 'build-output' }, { relativePath: 'node_modules', reason: 'dependency' }], at(550))
  db.projectIndex.upsertExclusions('/work/alpha', [{ relativePath: 'dist/bundle.js', reason: 'generated' }, { relativePath: 'coverage', reason: 'generated' }], at(551))
  db.projectIndex.refreshRelationHashes('/work/alpha')

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
    executions: db.executions.list('/work/alpha'),
    executionById: Object.fromEntries(['exec-g1', 'exec-g2'].map((id) => [id, db.executions.get(id)])),
    checkpoints: db.checkpoints.list('exec-g1'),
    checkpointById: { 'ck-g1': db.checkpoints.get('ck-g1') },
    // The TypeScript listFiles spreads its raw row, so the internal `existsFlag` alias reaches
    // callers. It is not part of CheckpointFileRecord, so Rust does not reproduce it: an
    // intentional difference, removed here before the comparison.
    checkpointFiles: { 'ck-g1': db.checkpoints.listFiles('ck-g1').map((file) => Object.fromEntries(Object.entries(file).filter(([key]) => key !== 'existsFlag'))) },
    changeSets: db.changeSets.list('exec-g1'),
    changeSetLatest: db.changeSets.getByExecutionId('exec-g1'),
    changeSetById: { 'cs-g1': db.changeSets.getById('cs-g1', 'exec-g1') },
    changes: db.changeSets.listChanges('cs-g1'),
    changeById: Object.fromEntries(['chg-g1', 'chg-g2'].map((id) => [id, db.changeSets.getChange(id)])),
    hunks: db.changeSets.listHunks('chg-g1'),
    hunkById: Object.fromEntries(['hk-g1', 'hk-g2'].map((id) => [id, db.changeSets.getHunk(id)])),
    workspaceEvidence: db.workspaceEvidence.list('/work/alpha'),
    validationRuns: db.validation.list('/work/alpha'),
    validationRunLatest: db.validation.latest('/work/alpha'),
    validationRunPage: db.validation.page('/work/alpha', 0, 1),
    validationRunsBeta: db.validation.list('/work/beta'),
    commands: { 'exec-g1': db.executionEvidence.listCommands('exec-g1') },
    errors: { 'exec-g1': db.executionEvidence.listErrors('exec-g1') },
    validationLinks: { 'exec-g1': db.executionEvidence.listValidationLinks('exec-g1') },
    suggestionList: db.suggestions.list('conv-1'),
    suggestionPage: db.suggestions.page('conv-1', 0, 2),
    suggestionById: Object.fromEntries([...suggestionIds].sort().map((id) => [id, db.suggestions.get(id)])),
    suggestionIdentities: Object.fromEntries(IDENTITY_TITLES.map((title) => [title, suggestionIdentity({ category: 'bug', title })])),
    sanitizedTitles: Object.fromEntries(IDENTITY_TITLES.map((title) => [title, sanitizeSuggestionTitle(title)])),
    brainMemoryById: Object.fromEntries([workspaceBrain, conversationBrain, electronBrain, constraintBrain, betaBrain].map((memory) => [memory.id, db.getBrainMemory(memory.id, '/work/alpha')])),
    brainMemoryAfterDelete: { [removedBrain.id]: db.getBrainMemory(removedBrain.id, '/work/alpha') },
    brainMemoryHistory: Object.fromEntries([workspaceBrain, conversationBrain, electronBrain, constraintBrain].map((memory) => [memory.id, db.listBrainMemoryHistory(memory.id, '/work/alpha')])),
    brainMemoryPage: db.listBrainMemoryPage('/work/alpha', 0, 2),
    brainMemorySearch: db.listBrainMemoryPage('/work/alpha', 0, 10, 'sqlite electron'),
    brainMemoryStatus: db.listBrainMemoryPage('/work/alpha', 0, 1, '', 'active'),
    brainMemoryRetrieve: db.retrieveBrainMemories('/work/alpha', 'conv-1', 'sqlite migrações português'),
    brainMemoryEquivalent: {
      active: db.findEquivalentBrainMemory('/work/alpha', 'workspace', null, '  usar sqlite LOCAL para persistência e migrações.  '),
      conversation: db.findEquivalentBrainMemory('/work/alpha', 'conversation', 'conv-1', 'prefere RESPOSTAS em português.'),
      outdated: db.findEquivalentBrainMemory('/work/alpha', 'workspace', null, 'O projeto usa Electron.'),
    },
    brainMemoryErrors: {
      invalidTransition: brainCaught(() => db.updateBrainMemory(constraintBrain.id, '/work/alpha', { status: 'outdated' })),
      missing: brainCaught(() => db.updateBrainMemory('missing-brain', '/work/alpha', { status: 'active' })),
      conversationRequired: brainCaught(() => db.createBrainMemory('/work/alpha', { kind: 'fact', scope: 'conversation', content: 'Sem conversa.' })),
      otherWorkspace: brainCaught(() => db.createBrainMemory('/work/beta', { kind: 'fact', scope: 'conversation', conversationId: 'conv-1', content: 'Conversa de outro workspace.' })),
    },
    brainSafety: BRAIN_SAFETY_CASES.map((input) => ({ input, safe: isSafeBrainMemoryContent(input) })),
    projectRuns: db.projectIndex.listRuns('/work/alpha'),
    projectRunsLimited: db.projectIndex.listRuns('/work/alpha', 1),
    projectRunLatest: db.projectIndex.latestRun('/work/alpha'),
    projectFiles: db.projectIndex.listFiles('/work/alpha'),
    projectFilesLimited: db.projectIndex.listFiles('/work/alpha', 2),
    projectFileById: Object.fromEntries(['src/app.ts', 'src/util.ts', 'src/broken.ts', 'src/old.ts', 'src/gone.ts', 'README.md', 'dist/bundle.js'].map((relativePath) => [relativePath, db.projectIndex.getFile('/work/alpha', relativePath)])),
    projectExclusions: db.projectIndex.listExclusions('/work/alpha'),
    projectStackEvidence: db.projectIndex.listStackEvidence('/work/alpha'),
    projectSymbols: {
      all: db.projectIndex.listSymbols('/work/alpha', '').map(contractOnly),
      query: db.projectIndex.listSymbols('/work/alpha', '  RUN  ').map(contractOnly),
      limited: db.projectIndex.listSymbols('/work/alpha', '', 2).map(contractOnly),
    },
    projectSymbolsForFile: {
      app: db.projectIndex.listSymbolsForFile('/work/alpha', 'src/app.ts').map(contractOnly),
      missing: db.projectIndex.listSymbolsForFile('/work/alpha', 'src/missing.ts').map(contractOnly),
    },
    projectImports: {
      all: db.projectIndex.listImports('/work/alpha').map(contractOnly),
      app: db.projectIndex.listImports('/work/alpha', 'src/app.ts').map(contractOnly),
    },
    projectExports: {
      all: db.projectIndex.listExports('/work/alpha').map(contractOnly),
      util: db.projectIndex.listExports('/work/alpha', 'src/util.ts').map(contractOnly),
    },
    projectSummary: db.projectIndex.summary('/work/alpha'),
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

// Workspace evidence is written through the TypeScript repository. The database is copied
// before a parent manifest is marked stale, so Rust can repeat the same steps on the copy and
// must read the same manifests, stale chain and verification.
function evidenceParity(BetterSqlite3, WorkspaceEvidenceRepository, directory) {
  const database = new BetterSqlite3(path.join(directory, 'parity.db'))
  database.pragma('foreign_keys = ON')
  const evidence = new WorkspaceEvidenceRepository(database)
  const parent = evidence.record({
    kind: 'project-index',
    workspace: '/work/alpha',
    sourceId: 'pix-gate',
    startedAt: at(50),
    paths: [{ path: 'src/a.ts', hash: 'a'.repeat(64) }, { path: 'docs/readme.md', hash: null }],
  })
  const child = evidence.record({
    kind: 'before',
    workspace: '/work/alpha',
    sourceId: 'before-gate',
    startedAt: at(51),
    references: [{ kind: 'manifest', id: parent.id }],
  })
  evidence.saveVerification(child.id, { checkedAt: at(52), matchedPaths: 1, mismatchedPaths: 0, uncheckedPaths: 1 })
  database.exec(`VACUUM INTO '${path.join(directory, 'evidence-before.db')}'`)
  const reason = 'Fonte alterada no gate.'
  evidence.markStale(parent.id, reason)
  const expected = {
    parentId: parent.id,
    reason,
    list: evidence.list('/work/alpha'),
    bySource: evidence.bySource('project-index', 'pix-gate'),
    indexReferences: evidence.indexReferences('/work/alpha'),
  }
  database.close()
  return expected
}

function reconcileParity(BetterSqlite3, OperationRecoveryRepository, directory) {
  const database = new BetterSqlite3(path.join(directory, 'parity.db'))
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

// Review suggestions are reconciled on a copy of the seeded database, taken before any other
// parity scenario changes it. The TypeScript run and the Rust run read the same review input.
// Identifiers and clock values are left out of the snapshot because each run makes its own;
// titles, statuses, severities, results and the comparison are compared exactly.
// A file record as the project index stores it. The change scenario needs only the paths.
function scenarioFile(relativePath) {
  return {
    workspace: '/work/alpha', relativePath, classification: 'source', language: 'TypeScript', extension: '.ts',
    size: 1, mtimeMs: 1, ctimeMs: 1, mode: 420, observedHash: null, analyzedHash: null, state: 'discovered',
    excluded: false, exclusionReason: null, parserId: null, parserVersion: null, error: null,
    discoveredAt: '2026-01-01T12:00:00.000Z', analyzedAt: null,
  }
}

// Files a change no longer finds are removed on a copy of the database saved after the seed. The
// requested paths are normalized, the stored paths are compared as they are: a stored path with a
// backslash is not under a directory. Rust must keep and remove the same files.
const CHANGE_FILES = ['src/a.ts', 'src/a/b.ts', 'src/abc.ts', 'src\\win.ts', 'src/sub/deep.ts', 'lib/x.ts', 'srcx.ts']
const CHANGE_REQUESTED = ['./src/', 'src\\sub', 'lib/x.ts/', '']
const CHANGE_PRESENT = ['src/a/b.ts']

function changeParity(BetterSqlite3, ProjectIndexRepository, directory) {
  const copy = path.join(directory, 'change-ts.db')
  fs.copyFileSync(path.join(directory, 'project-before.db'), copy)
  const database = new BetterSqlite3(copy)
  database.pragma('foreign_keys = ON')
  const repository = new ProjectIndexRepository(database, {
    run: (_operation, callback) => database.transaction(callback)(),
  })
  for (const relativePath of CHANGE_FILES) repository.upsertFile(scenarioFile(relativePath))
  repository.removeMissingForChange('/work/alpha', CHANGE_REQUESTED, CHANGE_PRESENT)
  const remaining = database.prepare('SELECT relative_path AS path FROM project_index_files WHERE workspace=? ORDER BY relative_path').all('/work/alpha').map((row) => row.path)
  database.close()
  return { files: CHANGE_FILES.map(scenarioFile), requested: CHANGE_REQUESTED, present: CHANGE_PRESENT, remaining }
}

function suggestionParity(BetterSqlite3, SuggestionRepository, directory, review) {
  const copy = path.join(directory, 'suggestions-ts.db')
  fs.copyFileSync(path.join(directory, 'suggestions-before.db'), copy)
  const database = new BetterSqlite3(copy)
  database.pragma('foreign_keys = ON')
  const repository = new SuggestionRepository(database, {
    run: (_operation, callback) => database.transaction(callback)(),
  })
  const result = repository.reconcile('conv-1', '/work/alpha', review, true)
  const snapshot = {
    rows: database.prepare("SELECT title, status, severity, confidence, source, responsible, result FROM suggestions WHERE conversation_id='conv-1' ORDER BY title").all(),
    decisions: database.prepare("SELECT s.title AS title, d.status AS status, d.result AS result FROM suggestion_decisions d JOIN suggestions s ON s.id = d.suggestion_id WHERE s.conversation_id='conv-1' ORDER BY s.title, d.status, d.result").all(),
    returned: result.suggestions.map((item) => ({ title: item.title, status: item.status, severity: item.severity, historyStatuses: item.history.map((entry) => entry.status) })),
    comparison: {
      newSuggestions: result.comparison.newSuggestions.map((item) => item.title),
      persistentSuggestions: result.comparison.persistentSuggestions.map((item) => item.title),
      resolvedSuggestions: result.comparison.resolvedSuggestions.map((item) => item.title),
      severityChanges: result.comparison.severityChanges.map(({ title, from, to }) => ({ title, from, to })),
    },
  }
  database.close()
  return snapshot
}

compileTree(path.join(root, 'electron'))
compileTree(path.join(root, 'shared'))
const { LocalDatabase } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/Database.js')
const { OperationRecoveryRepository } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/OperationRecoveryRepository.js')
const { WorkspaceEvidenceRepository } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/WorkspaceEvidenceRepository.js')
const { SuggestionRepository } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/SuggestionRepository.js')
const { suggestionIdentity, sanitizeSuggestionTitle } = createRequire(path.join(buildDirectory, 'probe.js'))('./shared/suggestions.js')
const { isSafeBrainMemoryContent } = createRequire(path.join(buildDirectory, 'probe.js'))('./shared/brainMemory.js')
const { ProjectIndexRepository } = createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/ProjectIndexRepository.js')
const BetterSqlite3 = createRequire(path.join(root, 'package.json'))('better-sqlite3')

const gateDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'nocturne-row-gate-'))
try {
  const expected = seed(LocalDatabase, gateDirectory)
  // The parity scenarios change rows the row gate does not expect, so they run on a copy.
  const copy = new BetterSqlite3(path.join(gateDirectory, 'nocturne.db'))
  copy.exec(`VACUUM INTO '${path.join(gateDirectory, 'parity.db')}'`)
  copy.exec(`VACUUM INTO '${path.join(gateDirectory, 'suggestions-before.db')}'`)
  copy.exec(`VACUUM INTO '${path.join(gateDirectory, 'project-before.db')}'`)
  copy.close()
  const evidence = evidenceParity(BetterSqlite3, WorkspaceEvidenceRepository, gateDirectory)
  const reconciled = reconcileParity(BetterSqlite3, OperationRecoveryRepository, gateDirectory)
  const suggestionSnapshot = suggestionParity(BetterSqlite3, SuggestionRepository, gateDirectory, SUGGESTION_REVIEW)
  const changeScenario = changeParity(BetterSqlite3, ProjectIndexRepository, gateDirectory)
  for (const suffix of ['-wal', '-shm']) fs.rmSync(path.join(gateDirectory, `nocturne.db${suffix}`), { force: true })
  fs.writeFileSync(path.join(gateDirectory, 'expected.json'), `${JSON.stringify(expected, null, 2)}\n`)
  fs.writeFileSync(path.join(gateDirectory, 'reconcile-expected.json'), `${JSON.stringify(reconciled, null, 2)}\n`)
  fs.writeFileSync(path.join(gateDirectory, 'evidence-expected.json'), `${JSON.stringify(evidence, null, 2)}\n`)
  fs.writeFileSync(path.join(gateDirectory, 'change-expected.json'), `${JSON.stringify(changeScenario, null, 2)}\n`)
  fs.writeFileSync(path.join(gateDirectory, 'suggestion-expected.json'),`${JSON.stringify({ review: SUGGESTION_REVIEW, snapshot: suggestionSnapshot }, null, 2)}\n`)
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
