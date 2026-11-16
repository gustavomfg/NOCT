// Runs the Rust shell locally in one of two modes. docs/rust-shell.md explains each mode and what has
// been verified.
//
//   node scripts/run-rust-shell.mjs dev [--data-dir <absolute path> | --from-fixture <release tag>] [--check]
//   node scripts/run-rust-shell.mjs release [--data-dir <absolute path> | --from-fixture <release tag>] [--check]
//
// dev: `trunk serve` starts the interface on the dev port, and a debug build of the shell loads it from
//      `build.devUrl`. The Trunk server stops when the shell exits.
// release: `trunk build --release` writes the interface, and a release build compiled with the
//      `custom-protocol` feature embeds it and loads it. No dev server is needed.
// --data-dir sets NOCT_DATA_DIR for the shell. Without it the shell uses its default data folder.
// --from-fixture <tag> starts the shell from the synthetic database of a pinned release line (for example v1.0.2).
//      The database is built from that tag's sources into target/shell-fixtures, copied into a new folder there,
//      and the shell opens the copy. The printed path is the folder used. No real data folder is touched.
// --check prints the plan, verifies the tools and the dev port configuration, and starts nothing. With
//      --from-fixture it still builds and copies the fixture, so the path it prints is a real one.
import { spawn, spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(import.meta.dirname, '..')
const shellDirectory = path.join(root, 'crates', 'noct-app')
const uiDirectory = path.join(root, 'crates', 'noct-ui')
const DEV_PORT = 1420
// Must match `rust-toolchain.toml` and the trunk version of the workflow (`cargo install --locked trunk@`).
const TRUNK_VERSION = '0.22.1'
const SERVER_TIMEOUT_MS = 300_000

function usage(message) {
  console.error(
    `${message}\nusage: node scripts/run-rust-shell.mjs <dev|release> [--data-dir <absolute path> | --from-fixture <release tag>] [--check]`,
  )
  process.exitCode = 2
}

// The dev URL and the dev command of the shell must name the port that trunk serves.
export function checkDevConfiguration() {
  const config = JSON.parse(fs.readFileSync(path.join(shellDirectory, 'tauri.conf.json'), 'utf8'))
  const expectedUrl = `http://localhost:${DEV_PORT}`
  if (config.build.devUrl !== expectedUrl) {
    throw new Error(`tauri.conf.json build.devUrl is ${config.build.devUrl}, expected ${expectedUrl}`)
  }
  if (!config.build.beforeDevCommand.includes(`--port ${DEV_PORT}`)) {
    throw new Error(`tauri.conf.json build.beforeDevCommand does not serve port ${DEV_PORT}`)
  }
}

function toolVersion(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' })
  if (result.error || result.status !== 0) return null
  return result.stdout.trim()
}

function checkTools() {
  const cargo = toolVersion('cargo', ['--version'])
  if (!cargo) throw new Error('cargo was not found on PATH. Install the toolchain pinned in rust-toolchain.toml.')
  const trunk = toolVersion('trunk', ['--version'])
  if (!trunk) throw new Error(`trunk was not found on PATH. Install it with: cargo install --locked trunk@${TRUNK_VERSION}`)
  if (!trunk.endsWith(TRUNK_VERSION)) {
    throw new Error(`trunk is "${trunk}", expected ${TRUNK_VERSION}. Install it with: cargo install --locked trunk@${TRUNK_VERSION}`)
  }
  return { cargo, trunk }
}

async function devServerAnswers() {
  try {
    const response = await fetch(`http://localhost:${DEV_PORT}/`)
    return response.ok
  } catch {
    return false
  }
}

function waitForDevServer(child) {
  const deadline = Date.now() + SERVER_TIMEOUT_MS
  return new Promise((resolve, reject) => {
    const poll = async () => {
      if (child.exitCode !== null) {
        reject(new Error(`trunk serve stopped with code ${child.exitCode} before the interface was ready`))
        return
      }
      if (await devServerAnswers()) {
        resolve()
        return
      }
      if (Date.now() > deadline) {
        reject(new Error(`trunk serve did not answer on port ${DEV_PORT} within ${SERVER_TIMEOUT_MS / 1000} seconds`))
        return
      }
      setTimeout(poll, 500)
    }
    poll()
  })
}

function run(command, args, options) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: 'inherit', ...options })
    child.once('error', reject)
    child.once('exit', (code, signal) => {
      if (signal) reject(new Error(`${command} stopped by ${signal}`))
      else resolve(code ?? 1)
    })
  })
}

