#!/usr/bin/env node
/* global window, document, getComputedStyle, matchMedia */
// Checks the interface of the shell in a browser, with a mocked shell. The page is the Trunk build in
// crates/noct-ui/dist, served with the Content-Security-Policy the shell ships with. Before the page runs, the
// mock defines `window.__TAURI_INTERNALS__.invoke`, which is the function the shell's webview defines
// (tauri-2.12.5/scripts/core.js). Each scenario answers `settings_get` in a different way.
//
// What this proves: the interface calls `settings_get` with an empty payload; it shows the loading state, the
// settings, the shell's error and the way to try again; it refuses a reply outside the contract; it reports a
// page outside the shell; the theme follows the saved setting or the system, and the sidebar button changes it
// for the session; the three modes and Ajustes switch and keep their state (aria-pressed, aria-current); the
// question box keeps its label and reports that it does not answer yet; the layout fits 720x600 and 1440x920;
// the focus is visible; the glass layer can be switched off; the reduced-motion and reduced-transparency
// preferences are honoured where the browser reports them; and no inline style is set under the policy.
//
// What it does not prove: the shell's real IPC, the webview of each system, or how the glass performs. Chromium
// is an approximation of the webview; the real window is checked by hand and recorded in docs/rust-shell.md.
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

const LIGHT = 'NOCT claro'
const DARK = 'NOCT escuro'
const SHELL_ERROR = { code: 'persistence', message: 'Não foi possível acessar os dados locais.' }

function settingsSaved(theme) {
  return {
    approvalPolicy: 'untrusted',
    diagnosticMode: true,
    ...(theme ? { theme } : {}),
    language: 'en',
    pandocVersion: '3.1.2',
  }
}

// The Tema row shows the saved setting, not the theme in use: a setting never chosen reads as not informed.
function shownFor(savedTheme) {
  const themeRow = savedTheme === 'dark' ? DARK : savedTheme === 'light' ? LIGHT : 'Não informado'
  return [
    ['Tema', themeRow],
    ['Idioma', 'English'],
    ['Segurança do agente', 'Pedir aprovação antes de cada alteração de arquivo'],
    ['Diagnóstico', 'Ativo'],
    ['Pandoc', '3.1.2'],
  ]
}

