// Transpiles electron/database/migrations.ts into the Rust migrations of crates/noct-db.
//
//   node scripts/transpile-migrations.mjs electron/database/migrations.ts \
//     crates/noct-db/src/migrations/generated.rs
//
// Only the constructs that appear in the TypeScript migrations are accepted; anything
// else aborts so that nothing is silently dropped. The SQL is copied byte-for-byte.
// After regenerating, run scripts/generate-schema-fixtures.mjs and the Rust tests:
// they fail if the two implementations diverge.
import { createRequire } from 'node:module'
import fs from 'node:fs'
import path from 'node:path'

const [, , sourceArg, outputPath] = process.argv
if (!sourceArg || !outputPath) {
  console.error('usage: node scripts/transpile-migrations.mjs <migrations.ts> <generated.rs>')
  process.exit(2)
}
const sourcePath = path.resolve(sourceArg)
const ts = createRequire(sourcePath)('typescript')
const text = fs.readFileSync(sourcePath, 'utf8')
const sf = ts.createSourceFile(sourcePath, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)

const fail = (node, message) => {
  const { line, character } = sf.getLineAndCharacterOfPosition(node.getStart(sf))
  throw new Error(`${sourcePath}:${line + 1}:${character + 1}: ${message}`)
}

function rawString(value) {
  let hashes = '#'
  while (value.includes(`"${hashes}`)) hashes += '#'
  return `r${hashes}"${value}"${hashes}`
}

const hasColumnCall = (node) =>
  ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === 'hasColumn'
    ? node
    : null

function columnArgs(call) {
  const [dbArg, tableArg, columnArg] = call.arguments
  if (!dbArg || dbArg.getText(sf) !== 'db' || !ts.isStringLiteral(tableArg) || !ts.isStringLiteral(columnArg)) {
    fail(call, 'unsupported hasColumn arguments')
  }
  return { table: tableArg.text, column: columnArg.text }
}

function leadingComments(node) {
  const ranges = ts.getLeadingCommentRanges(text, node.getFullStart()) ?? []
  return ranges
    .map((range) => text.slice(range.pos, range.end))
    .filter((comment) => comment.startsWith('//'))
    .map((comment) => comment.replace(/\s+$/, ''))
}

function execLiteral(call) {
  if (!ts.isCallExpression(call) || !ts.isPropertyAccessExpression(call.expression)
    || call.expression.expression.getText(sf) !== 'db' || call.expression.name.text !== 'exec') {
    fail(call, 'expected db.exec(...)')
  }
  const [arg] = call.arguments
  if (!arg || !(ts.isNoSubstitutionTemplateLiteral(arg) || ts.isStringLiteral(arg))) fail(call, 'db.exec needs a literal')
  return arg.text
}

function translateStatement(statement, indent) {
  const pad = ' '.repeat(indent)
  const lines = []
  lines.push(...leadingComments(statement).map((comment) => `${pad}${comment}`))
  if (ts.isBlock(statement)) {
    for (const inner of statement.statements) lines.push(...translateStatement(inner, indent))
    return lines
  }
  if (ts.isExpressionStatement(statement)) {
    lines.push(`${pad}conn.execute_batch(${rawString(execLiteral(statement.expression))})?;`)
    return lines
  }
  if (ts.isIfStatement(statement)) {
    let negate = false
    let condition = statement.expression
    if (ts.isPrefixUnaryExpression(condition) && condition.operator === ts.SyntaxKind.ExclamationToken) {
      negate = true
      condition = condition.operand
    }
    const call = hasColumnCall(condition)
    if (!call) fail(statement, 'unsupported if condition')
    const { table, column } = columnArgs(call)
    lines.push(`${pad}if ${negate ? '!' : ''}has_column(conn, ${JSON.stringify(table)}, ${JSON.stringify(column)})? {`)
    lines.push(...translateStatement(statement.thenStatement, indent + 4))
    lines.push(`${pad}}`)
    if (statement.elseStatement) fail(statement, 'else branches are not expected')
    return lines
  }
  fail(statement, `unsupported statement kind ${ts.SyntaxKind[statement.kind]}`)
}

// Locate: const migrations: Migration[] = [ ... ]
let migrationsArray
sf.forEachChild((node) => {
  if (!ts.isVariableStatement(node)) return
  for (const decl of node.declarationList.declarations) {
    if (decl.name.getText(sf) === 'migrations' && decl.initializer && ts.isArrayLiteralExpression(decl.initializer)) {
      migrationsArray = decl.initializer
    }
  }
})
if (!migrationsArray) throw new Error('migrations array not found')

const functions = []
const dispatch = []
for (const element of migrationsArray.elements) {
  if (!ts.isObjectLiteralExpression(element)) fail(element, 'migration must be an object literal')
  let version
  let up
  for (const property of element.properties) {
    if (!ts.isPropertyAssignment(property)) fail(property, 'unsupported migration property')
    const name = property.name.getText(sf)
    if (name === 'version') version = Number(property.initializer.getText(sf))
    if (name === 'up') up = property.initializer
  }
  if (!version || !up || !ts.isArrowFunction(up)) fail(element, 'migration needs version and arrow up()')
  const body = up.body
  const fnName = `v${String(version).padStart(2, '0')}`
  const lines = []
  if (ts.isBlock(body)) {
    for (const statement of body.statements) lines.push(...translateStatement(statement, 4))
  } else {
    // Expression body: `up: (db) => db.exec(...)`
    lines.push(`    conn.execute_batch(${rawString(execLiteral(body))})?;`)
  }
  functions.push(`fn ${fnName}(conn: &Connection) -> Result<()> {\n${lines.join('\n')}\n    Ok(())\n}\n`)
  dispatch.push(`        ${version} => ${fnName}(conn),`)
}

const versions = dispatch.length
const output = `//! Schema migrations 1..=${versions}, ported from electron/database/migrations.ts.
//!
//! GENERATED by scripts/transpile-migrations.mjs. Do not edit by hand.
//! The SQL is copied verbatim from the TypeScript source, so the statements are
//! byte-identical. Conditional steps keep the same \`hasColumn\` guards. Migration 1 is
//! re-run on existing databases (see \`migrate_to\`) and must stay idempotent.

use super::{has_column, Result};
use rusqlite::Connection;

pub(super) fn apply(version: u32, conn: &Connection) -> Result<()> {
    match version {
${dispatch.join('\n')}
        _ => unreachable!("no migration registered for version {version}"),
    }
}

${functions.join('\n')}`

fs.writeFileSync(outputPath, output)
console.log(`wrote ${versions} migrations to ${outputPath}`)
