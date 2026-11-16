// Release-line fixtures of phase 2 (ADR 0001).
//
// Each tagged release line writes its own database, through its own LocalDatabase compiled from
// the sources of that tag. The current TypeScript code upgrades a copy of that database: the
// reference. The Rust layer must reach the same schema and the same rows from the same legacy
// file, and each migration must have the effect that `scripts/fixtures/release-lines.json` names.
//
//   node scripts/release-lines.mjs --pin   recomputes the commit and source digest of every line
//
// `scripts/verify-row-gate.mjs` calls `writeReleaseLines`; `crates/noct-db/tests/row_gate.rs`
// reads the manifest and does the comparison. Only synthetic data is written, and every file goes
// under node_modules/.cache or the gate directory in target/, never into the repository.
import fs from 'node:fs'
import path from 'node:path'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const root = path.resolve(import.meta.dirname, '..')
const requireFromRoot = createRequire(path.join(root, 'package.json'))
const ts = requireFromRoot('typescript')
const pinsFile = path.join(root, 'scripts', 'fixtures', 'release-lines.json')
const cacheDirectory = path.join(root, 'node_modules', '.cache', 'noct-release-lines')

// Fixed instant: the seeded rows are the same on every run.
const AT = '2026-01-01T12:00:00.000Z'

// Which release lines expose each write path. A line that cannot write a collection does not
// get it; a write that is exposed and fails stops the gate, it is never replaced by SQL.
const EXPOSES = {
  suggestions: ['v0.5.0-beta', 'v0.6.0-beta', 'v0.7.0-beta', 'v0.8.0-beta', 'v0.9.0-beta'],
  brain: ['v0.9.0-beta'],
  providers: ['v0.9.0-beta', 'v0.9.5-beta', 'v1.0.0', 'v1.0.1', 'v1.0.2'],
  codexThread: ['v0.5.0-beta', 'v0.6.0-beta', 'v0.7.0-beta', 'v0.8.0-beta', 'v0.9.5-beta', 'v1.0.0', 'v1.0.1', 'v1.0.2'],
  changes: ['v1.0.1', 'v1.0.2'],
  projectIndex: ['v1.0.1'],
}

const git = (args) => execFileSync('git', args, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 })

export function readPins() {
  return JSON.parse(fs.readFileSync(pinsFile, 'utf8'))
}

function commitOf(tag) {
  return git(['rev-parse', '--verify', `${tag}^{commit}`]).trim()
}

// The TypeScript sources of a release line, read from its commit. The digest covers every file
// name and content, so a moved tag or an edited source changes it.
function sourcesOf(commit) {
  const files = git(['ls-tree', '-r', '--name-only', commit, '--', 'electron', 'shared'])
    .split('\n')
    .filter((file) => file.endsWith('.ts') && !file.endsWith('.d.ts'))
  const digest = createHash('sha256')
  const contents = new Map()
  for (const file of files) {
    const content = execFileSync('git', ['show', `${commit}:${file}`], { cwd: root, maxBuffer: 64 * 1024 * 1024 })
    contents.set(file, content)
    digest.update(`${file}\0${createHash('sha256').update(content).digest('hex')}\n`)
  }
  return { sourcesSha256: digest.digest('hex'), contents }
}

function transpileTree(sourceDirectory, outputDirectory) {
  for (const entry of fs.readdirSync(sourceDirectory, { withFileTypes: true })) {
    const source = path.join(sourceDirectory, entry.name)
    if (entry.isDirectory()) {
      transpileTree(source, path.join(outputDirectory, entry.name))
      continue
    }
    if (!entry.name.endsWith('.ts') || entry.name.endsWith('.d.ts')) continue
    const { outputText } = ts.transpileModule(fs.readFileSync(source, 'utf8'), {
      fileName: source,
      compilerOptions: {
        module: ts.ModuleKind.CommonJS,
        target: ts.ScriptTarget.ES2022,
        esModuleInterop: true,
      },
    })
    fs.mkdirSync(outputDirectory, { recursive: true })
    fs.writeFileSync(path.join(outputDirectory, entry.name.replace(/\.ts$/, '.js')), outputText)
  }
}

