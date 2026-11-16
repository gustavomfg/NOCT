#!/usr/bin/env node
/* global window, document */
// Checks the settings area of the shell's interface in a browser, with a mocked shell. The page is the Trunk
// build in crates/noct-ui/dist, served with the Content-Security-Policy the shell ships with. Before the
// page runs, the mock defines `window.__TAURI_INTERNALS__.invoke`, which is the function the shell's webview
// defines (tauri-2.12.5/scripts/core.js). Each scenario answers `settings_get` in a different way.
//
// What this proves: the interface calls `settings_get` with an empty payload, shows the loading state, the
// settings, the shell's error and the way to try again, refuses a reply outside the contract, reports a page
// outside the shell, and sets no inline style under the policy. What it does not prove: the shell's real
// IPC, or the webview of each system. Chromium is an approximation of the webview; the real window is checked
// by hand and recorded in docs/rust-shell.md.
//
// Usage: node scripts/check-shell-ui.mjs

import fs from 'node:fs'
import http from 'node:http'
import path from 'node:path'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(path.join(root, 'package.json'))
const { chromium } = require('playwright')

// The version of Tauri whose `__TAURI_INTERNALS__.invoke` the mock follows (tauri-2.12.5/scripts/core.js).
// `crates/noct-app/tests/shell_ui_mock.rs` fails when Cargo.lock names another version, so a Tauri update is
// not taken until the mock has been checked against it.
const MOCKED_TAURI_VERSION = '2.12.5'

const dist = path.join(root, 'crates', 'noct-ui', 'dist')
const config = JSON.parse(fs.readFileSync(path.join(root, 'crates', 'noct-app', 'tauri.conf.json'), 'utf8'))
const policy = config.app.security.csp
if (!fs.existsSync(path.join(dist, 'index.html'))) {
  console.error('The UI is not built. Run: cd crates/noct-ui && trunk build --release')
  process.exit(2)
}

const SETTINGS = {
  approvalPolicy: 'untrusted',
  diagnosticMode: true,
  theme: 'light',
  language: 'en',
  pandocVersion: '3.1.2',
}
const SHOWN = [
  ['Tema', 'NOCT claro'],
  ['Idioma', 'English'],
  ['Segurança do agente', 'Pedir aprovação antes de cada alteração de arquivo'],
  ['Diagnóstico', 'Ativo'],
  ['Pandoc', '3.1.2'],
]
const SHELL_ERROR = { code: 'persistence', message: 'Não foi possível acessar os dados locais.' }

// `slow` waits until the check releases the reply, so the loading state can be seen.
const SCENARIOS = [
  { name: 'ready', mode: 'ready' },
  { name: 'slow-then-ready', mode: 'slow' },
  { name: 'error-then-retry', mode: 'error-then-retry' },
  { name: 'reply-outside-the-contract', mode: 'invalid' },
  { name: 'no-shell', mode: 'no-runtime' },
]

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

// Runs before the page's own scripts. The mock answers `invoke` the way the scenario says, and records each
// call so the check can tell that the interface asked for `settings_get` with an empty payload.
function mockFor(scenario) {
  return { scenario, settings: SETTINGS, shellError: SHELL_ERROR }
}
function installMock(config) {
  window.__noctCalls = []
  window.__noctAttempts = 0
  if (config.scenario.mode === 'no-runtime') return
  window.__TAURI_INTERNALS__ = {
    invoke(command, payload) {
      window.__noctCalls.push({ command, payload })
      window.__noctAttempts += 1
      const attempt = window.__noctAttempts
      return new Promise((resolve, reject) => {
        const answer = () => {
          switch (config.scenario.mode) {
            case 'ready':
              resolve(config.settings)
              break
            case 'error-then-retry':
              if (attempt === 1) reject(config.shellError)
              else resolve(config.settings)
              break
            case 'invalid':
              resolve({ approvalPolicy: 'always' })
              break
            case 'slow':
              window.__releaseSettings = () => resolve(config.settings)
              break
          }
        }
        if (config.scenario.mode === 'slow') answer()
        else setTimeout(answer, 0)
      })
    },
  }
}

