// Rust writes, TypeScript reads and writes, Rust reads back. The TypeScript repositories are the
// compiled application code (see row-gate-typescript.mjs); this script only loads them. It opens a
// database that Rust created, reads it, writes to it, and leaves the result for Rust to check.
//
//   node scripts/verify-rust-written-gate.mjs
//
// Everything is written under target/, in a temporary folder that is removed afterwards. No
// database is committed, and no path of the user's data is used. verify-row-gate.mjs runs this
// script after its own checks, so CI runs it with the row gate.
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { spawnSync } from 'node:child_process'
import { buildDirectory, compileTypeScript, loadLocalDatabase, root } from './row-gate-typescript.mjs'

const FILE = 'nocturne.db'
const WORKSPACE_ALPHA = '/synthetic/alpha'

// The schema version the TypeScript code declares (shared/constants.ts), read from the compiled output.
function schemaVersion() {
  return createRequire(path.join(buildDirectory, 'probe.js'))('./shared/constants.js').DATABASE_SCHEMA_VERSION
}

// Read before TypeScript opens the file, so the check sees what Rust wrote.
function userVersion(file) {
  const BetterSqlite3 = createRequire(path.join(root, 'package.json'))('better-sqlite3')
  const database = new BetterSqlite3(file, { readonly: true, fileMustExist: true })
  try {
    return database.pragma('user_version', { simple: true })
  } finally {
    database.close()
  }
}

// The rows the Rust side reads back, in the JSON form both sides compare. A field that is undefined
// or a class instance cannot make the two sides differ.
const normalise = (value) => JSON.parse(JSON.stringify(value))

function logicalState(database) {
  const conversations = database.conversations.list()
  return {
    workspaces: database.workspaces.list(),
    settings: database.settings.get(),
    conversations,
    messages: Object.fromEntries(
      conversations.map((conversation) => [conversation.id, database.conversations.listMessages(conversation.id)]),
    ),
  }
}

function runRustTest(directory, test) {
  const run = spawnSync(
    'cargo',
    ['test', '-p', 'noct-db', '--test', 'rust_written_gate', test, '--', '--ignored', '--nocapture'],
    {
      cwd: root,
      stdio: 'inherit',
      env: { ...process.env, NOCT_RUST_WRITTEN_DIR: directory },
    },
  )
  if (run.status !== 0) throw new Error(`${test} failed with status ${run.status ?? 'signal'}`)
}

const directory = path.join(root, 'target', `rust-written-gate-${process.pid}`)
fs.rmSync(directory, { recursive: true, force: true })
fs.mkdirSync(directory, { recursive: true })
try {
  compileTypeScript()
  const LocalDatabase = loadLocalDatabase()
  const schema = schemaVersion()

  // 1. Rust creates the database through its repositories, and records the rows it wrote.
  runRustTest(directory, 'rust_writes_the_rows_the_typescript_repositories_read')
  assert.equal(userVersion(path.join(directory, FILE)), schema, 'the Rust database is at the schema TypeScript declares')
  const rustRows = JSON.parse(fs.readFileSync(path.join(directory, 'rust-written-expected.json'), 'utf8'))

  // 2. TypeScript opens that database and reads the same rows.
  const database = new LocalDatabase(directory)
  try {
    assert.deepStrictEqual(normalise(logicalState(database)), rustRows, 'TypeScript reads what Rust wrote')

    // 3. TypeScript writes into the Rust database, through its own repositories.
    database.settings.set({ theme: 'dark' })
    database.workspaces.setFavorite(WORKSPACE_ALPHA, true)
    const [target] = database.conversations.list().filter((conversation) => conversation.workspace === WORKSPACE_ALPHA)
    assert.ok(target, 'the conversation Rust wrote is listed by TypeScript')
    database.conversations.addMessage(target.id, 'assistant', 'Resposta escrita pelo TypeScript depois do Rust.', { model: 'local' })
    database.conversations.renameFromPrompt(target.id, 'Título reescrito pelo TypeScript')
    fs.writeFileSync(
      path.join(directory, 'typescript-written-expected.json'),
      `${JSON.stringify(normalise(logicalState(database)), null, 2)}\n`,
    )
  } finally {
    database.close()
  }

  // 4. Rust opens the database after TypeScript wrote to it, and compares every row.
  runRustTest(directory, 'rust_reads_what_the_typescript_repositories_wrote')
  console.log('verify-rust-written-gate: Rust writes, TypeScript reads and writes, Rust reads back: passed')
} catch (error) {
  console.error(error instanceof Error ? error.message : error)
  process.exitCode = 1
} finally {
  fs.rmSync(directory, { recursive: true, force: true })
}
