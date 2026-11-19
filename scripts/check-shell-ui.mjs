#!/usr/bin/env node
/* global window, document, getComputedStyle, matchMedia */
// Checks the interface of the shell in a browser, with a mocked shell. The page is the Trunk build in
// crates/noct-ui/dist, served with the Content-Security-Policy the shell ships with. Before the page runs, the
// mock defines `window.__TAURI_INTERNALS__.invoke`, which is the function the shell's webview defines
// (tauri-2.12.5/scripts/core.js). Each scenario answers `settings_get` and `workspaces_list` in its own way.
//
// What this proves: the interface calls only `settings_get` (with an empty payload), `workspaces_list` (with the
// first page request, `{ input: { limit: 500 } }`), and `settings_set` (with the one setting that changed, once for
// each change: the theme button, or a setting in Ajustes);
// the settings show their loading state, values, the shell's error and the way to try again; each setting in Ajustes is
// saved alone and shows the settings the shell replies with, and a refused change says so and goes back to the saved value; the sidebar lists
// the projects the shell returns, in order, with the note of each one that cannot be used as it is, and shows its
// own loading, empty, error and retry states; a reply outside the contract is an error, not a default or a
// shorter list; a page outside the shell says so; the theme follows the saved setting or the system, and the
// sidebar button changes it for the session; the modes and Ajustes keep their state (aria-pressed, aria-current); Construir and Documentar say they are not
// available yet and offer no approval, revert or confirmation control;
// the chat of Revisar is off by default: its switch says so, and the question box and the send button are disabled,
// so nothing is sent; with the switch on (a mocked shell), the answer streams in order even when its events arrive out of
// order, a cancel keeps the text so far and ignores what follows it, a save keeps the answer in a conversation of the
// project only when the person asks, and each failure says that nothing was saved; the layout fits the window's minimum
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

const SHELL_ERROR = { code: 'persistence', message: 'Não foi possível acessar os dados locais.' }
const PROJECTS_UNAVAILABLE = 'A pasta não existe.'
const NEEDS_AUTHORIZATION = 'Não autorizado. Escolha a pasta de novo para usá-la.'
const MORE_PROJECTS = 'Há mais projetos do que esta lista mostra.'
const READ_COMMANDS = ['settings_get', 'workspaces_list', 'provider_key_status', 'provider_consent_status', 'models_bindings_get', 'provider_chat_status', 'models_list', 'file_changes_list']
// The provider commands that change something. The provider flow checks each one; the strict list below allows them.
const PROVIDER_WRITES = ['provider_key_save', 'provider_key_remove', 'provider_consent_grant', 'provider_consent_revoke', 'models_set_default_model', 'provider_chat_set_enabled', 'workspace_add', 'file_change_approve', 'file_change_reject', 'file_change_undo']
// The shell's refusal of an identifier that is not an OpenRouter model id, as the interface shows it.
// The chat's commands. The question and the save are the only writes of the chat, and each is a command the person started.
const CHAT_COMMANDS = ['provider_chat_send', 'provider_chat_cancel', 'conversations_create', 'ai_save_assistant']
// The words the chat shows, copied from crates/noct-ui/src/chat.rs. The check compares them, so a change there is seen here.
const NEED_CHAT_OFF = 'O chat está desligado. Nada é enviado.'
const NEED_KEY = 'Falta a chave do OpenRouter.'
const NEED_MODEL = 'Falta o modelo deste projeto.'
const NEED_PERMISSION = 'Este projeto ainda não pode enviar arquivos.'
const CHAT_MODEL = 'deepseek/deepseek-chat-v3:free'
// The project the mock's folder picker returns.
const ADDED_PROJECT = { path: '/work/novo', name: 'novo', favorite: false, authorized: true, availability: 'available', createdAt: '2026-01-04T00:00:00.000Z', lastOpenedAt: '2026-01-04T00:00:00.000Z' }
const COMPLETE = 'Resposta completa. Você pode salvá-la na conversa.'
const CANCELLED = 'Cancelado. A resposta parcial não é salva.'
const STREAM_TRUNCATED = 'A resposta foi interrompida antes do fim. Nada foi salvo.'
const CHAT_PROJECT = '/work/alpha'
const CHAT_QUESTION = 'Qual é o estado do projeto?'
const delta = (text) => ({ kind: 'delta', text })
const DONE = { kind: 'done' }
// An answer in two pieces: the first ends inside an open code fence, so the page draws Markdown that is not complete yet;
// the second closes it and brings a wide table and markup that must stay text, a link and a remote image.
const MARKDOWN_HEAD = [
  '## Visão geral',
  '',
  ...Array.from({ length: 40 }, (_, index) => `Parágrafo ${index + 1} com **negrito**, *itálico* e \`código\`.`),
  '',
  '- um',
  '- dois',
  '',
  '> uma citação',
  '',
  '```rust',
  'fn main() {',
].join('\n')
const MARKDOWN_TAIL = [
  '',
  '    println!("oi");',
  '}',
  '```',
  '',
  '| coluna com um nome longo | outra coluna bem larga | mais uma coluna | e outra | e a última coluna da tabela larga |',
  '|---|:-:|--:|---|---|',
  '| valor um bem comprido | valor dois | 3 | quatro | cinco cinco cinco cinco cinco cinco |',
  '| só uma célula |',
  '',
  '<script>window.__noctInjected = true</script>',
  '',
  'Texto <img src="https://evil.invalid/x.png" onerror="window.__noctInjected = true"> e <a href="javascript:alert(1)">clique</a>.',
  '',
  'Veja [o site](javascript:alert(1)) e ![logo](https://evil.invalid/logo.png).',
].join('\n')
const MODEL_REFUSED = { code: 'invalid-input', message: 'O NOCT recusou o identificador do modelo.' }
const SECURE_STORAGE_ERROR = {
  code: 'secure-storage-unavailable',
  message: 'O sistema operacional não oferece armazenamento seguro de credenciais. Nada foi salvo.',
}
// A key the check made up. It is not a credential; the check proves that it never reaches the page after a save.
const TEST_KEY = 'sk-or-v1-check-only-0123456789'
// A consent call carries the path of a project and nothing else.
const CONSENT_PAYLOAD = /^\{"input":\{"workspace":"\/work\/[a-z]+"\}\}$/
const EXAMPLE_NAMES = ['frx', 'ovra']
// The payload of each read: `settings_get` takes none; `workspaces_list` takes the first page at the largest limit.
const PAYLOAD_OF = { settings_get: '{}', workspaces_list: '{"input":{"limit":500}}', provider_key_status: '{}', provider_chat_status: '{}', models_list: '{}', file_changes_list: '{"input":"/work/alpha"}' }
// The changes of the Construir scenario: one edit and one new file, both waiting for a decision.
const CHANGES = [
  { id: 'c1', workspace: '/work/alpha', path: 'src/main.rs', status: 'pending', base: 'título\nlinha antiga\nfim\n', content: 'título\nlinha nova\nfim\n' },
  { id: 'c2', workspace: '/work/alpha', path: 'scripts/setup.sh', status: 'pending', base: null, content: 'nota\n' },
  { id: 'c3', workspace: '/work/alpha', path: 'docs/guia.md', status: 'pending', base: '# Guia\nantes\n', content: '# Guia\ndepois\n' },
]
const CONFLICT = { code: 'change-conflict', message: 'O arquivo mudou desde a proposta. Nada foi escrito.' }
// OpenRouter's list, as the shell sends it: two free models and a paid one.
const CATALOG = [
  { id: 'deepseek/deepseek-chat-v3:free', name: 'DeepSeek V3', contextLength: 131072, free: true },
  { id: 'meta-llama/llama-3.3-70b-instruct:free', name: 'Llama 3.3 70B', contextLength: 65536, free: true },
  { id: 'openai/gpt-4o', name: 'GPT-4o', contextLength: 128000, free: false },
]
const MODELS_FAILURE = { code: 'models-network', message: 'Não foi possível buscar a lista de modelos do OpenRouter.' }

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

