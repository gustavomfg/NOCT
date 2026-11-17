#!/usr/bin/env node
/* global window, document, getComputedStyle, matchMedia */
// Checks the interface of the shell in a browser, with a mocked shell. The page is the Trunk build in
// crates/noct-ui/dist, served with the Content-Security-Policy the shell ships with. Before the page runs, the
// mock defines `window.__TAURI_INTERNALS__.invoke`, which is the function the shell's webview defines
// (tauri-2.12.5/scripts/core.js). Each scenario answers `settings_get` and `workspaces_list` in its own way.
//
// What this proves: the interface calls only `settings_get` (with an empty payload), `workspaces_list` (with the
// first page request, `{ input: { limit: 500 } }`), and `settings_set` (with the theme and nothing else, once for
// each press of the theme button);
// the settings show their loading state, values, the shell's error and the way to try again; the sidebar lists
// the projects the shell returns, in order, with the note of each one that cannot be used as it is, and shows its
// own loading, empty, error and retry states; a reply outside the contract is an error, not a default or a
// shorter list; a page outside the shell says so; the theme follows the saved setting or the system, and the
// sidebar button changes it for the session; the modes and Ajustes keep their state (aria-pressed, aria-current); Construir and Documentar say they are not
// available yet and offer no approval, revert or confirmation control;
// the question box keeps its label and reports that it does not answer yet; the layout fits the window's minimum
// size (read from crates/noct-app/src/window.rs) and 1440x920 without horizontal scroll, with the modes and the
// project names inside their panes; a card keeps its box under the pointer; the focus is visible; the glass layer
// can be switched off; the reduced-motion and reduced-transparency preferences are honoured where the browser
// reports them; and no inline style is set under the policy.
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

// The smallest size the window can be is named in the shell's source. The layout is checked at that size, so the
// minimum and the check cannot drift apart.
const windowSource = fs.readFileSync(path.join(root, 'crates', 'noct-app', 'src', 'window.rs'), 'utf8')
const minimum = {
  width: Number(windowSource.match(/MIN_WINDOW_WIDTH: f64 = (\d+(?:\.\d+)?);/)?.[1]),
  height: Number(windowSource.match(/MIN_WINDOW_HEIGHT: f64 = (\d+(?:\.\d+)?);/)?.[1]),
}
if (!(minimum.width > 0 && minimum.height > 0)) {
  console.error('crates/noct-app/src/window.rs must name MIN_WINDOW_WIDTH and MIN_WINDOW_HEIGHT')
  process.exit(2)
}
const SIZES = [minimum, { width: 1440, height: 920 }]

const LIGHT = 'NOCT claro'
const DARK = 'NOCT escuro'
const SHELL_ERROR = { code: 'persistence', message: 'Não foi possível acessar os dados locais.' }
const PROJECTS_UNAVAILABLE = 'A pasta não existe.'
const NEEDS_AUTHORIZATION = 'Não autorizado. Escolha a pasta de novo para usá-la.'
const MORE_PROJECTS = 'Há mais projetos do que esta lista mostra.'
const READ_COMMANDS = ['settings_get', 'workspaces_list']
const EXAMPLE_NAMES = ['frx', 'ovra']
// The payload of each read: `settings_get` takes none; `workspaces_list` takes the first page at the largest limit.
const PAYLOAD_OF = { settings_get: '{}', workspaces_list: '{"input":{"limit":500}}' }

// What the shell returns for `workspaces_list`, as the items of its page: one project of each kind the sidebar shows.
const PROJECTS = [
  {
    path: '/work/alpha',
    name: 'alpha',
    favorite: true,
    authorized: true,
    availability: 'available',
    createdAt: '2026-01-01T00:00:00.000Z',
    lastOpenedAt: '2026-01-02T00:00:00.000Z',
  },
  {
    path: '/work/beta',
    name: 'beta',
    favorite: false,
    authorized: false,
    availability: 'missing',
    availabilityMessage: PROJECTS_UNAVAILABLE,
    createdAt: '2026-01-01T00:00:00.000Z',
    lastOpenedAt: '2026-01-02T00:00:00.000Z',
  },
  {
    path: '/work/gamma',
    name: 'gamma',
    favorite: false,
    authorized: false,
    availability: 'available',
    createdAt: '2026-01-01T00:00:00.000Z',
    lastOpenedAt: '2026-01-02T00:00:00.000Z',
  },
]

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

