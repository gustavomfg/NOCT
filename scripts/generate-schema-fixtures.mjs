// Generates the schema fingerprints that crates/noct-db compares against.
//
// The TypeScript migrations are the reference. Every scenario is built with the real
// migrateDatabase() on an in-memory database and fingerprinted with the canonical dump
// implemented in crates/noct-db/src/schema.rs. Keep the two dump functions in sync.
//
// Scenarios:
//   fresh          migrations 1..latest applied to an empty database
//   state-NN       the database as it looks after migration NN (a supported legacy state)
//   upgrade-NN     state-NN upgraded to latest, the path an existing user takes
//
//   node scripts/generate-schema-fixtures.mjs
import Database from 'better-sqlite3'
import { createHash } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { DATABASE_SCHEMA_VERSION } from '../shared/constants.ts'
import { migrateDatabase, migrations } from '../electron/database/migrations.ts'

const outputPath = path.join(import.meta.dirname, '..', 'crates', 'noct-db', 'tests', 'fixtures', 'schema-hashes.json')

function canonicalDump(db) {
  const lines = [`user_version\t${db.pragma('user_version', { simple: true })}`]
  const objects = db
    .prepare('SELECT type, name, tbl_name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name')
    .all()
  for (const row of objects) {
    const normalized = row.sql.trim().split(/\s+/).join(' ')
    lines.push(['obj', row.type, row.name, row.tbl_name, normalized].join('\t'))
  }
  const tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").all()
  for (const { name: table } of tables) {
    for (const column of db.prepare(`PRAGMA table_info(${table})`).all()) {
      lines.push([
        'col',
        table,
        column.cid,
        column.name,
        column.type,
        column.notnull,
        column.dflt_value ?? 'null',
        column.pk,
      ].join('\t'))
    }
  }
  return lines.map((line) => `${line}\n`).join('')
}

const fingerprint = (db) => createHash('sha256').update(canonicalDump(db), 'utf8').digest('hex')
const pad = (version) => String(version).padStart(2, '0')

const latest = migrations.at(-1).version
if (latest !== DATABASE_SCHEMA_VERSION) {
  throw new Error(`migrations end at ${latest} but shared/constants.ts declares ${DATABASE_SCHEMA_VERSION}`)
}

const hashes = {}

const fresh = new Database(':memory:')
migrateDatabase(fresh, 0)
hashes.fresh = fingerprint(fresh)
fresh.close()

for (let version = 1; version < latest; version += 1) {
  const db = new Database(':memory:')
  migrateDatabase(db, 0, migrations.filter((migration) => migration.version <= version))
  hashes[`state-${pad(version)}`] = fingerprint(db)
  migrateDatabase(db, version)
  hashes[`upgrade-${pad(version)}`] = fingerprint(db)
  db.close()
}

const fixture = {
  generatedBy: 'scripts/generate-schema-fixtures.mjs',
  latest,
  hashes,
}
fs.mkdirSync(path.dirname(outputPath), { recursive: true })
fs.writeFileSync(outputPath, `${JSON.stringify(fixture, null, 2)}\n`)
console.log(`wrote ${Object.keys(hashes).length} schema fingerprints to ${outputPath}`)