// `mode` says how `settings_get` answers, and `projects` how `workspaces_list` answers (default `ready`). `full`
// runs the interaction checks, which need the settings to show.
const SCENARIOS = [
  { name: 'ready-light', mode: 'ready', theme: 'light', full: true, chatEnabled: false, expectTheme: 'light' },
  { name: 'ready-dark', mode: 'ready', theme: 'dark', full: true, chatEnabled: false, expectTheme: 'dark' },
  { name: 'build-and-document-without-changes', mode: 'ready', theme: 'light', unavailableModes: true, expectTheme: 'light' },
  { name: 'build-approves-rejects-and-undoes', mode: 'ready', theme: 'dark', changes: true, expectTheme: 'dark' },
  {
    name: 'chat-proposes-changes',
    mode: 'chat',
    theme: 'light',
    keyConfigured: true,
    chat: {
      kind: 'proposes',
      sequence: [
        [0, delta('Sugiro:\n````noct-change path=src/x.rs\nfn x() {}\n````\n')],
        [1, { kind: 'proposed', changes: [{ id: 'c9', path: 'src/x.rs' }], refused: 1 }],
        [2, DONE],
      ],
    },
    expectTheme: 'light',
  },
  {
    name: 'chat-renders-markdown-without-html',
    mode: 'chat',
    theme: 'dark',
    keyConfigured: true,
    chat: {
      kind: 'markdown',
      sequence: [
        [0, delta(MARKDOWN_HEAD)],
        'hold',
        [1, delta(MARKDOWN_TAIL)],
        [2, DONE],
      ],
    },
    expectTheme: 'dark',
  },
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
  { name: 'provider-key-not-configured', mode: 'ready', theme: 'light', provider: true, expectTheme: 'light' },
  { name: 'provider-key-configured', mode: 'ready', theme: 'light', keyConfigured: true, provider: true, granted: ['/work/alpha'], modelsFail: true, expectTheme: 'light' },
  { name: 'provider-key-save-refused', mode: 'ready', theme: 'light', keyFails: SECURE_STORAGE_ERROR, provider: true, expectTheme: 'light' },
  { name: 'provider-reply-outside-the-contract', mode: 'invalid', theme: 'light', provider: true, expectTheme: 'light' },
  { name: 'chat-turned-off-in-ajustes', mode: 'chat', theme: 'light', chatEnabled: false, expectTheme: 'light' },
  { name: 'chat-needs-a-model', mode: 'chat', theme: 'light', keyConfigured: true, noModel: true, expectTheme: 'light' },
  {
    name: 'chat-streams-in-order-and-saves-on-request',
    mode: 'chat',
    theme: 'light',
    keyConfigured: true,
    chat: { kind: 'stream-and-save', sequence: [[1, delta(' mundo')], [0, delta('Olá')], [2, DONE]] },
    expectTheme: 'light',
  },
  {
    name: 'chat-save-refused-keeps-the-answer',
    mode: 'chat',
    theme: 'light',
    keyConfigured: true,
    chat: { kind: 'save-refused', sequence: [[0, delta('Olá mundo')], [1, DONE]], saveFails: { code: 'unknown-conversation', message: 'A conversa não existe mais.' } },
    expectTheme: 'light',
  },
  {
    name: 'chat-cancel-keeps-the-text-and-ignores-what-follows',
    mode: 'chat',
    theme: 'light',
    keyConfigured: true,
    chat: { kind: 'hold', sequence: [[0, delta('parcial')], 'hold'], afterCancel: [[1, DONE]] },
    expectTheme: 'light',
  },
  {
    name: 'chat-refused-by-the-shell',
    mode: 'chat',
    theme: 'light',
    keyConfigured: true,
    chat: { kind: 'refused', refused: { code: 'consent-required', message: 'Este projeto ainda não tem consentimento para o OpenRouter. Nada foi enviado.' } },
    expectTheme: 'light',
  },
  {
    name: 'chat-stream-failure-saves-nothing',
    mode: 'chat',
    theme: 'light',
    keyConfigured: true,
    chat: { kind: 'stream-fails', sequence: [[0, delta('pa')], [1, { kind: 'failed', code: 'stream-truncated' }]] },
    expectTheme: 'light',
  },
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
  // The settings the shell holds: a save changes them, and the next read or save shows the change.
  const saved = { ...config.settings }
  // The key state and the consent of each project. The key itself is never kept, and never returned.
  const providerState = {
    configured: config.scenario.keyConfigured === true,
    granted: Object.fromEntries((config.scenario.granted ?? []).map((path) => [path, true])),
    // A chat scenario with a key has the model of its project saved, unless it checks the missing model.
    models:
      config.scenario.mode === 'chat' && config.scenario.keyConfigured && !config.scenario.noModel
        ? { '/work/alpha': { workspaceId: '/work/alpha', defaultBinding: { providerId: 'openrouter', modelId: config.chatModel } } }
        : {},
  }
  // The journal of changes: the scenario's own, and the change an answer proposed.
  const journal = config.scenario.changes
    ? config.changes.map((change) => ({ ...change }))
    : config.scenario.chat?.kind === 'proposes'
      ? [{ id: 'c9', workspace: '/work/alpha', path: 'src/x.rs', status: 'pending', base: null, content: 'fn x() {}\n' }]
      : []
  const decide = (id, status) => {
    const change = journal.find((entry) => entry.id === id)
    change.status = status
    return { ...change }
  }
  // The chat is on unless the scenario turned it off; Ajustes and the line over the composer change it.
  const chatState = { enabled: config.scenario.chatEnabled !== false }
  // The projects the folder picker added.
  const added = []
  // `workspaces_list` answers with a page: the items, and whether more follow them.
  // The cursor of a page with more is the key of its last project, as the shell writes it.
  const cursorOf = (items) => {
    const last = items[items.length - 1]
    return last ? { favorite: last.favorite, lastOpenedAt: last.lastOpenedAt, path: last.path } : null
  }
  const replyOf = (command, payload) => {
    if (command === 'settings_set') {
      Object.assign(saved, payload.input)
      return { ...saved }
    }
    if (command === 'provider_key_status') return { provider: 'openrouter', configured: providerState.configured }
    if (command === 'provider_key_save') {
      providerState.configured = true
      return { provider: 'openrouter', configured: true }
    }
    if (command === 'provider_key_remove') {
      providerState.configured = false
      return { provider: 'openrouter', configured: false }
    }
    if (command === 'provider_consent_status') {
      const workspace = payload.input.workspace
      return { workspace, provider: 'openrouter', granted: providerState.granted[workspace] === true }
    }
    if (command === 'provider_consent_grant' || command === 'provider_consent_revoke') {
      const workspace = payload.input.workspace
      providerState.granted[workspace] = command === 'provider_consent_grant'
      return { workspace, provider: 'openrouter', granted: providerState.granted[workspace] }
    }
    if (command === 'models_bindings_get') return providerState.models[payload.input.workspace] ?? null
    if (command === 'provider_chat_status') return { enabled: chatState.enabled }
    if (command === 'models_list') return config.catalog
    if (command === 'file_changes_list') return journal.filter((change) => change.workspace === payload.input).map((change) => ({ ...change }))
    if (command === 'file_change_approve') return decide(payload.input, 'applied')
    if (command === 'file_change_reject') return decide(payload.input, 'rejected')
    if (command === 'file_change_undo') return decide(payload.input, 'undone')
    if (command === 'provider_chat_set_enabled') {
      chatState.enabled = payload.input.enabled === true
      return { enabled: chatState.enabled }
    }
    if (command === 'workspace_add') {
      added.push(config.addedProject)
      return config.addedProject
    }
    if (command === 'conversations_create') return { id: 'conv-1', title: 'alpha', workspace: payload.input, createdAt: '2026-01-03T00:00:00.000Z' }
    if (command === 'ai_save_assistant') {
      return { id: 'msg-1', conversationId: payload.input.conversationId, role: 'assistant', content: payload.input.content, createdAt: '2026-01-03T00:00:00.000Z' }
    }
    if (command === 'models_set_default_model') {
      const { workspace, modelId } = payload.input
      const next = { workspaceId: workspace, ...providerState.models[workspace] }
      if (modelId === null) delete next.defaultBinding
      else next.defaultBinding = { providerId: 'openrouter', modelId }
      providerState.models[workspace] = next
      return next
    }
    if (command !== 'workspaces_list') return saved
    const more = config.scenario.more === true
    return { items: [...config.projects, ...added], hasMore: more, nextCursor: more ? cursorOf(config.projects) : null }
  }
  const invalidOf = (command) =>
    command === 'workspaces_list'
      ? { items: [{ path: '/work/x', name: 'x', favorite: false, authorized: true, availability: 'online', createdAt: '2026-01-01T00:00:00.000Z', lastOpenedAt: '2026-01-02T00:00:00.000Z' }], hasMore: false, nextCursor: null }
      : { approvalPolicy: 'always' }

  // The callback registry of the shell. A channel is registered here, and the shell calls it with `{ message, index }`
  // for each event and `{ end, index }` when the call is over. The scenario says which events go out, in which order.
  const callbacks = {}
  let callbackCount = 0
  const chatRun = { channel: null, pending: [], end: 0 }
  const emitUntilHold = () => {
    while (chatRun.pending.length > 0 && chatRun.pending[0] !== 'hold') {
      const next = chatRun.pending.shift()
      window.__TAURI_INTERNALS__.runCallback(chatRun.channel, { message: next.frame, index: next.index })
    }
    if (chatRun.pending.length === 0) window.__TAURI_INTERNALS__.runCallback(chatRun.channel, { end: true, index: chatRun.end })
  }
  const startChat = (payload) => {
    const chat = config.scenario.chat
    if (chat.refused) return Promise.reject(chat.refused)
    chatRun.channel = Number(payload.onEvent.slice('__CHANNEL__:'.length))
    chatRun.pending = chat.sequence.map((item) => (item === 'hold' ? 'hold' : { index: item[0], frame: item[1] }))
    chatRun.end = chat.sequence.filter((item) => item !== 'hold').length
    return new Promise((resolve) => setTimeout(() => { resolve(null); emitUntilHold() }, 0))
  }
  // Lets a held answer go on, as the next pieces of a stream that was only slow.
  window.__noctChatResume = () => {
    if (chatRun.pending[0] === 'hold') chatRun.pending.shift()
    emitUntilHold()
  }
  const cancelChat = () => {
    // A stale event after the cancel is sent on purpose: the page must ignore it.
    chatRun.pending = (config.scenario.chat?.afterCancel ?? []).map((item) => ({ index: item[0], frame: item[1] }))
    emitUntilHold()
    return Promise.resolve(null)
  }

  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) {
      callbackCount += 1
      callbacks[callbackCount] = callback
      return callbackCount
    },
    runCallback(id, data) {
      callbacks[id]?.(data)
    },
    invoke(command, payload) {
      window.__noctCalls.push({ command, payload })
      const attempt = (window.__noctAttempts[command] = (window.__noctAttempts[command] ?? 0) + 1)
      const mode = modeOf(command)
      // A save the scenario refuses gets the shell's own error, as a refused read does.
      if (command === 'settings_set' && config.scenario.saveFails) return Promise.reject(config.shellError)
      if (command === 'provider_key_save' && config.scenario.keyFails) return Promise.reject(config.scenario.keyFails)
      if (command === 'models_list' && config.scenario.modelsFail) return Promise.reject(config.modelsFailure)
      // The first approval of the Construir scenario meets a file that changed since the proposal.
      if (command === 'file_change_approve' && config.scenario.changes && attempt === 1) return Promise.reject(config.conflict)
      if (command === 'provider_chat_send') return startChat(payload)
      if (command === 'provider_chat_cancel') return cancelChat()
      if (command === 'ai_save_assistant' && config.scenario.chat?.saveFails) return Promise.reject(config.scenario.chat.saveFails)
      // The shell refuses an identifier with no vendor, as the core does.
      if (command === 'models_set_default_model' && typeof payload.input.modelId === 'string' && !payload.input.modelId.includes('/')) {
        return Promise.reject(config.modelRefused)
      }
      return new Promise((resolve, reject) => {
        const answer = () => {
          switch (mode) {
            case 'ready':
            case 'chat':
              resolve(replyOf(command, payload))
              break
            case 'empty':
              if (command === 'workspaces_list') resolve({ items: [...added], hasMore: false, nextCursor: null })
              else if (command === 'workspace_add' || command === 'provider_chat_status') resolve(replyOf(command, payload))
              else resolve([])
              break
            case 'error-then-retry':
              if (attempt === 1) reject(config.shellError)
              else resolve(replyOf(command, payload))
              break
            case 'invalid':
              resolve(invalidOf(command))
              break
            case 'slow':
              window.__noctReleases[command] = () => resolve(replyOf(command, payload))
              break
          }
        }
        if (mode === 'slow') answer()
        else setTimeout(answer, 0)
      })
    },
  }
}