async function runDev(env) {
  if (await devServerAnswers()) {
    throw new Error(`port ${DEV_PORT} already answers. Stop the other server, or the shell would load it.`)
  }
  const trunk = spawn('trunk', ['serve', '--port', String(DEV_PORT)], { cwd: uiDirectory, stdio: 'inherit' })
  const stopTrunk = () => {
    if (trunk.exitCode === null) trunk.kill()
  }
  process.once('exit', stopTrunk)
  try {
    await waitForDevServer(trunk)
    return await run('cargo', ['run', '-p', 'noct-app'], { cwd: root, env })
  } finally {
    stopTrunk()
  }
}

async function runRelease(env) {
  const build = await run('trunk', ['build', '--release'], { cwd: uiDirectory, env })
  if (build !== 0) return build
  return run('cargo', ['run', '-p', 'noct-app', '--release', '--features', 'custom-protocol'], { cwd: root, env })
}

function plan(mode, dataDirectory) {
  const data = dataDirectory ?? '(the default data folder of the shell)'
  if (mode === 'dev') {
    return [
      `mode: dev`,
      `1. trunk serve --port ${DEV_PORT}   (in crates/noct-ui; stopped when the shell exits)`,
      `2. cargo run -p noct-app            (debug build; loads http://localhost:${DEV_PORT})`,
      `data folder: ${data}`,
    ]
  }
  return [
    `mode: release`,
    `1. trunk build --release            (in crates/noct-ui; writes crates/noct-ui/dist)`,
    `2. cargo run -p noct-app --release --features custom-protocol   (embeds dist; no dev server)`,
    `data folder: ${data}`,
  ]
}

// The options after the mode. A value option takes the argument that follows it, and an unknown argument is an error.
export function parseOptions(rest) {
  const options = { check: false, dataDirectory: undefined, fixture: undefined }
  for (let index = 0; index < rest.length; index += 1) {
    const argument = rest[index]
    if (argument === '--check') {
      options.check = true
    } else if (argument === '--data-dir' || argument === '--from-fixture') {
      const value = rest[index + 1]
      index += 1
      if (value === undefined || value.startsWith('--')) throw new Error(`${argument} needs a value.`)
      if (argument === '--data-dir') {
        if (!path.isAbsolute(value)) throw new Error('--data-dir needs an absolute path.')
        options.dataDirectory = value
      } else {
        options.fixture = value
      }
    } else {
      throw new Error(`unknown argument: ${argument}`)
    }
  }
  if (options.dataDirectory && options.fixture) {
    throw new Error('--data-dir and --from-fixture cannot be used together: the fixture names its own data folder.')
  }
  return options
}

// The fixtures of this launcher are written under target/, which the repository ignores.
const fixturesDirectory = path.join(root, 'target', 'shell-fixtures')

// Copies the fixture database of a pinned release line into a new folder under target/, and returns that folder.
// The shell opens the copy, so neither the fixture nor a data folder of an installation is written. Each call
// makes a new folder, which stays until it is deleted.
export async function copyFixture(tag) {
  const { readPins, writeFixture } = await import('./release-lines.mjs')
  if (!readPins().lines.some((line) => line.tag === tag)) {
    throw new Error(`${tag} is not a pinned release line. The pinned lines are in scripts/fixtures/release-lines.json.`)
  }
  const source = writeFixture(tag, path.join(fixturesDirectory, tag, 'source'))
  const destination = fs.mkdtempSync(path.join(fixturesDirectory, `${tag}-`))
  fs.copyFileSync(source, path.join(destination, 'nocturne.db'))
  return destination
}

async function main(argv) {
  const [mode, ...rest] = argv
  if (mode !== 'dev' && mode !== 'release') return usage('the first argument must be dev or release.')
  let options
  try {
    options = parseOptions(rest)
  } catch (error) {
    return usage(error.message)
  }
  const { check } = options

  checkDevConfiguration()
  const tools = checkTools()
  let dataDirectory = options.dataDirectory
  if (options.fixture) {
    dataDirectory = await copyFixture(options.fixture)
    console.log(`fixture: ${options.fixture}; the shell opens a copy in ${dataDirectory}`)
  }
  for (const line of plan(mode, dataDirectory)) console.log(line)
  console.log(`tools: ${tools.cargo}; ${tools.trunk}`)
  if (check) {
    if (mode === 'dev' && (await devServerAnswers())) {
      console.log(`warning: port ${DEV_PORT} already answers; dev mode would refuse to start.`)
    }
    console.log('check passed: nothing was started.')
    return
  }

  const env = { ...process.env }
  if (dataDirectory) env.NOCT_DATA_DIR = dataDirectory
  const code = mode === 'dev' ? await runDev(env) : await runRelease(env)
  process.exitCode = code
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message)
    process.exitCode = 1
  })
}
