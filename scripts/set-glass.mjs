#!/usr/bin/env node
// Switches the glass layer of the shell's window for a comparison: `on` paints the panes with blur over the
// gradient, `off` keeps the solid colours. It changes one attribute, data-glass, on the <html> element of
// crates/noct-ui/index.html, and nothing else. The committed default is `on`.
//
// Usage:
//   node scripts/set-glass.mjs status
//   node scripts/set-glass.mjs off      # then start the shell again, see docs/rust-shell.md
//   node scripts/set-glass.mjs on       # restores the default
//
// Restart the dev run after a switch (`node scripts/run-rust-shell.mjs dev`), so the window loads the page
// again; whether Trunk's live reload reaches the window under the shell's policy has not been checked. Revert
// before committing: the Rust test
// `the_window_starts_with_the_glass_layer_on` fails while the page is off.

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const page = path.join(root, 'crates', 'noct-ui', 'index.html')
const VALUES = ['on', 'off']
const ATTRIBUTE = /(<html\b[^>]*\sdata-glass=")(on|off)(")/

function current(text) {
  const match = text.match(ATTRIBUTE)
  if (!match) {
    throw new Error('crates/noct-ui/index.html has no data-glass attribute on its <html> element')
  }
  return match[2]
}

const command = process.argv[2]
if (!command || !['status', ...VALUES].includes(command)) {
  console.error('Usage: node scripts/set-glass.mjs status|on|off')
  process.exit(2)
}

const text = fs.readFileSync(page, 'utf8')
const before = current(text)

if (command === 'status') {
  console.log(`data-glass="${before}" in crates/noct-ui/index.html`)
  process.exit(0)
}

if (before === command) {
  console.log(`data-glass is already "${command}"; nothing changed`)
  process.exit(0)
}

fs.writeFileSync(page, text.replace(ATTRIBUTE, `$1${command}$3`))
console.log(`data-glass changed from "${before}" to "${command}" in crates/noct-ui/index.html`)
console.log('Restart the shell to see it. Run `node scripts/set-glass.mjs on` to restore the default.')