// The provider section of Ajustes. The key field is write-only: it is a password field, it is emptied when the key is
// sent, and the key is in no part of the page afterwards. The consent is asked for one project at a time, and says
// what may be sent before it is confirmed.
async function providerFlow(page, scenario, check) {
  const calls = () => page.evaluate(() => window.__noctCalls ?? [])
  await page.getByTestId('provider').waitFor({ timeout: 8000 })
  const keyState = page.getByTestId('key-state')

  if (scenario.mode === 'invalid') {
    const alert = page.getByTestId('key-error')
    await alert.waitFor({ timeout: 8000 })
    check((await alert.textContent())?.includes('O NOCT respondeu de forma inesperada.'), 'a key reply outside the contract is an error')
    return
  }

  const configured = scenario.keyConfigured === true
  await keyState.getByText(configured ? 'Configurada' : 'Sem chave', { exact: true }).waitFor({ timeout: 8000 })

  // The key is changed only after "Trocar" opens the field, and the field is a password field.
  await page.getByTestId('key-change').click()
  const input = page.getByLabel('Nova chave do OpenRouter', { exact: true })
  check((await input.getAttribute('type')) === 'password', 'the key field is a password field')
  check((await input.getAttribute('autocomplete')) === 'off', 'the key field turns off browser autocompletion')

  await input.fill(TEST_KEY)
  const save = page.getByTestId('key-save')
  check(await save.isEnabled(), 'the save button is enabled once a key is typed')
  await save.click()

  if (scenario.keyFails) {
    const alert = page.getByTestId('key-save-error')
    await alert.waitFor({ timeout: 8000 })
    check((await alert.textContent())?.includes(SECURE_STORAGE_ERROR.message), 'a refused save shows the shell message')
    check((await input.inputValue()) === '', 'the key field is emptied even when the save is refused')
    check(!(await page.content()).includes(TEST_KEY), 'the key is in no part of the page after a refused save')
    check((await keyState.textContent())?.trim() === 'Sem chave', 'a refused save leaves the status as it was')
    return
  }

  await page.waitForFunction(() => (window.__noctCalls ?? []).some((call) => call.command === 'provider_key_save'))
  await page.waitForFunction(() => !document.querySelector('[data-testid="key-save-error"]'))
  // The field closes when the reply of the save is in; the badge may say so already, when a key was configured before.
  await page.getByLabel('Nova chave do OpenRouter', { exact: true }).waitFor({ state: 'detached', timeout: 8000 })
  await keyState.getByText('Configurada', { exact: true }).waitFor({ timeout: 8000 })
  check((await page.getByLabel('Nova chave do OpenRouter', { exact: true }).count()) === 0, 'the key field closes once the key is sent')
  check(!(await page.content()).includes(TEST_KEY), 'the key is in no attribute, text or script of the page after the save')
  const saves = (await calls()).filter((call) => call.command === 'provider_key_save')
  check(
    saves.length === 1 &&
      Object.keys(saves[0].payload).join() === 'input' &&
      Object.keys(saves[0].payload.input).sort().join() === 'key,provider' &&
      saves[0].payload.input.provider === 'openrouter' &&
      saves[0].payload.input.key === TEST_KEY,
    'the key is sent once, with its provider and nothing else',
  )

  await page.getByTestId('key-remove').click()
  await keyState.getByText('Sem chave', { exact: true }).waitFor({ timeout: 8000 })

  // The projects with permission. A granted project is listed, and its revocation is one call that names it. Ajustes never
  // grants a permission: the chat asks for it.
  const permissions = page.getByTestId('permissions')
  const none = permissions.getByTestId('permissions-status').getByText('Nenhum projeto com permissão ainda.', { exact: true })
  if (scenario.granted) {
    await permissions.getByText('alpha', { exact: true }).waitFor({ timeout: 8000 })
    await permissions.getByRole('button', { name: 'Revogar', exact: true }).click()
  }
  await none.waitFor({ timeout: 8000 })
  const revokes = (await calls()).filter((call) => call.command === 'provider_consent_revoke')
  check(
    scenario.granted
      ? revokes.length === 1 && revokes[0].payload.input.workspace === '/work/alpha'
      : revokes.length === 0,
    'a revocation names its project, and only a chosen revocation is sent',
  )
  check(!(await calls()).some((call) => call.command === 'provider_consent_grant'), 'Ajustes never grants a permission')

  // The default model of the current project. With the list, it is chosen from it; without it, the id is typed. Every write
  // names the project, and clearing sends null.
  if (scenario.modelsFail) {
    const fallback = page.getByTestId('models-fallback')
    await fallback.waitFor({ timeout: 8000 })
    check((await fallback.textContent())?.includes('Digite o id do modelo'), 'without the list, the group says why and offers the typed id')
    const modelField = page.getByLabel('Id do modelo padrão', { exact: true })
    check((await modelField.getAttribute('autocomplete')) === 'off', 'the model field turns off browser autocompletion')
    await modelField.fill('gpt-4o')
    await page.getByTestId('model-save').click()
    await page.getByTestId('model-error').getByText(MODEL_REFUSED.message, { exact: true }).waitFor({ timeout: 8000 })
    await modelField.fill('openai/gpt-4o:free')
    await page.getByTestId('model-save').click()
    await page.getByTestId('model-saved').waitFor({ timeout: 8000 })
    check((await page.getByTestId('model-current').textContent())?.trim() === 'openai/gpt-4o:free', 'the saved identifier is shown as the current model')
    await page.getByTestId('model-clear').click()
    await page.waitForFunction(() => !document.querySelector('[data-testid="model-clear"]'))
  } else {
    const picker = page.getByTestId('model-picker')
    await picker.waitFor({ timeout: 8000 })
    const options = picker.getByRole('option')
    await options.first().waitFor({ timeout: 8000 })
    const search = picker.getByRole('combobox', { name: 'Buscar modelo por nome ou id' })
    check((await picker.getByTestId('free-only').getAttribute('aria-pressed')) === 'true', '"Só gratuitos" is on by default')
    check((await options.count()) === 2, 'the free filter shows the two free models')
    const first = (await options.first().textContent()) ?? ''
    check(first.includes('DeepSeek V3') && first.includes('deepseek/deepseek-chat-v3:free') && first.includes('131k') && first.includes('Grátis'), 'a model shows its name, id, context and the free tag')
    await search.fill('gpt')
    await picker.getByTestId('models-empty').waitFor({ timeout: 8000 })
    await picker.getByTestId('free-only').click()
    await options.first().waitFor({ timeout: 8000 })
    check((await options.count()) === 1 && !((await options.first().textContent()) ?? '').includes('Grátis'), 'without the filter, a paid model is found by its name')
    await search.fill('')
    await search.press('ArrowDown')
    check((await search.getAttribute('aria-activedescendant')) === 'settings-models-option-1', 'the arrows move through the list')
    await search.press('ArrowUp')
    await search.fill('openai/')
    await search.press('Enter')
    await page.getByTestId('model-saved').waitFor({ timeout: 8000 })
    check((await page.getByTestId('model-current').textContent())?.trim() === 'openai/gpt-4o', 'Enter saves the chosen model')
    check((await page.getByTestId('model-picker').count()) === 0, 'the picker closes once a model is saved')
    await page.getByTestId('model-choose').click()
    await page.getByTestId('model-picker').waitFor({ timeout: 8000 })
    await page.getByRole('combobox', { name: 'Buscar modelo por nome ou id' }).press('Escape')
    check((await page.getByTestId('model-picker').count()) === 0, 'Esc closes the picker')
    await page.getByTestId('model-clear').click()
    await page.waitForFunction(() => !document.querySelector('[data-testid="model-clear"]'))
  }

  const modelWrites = (await calls()).filter((call) => call.command === 'models_set_default_model')
  check(
    modelWrites.map((call) => String(call.payload.input.modelId)).join('|') ===
      (scenario.modelsFail ? 'gpt-4o|openai/gpt-4o:free|null' : 'openai/gpt-4o|null'),
    'the chosen or typed identifiers are sent as they are, and clearing sends null',
  )
  check(
    modelWrites.every(
      (call) =>
        Object.keys(call.payload.input).sort().join() === 'modelId,workspace' && call.payload.input.workspace === '/work/alpha',
    ),
    'each model write carries only the project and the identifier, for the chosen project',
  )
}

