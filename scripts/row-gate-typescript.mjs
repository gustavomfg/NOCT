// Compiles the TypeScript persistence code for the row gate, under node_modules/.cache, so the gate
// scripts can load the repositories the application runs. It is shared by verify-row-gate.mjs and
// verify-rust-written-gate.mjs. It changes no TypeScript source: the compiled output is only read.
import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'

export const root = path.resolve(import.meta.dirname, '..')
export const buildDirectory = path.join(root, 'node_modules', '.cache', 'noct-row-gate')
export const ts = createRequire(path.join(root, 'package.json'))('typescript')

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

/** Compiles `electron/` and `shared/` into `buildDirectory`. */
export function compileTypeScript() {
  compileTree(path.join(root, 'electron'))
  compileTree(path.join(root, 'shared'))
}

/** The compiled `LocalDatabase` class. Call `compileTypeScript` first. */
export function loadLocalDatabase() {
  return createRequire(path.join(buildDirectory, 'probe.js'))('./electron/database/Database.js').LocalDatabase
}