// Compiles one pinned release line into the cache. A moved tag, or sources that differ from the
// pinned digest, stop the gate here. The cache is under node_modules, so Node loads the output as
// CommonJS regardless of the root package type.
export function materialize(line) {
  const commit = commitOf(line.tag)
  if (commit !== line.commit) {
    throw new Error(`${line.tag} resolves to ${commit}, but the fixtures pin ${line.commit}: the tag moved`)
  }
  const { sourcesSha256, contents } = sourcesOf(commit)
  if (sourcesSha256 !== line.sourcesSha256) {
    throw new Error(`${line.tag}: sources digest ${sourcesSha256} differs from the pinned ${line.sourcesSha256}`)
  }
  const lineDirectory = path.join(cacheDirectory, line.tag)
  const sourceDirectory = path.join(lineDirectory, 'src')
  fs.rmSync(lineDirectory, { recursive: true, force: true })
  for (const [file, content] of contents) {
    const target = path.join(sourceDirectory, file)
    fs.mkdirSync(path.dirname(target), { recursive: true })
    fs.writeFileSync(target, content)
  }
  const buildDirectory = path.join(lineDirectory, 'build')
  transpileTree(sourceDirectory, buildDirectory)
  return buildDirectory
}

// The fixed seed of one release line, written through that line's own LocalDatabase. Every step
// is recorded; any failed step throws with the names of the failures.
export function seedLine(LocalDatabase, tag, directory) {
  fs.rmSync(directory, { recursive: true, force: true })
  fs.mkdirSync(directory, { recursive: true })
  const workspace = path.join(directory, 'fixture-project')
  fs.mkdirSync(workspace)
  const steps = {}
  const run = (name, action) => {
    try {
      const value = action()
      steps[name] = 'ok'
      return value
    } catch (error) {
      steps[name] = `error: ${String(error?.message ?? error).split('\n')[0]}`
      return undefined
    }
  }

  const database = run('construct', () => new LocalDatabase(directory))
  if (database) {
    const conversation = run('createConversation', () => database.createConversation(workspace))
    if (conversation) {
      run('addMessage.user', () => database.addMessage(conversation.id, 'user', 'pergunta do fixture'))
      run('addMessage.assistant', () => database.addMessage(conversation.id, 'assistant', 'resposta do fixture'))
      run('addArtifact', () => database.addArtifact(conversation.id, workspace, 'markdown', 'Relatório do fixture', null, '# Relatório'))
    }
    run('setSettings.base', () => database.setSettings({ 'fixture.theme': 'dark', 'fixture.locale': 'pt-BR' }))
    run('setWorkspaceMemory', () => database.setWorkspaceMemory(workspace, 'memória do fixture'))
    run('setWorkspaceFavorite', () => database.setWorkspaceFavorite(workspace, true))
    run('recordApproval', () => database.recordApproval('fixture-key', true, 'npm test', 'low'))

    // M32 deletes these four keys.
    run('setSettings.vendor', () => database.setSettings({
      aiEngine: 'codex', codexPath: '/usr/bin/codex', codexModel: 'fixture-model', codexReasoningEffort: 'high',
    }))

    if (conversation && EXPOSES.suggestions.includes(tag)) {
      // The release's own state machine allows pending -> accepted -> applied, so the applied
      // suggestion takes that path.
      const value = {
        title: 'Revisar tratamento de erro', description: 'Descrição do fixture', reasoning: 'Justificativa do fixture',
        category: 'bug', severity: 'medium', affectedFiles: ['src/a.ts'], proposedChanges: 'Ajustar o fluxo',
        expectedBenefits: ['clareza'], complexity: 'low', risk: 'low',
      }
      run('suggestions.pending', () => database.addSuggestion(conversation.id, workspace, value))
      const applied = run('suggestions.applied', () => database.addSuggestion(conversation.id, workspace, { ...value, title: 'Aplicada no fixture' }))
      const rejected = run('suggestions.rejected', () => database.addSuggestion(conversation.id, workspace, { ...value, title: 'Rejeitada no fixture' }))
      if (applied) {
        run('suggestions.applied.accepted', () => database.setSuggestionStatus(applied.id, 'accepted'))
        run('suggestions.applied.status', () => database.setSuggestionStatus(applied.id, 'applied', 'resultado do fixture'))
      }
      if (rejected) run('suggestions.rejected.status', () => database.setSuggestionStatus(rejected.id, 'rejected'))
    }

    if (EXPOSES.brain.includes(tag)) {
      run('brain.created', () => database.createBrainMemory(workspace, {
        kind: 'fact', scope: 'workspace', content: 'Memória do fixture A', status: 'active', sourceType: 'manual',
      }))
      run('brain.candidate', () => database.createBrainMemory(workspace, {
        kind: 'decision', scope: 'workspace', content: 'Memória do fixture B', status: 'candidate', sourceType: 'manual',
      }))
    }

    if (EXPOSES.providers.includes(tag)) {
      run('providers.create', () => database.providerConfigurations.create({
        providerType: 'openai-compatible', displayName: 'Provedor do fixture', source: 'local',
        baseUrl: 'http://127.0.0.1:1234/v1', enabled: true, requiresAuthentication: false, timeoutMs: 30000,
      }))
    }

    if (conversation && EXPOSES.codexThread.includes(tag)) {
      // Releases 0.5 to 0.8 write the thread through setThread; later ones through the named setter.
      const setter = typeof database.setConversationCodexThread === 'function' ? 'setConversationCodexThread' : 'setThread'
      run(`codexThread.${setter}`, () => database[setter](conversation.id, 'thread-fixture-1'))
    }

    if (conversation && EXPOSES.changes.includes(tag)) {
      const execution = {
        id: 'exec-fixture-1', workspace, conversationId: conversation.id, prompt: 'Prompt do fixture', mode: 'build',
        status: 'completed', decision: 'pending', retryOf: null, startedAt: AT, finishedAt: AT, error: null,
      }
      run('changes.execution', () => database.executions.create(execution))
      const checkpoint = (id, phase) => ({
        id, executionId: execution.id, workspace, phase, status: 'ready', capturedAt: AT, rootPath: workspace, error: null,
      })
      run('changes.checkpoint.before', () => database.checkpoints.create(checkpoint('cp-before-1', 'before')))
      run('changes.checkpoint.after', () => database.checkpoints.create(checkpoint('cp-after-1', 'after')))
      const changeSet = {
        id: 'cs-fixture-1', executionId: execution.id, beforeCheckpointId: 'cp-before-1', afterCheckpointId: 'cp-after-1',
        status: 'pending', createdAt: AT, updatedAt: AT,
      }
      const change = (id, relativePath, origin, operation) => ({
        id, executionId: execution.id, changeSetId: changeSet.id, checkpointId: 'cp-after-1', relativePath, originalPath: null,
        operation, origin, beforeHash: null, afterHash: null, beforeSize: null, afterSize: null, status: 'pending',
        validationStatus: 'unknown', policy: 'allowed', policyReason: null, createdAt: AT, updatedAt: AT,
      })
      run('changes.save', () => database.changeSets.save(changeSet, [
        change('chg-file-1', 'src/a.ts', 'codex-file-change', 'modify'),
        change('chg-command-1', 'src/b.ts', 'codex-command', 'create'),
      ]))
    }

    if (EXPOSES.projectIndex.includes(tag)) {
      const hash = (char) => char.repeat(64)
      const file = {
        workspace, relativePath: 'src/index.ts', classification: 'source', language: 'typescript', extension: '.ts',
        size: 120, mtimeMs: 1, ctimeMs: 1, mode: 420, observedHash: hash('a'), analyzedHash: hash('a'),
        state: 'indexed', excluded: false, exclusionReason: null, parserId: 'typescript', parserVersion: '1',
        error: null, discoveredAt: AT, analyzedAt: AT,
      }
      run('projectIndex.saveFileAnalysis', () => database.projectIndex.saveFileAnalysis({
        file,
        symbols: [],
        imports: [{
          id: 'imp-fixture-1', workspace, sourcePath: 'src/index.ts', sourceHash: hash('a'), specifier: './util',
          targetPath: 'src/util.ts', targetHash: hash('b'), kind: 'named', importedNames: ['util'],
          location: { startLine: 1, startColumn: 1, endLine: 1, endColumn: 30 }, resolution: 'local',
        }],
        exports: [{
          id: 'exp-fixture-1', workspace, sourcePath: 'src/index.ts', sourceHash: hash('a'), name: 'main', kind: 'named',
          targetPath: null, targetHash: null, location: { startLine: 2, startColumn: 1, endLine: 2, endColumn: 20 },
        }],
      }))
    }

    run('close', () => database.close())
  }

  const failures = Object.entries(steps).filter(([, value]) => value !== 'ok')
  if (failures.length > 0) {
    throw new Error(`${tag}: ${failures.map(([name, value]) => `${name} ${value}`).join('; ')}`)
  }
  return steps
}