// The chat of Revisar. With the switch off it says so, and the question box and the send button are disabled, so nothing
// is sent. With the switch on, the question goes with its project and its text only; the answer streams in order; a cancel
// keeps the text so far and ignores what follows; a save keeps the answer in a conversation of the project, and only when
// the person asks; each failure says what happened and that nothing was saved.
async function chatFlow(page, scenario, check) {
  await page.getByRole('button', { name: 'Revisar', exact: true }).click()
  const calls = () => page.evaluate(() => window.__noctCalls ?? [])
  const question = page.locator('#chat-question')
  await question.waitFor({ timeout: 8000 })
  const need = page.getByTestId('chat-need')
  const needText = async () => ((await need.count()) ? (await need.locator('.need__text').textContent())?.trim() : '')
  const send = page.getByTestId('chat-send')
  const answer = page.getByTestId('chat-answer')
  const save = page.getByTestId('chat-save')
  // Before the first question, a project with a key shows its name and four suggestions.
  if (scenario.keyConfigured) {
    await page.getByRole('heading', { level: 1 }).filter({ hasText: 'alpha' }).waitFor({ timeout: 8000 })
    check((await page.locator('.suggest button').count()) === 4, 'the empty conversation offers four suggestions')
  }
  if (scenario.chatEnabled === false) {
    await need.getByText(NEED_CHAT_OFF, { exact: true }).waitFor({ timeout: 8000 })
    check(await question.isEnabled(), 'the question box takes text while the chat is off')
    await question.fill('rascunho')
    check(await send.isDisabled(), 'the send button is disabled while the chat is off')
    check(!(await calls()).some((call) => call.command === 'provider_chat_send'), 'no question is sent while the chat is off')
    // Without a key, the conversation asks for one, and the button for it is there.
    await page.getByTestId('empty-no-key').waitFor({ timeout: 8000 })
    check(await page.getByRole('button', { name: 'Adicionar chave', exact: true }).isEnabled(), 'without a key, the conversation offers to add one')
    // "Ligar" turns the chat on, and the line then names the next missing step: the key.
    await need.getByRole('button', { name: 'Ligar', exact: true }).click()
    // The key is the next missing step, and the empty conversation already asks for it: the line goes away.
    await page.waitForFunction(() => !document.querySelector('[data-testid="chat-need"]'), null, { timeout: 8000 })
    const switches = (await calls()).filter((call) => call.command === 'provider_chat_set_enabled')
    check(switches.length === 1 && switches[0].payload.input.enabled === true, '"Ligar" turns the chat on with one call')
    check((await question.inputValue()) === 'rascunho', 'the draft stays in the composer')
    await question.fill('')
    return
  }
  if (scenario.noModel) {
    await need.getByText(NEED_MODEL, { exact: true }).waitFor({ timeout: 8000 })
    check(await question.isEnabled(), 'the question box takes text without a model')
    await question.fill(CHAT_QUESTION)
    check(await send.isDisabled(), 'nothing can be sent without a model')
    await need.getByRole('button', { name: 'Escolher modelo', exact: true }).click()
    await page.getByTestId('provider').waitFor({ timeout: 8000 })
    // The picker is open while the project has no model; the first free model is chosen with Enter.
    const search = page.getByRole('combobox', { name: 'Buscar modelo por nome ou id' })
    await search.waitFor({ timeout: 8000 })
    await page.getByTestId('model-picker').getByRole('option').first().waitFor({ timeout: 8000 })
    await search.press('Enter')
    await page.getByTestId('model-saved').waitFor({ timeout: 8000 })
    check((await page.getByTestId('model-card-name').textContent())?.trim() === CHAT_MODEL, 'the sidebar card shows the saved model')
    await page.getByRole('button', { name: 'Revisar', exact: true }).click()
    check((await page.getByTestId('composer-model').textContent())?.trim() === CHAT_MODEL, 'the composer pill shows the saved model')
    await need.getByText(NEED_PERMISSION, { exact: true }).waitFor({ timeout: 8000 })
    check(await send.isEnabled(), 'with a project, a key and a model, the question can be sent')
    // The pill opens the same picker over the composer; Esc closes it and gives the focus back to the pill.
    const pill = page.getByTestId('composer-model')
    await pill.click()
    const popover = page.getByTestId('composer-picker')
    await popover.waitFor({ timeout: 8000 })
    check((await pill.getAttribute('aria-expanded')) === 'true', 'the pill says the picker is open')
    await popover.getByRole('combobox').press('Escape')
    check((await popover.count()) === 0, 'Esc closes the picker over the composer')
    check(await pill.evaluate((element) => document.activeElement === element), 'the focus goes back to the pill')
    await pill.click()
    await popover.getByRole('option', { name: /Llama 3.3 70B/ }).click()
    await popover.waitFor({ state: 'detached', timeout: 8000 })
    check((await pill.textContent())?.trim() === 'meta-llama/llama-3.3-70b-instruct:free', 'a model chosen over the composer is the one the pill shows')
    return
  }
  // The project is the one the sidebar has chosen: there is no project control in the chat.
  const current = await page.locator('.proj[aria-current="true"] .project__name').textContent()
  check(current?.trim() === 'alpha', 'the question goes to the project that the sidebar has chosen')
  // A suggestion fills the composer, and sends nothing.
  await page.locator('.suggest button', { hasText: 'Resumo do projeto' }).click()
  check((await question.inputValue()) === 'Resumo do projeto', 'a suggestion fills the composer')
  check(!(await calls()).some((call) => call.command === 'provider_chat_send'), 'a suggestion sends nothing')
  await question.fill(CHAT_QUESTION)
  // The permission of the project is read first, and the send button waits for that reply: the line says so until it comes.
  await need.getByText(NEED_PERMISSION, { exact: true }).waitFor({ timeout: 8000 })
  check(await question.isEnabled(), 'the question box is never disabled when a project, a key and a model exist')
  check(await send.isEnabled(), 'the send button is enabled once a project and a question are chosen')
  // The project has no permission yet: the question stays in the composer, nothing is sent, and a card asks for it.
  const card = page.getByTestId('consent-card')
  await send.click()
  await card.waitFor({ timeout: 8000 })
  const cardText = (await card.textContent()) ?? ''
  check(cardText.includes('Sua pergunta') && cardText.includes('128 KiB'), 'the card names what is sent: the question and the budget')
  check(!cardText.includes('histórico'), 'the card does not claim that the earlier turns are sent')
  check((await question.inputValue()) === CHAT_QUESTION, 'the question stays in the composer until the project is allowed')
  check(!(await calls()).some((call) => call.command === 'provider_chat_send'), 'nothing is sent before the permission')
  await page.getByRole('button', { name: 'Agora não', exact: true }).click()
  check((await card.count()) === 0, '"Agora não" hides the card, and nothing is sent')
  await send.click()
  await card.waitFor({ timeout: 8000 })
  await page.getByTestId('consent-allow').click()
  await page.getByTestId('consent-granted').waitFor({ timeout: 8000 })
  check(await send.isEnabled(), 'after the permission, the question can be sent')
  check((await needText()) === '', 'nothing is missing once the project is allowed')
  await send.click()
  await page.waitForFunction(() => (window.__noctCalls ?? []).some((call) => call.command === 'provider_chat_send'))
  const sent = (await calls()).find((call) => call.command === 'provider_chat_send')
  check(
    sent?.payload.input.workspace === CHAT_PROJECT && sent.payload.input.question === CHAT_QUESTION,
    'the question names its project and its text, and nothing else',
  )
  check(/^__CHANNEL__:\d+$/.test(sent?.payload.onEvent ?? ''), 'the answer comes on a channel')
  check(!JSON.stringify(sent?.payload ?? {}).includes('sk-or'), 'no key is in the question')

  const { chat } = scenario
  if (chat.kind === 'proposes') {
    const proposals = page.getByTestId('chat-proposals')
    await proposals.waitFor({ timeout: 8000 })
    const text = (await proposals.textContent()) ?? ''
    check(text.includes('1 alteração proposta') && text.includes('src/x.rs'), 'the answer names the change it proposed')
    check((await page.getByTestId('chat-proposals-refused').textContent())?.includes('1 bloco'), 'a refused block is counted')
    check((await answer.textContent())?.includes('[alteração proposta: src/x.rs]'), 'the block is shown as one line, not the whole file')
    check(!((await answer.textContent()) ?? '').includes('fn x()'), 'the text of the file is not in the conversation')
    check(!(await calls()).some((call) => call.command.startsWith('file_change_')), 'nothing is approved from the conversation')
    await page.getByTestId('open-build').click()
    await page.getByRole('heading', { name: 'Construir', level: 1 }).waitFor({ timeout: 8000 })
    await page.getByTestId('change').filter({ hasText: 'src/x.rs' }).waitFor({ timeout: 8000 })
    check((await page.getByTestId('change').filter({ hasText: 'src/x.rs' }).count()) === 1, 'the proposed change waits in Construir')
    return
  }
  if (chat.kind === 'markdown') {
    const requests = []
    page.on('request', (request) => requests.push(request.url()))
    // The first piece ends inside an open code fence: the block is drawn as code up to the end, and nothing breaks.
    await answer.getByTestId('answer-code').waitFor({ timeout: 8000 })
    check((await answer.locator('h4', { hasText: 'Visão geral' }).count()) === 1, 'a heading of the answer is drawn as a heading')
    check((await answer.locator('strong').count()) >= 40 && (await answer.locator('em').count()) >= 40, 'bold and italic are drawn')
    check((await answer.getByTestId('answer-code').textContent())?.includes('fn main() {'), 'an open code fence is drawn as code while it streams')
    // The person scrolls up to read; the rest of the answer and its end do not move the view.
    const main = page.locator('.main')
    await page.waitForFunction(() => {
      const area = document.querySelector('.main')
      return area.scrollTop > 0 && area.scrollTop + area.clientHeight >= area.scrollHeight - 2
    })
    await main.evaluate((element) => { element.scrollTop = 0 })
    await page.evaluate(() => window.__noctChatResume())
    await page.getByTestId('chat-status').getByText(COMPLETE, { exact: true }).waitFor({ timeout: 8000 })
    await page.waitForTimeout(150)
    const top = await main.evaluate((element) => element.scrollTop)
    check(top === 0, `an answer that ends does not scroll a person who went up to read (at ${top})`)
    const code = answer.getByTestId('answer-code')
    check((await code.locator('pre code').textContent()) === 'fn main() {\n    println!("oi");\n}', 'the closed code block holds the code, and only it')
    check((await code.getByRole('button', { name: 'Copiar o código' }).count()) === 1, 'a code block has its own copy button')
    check((await code.locator('pre').evaluate((element) => getComputedStyle(element).fontFamily)).includes('monospace'), 'code is in a monospace font')
    const table = answer.getByTestId('answer-table')
    check((await table.locator('th').count()) === 5 && (await table.locator('tbody tr').count()) === 2, 'the table has its head and its rows')
    check(await table.evaluate((element) => element.scrollWidth > element.clientWidth && getComputedStyle(element).overflowX === 'auto'), 'a wide table scrolls sideways inside the answer')
    check(await answer.evaluate((element) => element.scrollWidth <= element.clientWidth + 1), 'the answer itself does not get wider than the conversation')
    check((await answer.locator('script, img, a, iframe, [onerror], [href], [src], [style]').count()) === 0, 'no markup of the answer becomes an element: no script, image, link or inline style')
    const text = (await answer.textContent()) ?? ''
    check(text.includes('<script>window.__noctInjected = true</script>'), 'raw HTML is shown as text')
    check(text.includes('<img src="https://evil.invalid/x.png"') && text.includes('<a href="javascript:alert(1)">'), 'inline HTML is shown as text')
    check(text.includes('o site (javascript:alert(1))'), 'a link is its text and its address, written out')
    check(text.includes('[imagem: logo]') && !text.includes('logo.png'), 'an image is its description, and its address is not used')
    check((await page.evaluate(() => window.__noctInjected)) === undefined, 'nothing in the answer ran')
    check(!requests.some((url) => url.includes('evil.invalid')), 'the answer makes no request')
    check((await answer.locator('.md-code, .md-table, .md-list, .md-quote').count()) === 4, 'lists, quotes, code and tables are drawn with their classes')
    // The room under the last line does not depend on the fade: it is padding of the conversation itself.
    check((await page.locator('.thread').evaluate((element) => getComputedStyle(element).paddingBottom)) === '28px', 'the conversation keeps room under its last line')
    await main.evaluate((element) => { element.scrollTop = element.scrollHeight })
    const gap = await page.evaluate(() => {
      const last = document.querySelector('.thread > .msg-ai:last-of-type .msg-tools').getBoundingClientRect()
      const composer = document.querySelector('.composer').getBoundingClientRect()
      return composer.top - last.bottom
    })
    check(gap >= 20, `at the end, the last line is clear of the composer (${gap}px)`)
    return
  }
  if (chat.kind === 'refused') {
    const alert = page.getByTestId('chat-error')
    await alert.waitFor({ timeout: 8000 })
    check((await alert.textContent())?.trim() === chat.refused.message, 'a refusal of the shell is shown with its message')
    check((await page.getByTestId('chat-answer').count()) === 0, 'no answer area is shown for a refused question')
    check(!(await save.isEnabled()), 'nothing can be saved after a refusal')
    return
  }
  if (chat.kind === 'stream-fails') {
    const alert = page.getByTestId('chat-error')
    await alert.waitFor({ timeout: 8000 })
    check((await alert.textContent())?.trim() === STREAM_TRUNCATED, 'an answer that breaks says so, and that nothing was saved')
    check((await answer.textContent())?.trim() === 'pa', 'the text that arrived stays on screen')
    check(!(await save.isEnabled()), 'a broken answer cannot be saved')
    return
  }
  if (chat.kind === 'hold') {
    const cancel = page.getByTestId('chat-cancel')
    await cancel.waitFor({ timeout: 8000 })
    await page.waitForFunction(() => document.querySelector('[data-testid="chat-answer"]')?.textContent === 'parcial')
    await cancel.click()
    await page.getByTestId('chat-status').getByText(CANCELLED, { exact: true }).waitFor({ timeout: 8000 })
    await page.waitForTimeout(200)
    check((await answer.textContent())?.trim() === 'parcial', 'a cancel keeps the text so far, and nothing after it')
    check(!(await save.isEnabled()), 'a cancelled answer cannot be saved')
    check((await page.getByTestId('chat-cancel').count()) === 0, 'the cancel control goes once the answer has stopped')
    check((await calls()).some((call) => call.command === 'provider_chat_cancel'), 'the cancel reaches the shell')
    return
  }

  await page.getByTestId('chat-status').getByText(COMPLETE, { exact: true }).waitFor({ timeout: 8000 })
  check((await answer.textContent())?.trim() === 'Olá mundo', 'the answer reads in order, whatever the order its events arrive in')
  check(await save.isEnabled(), 'a complete answer can be saved')
  await save.click()
  if (chat.kind === 'save-refused') {
    const alert = page.getByTestId('chat-save-error')
    await alert.waitFor({ timeout: 8000 })
    check((await alert.textContent())?.trim() === chat.saveFails.message, 'a save that the shell refuses says so')
    check((await page.getByTestId('chat-saved').count()) === 0, 'a refused save is not shown as saved')
    check(await save.isEnabled(), 'a refused save can be tried again')
    return
  }
  await page.getByTestId('chat-saved').waitFor({ timeout: 8000 })
  const log = await calls()
  const created = log.find((call) => call.command === 'conversations_create')
  check(created?.payload.input === CHAT_PROJECT, 'the conversation is created in the project of the answer')
  const saved = log.find((call) => call.command === 'ai_save_assistant')
  check(
    saved?.payload.input.conversationId === 'conv-1' && saved.payload.input.content === 'Olá mundo',
    'the answer is saved in that conversation, and nothing else is written',
  )
  check(!(await save.isEnabled()), 'a saved answer is saved once')
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
      modelRefused: MODEL_REFUSED,
      chatModel: CHAT_MODEL,
      addedProject: ADDED_PROJECT,
      catalog: CATALOG,
      modelsFailure: MODELS_FAILURE,
      changes: CHANGES,
      conflict: CONFLICT,
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
      if (scenario.provider) await providerFlow(page, scenario, check)
      if (scenario.mode === 'chat') await chatFlow(page, scenario, check)
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
        // The theme select shows the theme of the page. Pandoc is read from its row. Idioma, Segurança do agente and
        // Diagnóstico are selects, and carry their values.
        const pageTheme = await html.getAttribute('data-theme')
        check((await page.getByLabel('Tema', { exact: true }).inputValue()) === pageTheme, 'the theme select shows the theme of the page')
        const pandoc = page.locator('.item', { has: page.getByText('Pandoc', { exact: true }) }).locator('.body')
        check((await pandoc.textContent())?.trim() === '3.1.2', 'Pandoc shows 3.1.2')
        for (const [label, value] of [['Idioma', 'en'], ['Segurança do agente', 'untrusted'], ['Diagnóstico', 'on']]) {
          check((await page.getByLabel(label).inputValue()) === value, `${label} selects ${value}`)
        }
      }

      if (scenario.mode === 'invalid') {
        const alert = page.getByTestId('settings-error')
        await alert.waitFor({ timeout: 8000 })
        check((await alert.textContent())?.includes('O NOCT respondeu de forma inesperada.'), 'an unexpected reply is an error')
        check(!(await values.isVisible()), 'an unexpected reply shows no values')
      }

      if (scenario.mode === 'no-runtime') {
        const alert = page.getByTestId('settings-error')
        await alert.waitFor({ timeout: 8000 })
        check((await alert.textContent())?.includes('Não foi possível falar com o NOCT'), 'a page outside the shell says so')
        check(!(await values.isVisible()), 'no values are shown without the shell')
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
          check((await page.getByTestId('projects-empty').textContent())?.includes('Nenhum projeto ainda.'), 'an empty list says so')
          check(await page.getByTestId('projects').count() === 0, 'an empty list shows no items')
          await page.getByRole('button', { name: 'Revisar', exact: true }).click()
          await page.getByTestId('empty-no-project').waitFor({ timeout: 8000 })
          check(await page.locator('#chat-question').isDisabled(), 'without a project, the question box is disabled and the line says why')
          check(await page.getByTestId('empty-no-project').getByRole('button', { name: 'Adicionar projeto', exact: true }).isEnabled(), 'the empty conversation offers to add a project')
          check(await page.getByTestId('chat-need').count() === 0, 'the line does not repeat what the empty conversation asks')
          await page.getByTestId('add-project').click()
          await page.locator('.proj[aria-current="true"] .project__name', { hasText: 'novo' }).waitFor({ timeout: 8000 })
          check(await page.getByTestId('empty-no-project').count() === 0, 'an added project becomes the current one')
          check(await page.locator('#chat-question').isEnabled(), 'with a project, the question box takes text')
          check((await page.getByTestId('crumb').textContent())?.includes('novo'), 'the trail names the added project')
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
      // The glass switch turns the layer off and on, and keeps the choice for this viewer.
      // Chat scenarios leave Ajustes for Revisar; opening it again is a no-op when it is already open.
      await page.getByRole('button', { name: 'Ajustes', exact: true }).click()
      const glassSwitch = page.getByRole('switch', { name: 'Efeito de vidro', exact: true })
      await glassSwitch.click()
      check((await html.getAttribute('data-glass')) === 'off', 'the glass switch turns the layer off')
      check((await glassSwitch.getAttribute('aria-checked')) === 'false', 'the glass switch reports off')
      check((await page.evaluate(() => window.localStorage.getItem('noct.glass'))) === 'off', 'the glass choice is kept for this viewer')
      await glassSwitch.click()
      check((await html.getAttribute('data-glass')) === 'on', 'the glass switch turns the layer back on')

      // Construir with changes: each pending change shows its file and diff; Aprovar writes nothing when the file changed since
      // the proposal and says so; a second Aprovar applies it; Desfazer undoes it; Recusar closes the other one.
      if (scenario.changes) {
        await page.getByRole('button', { name: 'Construir', exact: true }).click()
        const cards = page.getByTestId('change')
        await cards.first().waitFor({ timeout: 8000 })
        check((await cards.count()) === 2, 'each pending change has its card')
        const main = cards.filter({ hasText: 'src/main.rs' })
        check((await main.locator('.diff__line--del').textContent())?.includes('linha antiga'), 'the diff shows the removed line')
        check((await main.locator('.diff__line--add').textContent())?.includes('linha nova'), 'the diff shows the added line')
        check((await main.textContent())?.includes('+1 −1'), 'the card counts the lines it adds and removes')
        check((await cards.filter({ hasText: 'scripts/setup.sh' }).textContent())?.includes('Arquivo novo'), 'a new file says so')
        check((await main.getByTestId('change-overwrites').count()) === 1 && (await main.getByRole('note').count()) === 1, 'a change to a file that exists says it overwrites it')
        check((await cards.filter({ hasText: 'scripts/setup.sh' }).getByTestId('change-overwrites').count()) === 0, 'a new file is not marked as an overwrite')
        const script = cards.filter({ hasText: 'scripts/setup.sh' })
        check(
          (await script.getByTestId('change-runs-on-build').textContent())?.trim() === 'Este arquivo roda código quando você compila ou instala. Leia o diff com atenção.',
          'a file that runs on build or install carries the warning',
        )
        check(await script.getByTestId('change-runs-on-build').isVisible(), 'the build warning is visible')
        check(await script.getByRole('button', { name: 'Aprovar a alteração em scripts/setup.sh', exact: true }).isEnabled(), 'the build warning does not block the approval')
        check((await main.getByTestId('change-runs-on-build').count()) === 0, 'an ordinary source file has no build warning')
        check((await cards.filter({ hasText: 'scripts/setup.sh' }).getByTestId('change-path').textContent()) === 'scripts/setup.sh', 'the card shows the whole path')
        check(await main.getByTestId('change-path').evaluate((element) => getComputedStyle(element).textOverflow !== 'ellipsis' && element.scrollWidth <= element.clientWidth + 1), 'the path is never cut')
        await page.getByRole('button', { name: 'Aprovar a alteração em src/main.rs', exact: true }).click()
        await page.getByTestId('change-error').getByText(CONFLICT.message, { exact: true }).waitFor({ timeout: 8000 })
        check((await cards.count()) === 2, 'a refused approval leaves the change waiting')
        await page.getByRole('button', { name: 'Aprovar a alteração em src/main.rs', exact: true }).click()
        await page.getByTestId('changes-applied').waitFor({ timeout: 8000 })
        check((await page.getByTestId('change-error').count()) === 0, 'an approval that worked clears the error')
        await page.getByRole('button', { name: 'Desfazer a alteração em src/main.rs', exact: true }).click()
        await page.getByRole('button', { name: 'Recusar a alteração em scripts/setup.sh', exact: true }).click()
        await page.getByTestId('changes-closed').waitFor({ timeout: 8000 })
        await page.waitForFunction(() => document.querySelectorAll('[data-testid="changes-closed"] .item').length === 2)
        check((await cards.count()) === 0, 'nothing waits once both are decided')
        const decisions = (await page.evaluate(() => window.__noctCalls ?? []))
          .filter((call) => call.command.startsWith('file_change_'))
          .map((call) => `${call.command}:${call.payload.input}`)
        check(
          decisions.join() === 'file_change_approve:c1,file_change_approve:c1,file_change_undo:c1,file_change_reject:c2',
          `each decision is one call that names its change (${decisions.join()})`,
        )
        check((await page.getByText('docs/guia.md').count()) === 0, 'Construir does not show a Markdown change')
        // Documentar shows the Markdown change, with the same approval.
        await page.getByRole('button', { name: 'Documentar', exact: true }).click()
        await page.getByRole('heading', { name: 'Documentar', level: 1 }).waitFor({ timeout: 8000 })
        await cards.first().waitFor({ timeout: 8000 })
        check((await cards.count()) === 1 && (await cards.getByTestId('change-path').textContent()) === 'docs/guia.md', 'Documentar shows only the Markdown change')
        check((await cards.getByTestId('change-overwrites').count()) === 1, 'Documentar marks an overwrite too')
        await page.getByRole('button', { name: 'Aprovar a alteração em docs/guia.md', exact: true }).click()
        await page.getByTestId('changes-applied').waitFor({ timeout: 8000 })
        check((await cards.count()) === 0, 'an approved Markdown change is applied')
      }

      // With no change proposed, Construir and Documentar show their three steps and no control for the work.
      if (scenario.unavailableModes) {
        await page.getByRole('button', { name: 'Construir', exact: true }).click()
        check(await page.getByRole('heading', { name: 'Construir', level: 1 }).isVisible(), 'Construir shows its heading')
        const stepsBox = page.getByTestId('build-steps')
        await stepsBox.waitFor({ timeout: 8000 })
        check((await stepsBox.locator('.step').count()) === 3, 'Construir shows the three steps')
        const stepWords = (await stepsBox.textContent()) ?? ''
        check(
          ['Você pede', 'O NOCT mostra o diff', 'Você aprova ou recusa'].every((words) => stepWords.includes(words)),
          'the steps say what the person does and what the window shows',
        )
        check(await page.getByTestId('unavailable').count() === 0, 'Construir does not say it is not available')
        check(await page.getByTestId('changes-empty').isVisible(), 'Construir says that no change was proposed yet')
        for (const control of ['Aprovar', 'Negar', 'Reverter', 'Confirmar alteração']) {
          check(await page.getByRole('button', { name: control, exact: true }).count() === 0, 'Construir offers no ' + control + ' control')
        }
        check(await page.locator('.diff, .prose, .cmd').count() === 0, 'Construir shows no example diff or command')
        await page.getByRole('button', { name: 'Documentar', exact: true }).click()
        check(await page.getByRole('heading', { name: 'Documentar', level: 1 }).isVisible(), 'Documentar shows its heading')
        await page.getByTestId('changes-empty').getByText('Nenhuma alteração de documentação proposta ainda.', { exact: true }).waitFor({ timeout: 8000 })
        check(await page.getByTestId('unavailable').count() === 0, 'Documentar does not say it is not available')
        for (const control of ['Aprovar', 'Negar', 'Reverter', 'Confirmar alteração']) {
          check(await page.getByRole('button', { name: control, exact: true }).count() === 0, `Documentar offers no ${control} control`)
        }
        check(await page.locator('.diff, .prose, .cmd').count() === 0, 'Documentar shows no example diff or command')
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
        // A setting the shell does not save says so, and its control goes back to the saved value.
        await page.getByRole('button', { name: 'Ajustes', exact: true }).click()
        await page.getByLabel('Idioma').selectOption('pt-BR')
        const refused = page.getByTestId('settings-save-error')
        await refused.waitFor({ timeout: 8000 })
        check((await refused.textContent())?.includes(SHELL_ERROR.message), 'a setting that is not saved says so with the shell message')
        check((await page.getByLabel('Idioma').inputValue()) === 'en', 'the language goes back to the saved value')
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
        await page.getByTestId('build-steps').waitFor({ timeout: 8000 })
        check(await page.getByTestId('build-steps').isVisible(), 'the build mode shows its three steps')
        await page.getByRole('button', { name: 'Documentar', exact: true }).click()
        check(await page.getByRole('heading', { name: 'Documentar', level: 1 }).isVisible(), 'the document mode is shown')
        await page.getByRole('button', { name: 'Ajustes', exact: true }).click()
        check((await page.getByRole('button', { name: 'Ajustes', exact: true }).getAttribute('aria-current')) === 'page', 'Ajustes is the current page while open')
        check((await pressed('Documentar')) === 'false', 'no mode is pressed while Ajustes is open')
        check(await values.isVisible(), 'Ajustes shows the saved values')
        // Ajustes saves each change at once, with the one setting that changed, and shows the settings the shell replied with.
        for (const [label, value, expected] of [
          ['Idioma', 'pt-BR', '{"input":{"language":"pt-BR"}}'],
          ['Segurança do agente', 'on-request', '{"input":{"approvalPolicy":"on-request"}}'],
          ['Diagnóstico', 'off', '{"input":{"diagnosticMode":false}}'],
        ]) {
          const before = await page.evaluate(() => window.__noctCalls.length)
          await page.getByLabel(label).selectOption(value)
          await page.waitForFunction((count) => window.__noctCalls.length > count, before)
          await page.waitForFunction(() => !document.querySelector('[data-testid="settings-saving"]'))
          const last = await page.evaluate(() => window.__noctCalls.filter((call) => call.command === 'settings_set').at(-1))
          check(JSON.stringify(last?.payload) === expected, `${label} is saved alone`)
          check((await page.getByLabel(label).inputValue()) === value, `${label} shows the value the shell replied with`)
        }
        check((await page.getByTestId('settings-save-error').count()) === 0, 'a saved change shows no error')
        await page.getByRole('button', { name: 'Revisar', exact: true }).click()
        check((await pressed('Revisar')) === 'true', 'a mode leaves Ajustes')

        // The chat is off by default: its switch says so, and nothing can be sent.
        await chatFlow(page, { ...scenario, chatEnabled: false }, check)

        // A card keeps its box under the pointer: the hover changes the shadow only.
        const card = page.getByTestId('model-card')
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
        // content pane, the project names inside the sidebar, and the question box inside the window's width.
        for (const size of SIZES) {
          await page.setViewportSize(size)
          const fit = await page.evaluate(() => {
            const box = (selector) => document.querySelector(selector).getBoundingClientRect()
            const content = box('.content')
            const modes = box('.segmented')
            const sidebar = box('.sidebar')
            return {
              horizontal: document.documentElement.scrollWidth <= window.innerWidth,
              questionRight: box('#chat-question').right,
              innerWidth: window.innerWidth,
              sidebar: sidebar.width,
              modesInside: modes.left >= content.left && modes.right <= content.right,
              projectsInside: [...document.querySelectorAll('.project')].every((item) => item.getBoundingClientRect().right <= sidebar.right),
            }
          })
          const label = `${size.width}x${size.height}`
          check(fit.horizontal, `no horizontal scroll at ${label}`)
          check(fit.questionRight <= fit.innerWidth, `the question box fits the window width at ${label}`)
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
        check(
          calls.every((call) => [...READ_COMMANDS, 'settings_set', ...PROVIDER_WRITES, ...CHAT_COMMANDS].includes(call.command)),
          'only the read commands, the setting saves, the provider writes and the chat commands are called',
        )
        const reads = calls.filter((call) => ![...PROVIDER_WRITES, 'settings_set', ...CHAT_COMMANDS].includes(call.command))
        check(
          reads.every((call) =>
            ['provider_consent_status', 'models_bindings_get'].includes(call.command)
              ? CONSENT_PAYLOAD.test(JSON.stringify(call.payload))
              : JSON.stringify(call.payload) === PAYLOAD_OF[call.command],
          ),
          'each read is sent its own payload: settings_get none, workspaces_list the first page at the largest limit, the key status none, a consent its project',
        )
        const saves = [...savesBeforeReload, ...calls.filter((call) => call.command === 'settings_set')]
        check(
          saves.every((call) => /^\{"input":\{"(theme":"(light|dark)"|language":"(pt-BR|en)"|approvalPolicy":"(untrusted|on-request)"|diagnosticMode":(true|false))\}\}$/.test(JSON.stringify(call.payload))),
          'each save sends one setting and nothing else',
        )
        const themeSaves = saves.filter((call) => Object.hasOwn(call.payload.input, 'theme'))
        check(
          themeSaves.length === (scenario.full ? 2 : scenario.saveFails ? 1 : 0),
          `the theme is saved once for each press of the theme button (saves: ${themeSaves.length})`,
        )
        const settingSaves = saves.length - themeSaves.length
        check(
          settingSaves === (scenario.full ? 3 : scenario.saveFails ? 1 : 0),
          `each change in Ajustes is saved once (saves: ${settingSaves})`,
        )
        check(count('settings_get') === (scenario.mode === 'error-then-retry' ? 2 : 1), 'settings_get is called once, or once more after a retry')
        check(count('workspaces_list') === (projects === 'error-then-retry' || projects === 'empty' ? 2 : 1), 'workspaces_list is called once, or once more after a retry or an added project')
        // The key is read once, when the window opens: Ajustes shows the key the sidebar holds, and a change is shown from the
        // reply of its own save.
        check(count('provider_key_status') === 1, `provider_key_status is read once, on load (reads: ${count('provider_key_status')})`)
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
    check(result.pageErrors.length === 0, `the page throws no error${result.pageErrors.length ? `: ${result.pageErrors.join(' | ')}` : ''}`)
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
