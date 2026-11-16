// Checks the launcher's options and the copy of a release-line fixture. The fixture is built from the sources
// of the pinned tag, so this needs the tags of the repository and the Node dependencies (npm ci).
// Run with: node --test scripts/run-rust-shell.test.mjs
import assert from 'node:assert/strict'
import fs from 'node:fs'
import { createRequire } from 'node:module'
import path from 'node:path'
import { test } from 'node:test'
import { copyFixture, parseOptions } from './run-rust-shell.mjs'

const root = path.resolve(import.meta.dirname, '..')
const fixturesDirectory = path.join(root, 'target', 'shell-fixtures')

test('a data folder must be absolute', () => {
  assert.throws(() => parseOptions(['--data-dir', 'relative']), /needs an absolute path/)
})

test('a value option needs its value', () => {
  assert.throws(() => parseOptions(['--from-fixture']), /--from-fixture needs a value/)
  assert.throws(() => parseOptions(['--from-fixture', '--check']), /--from-fixture needs a value/)
  assert.throws(() => parseOptions(['--data-dir']), /--data-dir needs a value/)
})

test('an unknown argument is refused', () => {
  assert.throws(() => parseOptions(['--pandoc']), /unknown argument: --pandoc/)
})

test('a data folder and a fixture cannot be combined', () => {
  assert.throws(
    () => parseOptions(['--data-dir', path.resolve('some-data'), '--from-fixture', 'v1.0.2']),
    /cannot be used together/,
  )
})

test('the options are read as they are written', () => {
  const dataDirectory = path.resolve('some-data')

  assert.deepEqual(parseOptions(['--check', '--data-dir', dataDirectory]), {
    check: true,
    dataDirectory,
    fixture: undefined,
  })
  assert.deepEqual(parseOptions(['--from-fixture', 'v1.0.2']), {
    check: false,
    dataDirectory: undefined,
    fixture: 'v1.0.2',
  })
  assert.deepEqual(parseOptions([]), { check: false, dataDirectory: undefined, fixture: undefined })
})

test('only a pinned release line can be a fixture', async () => {
  await assert.rejects(copyFixture('v9.9.9'), /is not a pinned release line/)
})

test('a fixture is copied into a new folder under target/, and the copy holds the seeded workspace', async () => {
  const first = await copyFixture('v1.0.2')
  const second = await copyFixture('v1.0.2')
  try {
    assert.ok(path.isAbsolute(first), 'the folder is named by an absolute path')
    assert.ok(first.startsWith(fixturesDirectory + path.sep), 'the folder is under target/shell-fixtures')
    assert.notEqual(first, second, 'each run gets a folder of its own')
    assert.deepEqual(
      fs.readdirSync(first).sort(),
      ['nocturne.db'],
      'only the database is copied, without sidecar files',
    )

    const Database = createRequire(path.join(root, 'package.json'))('better-sqlite3')
    const database = new Database(path.join(first, 'nocturne.db'), { readonly: true, fileMustExist: true })
    try {
      const workspaces = database.prepare('SELECT name, favorite FROM workspaces').all()
      assert.deepEqual(workspaces, [{ name: 'fixture-project', favorite: 1 }])
    } finally {
      database.close()
    }
  } finally {
    fs.rmSync(first, { recursive: true, force: true })
    fs.rmSync(second, { recursive: true, force: true })
  }
})