function removeSidecars(file) {
  for (const suffix of ['-wal', '-shm']) fs.rmSync(`${file}${suffix}`, { force: true })
}

// For every release line: the legacy file that the release wrote, and the reference that the
// current TypeScript code upgrades from a copy of it. Then the manifest the Rust test reads.
export function writeReleaseLines(gateDirectory, CurrentLocalDatabase) {
  const pins = readPins()
  const outputDirectory = path.join(gateDirectory, 'release-lines')
  const lines = pins.lines.map((line) => {
    const buildDirectory = materialize(line)
    const { LocalDatabase } = createRequire(path.join(buildDirectory, 'index.cjs'))('./electron/database/Database.js')
    const lineDirectory = path.join(outputDirectory, line.tag)
    const legacyDirectory = path.join(lineDirectory, 'legacy')
    seedLine(LocalDatabase, line.tag, legacyDirectory)
    const legacyFile = path.join(legacyDirectory, 'nocturne.db')
    removeSidecars(legacyFile)

    const referenceDirectory = path.join(lineDirectory, 'reference')
    fs.rmSync(referenceDirectory, { recursive: true, force: true })
    fs.mkdirSync(referenceDirectory, { recursive: true })
    fs.copyFileSync(legacyFile, path.join(referenceDirectory, 'nocturne.db'))
    new CurrentLocalDatabase(referenceDirectory).close()
    removeSidecars(path.join(referenceDirectory, 'nocturne.db'))

    return {
      tag: line.tag,
      legacy: path.relative(gateDirectory, legacyFile),
      reference: path.relative(gateDirectory, path.join(referenceDirectory, 'nocturne.db')),
    }
  })

  fs.mkdirSync(outputDirectory, { recursive: true })
  fs.writeFileSync(
    path.join(outputDirectory, 'manifest.json'),
    `${JSON.stringify({ lines, expectations: pins.expectations, clockCells: pins.clockCells }, null, 2)}\n`,
  )
}

