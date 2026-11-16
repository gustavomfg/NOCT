#!/usr/bin/env node
/* global window, document */
// Checks that the shell renders, and that its WebAssembly runs, under the Content-Security-Policy it ships
// with. The page is the Trunk build in crates/noct-ui/dist, served with the policy as a response header and
// opened in the headless Chromium that the repository's Playwright already uses. The check fails when the
// browser reports a policy violation, when the page does not render, or when a click does not reach the
// WebAssembly counter. Chromium is an approximation of the webview of each system; the directives are the
// same, and the webview check is recorded in the migration notes.
//
// Usage:
//   node scripts/check-shell-csp.mjs                    check the policy in crates/noct-app/tauri.conf.json
//   node scripts/check-shell-csp.mjs --csp "<policy>"   check a candidate policy

import fs from 'node:fs'
import http from 'node:http'
import path from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(path.join(root, 'package.json'))
const { chromium } = require('playwright')

const dist = path.join(root, 'crates', 'noct-ui', 'dist')
const config = JSON.parse(fs.readFileSync(path.join(root, 'crates', 'noct-app', 'tauri.conf.json'), 'utf8'))
const flag = process.argv.indexOf('--csp')
const policy = flag === -1 ? config.app.security.csp : process.argv[flag + 1]
if (!policy) {
  console.error('No policy to check.')
  process.exit(2)
}
if (!fs.existsSync(path.join(dist, 'index.html'))) {
  console.error('The UI is not built. Run: cd crates/noct-ui && trunk build')
  process.exit(2)
}

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.wasm': 'application/wasm',
  '.woff2': 'font/woff2',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
}

function serve() {
  const server = http.createServer((request, response) => {
    const requested = decodeURIComponent(new URL(request.url, 'http://127.0.0.1').pathname)
    const file = path.join(dist, requested === '/' ? 'index.html' : requested)
    if (!file.startsWith(dist + path.sep) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      response.writeHead(404).end()
      return
    }
    response.writeHead(200, {
      'Content-Type': TYPES[path.extname(file)] ?? 'application/octet-stream',
      'Content-Security-Policy': policy,
    })
    fs.createReadStream(file).pipe(response)
  })
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)))
}

const browser = await chromium.launch()
const server = await serve()
const result = { policy, rendered: false, clicked: false, violations: [], pageErrors: [] }
try {
  const page = await browser.newPage()
  await page.addInitScript(() => {
    window.__cspViolations = []
    document.addEventListener('securitypolicyviolation', (event) => {
      window.__cspViolations.push(`${event.violatedDirective} ${event.blockedURI || 'inline'}`)
    })
  })
  page.on('pageerror', (error) => result.pageErrors.push(error.message))
  await page.goto(`http://127.0.0.1:${server.address().port}/`)
  const counter = '[data-testid="presses"]'
  await page.waitForFunction(
    (selector) => document.querySelector(selector)?.textContent?.includes('0 toques'),
    counter,
    { timeout: 8000 },
  ).then(() => { result.rendered = true }, () => undefined)
  if (result.rendered) {
    await page.getByRole('button', { name: 'Tocar' }).click()
    await page.waitForFunction(
      (selector) => document.querySelector(selector)?.textContent?.includes('1 toques'),
      counter,
      { timeout: 5000 },
    ).then(() => { result.clicked = true }, () => undefined)
  }
  result.violations = await page.evaluate(() => window.__cspViolations)
} finally {
  await browser.close()
  server.close()
}

const passed = result.rendered && result.clicked && result.violations.length === 0 && result.pageErrors.length === 0
console.log(JSON.stringify({ ...result, passed }, null, 2))
process.exitCode = passed ? 0 : 1