const browser = await chromium.launch()
const server = await serve()
const results = []
try {
  for (const scenario of SCENARIOS) {
    const result = { scenario: scenario.name, passed: false, failures: [], violations: [], pageErrors: [] }
    const page = await browser.newPage()
    await page.addInitScript(() => {
      window.__cspViolations = []
      document.addEventListener('securitypolicyviolation', (event) => {
        window.__cspViolations.push(`${event.violatedDirective} ${event.blockedURI || 'inline'}`)
      })
    })
    await page.addInitScript(installMock, mockFor(scenario))
    page.on('pageerror', (error) => result.pageErrors.push(error.message))
    const check = (condition, message) => { if (!condition) result.failures.push(message) }

    try {
      await page.goto(`http://127.0.0.1:${server.address().port}/`)
      const values = page.getByTestId('settings-values')

      if (scenario.mode === 'slow') {
        const loading = page.getByRole('status')
        await loading.waitFor({ timeout: 8000 })
        check((await loading.textContent())?.includes('Carregando configurações'), 'the loading state is shown')
        await page.evaluate(() => window.__releaseSettings())
        await values.waitFor({ timeout: 8000 })
      }

      if (scenario.mode === 'ready' || scenario.mode === 'error-then-retry') {
        if (scenario.mode === 'error-then-retry') {
          const alert = page.getByRole('alert')
          await alert.waitFor({ timeout: 8000 })
          check((await alert.textContent())?.includes(SHELL_ERROR.message), 'the shell error message is shown')
          await page.getByRole('button', { name: 'Tentar novamente' }).click()
        }
        await values.waitFor({ timeout: 8000 })
        for (const [label, value] of SHOWN) {
          const pair = page.locator('dt', { hasText: label }).locator('xpath=following-sibling::dd[1]')
          check((await pair.textContent())?.trim() === value, `${label} shows ${value}`)
        }
      }

      if (scenario.mode === 'invalid') {
        const alert = page.getByRole('alert')
        await alert.waitFor({ timeout: 8000 })
        check((await alert.textContent())?.includes('O NOCT respondeu de forma inesperada.'), 'an unexpected reply is an error')
        check(await values.count() === 0, 'an unexpected reply shows no values')
      }

      if (scenario.mode === 'no-runtime') {
        const alert = page.getByRole('alert')
        await alert.waitFor({ timeout: 8000 })
        check((await alert.textContent())?.includes('Não foi possível falar com o NOCT'), 'a page outside the shell says so')
        check(await values.count() === 0, 'no values are shown without the shell')
      }

      // The skeleton keeps working alongside the settings area.
      await page.getByRole('button', { name: 'Tocar' }).click()
      await page.getByTestId('presses').filter({ hasText: '1 toques' }).waitFor({ timeout: 5000 })

      const calls = await page.evaluate(() => window.__noctCalls ?? [])
      if (scenario.mode !== 'no-runtime') {
        check(calls.length >= 1, 'the interface calls the shell')
        check(calls.every((call) => call.command === 'settings_get'), 'only settings_get is called')
        check(calls.every((call) => JSON.stringify(call.payload) === '{}'), 'the payload is empty')
      }
      if (scenario.mode === 'error-then-retry') {
        check(calls.length === 2, 'a retry calls the shell once more')
      }

      const inline = await page.evaluate(() => document.querySelectorAll('[style], style').length)
      check(inline === 0, 'no inline style is set in the page')
      result.violations = await page.evaluate(() => window.__cspViolations)
      check(result.violations.length === 0, 'the policy reports no violation')
    } catch (error) {
      result.failures.push(`the scenario did not finish: ${error.message}`)
    } finally {
      await page.close()
    }
    check(result.pageErrors.length === 0, 'the page throws no error')
    result.passed = result.failures.length === 0
    results.push(result)
  }
} finally {
  await browser.close()
  server.close()
}

const passed = results.every((result) => result.passed)
console.log(JSON.stringify({ mockedTauriVersion: MOCKED_TAURI_VERSION, policy, passed, results }, null, 2))
process.exitCode = passed ? 0 : 1