// The fixture database of one pinned release line, written into `directory` with the synthetic data of
// `seedLine`: the file that the release itself leaves behind. The shell starts from a copy of it through
// `scripts/run-rust-shell.mjs --from-fixture`. Returns the path of the database file.
export function writeFixture(tag, directory) {
  const line = readPins().lines.find((candidate) => candidate.tag === tag)
  if (!line) throw new Error(`${tag} is not a pinned release line`)
  const buildDirectory = materialize(line)
  const { LocalDatabase } = createRequire(path.join(buildDirectory, 'index.cjs'))('./electron/database/Database.js')
  seedLine(LocalDatabase, tag, directory)
  const file = path.join(directory, 'nocturne.db')
  removeSidecars(file)
  return file
}

// Recomputes the commit and source digest of every line. Run by hand; the diff is reviewed.
export function pin() {
  const pins = readPins()
  for (const line of pins.lines) {
    line.commit = commitOf(line.tag)
    line.sourcesSha256 = sourcesOf(line.commit).sourcesSha256
  }
  fs.writeFileSync(pinsFile, `${JSON.stringify(pins, null, 2)}\n`)
  return pins.lines
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv.includes('--pin')) {
    for (const line of pin()) console.log(`${line.tag} ${line.commit} ${line.sourcesSha256}`)
  } else {
    console.log('usage: node scripts/release-lines.mjs --pin')
    process.exitCode = 2
  }
}
