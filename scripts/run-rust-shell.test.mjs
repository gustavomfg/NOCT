// Checks the launcher's options. Run with: node --test scripts/run-rust-shell.test.mjs
import assert from 'node:assert/strict'
import path from 'node:path'
import { test } from 'node:test'
import { parseOptions } from './run-rust-shell.mjs'

test('a data folder must be absolute', () => {
  assert.throws(() => parseOptions(['--data-dir', 'relative']), /needs an absolute path/)
})

test('a value option needs its value', () => {
  assert.throws(() => parseOptions(['--data-dir']), /--data-dir needs a value/)
  assert.throws(() => parseOptions(['--data-dir', '--check']), /--data-dir needs a value/)
})

test('an unknown argument is refused', () => {
  assert.throws(() => parseOptions(['--pandoc']), /unknown argument: --pandoc/)
})

test('the release-line fixture option is removed with the TypeScript side', () => {
  assert.throws(() => parseOptions(['--from-fixture', 'v1.0.2']), /unknown argument: --from-fixture/)
})

test('the options are read as they are written', () => {
  const dataDirectory = path.resolve('some-data')

  assert.deepEqual(parseOptions(['--check', '--data-dir', dataDirectory]), {
    check: true,
    dataDirectory,
  })
  assert.deepEqual(parseOptions([]), { check: false, dataDirectory: undefined })
})