// `mode` says how `settings_get` answers, and `projects` how `workspaces_list` answers (default `ready`). `full`
// runs the interaction checks, which need the settings to show.
const SCENARIOS = [
  { name: 'ready-light', mode: 'ready', theme: 'light', full: true, expectTheme: 'light' },
  { name: 'ready-dark', mode: 'ready', theme: 'dark', full: true, expectTheme: 'dark' },
  { name: 'build-and-document-not-yet-available', mode: 'ready', theme: 'light', unavailableModes: true, expectTheme: 'light' },
  { name: 'ready-without-a-saved-theme-follows-the-system', mode: 'ready', theme: undefined, scheme: 'dark', expectTheme: 'dark' },
  { name: 'slow-then-ready', mode: 'slow', theme: 'light', expectTheme: 'light' },
  { name: 'error-then-retry', mode: 'error-then-retry', theme: 'light', expectTheme: 'light' },
  { name: 'reply-outside-the-contract', mode: 'invalid', theme: undefined, expectTheme: 'light' },
  { name: 'no-shell', mode: 'no-runtime', theme: undefined, expectTheme: 'light' },
  { name: 'no-shell-with-a-dark-system', mode: 'no-runtime', theme: undefined, scheme: 'dark', expectTheme: 'dark' },
  { name: 'reduced-motion', mode: 'ready', theme: 'light', reducedMotion: true, expectTheme: 'light' },
  { name: 'reduced-transparency', mode: 'ready', theme: 'light', reducedTransparency: true, expectTheme: 'light' },
  { name: 'projects-empty', mode: 'ready', theme: 'light', projects: 'empty', expectTheme: 'light' },
  { name: 'projects-slow-then-ready', mode: 'ready', theme: 'light', projects: 'slow', expectTheme: 'light' },
  { name: 'projects-error-then-retry', mode: 'ready', theme: 'light', projects: 'error-then-retry', expectTheme: 'light' },
  { name: 'projects-reply-outside-the-contract', mode: 'ready', theme: 'light', projects: 'invalid', expectTheme: 'light' },
  { name: 'projects-with-more-to-come', mode: 'ready', theme: 'light', projects: 'ready', more: true, expectTheme: 'light' },
  { name: 'theme-not-saved-says-so', mode: 'ready', theme: 'light', saveFails: true, expectTheme: 'light' },
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

// Runs before the page's own scripts. The mock answers each read command the way its scenario says, and records
// each call so the check can tell which commands the interface asked for, and with which payload.
function installMock(config) {
  window.__noctCalls = []
  window.__noctAttempts = {}
  window.__noctReleases = {}
  if (config.scenario.mode === 'no-runtime') return

  const modeOf = (command) => (command === 'workspaces_list' ? config.scenario.projects : config.scenario.mode)
  // `workspaces_list` answers with a page: the items, and whether more follow them.
  // The cursor of a page with more is the key of its last project, as the shell writes it.
  const cursorOf = (items) => {
    const last = items[items.length - 1]
    return last ? { favorite: last.favorite, lastOpenedAt: last.lastOpenedAt, path: last.path } : null
  }
  const replyOf = (command) => {
    if (command !== 'workspaces_list') return config.settings
    const more = config.scenario.more === true
    return { items: config.projects, hasMore: more, nextCursor: more ? cursorOf(config.projects) : null }
  }
  const invalidOf = (command) =>
    command === 'workspaces_list'
      ? { items: [{ path: '/work/x', name: 'x', favorite: false, authorized: true, availability: 'online', createdAt: '2026-01-01T00:00:00.000Z', lastOpenedAt: '2026-01-02T00:00:00.000Z' }], hasMore: false, nextCursor: null }
      : { approvalPolicy: 'always' }

  window.__TAURI_INTERNALS__ = {
    invoke(command, payload) {
      window.__noctCalls.push({ command, payload })
      const attempt = (window.__noctAttempts[command] = (window.__noctAttempts[command] ?? 0) + 1)
      const mode = modeOf(command)
      // A save the scenario refuses gets the shell's own error, as a refused read does.
      if (command === 'settings_set' && config.scenario.saveFails) return Promise.reject(config.shellError)
      return new Promise((resolve, reject) => {
        const answer = () => {
          switch (mode) {
            case 'ready':
              resolve(replyOf(command))
              break
            case 'empty':
              resolve(command === 'workspaces_list' ? { items: [], hasMore: false, nextCursor: null } : [])
              break
            case 'error-then-retry':
              if (attempt === 1) reject(config.shellError)
              else resolve(replyOf(command))
              break
            case 'invalid':
              resolve(invalidOf(command))
              break
            case 'slow':
              window.__noctReleases[command] = () => resolve(replyOf(command))
              break
          }
        }
        if (mode === 'slow') answer()
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
    const projects = scenario.projects ?? 'ready'
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
      scenario: { ...scenario, projects },
      settings: settingsSaved(scenario.theme),
      projects: PROJECTS,
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
      // The states of the settings read are shown in Ajustes, so it is opened first.
      await page.getByRole('button', { name: 'Ajustes', exact: true }).click()
      const values = page.getByTestId('settings-values')
      const themeButton = page.getByRole('button', { name: /^Usar tema (escuro|claro)$/ })
      const html = page.locator('html')
      // The sidebar button names the theme it switches to.
      const labelFor = (theme) => (theme === 'light' ? 'Usar tema escuro' : 'Usar tema claro')
      const sidebarProjects = page.getByRole('navigation', { name: 'Projetos' })

      if (scenario.mode === 'slow') {
        const loading = page.getByTestId('settings-loading')
        await loading.waitFor({ timeout: 8000 })
        check((await loading.textContent())?.includes('Carregando configurações'), 'the loading state is shown')
        await page.evaluate(() => window.__noctReleases.settings_get())
        await values.waitFor({ timeout: 8000 })
      }

      if (scenario.mode === 'ready' || scenario.mode === 'error-then-retry') {
        if (scenario.mode === 'error-then-retry') {
          const alert = page.getByTestId('settings-error')
          await alert.waitFor({ timeout: 8000 })
          check((await alert.textContent())?.includes(SHELL_ERROR.message), 'the shell error message is shown')
          await alert.getByRole('button', { name: 'Tentar novamente' }).click()
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
        const projectsAlert = page.getByTestId('projects-error')
        await projectsAlert.waitFor({ timeout: 8000 })
        check((await projectsAlert.textContent())?.includes('Não foi possível falar com o NOCT'), 'the sidebar says a page outside the shell has no projects')
        check(await page.getByTestId('projects').count() === 0, 'no projects are listed without the shell')
      } else {
        // The sidebar's project list: its own states, then the shell's projects in the shell's order.
        if (projects === 'slow') {
          const loading = page.getByTestId('projects-loading')
          await loading.waitFor({ timeout: 8000 })
          check((await loading.textContent())?.includes('Carregando projetos'), 'the sidebar shows its loading state')
          await page.evaluate(() => window.__noctReleases.workspaces_list())
        }
        if (projects === 'error-then-retry') {
          const alert = page.getByTestId('projects-error')
          await alert.waitFor({ timeout: 8000 })
          check((await alert.textContent())?.includes(SHELL_ERROR.message), 'the sidebar shows the shell error message')
          await alert.getByRole('button', { name: 'Tentar novamente' }).click()
        }
        if (projects === 'invalid') {
          const alert = page.getByTestId('projects-error')
          await alert.waitFor({ timeout: 8000 })
          check((await alert.textContent())?.includes('O NOCT respondeu de forma inesperada.'), 'an unexpected project reply is an error')
          check(await page.getByTestId('projects').count() === 0, 'an unexpected project reply lists no projects')
        }
        if (projects === 'empty') {
          await page.getByTestId('projects-empty').waitFor({ timeout: 8000 })
          check((await page.getByTestId('projects-empty').textContent())?.includes('Nenhum projeto registrado ainda.'), 'an empty list says so')
          check(await page.getByTestId('projects').count() === 0, 'an empty list shows no items')
        }
        if (projects === 'ready' || projects === 'slow' || projects === 'error-then-retry') {
          const list = page.getByTestId('projects')
          await list.waitFor({ timeout: 8000 })
          const items = list.locator('.project')
          const names = await list.locator('.project__name').allTextContents()
          check(JSON.stringify(names) === '["alpha","beta","gamma"]', 'the sidebar lists the shell projects in order')
          for (const example of EXAMPLE_NAMES) {
            check(await sidebarProjects.getByText(example, { exact: true }).count() === 0, `the example project ${example} is not listed`)
          }
          check((await items.nth(0).locator('.project__tag').textContent())?.trim() === 'Favorito', 'a favourite project is tagged')
          check(await items.nth(0).locator('.project__note').count() === 0, 'a usable project has no note')
          check((await items.nth(1).locator('.project__note').textContent())?.trim() === PROJECTS_UNAVAILABLE, 'a missing project shows the shell message')
          check((await items.nth(2).locator('.project__note').textContent())?.trim() === NEEDS_AUTHORIZATION, 'a project not authorized in this session asks for its folder again')
          check((await items.nth(1).locator('.path').textContent())?.trim() === '/work/beta', 'each project shows its folder')
          // The note under the list appears exactly when the shell says more projects follow the page.
          const more = page.getByTestId('projects-more')
          const moreShown = await more.count()
          check(moreShown === (scenario.more === true ? 1 : 0), 'the list says more projects follow only when the page has more')
          if (scenario.more === true) {
            check((await more.textContent())?.trim() === MORE_PROJECTS, 'the note says more projects follow')
          }
        }
      }

      // The theme: the saved setting, or the system when none was chosen. The attribute is what the CSS reads.
      check((await html.getAttribute('data-theme')) === scenario.expectTheme, `the theme is ${scenario.expectTheme}`)
      check((await html.getAttribute('data-glass')) === 'on', 'the glass layer is on by default')

      // Construir and Documentar are not available yet: each says so, and offers no control for the work.
      if (scenario.unavailableModes) {
        for (const name of ['Construir', 'Documentar']) {
          await page.getByRole('button', { name, exact: true }).click()
          check(await page.getByRole('heading', { name, level: 1 }).isVisible(), `${name} shows its heading`)
          check(await page.getByTestId('unavailable').count() === 1, `${name} says it is not available yet`)
          check(
            (await page.getByTestId('unavailable').textContent())?.includes('Ainda não disponível nesta versão'),
            `${name} gives the note`,
          )
          for (const control of ['Aprovar', 'Negar', 'Reverter', 'Confirmar alteração']) {
            check(await page.getByRole('button', { name: control, exact: true }).count() === 0, `${name} offers no ${control} control`)
          }
          check(await page.locator('.diff, .prose, .cmd').count() === 0, `${name} shows no example diff or command`)
        }
      }

      // The full scenario reloads the page, which clears the recorded calls; the saves made before the reload are kept here.
      let savesBeforeReload = []
      if (scenario.saveFails) {
        // A theme the shell does not save still changes on screen, and the window says why it was not kept.
        await themeButton.click()
        const alert = page.getByTestId('theme-error')
        await alert.waitFor({ timeout: 8000 })
        check((await alert.textContent())?.includes(SHELL_ERROR.message), 'a theme that is not saved says so with the shell message')
        check((await html.getAttribute('data-theme')) === 'dark', 'the theme still changes on screen when the save is refused')
      }

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
        check(await page.getByTestId('unavailable').isVisible(), 'the build mode says it is not available yet')
        await page.getByRole('button', { name: 'Documentar', exact: true }).click()
        check(await page.getByRole('heading', { name: 'Documentar', level: 1 }).isVisible(), 'the document mode is shown')
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

        // A card keeps its box under the pointer: the hover changes the shadow only.
        const card = page.locator('.card').first()
        const before = await card.boundingBox()
        await card.hover()
        await page.waitForTimeout(300)
        const after = await card.boundingBox()
        check(
          before && after && before.x === after.x && before.y === after.y && before.width === after.width && before.height === after.height,
          'a hovered card does not move or resize',
        )
        check((await card.evaluate((element) => getComputedStyle(element).transform)) === 'none', 'a hovered card has no transform')

        // Keyboard: the first Tab from the top of a fresh page reaches the theme button, with a visible outline.
        savesBeforeReload = await page.evaluate(() => (window.__noctCalls ?? []).filter((call) => call.command === 'settings_set'))
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

        // Layout: the window fits its minimum size and 1440x920 without horizontal scroll. The modes stay inside the
        // content pane, the project names inside the sidebar, and the question box on screen.
        for (const size of SIZES) {
          await page.setViewportSize(size)
          const fit = await page.evaluate(() => {
            const box = (selector) => document.querySelector(selector).getBoundingClientRect()
            const content = box('.content')
            const modes = box('.segmented')
            const sidebar = box('.sidebar')
            return {
              horizontal: document.documentElement.scrollWidth <= window.innerWidth,
              composerBottom: box('.composer').bottom,
              innerHeight: window.innerHeight,
              sidebar: sidebar.width,
              modesInside: modes.left >= content.left && modes.right <= content.right,
              projectsInside: [...document.querySelectorAll('.project')].every((item) => item.getBoundingClientRect().right <= sidebar.right),
            }
          })
          const label = `${size.width}x${size.height}`
          check(fit.horizontal, `no horizontal scroll at ${label}`)
          check(fit.composerBottom <= fit.innerHeight, `the question box is on screen at ${label}`)
          check(fit.sidebar >= 199 && fit.sidebar <= 249, `the sidebar keeps 200-248px at ${label}`)
          check(fit.modesInside, `the modes fit the content pane at ${label}`)
          check(fit.projectsInside, `the project names fit the sidebar at ${label}`)
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
        const count = (command) => calls.filter((call) => call.command === command).length
        check(calls.length >= 2, 'the interface calls the shell for settings and projects')
        check(calls.every((call) => [...READ_COMMANDS, 'settings_set'].includes(call.command)), 'only the read commands and the theme save are called')
        const reads = calls.filter((call) => call.command !== 'settings_set')
        check(
          reads.every((call) => JSON.stringify(call.payload) === PAYLOAD_OF[call.command]),
          'each read is sent its own payload: settings_get none, workspaces_list the first page at the largest limit',
        )
        const saves = [...savesBeforeReload, ...calls.filter((call) => call.command === 'settings_set')]
        check(
          saves.every((call) => /^\{"input":\{"theme":"(light|dark)"\}\}$/.test(JSON.stringify(call.payload))),
          'a theme save sends the theme and nothing else',
        )
        check(
          saves.length === (scenario.full ? 2 : scenario.saveFails ? 1 : 0),
          `the theme is saved once for each press of the theme button (saves: ${saves.length})`,
        )
        check(count('settings_get') === (scenario.mode === 'error-then-retry' ? 2 : 1), 'settings_get is called once, or once more after a retry')
        check(count('workspaces_list') === (projects === 'error-then-retry' ? 2 : 1), 'workspaces_list is called once, or once more after a retry')
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
console.log(JSON.stringify({ mockedTauriVersion: MOCKED_TAURI_VERSION, minimumWindow: minimum, policy, passed, results }, null, 2))
process.exitCode = passed ? 0 : 1