// `mode` says how `settings_get` answers. `full` runs the interaction checks, which need the settings to show.
const SCENARIOS = [
  { name: 'ready-light', mode: 'ready', theme: 'light', full: true, expectTheme: 'light' },
  { name: 'ready-dark', mode: 'ready', theme: 'dark', full: true, expectTheme: 'dark' },
  { name: 'ready-without-a-saved-theme-follows-the-system', mode: 'ready', theme: undefined, scheme: 'dark', expectTheme: 'dark' },
  { name: 'slow-then-ready', mode: 'slow', theme: 'light', expectTheme: 'light' },
  { name: 'error-then-retry', mode: 'error-then-retry', theme: 'light', expectTheme: 'light' },
  { name: 'reply-outside-the-contract', mode: 'invalid', theme: undefined, expectTheme: 'light' },
  { name: 'no-shell', mode: 'no-runtime', theme: undefined, expectTheme: 'light' },
  { name: 'no-shell-with-a-dark-system', mode: 'no-runtime', theme: undefined, scheme: 'dark', expectTheme: 'dark' },
  { name: 'reduced-motion', mode: 'ready', theme: 'light', reducedMotion: true, expectTheme: 'light' },
  { name: 'reduced-transparency', mode: 'ready', theme: 'light', reducedTransparency: true, expectTheme: 'light' },
]

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.wasm': 'application/wasm',
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
    const result = { scenario: scenario.name, passed: false, failures: [], notes: [], violations: [], pageErrors: [] }
    const context = await browser.newContext({
      colorScheme: scenario.scheme ?? 'light',
      reducedMotion: scenario.reducedMotion ? 'reduce' : 'no-preference',
    })
    const page = await context.newPage()
    await page.addInitScript(() => {
      window.__cspViolations = []
      document.addEventListener('securitypolicyviolation', (event) => {
        window.__cspViolations.push(`${event.violatedDirective} ${event.blockedURI || 'inline'}`)
      })
    })
    await page.addInitScript(installMock, {
      scenario,
      settings: settingsSaved(scenario.theme),
      shellError: SHELL_ERROR,
    })
    page.on('pageerror', (error) => result.pageErrors.push(error.message))
    const check = (condition, message) => { if (!condition) result.failures.push(message) }

    try {
      if (scenario.reducedTransparency) {
        const cdp = await context.newCDPSession(page)
        await cdp.send('Emulation.setEmulatedMedia', {
          features: [{ name: 'prefers-reduced-transparency', value: 'reduce' }],
        })
      }
      await page.goto(`http://127.0.0.1:${server.address().port}/`)
      // The states of the read (loading, error, values) are shown in Ajustes, so it is opened first.
      await page.getByRole('button', { name: 'Ajustes', exact: true }).click()
      const values = page.getByTestId('settings-values')
      const themeButton = page.getByRole('button', { name: /^Usar tema (escuro|claro)$/ })
      const html = page.locator('html')
      // The sidebar button names the theme it switches to.
      const labelFor = (theme) => (theme === 'light' ? 'Usar tema escuro' : 'Usar tema claro')

      if (scenario.mode === 'slow') {
        const loading = page.getByTestId('settings-loading')
        await loading.waitFor({ timeout: 8000 })
        check((await loading.textContent())?.includes('Carregando configurações'), 'the loading state is shown')
        await page.evaluate(() => window.__releaseSettings())
        await values.waitFor({ timeout: 8000 })
      }

      if (scenario.mode === 'ready' || scenario.mode === 'error-then-retry') {
        if (scenario.mode === 'error-then-retry') {
          const alert = page.getByTestId('settings-error')
          await alert.waitFor({ timeout: 8000 })
          check((await alert.textContent())?.includes(SHELL_ERROR.message), 'the shell error message is shown')
          await page.getByRole('button', { name: 'Tentar novamente' }).click()
        }
        await values.waitFor({ timeout: 8000 })
        for (const [label, value] of shownFor(scenario.theme)) {
          const pair = page.locator('dt', { hasText: label }).locator('xpath=following-sibling::dd[1]')
          check((await pair.textContent())?.trim() === value, `${label} shows ${value}`)
        }
      }

      if (scenario.mode === 'invalid') {
        const alert = page.getByTestId('settings-error')
        await alert.waitFor({ timeout: 8000 })
        check((await alert.textContent())?.includes('O NOCT respondeu de forma inesperada.'), 'an unexpected reply is an error')
        check(await values.count() === 0, 'an unexpected reply shows no values')
      }

      if (scenario.mode === 'no-runtime') {
        const alert = page.getByTestId('settings-error')
        await alert.waitFor({ timeout: 8000 })
        check((await alert.textContent())?.includes('Não foi possível falar com o NOCT'), 'a page outside the shell says so')
        check(await values.count() === 0, 'no values are shown without the shell')
      }

      // The theme: the saved setting, or the system when none was chosen. The attribute is what the CSS reads.
      check((await html.getAttribute('data-theme')) === scenario.expectTheme, `the theme is ${scenario.expectTheme}`)
      check((await html.getAttribute('data-glass')) === 'on', 'the glass layer is on by default')

      if (scenario.full) {
        // The sidebar button changes the theme for the session, and says what it will switch to.
        const other = scenario.expectTheme === 'light' ? 'dark' : 'light'
        check(await themeButton.getAttribute('aria-label') === labelFor(scenario.expectTheme), 'the button names the theme it switches to')
        await themeButton.click()
        check((await html.getAttribute('data-theme')) === other, `the sidebar button switches the theme to ${other}`)
        check(await page.getByRole('button', { name: labelFor(other), exact: true }).count() === 1, 'the button now names the other theme')
        await page.getByRole('button', { name: labelFor(other), exact: true }).click()
        check((await html.getAttribute('data-theme')) === scenario.expectTheme, 'the sidebar button switches back')

        // The three modes keep one pressed; Ajustes is the current page while open, and a mode leaves it.
        const pressed = (name) => page.getByRole('button', { name, exact: true }).getAttribute('aria-pressed')
        await page.getByRole('button', { name: 'Construir', exact: true }).click()
        check((await pressed('Construir')) === 'true' && (await pressed('Revisar')) === 'false', 'Construir is the pressed mode')
        check(await page.getByText('AGUARDANDO APROVAÇÃO').isVisible(), 'the build mode shows the approval card')
        await page.getByRole('button', { name: 'Aprovar', exact: true }).click()
        check(await page.getByText('Aprovado. Esta é uma tela de exemplo').isVisible(), 'approving shows the example result')
        await page.getByRole('button', { name: 'Documentar', exact: true }).click()
        check(await page.getByRole('button', { name: 'Confirmar alteração' }).isVisible(), 'the document mode shows its confirmation')
        await page.getByRole('button', { name: 'Ajustes', exact: true }).click()
        check((await page.getByRole('button', { name: 'Ajustes', exact: true }).getAttribute('aria-current')) === 'page', 'Ajustes is the current page while open')
        check((await pressed('Documentar')) === 'false', 'no mode is pressed while Ajustes is open')
        check(await values.isVisible(), 'Ajustes shows the saved values')
        await page.getByRole('button', { name: 'Revisar', exact: true }).click()
        check(await page.getByRole('heading', { name: 'Revisão', level: 1 }).isVisible(), 'a mode leaves Ajustes')

        // The question box has a label, and says it does not answer yet.
        await page.getByLabel('Pergunte sobre o projeto').fill('Quem sou eu?')
        await page.getByLabel('Pergunte sobre o projeto').press('Enter')
        check(
          (await page.getByTestId('ask-note').textContent())?.includes('Perguntas ainda não são respondidas'),
          'the question box says it does not answer yet',
        )

        // The sidebar's example items: one is current, and selecting another moves the current mark.
        const current = page.locator('.nav__item[aria-current="true"]')
        check((await current.textContent())?.trim() === 'noct', 'the first project is current')
        await page.getByRole('button', { name: 'frx', exact: true }).click()
        check((await page.getByRole('button', { name: 'frx', exact: true }).getAttribute('aria-current')) === 'true', 'selecting a project marks it current')

        // Keyboard: the first Tab from the top of a fresh page reaches the theme button, with a visible outline.
        await page.reload()
        await page.getByRole('button', { name: 'Ajustes', exact: true }).waitFor({ timeout: 8000 })
        await page.keyboard.press('Tab')
        const outline = await page.evaluate(() => {
          const element = document.activeElement
          const style = getComputedStyle(element)
          return { label: element.getAttribute('aria-label'), style: style.outlineStyle, width: style.outlineWidth }
        })
        check(outline.label?.startsWith('Usar tema'), 'the first Tab stop is the theme button')
        check(outline.style === 'solid' && outline.width === '2px', 'the focused control has a visible outline')

        // Layout: the window fits the two canonical sizes without horizontal scroll, and the question box stays
        // on screen. The minimum is 720x600, the default 1440x920.
        for (const size of [{ width: 720, height: 600 }, { width: 1440, height: 920 }]) {
          await page.setViewportSize(size)
          const fit = await page.evaluate(() => ({
            horizontal: document.documentElement.scrollWidth <= window.innerWidth,
            composerBottom: document.querySelector('.composer').getBoundingClientRect().bottom,
            innerHeight: window.innerHeight,
            sidebar: document.querySelector('.sidebar').getBoundingClientRect().width,
          }))
          check(fit.horizontal, `no horizontal scroll at ${size.width}x${size.height}`)
          check(fit.composerBottom <= fit.innerHeight, `the question box is on screen at ${size.width}x${size.height}`)
          check(fit.sidebar >= 199 && fit.sidebar <= 249, `the sidebar keeps 200-248px at ${size.width}`)
        }

        // The glass layer: on paints the panes with a backdrop blur; off keeps the solid colours.
        const blur = () => page.locator('.sidebar').evaluate((element) => getComputedStyle(element).backdropFilter)
        check((await blur()).includes('blur'), 'the sidebar is blurred with the glass on')
        await html.evaluate((element) => element.setAttribute('data-glass', 'off'))
        check((await blur()) === 'none', 'the sidebar has no blur with the glass off')
        await html.evaluate((element) => element.setAttribute('data-glass', 'on'))
        check((await blur()).includes('blur'), 'the blur returns when the glass is on again')
      }

      if (scenario.reducedMotion) {
        const transition = await page.locator('.card, .row').first().evaluate((element) => getComputedStyle(element).transitionDuration)
        check(transition.split(',').every((part) => part.trim() === '0s'), 'reduced motion removes the transitions')
      }

      if (scenario.reducedTransparency) {
        const reported = await page.evaluate(() => matchMedia('(prefers-reduced-transparency: reduce)').matches)
        if (reported) {
          const blur = await page.locator('.sidebar').evaluate((element) => getComputedStyle(element).backdropFilter)
          check(blur === 'none', 'reduced transparency removes the blur')
        } else {
          result.notes.push('not verified: this Chromium does not report prefers-reduced-transparency')
        }
      }

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
      await context.close()
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
