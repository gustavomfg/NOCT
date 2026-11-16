// Browser check for the Leptos spike in crates/noct-ui.
//
// Serves the trunk output and drives it in Chromium: the WebAssembly must mount, the
// signals must react to clicks and typing, the existing stylesheets must apply, and
// the page must produce no console or page errors. Run `trunk build --release` in
// crates/noct-ui first.
//
//   node scripts/check-leptos-spike.mjs [screenshot-path]
import assert from 'node:assert/strict'
import fs from 'node:fs'
import { createServer } from 'node:http'
import path from 'node:path'
import { chromium } from '@playwright/test'

const dist = path.join(import.meta.dirname, '..', 'crates', 'noct-ui', 'dist')
const contentTypes = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.css': 'text/css; charset=utf-8',
}

if (!fs.existsSync(path.join(dist, 'index.html'))) {
  console.error('crates/noct-ui/dist is missing: run `trunk build --release` in crates/noct-ui')
  process.exit(2)
}

const server = createServer((request, response) => {
  const pathname = new URL(request.url ?? '/', 'http://localhost').pathname
  const relative = pathname === '/' ? 'index.html' : decodeURIComponent(pathname.slice(1))
  const file = path.join(dist, relative)
  if (!file.startsWith(dist + path.sep) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
    response.writeHead(404).end()
    return
  }
  response.writeHead(200, { 'content-type': contentTypes[path.extname(file)] ?? 'application/octet-stream' })
  fs.createReadStream(file).pipe(response)
})
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve))
const { port } = server.address()

const errors = []
const browser = await chromium.launch()
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } })
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(`console: ${message.text()}`)
  })
  page.on('pageerror', (error) => errors.push(`pageerror: ${error.message}`))
  page.on('response', (response) => {
    if (response.status() >= 400) errors.push(`http ${response.status()}: ${response.url()}`)
  })

  const started = Date.now()
  await page.goto(`http://127.0.0.1:${port}/`)
  await page.waitForSelector('#leptos-spike')
  const mountMs = Date.now() - started

  await assert.doesNotReject(() => page.getByRole('heading', { name: 'NOCT' }).waitFor())
  const presses = page.getByTestId('presses')
  await assert.doesNotReject(() => presses.getByText('0 toques').waitFor())

  await page.getByRole('button', { name: 'Tocar' }).click()
  await page.getByRole('button', { name: 'Tocar' }).click()
  await assert.doesNotReject(() => presses.getByText('2 toques').waitFor())

  await page.getByRole('textbox', { name: 'Mensagem' }).fill('olá, mundo')
  await assert.doesNotReject(() => page.getByTestId('characters').getByText('10 caracteres').waitFor())

  const background = await page.evaluate(() => getComputedStyle(document.body).backgroundColor)
  assert.notEqual(background, 'rgba(0, 0, 0, 0)', 'the global stylesheet must reach the page')
  assert.notEqual(background, 'rgb(255, 255, 255)', 'the dark theme must apply')
  // Counts rules inside @import sheets too: index.css pulls in the rest of the system.
  const cssRules = await page.evaluate(() => {
    const countRules = (rules) => [...rules].reduce(
      (total, rule) => total + 1 + (rule.styleSheet ? countRules(rule.styleSheet.cssRules) : 0),
      0,
    )
    return [...document.styleSheets].reduce((total, sheet) => total + countRules(sheet.cssRules), 0)
  })
  assert.ok(cssRules > 200, `expected the design system rules to load, found ${cssRules}`)
  await page.evaluate(() => document.fonts.ready)
  const loadedFonts = await page.evaluate(() => [...document.fonts].filter((face) => face.status === 'loaded').length)
  assert.ok(loadedFonts > 0, 'the Geist font faces must load from the bundle')

  if (process.argv[2]) await page.screenshot({ path: process.argv[2] })
  assert.deepEqual(errors, [], 'the page must not log errors')

  console.log(JSON.stringify({
    mountMs,
    background,
    cssRules,
    loadedFonts,
    wasmBytes: fs.statSync(fs.readdirSync(dist).filter((name) => name.endsWith('.wasm')).map((name) => path.join(dist, name))[0]).size,
    result: 'pass',
  }, null, 2))
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error))
  if (errors.length > 0) console.error(errors.join('\n'))
  process.exitCode = 1
} finally {
  await browser.close()
  server.close()
}
