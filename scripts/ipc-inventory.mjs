// Prints docs/ipc-inventory.md from the IPC sources. It only reads: shared/ipc, electron/preload.ts and
// electron/ipc. The risk and port columns are classifications written here, and the script fails if a
// channel has none, so a new channel cannot be added without one.
//
//   node scripts/ipc-inventory.mjs > docs/ipc-inventory.md

import fs from 'node:fs'
import path from 'node:path'

const root = path.resolve(import.meta.dirname, '..')
const read = (file) => fs.readFileSync(path.join(root, file), 'utf8')

const EVENT_METHODS = {
  'workspace.changed': 'onChanged',
  'projectIndex.changed': 'onStatus',
  'semanticIndex.changed': 'onStatus',
  'engineeringIntelligence.changed': 'onChanged',
  'validation.changed': 'onStatus',
  'changeControl.changed': 'onChanged',
  'ai.event': 'onEvent',
  'ai.status': 'onStatus',
  'updates.changed': 'onStateChanged',
}

// Risk (primary) and port. Risk values: read, database write, filesystem read, filesystem write,
// process/shell, network, secret, control, event, and the OS clipboard forms.
// Port values: P3 and P4 and P5 are the packages of the migration plan; "P3 (ported)" means a Rust command
// already exists; "later" is outside the queue for now; "stop" is a stop condition of the plan; "event" is
// main-to-renderer and has no request.
const CLASS = {
  'recovery.list': ['read', 'P3', 'Returns every interrupted operation with no limit. A paginated form is needed before the port.'],
  'evidence.list': ['read', 'P3', 'Takes a conversation and an execution id.'],
  'workspace.select': ['database write', 'later', 'Opens a native folder dialog and authorises the workspace. Authorisation stays in the main process, and restored workspaces remain unauthorised until reselected.'],
  'workspace.validate': ['filesystem read', 'P3', 'Checks a path and its scope. No authorisation.'],
  'workspace.list': ['read', 'P3 (ported)', 'Rust command `workspaces_list`, paged: a required limit (1 to 500) and an optional cursor, the key of the last project of the previous page. `hasMore` says whether more follow, and `nextCursor` is the key that asks for them. The order breaks ties by path, which the TypeScript list does not. First cut: the interface reads the first page only.'],
  'workspace.remove': ['database write', 'P4', 'Removes a workspace record.'],
  'workspace.favorite': ['database write', 'P4 (ported)', 'Rust command `workspace_set_favorite`: the project must be saved, and the request has the same fields and bounds as `workspaceFavoriteSchema`. No interface control yet.'],
  'workspace.openTool': ['process/shell', 'later', 'Starts an editor or a terminal in the workspace.'],
  'workspace.watch': ['filesystem read', 'later', 'Starts or stops a filesystem watcher on the workspace.'],
  'workspace.changed': ['event', 'event', 'Watcher events.'],
  'projectIndex.status': ['read', 'P3', ''],
  'projectIndex.start': ['database write', 'later', 'Reads the workspace files and writes the index.'],
  'projectIndex.cancel': ['control', 'later', 'Cancels an indexing run in the main process.'],
  'projectIndex.retry': ['database write', 'later', 'Runs failed indexing again. Reads files.'],
  'projectIndex.summary': ['read', 'P3', ''],
  'projectIndex.files': ['read', 'P3', 'The preload default limit is 50,000. The port caps it at 500.'],
  'projectIndex.symbols': ['read', 'P3', 'Query up to 500 characters; limit up to 100 by schema.'],
  'projectIndex.imports': ['read', 'P3', ''],
  'projectIndex.exports': ['read', 'P3', ''],
  'projectIndex.stack': ['read', 'P3', ''],
  'projectIndex.exclusions': ['read', 'P3', ''],
  'projectIndex.changed': ['event', 'event', ''],
  'semanticIndex.status': ['read', 'P3', ''],
  'semanticIndex.start': ['database write', 'later', 'Reads the workspace and writes semantic units.'],
  'semanticIndex.cancel': ['control', 'later', ''],
  'semanticIndex.summary': ['read', 'P3', ''],
  'semanticIndex.search': ['read', 'P3', 'Filters by path, language, kind and symbol.'],
  'semanticIndex.changed': ['event', 'event', ''],
  'engineeringIntelligence.report': ['read', 'P3', 'Computes a health report from stored data.'],
  'engineeringIntelligence.changed': ['event', 'event', ''],
  'validation.run': ['process/shell', 'later', 'Runs project tools (ruff, biome, oxlint and others) through a process runner, then writes the run record.'],
  'validation.cancel': ['control', 'later', 'Stops a validation process.'],
  'validation.list': ['read', 'P3', ''],
  'validation.page': ['read', 'P3', 'Already paginated with offset and limit.'],
  'validation.latest': ['read', 'P3', ''],
  'validation.changed': ['event', 'event', ''],
  'changeControl.get': ['read', 'P3', ''],
  'changeControl.changes': ['read', 'P3', ''],
  'changeControl.diff': ['read', 'P3', 'The diff output must be bounded before the port.'],
  'changeControl.hunks': ['read', 'P3', ''],
  'changeControl.editHunk': ['database write', 'P4', 'Stores an edited patch for one hunk. Confirm it writes no file before the port.'],
  'changeControl.decideHunk': ['filesystem write', 'later', 'Accepting a hunk applies its patch to the workspace file.'],
  'changeControl.decide': ['filesystem write', 'later', 'Accepting a change applies it to the workspace.'],
  'changeControl.changed': ['event', 'event', ''],
  'conversations.list': ['read', 'P3 (ported)', 'Already a Rust command (`conversations_list`), paged: a required limit from 1 to 500 and a cursor.'],
  'conversations.page': ['read', 'P3', 'Paginated.'],
  'conversations.create': ['database write', 'P4 (ported)', 'Rust command `conversations_create`: the argument is the project path. The project must be saved, authorized and available, and it must pass the scope check with the blocked roots. The `.nocturne` workspace context is not created yet; the command writes the database only.'],
  'conversations.messages': ['read', 'P3', 'Returns every message with no limit. The port uses the paged form.'],
  'conversations.messagePage': ['read', 'P3', 'Paginated.'],
  'conversations.delete': ['database write', 'P4', 'Deletes a conversation and its messages.'],
  'ai.send': ['network', 'stop', 'Sends the prompt to a provider and, in build mode, can run tools. Provider network calls are a stop condition.'],
  'ai.cancel': ['control', 'later', 'Cancels the turn in progress.'],
  'ai.event': ['event', 'event', 'Agent events.'],
  'ai.saveAssistant': ['database write', 'P4', 'Saves an assistant message.'],
  'ai.approve': ['process/shell', 'later', 'Approves a pending action that can run a command or change files.'],
  'ai.status': ['event', 'event', ''],
  'ai.rollbackStatus': ['read', 'P3', 'Reads the rollback state. The rollback itself is a separate entry.'],
  'ai.rollback': ['filesystem write', 'later', 'Restores files from a checkpoint and changes the database.'],
  'files.attach': ['filesystem read', 'later', 'Opens a native dialog and reads the chosen files.'],
  'files.open': ['process/shell', 'later', 'Opens or reveals a path with the operating system.'],
  'files.preview': ['filesystem read', 'later', 'Reads a file for preview. Needs the path policy first.'],
  'memory.get': ['filesystem read', 'later', 'Reads the workspace context file and the database record.'],
  'memory.set': ['filesystem write', 'later', 'Writes the workspace context file atomically and the database record.'],
  'brain.page': ['read', 'P3', ''],
  'brain.history': ['read', 'P3', ''],
  'brain.create': ['database write', 'P4 (ported)', 'Rust command `brain_create`: the conversation must exist and its project must pass the same check as a conversation write. The content has the schema rules: trimmed, 1 to 8,000 UTF-16 units, and not a credential. Stored as a manual candidate, with its history entry in one transaction. No interface control yet.'],
  'brain.update': ['database write', 'P4 (ported)', 'Rust command `brain_update`: the same conversation and project checks as `brain_create`, and the memory must exist in that project. A status change must follow the lifecycle. The fields that are present are checked as in the schema, and at least one must be present. The credential rule of create applies to the content. No interface control yet.'],
  'brain.delete': ['database write', 'P4', ''],
  'brain.extract': ['database write', 'P4 (ported)', 'Rust command `brain_extract`: the reply is read the way `extractBrainMemoryCandidates` reads it, with the same marker, the same five-candidate cap and the same skipped items. The candidates are stored as one unit. A failed store is a reply with the TypeScript warning, not an error. No provider call.'],
  'artifacts.list': ['read', 'P3', ''],
  'artifacts.page': ['read', 'P3', ''],
  'artifacts.delete': ['database write', 'P4', 'Confirm whether it also removes files before the port.'],
  'suggestions.list': ['read', 'P3', ''],
  'suggestions.page': ['read', 'P3', ''],
  'suggestions.create': ['database write', 'P4 (ported)', 'Rust command `suggestions_create`: the reply is read by `extract_suggestions`, which follows `extractSuggestions` and `suggestionInputSchema`, with the same marker rules, the same strict items and the same two warnings. A structured reply is reconciled with the history of its conversation (`reconcile`, with the renderer-provided content unable to resolve missing suggestions), and the comparison text is appended to the reply. A reply without a usable snapshot stores nothing. The request is the shared `ReplyRequest` of `brain_extract`. Not ported: the log line of the TypeScript shell.'],
  'suggestions.status': ['database write', 'P4 (ported)', 'Rust command `suggestions_status`: the conversation and its project are checked as in `suggestions_create`, then the suggestion must belong to the conversation (`unknown-suggestion` otherwise), then the move must be one the lifecycle allows, with the table of `SuggestionRepository.setStatus`. The result is at most 20,000 units and is kept in the decision history. Not ported: the `.nocturne` decision log that the TypeScript shell writes after a move, and its warning.'],
  'data.export': ['filesystem write', 'later', 'Writes a full backup to a file the user picks. It holds user data.'],
  'data.import': ['database write', 'later', 'Reads a backup file and replaces data. Sensitive; it needs its own review.'],
  'diagnostics.openLogs': ['process/shell', 'later', 'Opens the log file with the operating system.'],
  'diagnostics.copy': ['read', 'P5', 'Returns the sanitised diagnostic report. Its redaction must match TypeScript.'],
  'diagnostics.export': ['filesystem write', 'P5', 'Writes the sanitised diagnostic to a file the user picks.'],
  'diagnostics.rendererError': ['filesystem write', 'P5', 'Logs a renderer error. It goes through the log redaction.'],
  'diagnostics.rendererStats': ['filesystem write', 'P5', 'Logs renderer performance numbers.'],
  'settings.get': ['read', 'P3 (ported)', 'Already a Rust command (`settings_get`).'],
  'settings.set': ['database write', 'P4 (ported)', 'Rust command `settings_set`. The sidebar theme button saves the theme through it; the other settings have no control yet.'],
  'updates.getState': ['read', 'P8', 'Updater state held in the process. No network.'],
  'updates.check': ['network', 'stop', 'Contacts the update server. The updater is a stop condition.'],
  'updates.download': ['network', 'stop', 'Downloads an update.'],
  'updates.retry': ['network', 'stop', 'Retries a download.'],
  'updates.install': ['process/shell', 'stop', 'Installs the update and restarts. Needs the signing key.'],
  'updates.changed': ['event', 'stop', 'Updater state.'],
  'providers.list': ['read', 'P3', 'Returns summaries. Confirm that no field holds a credential before the port.'],
  'providers.create': ['secret', 'stop', 'Receives a credential from the renderer and stores it. A vault change is a stop condition.'],
  'providers.update': ['secret', 'stop', 'Sets or clears a credential. A vault change is a stop condition.'],
  'providers.remove': ['secret', 'stop', 'Removes a provider and its credential.'],
  'providers.testConnection': ['network', 'stop', 'Calls the provider with the stored credential.'],
  'providers.diagnose': ['network', 'stop', 'Calls the provider with the stored credential.'],
  'models.list': ['read', 'P3', ''],
  'models.refresh': ['network', 'stop', 'Fetches the model catalogue from the provider.'],
  'models.bindings': ['read', 'P3', ''],
  'models.setBindings': ['database write', 'P4', ''],
  'git.status': ['process/shell', 'later', 'Runs git in the workspace.'],
  'git.commit': ['process/shell', 'later', 'Runs git commit in the workspace after the user confirms.'],
  'documents.prepareMarkdown': ['filesystem read', 'later', 'Builds a preview of an update to a Markdown file in the workspace.'],
  'documents.applyMarkdown': ['filesystem write', 'later', 'Writes the target file atomically, with an expected hash.'],
  'documents.export': ['process/shell', 'later', 'Runs a converter process and writes the file the user picks.'],
  'clipboard.readText': ['read (OS clipboard)', 'later', 'Returns up to 10,000 characters of the clipboard. Needs a user gesture before the port.'],
  'clipboard.writeText': ['write (OS clipboard)', 'later', 'Writes the clipboard.'],
}

// The channel catalogue.
const channelsSource = read('shared/ipc/channels.ts')
const catalogue = channelsSource.slice(channelsSource.indexOf('{'), channelsSource.indexOf('} as const'))
const channels = []
for (const line of catalogue.split('\n')) {
  const group = line.match(/^\s*(\w+): \{ (.*) \},?\s*$/)
  if (!group) continue
  for (const pair of group[2].matchAll(/(\w+): '([^']+)'/g)) {
    channels.push({ group: group[1], key: pair[1], value: pair[2] })
  }
}

// Where each channel is used.
const preload = read('electron/preload.ts')
const ipcDirectory = path.join(root, 'electron', 'ipc')
const ipcFiles = fs.readdirSync(ipcDirectory).filter((file) => file.endsWith('.ts')).sort()

function handlerOf(group, key) {
  const token = `IPC_CHANNELS.${group}.${key}`
  for (const file of ipcFiles) {
    const lines = fs.readFileSync(path.join(ipcDirectory, file), 'utf8').split('\n')
    const index = lines.findIndex((line) => line.includes(token) && !line.trimStart().startsWith('import'))
    if (index >= 0) return { file: `electron/ipc/${file}`, line: index + 1, lines, index }
  }
  return null
}

// What the input is checked with, read from the handler's own lines.
function validationOf(found) {
  const { lines, index } = found
  const first = lines[index]
  if (/\(\)\s*=>/.test(first) && !/\bvalue\b|\binput\b/.test(first)) return 'none (no input)'
  const block = []
  for (let i = index; i < Math.min(lines.length, index + 14); i++) {
    if (i > index && /\.handle\(|register\(IPC_CHANNELS|\bhandle\(IPC_CHANNELS/.test(lines[i])) break
    block.push(lines[i])
  }
  const text = block.join('\n')
  // Only a parse of the raw input counts. An output parse (`availabilitySchema.parse(await ...)`) does not.
  const inputLines = block.filter((line) => /\.parse\(\s*(value|input)\s*\)/.test(line))
  const schemas = [...new Set(inputLines.flatMap((line) => [...line.matchAll(/\b(\w*Schema)\b/g)].map((match) => match[1])))]
  const parts = []
  if (schemas.length) parts.push(`zod: ${schemas.join(', ')}`)
  else if (inputLines.length) parts.push('inline .parse')
  // The handler often passes the raw value to a helper in the same file, and that helper may pass it on.
  // Helpers are followed up to two levels deep.
  const follow = (source, depth) => {
    if (depth === 0) return
    for (const helper of new Set([...source.matchAll(/\b(\w+)\(value\)/g)].map((match) => match[1]))) {
      const definition = lines.findIndex((line) => new RegExp(`(const|function)\\s+${helper}\\b`).test(line))
      if (definition < 0) continue
      const body = lines.slice(definition, definition + 8).join('\n')
      const names = [...new Set([...body.matchAll(/\b(\w*Schema)\b/g)].map((match) => match[1]))]
      if (names.length) parts.push(`via ${helper}: zod ${names.join(', ')}`)
      else if (/\.parse\(/.test(body)) parts.push(`via ${helper}: inline .parse`)
      else follow(body, depth - 1)
    }
  }
  follow(text, 2)
  return parts.length ? parts.join('; ') : 'UNVERIFIED'
}

// The TypeScript signature from the renderer-facing API, when it is on one line.
const contracts = read('shared/ipc/contracts.ts')
function signatureOf(group, key) {
  if (EVENT_METHODS[`${group}.${key}`]) return 'event listener (see contracts.ts)'
  const start = contracts.search(new RegExp(`\\n\\s{2}${group}: \\{`))
  if (start < 0) return 'see contracts.ts'
  const open = contracts.indexOf('{', start)
  let depth = 0
  let close = open
  for (; close < contracts.length; close++) {
    if (contracts[close] === '{') depth++
    else if (contracts[close] === '}') { depth--; if (depth === 0) break }
  }
  const block = contracts.slice(open, close)
  const match = block.match(new RegExp(`(?:^|[\\s;{])${key}\\(([^()]*)\\)\\s*:\\s*`))
  if (!match) return 'see contracts.ts'
  // The return type ends at a `;` or a line end outside any brackets. `=>` is not a bracket.
  const returnStart = match.index + match[0].length
  let nesting = 0
  let end = returnStart
  for (; end < block.length; end++) {
    const char = block[end]
    if (char === '>' && block[end - 1] === '=') continue
    if ('<{(['.includes(char)) nesting++
    else if ('>})]'.includes(char)) {
      if (nesting === 0) break
      nesting--
    } else if ((char === ';' || char === '\n') && nesting === 0) break
  }
  return `${key}(${match[1].trim()}): ${block.slice(returnStart, end).trim()}`
}

const callerOf = (group, key) => `window.nocturne.${group}.${EVENT_METHODS[`${group}.${key}`] ?? key}`

// Checks. The script stops if a channel is not classified.
const missing = channels.filter((channel) => !CLASS[`${channel.group}.${channel.key}`])
if (missing.length) {
  console.error(`Channels without a classification: ${missing.map((c) => `${c.group}.${c.key}`).join(', ')}`)
  process.exit(1)
}
const unknown = Object.keys(CLASS).filter((name) => !channels.some((c) => `${c.group}.${c.key}` === name))
if (unknown.length) {
  console.error(`Classifications for channels that do not exist: ${unknown.join(', ')}`)
  process.exit(1)
}

const rows = channels.map((channel) => {
  const name = `${channel.group}.${channel.key}`
  const [risk, port, note] = CLASS[name]
  const found = handlerOf(channel.group, channel.key)
  const isEvent = risk === 'event'
  return {
    name,
    value: channel.value,
    caller: isEvent ? `${callerOf(channel.group, channel.key)} (listener)` : callerOf(channel.group, channel.key),
    contract: signatureOf(channel.group, channel.key),
    validation: isEvent ? 'not applicable (sent by the main process)' : found ? validationOf(found) : 'NO HANDLER FOUND',
    handler: found ? `${found.file}:${found.line}` : (isEvent ? 'sent from the main process' : 'NO HANDLER FOUND'),
    risk,
    port,
    note,
    inPreload: preload.includes(`channels.${channel.group}.${channel.key}`),
    isEvent,
  }
})

const invokable = rows.filter((row) => !row.isEvent)
const uncovered = invokable.filter((row) => !row.inPreload || row.handler === 'NO HANDLER FOUND')
if (uncovered.length) {
  console.error(`Invokable channels without a preload entry or a handler: ${uncovered.map((row) => row.name).join(', ')}`)
  process.exit(1)
}
const unvalidated = invokable.filter((row) => row.validation === 'UNVERIFIED')

const cell = (text) => String(text).replace(/\|/g, '\\|')
const count = (predicate) => rows.filter(predicate).length

const out = []
out.push('# IPC inventory')
out.push('')
out.push('Generated by `scripts/ipc-inventory.mjs` from `shared/ipc/channels.ts`, `shared/ipc/contracts.ts`,')
out.push('`electron/preload.ts` and `electron/ipc/`. Regenerate it after a channel changes:')
out.push('')
out.push('```sh')
out.push('node scripts/ipc-inventory.mjs > docs/ipc-inventory.md')
out.push('```')
out.push('')
out.push('The script fails if a channel has no classification, no preload entry or no handler. The classification')
out.push('(risk, port) is written in the script, not inferred, so a change of risk is a reviewed change.')
out.push('')
out.push('## Totals')
out.push('')
out.push(`- Channels in the catalogue: ${rows.length}.`)
out.push(`- Invokable from the renderer: ${invokable.length}. Main-to-renderer events: ${rows.length - invokable.length}.`)
out.push(`- Invokable channels with a preload entry and a handler: ${invokable.length - uncovered.length} of ${invokable.length}.`)
out.push(`- Invokable channels whose input check is not found in the handler: ${unvalidated.length}${unvalidated.length ? ` (${unvalidated.map((r) => r.name).join(', ')})` : ''}.`)
out.push(`- Already a Rust command: ${count((r) => r.port.includes('ported'))} (\`workspaces_list\`, \`conversations_list\`, \`conversations_create\`, \`brain_create\`, \`brain_extract\`, \`brain_update\`, \`suggestions_create\`, \`suggestions_status\`, \`settings_get\`, \`settings_set\`, \`workspace_set_favorite\`).`)
out.push('')
out.push('| Risk | Channels |')
out.push('| --- | ---: |')
const riskOrder = ['read', 'read (OS clipboard)', 'database write', 'filesystem read', 'filesystem write', 'process/shell', 'network', 'secret', 'control', 'write (OS clipboard)', 'event']
for (const risk of riskOrder) out.push(`| ${risk} | ${count((r) => r.risk === risk)} |`)
out.push('')
out.push('| Port | Channels |')
out.push('| --- | ---: |')
for (const port of ['P3 (ported)', 'P3', 'P4 (ported)', 'P4', 'P5', 'P8', 'later', 'stop', 'event']) {
  out.push(`| ${port} | ${count((r) => r.port === port)} |`)
}
out.push('')
out.push('Port meanings: P3 and P4 and P5 are the packages of the migration plan. `later` is outside the current queue.')
out.push('`stop` is a stop condition of the plan (vault, provider network, updater, signing). `event` has no request.')
out.push('')
out.push('## Port order')
out.push('')
out.push('One cut at a time. Each cut adds the ACL in the Tauri manifest, a capability only for the `main` window,')
out.push('input and output validation, the paged form with a limit from 1 to 500 and `has_more` for every list, ACL')
out.push('tests, service tests, a TypeScript and Rust differential, and a browser mock. No command returns a secret.')
out.push('')
out.push('1. **P3, first cut.** The four commands that already exist: `settings_get`, `workspaces_list`, `conversations_list`.')
out.push('   `settings_set` belongs to P4. The two lists need the paged form first.')
out.push('2. **P3, next cuts.** Reads with no write and no process: `conversations.page`, `conversations.messagePage`,')
out.push('   `validation.list`, `validation.page`, `validation.latest`, `semanticIndex.status`, `semanticIndex.summary`,')
out.push('   `semanticIndex.search`, `projectIndex.status`, `projectIndex.summary`, `changeControl.get`, `changeControl.changes`,')
out.push('   `changeControl.hunks`, `models.list`, `models.bindings`, `brain.page`, `brain.history`, `artifacts.list`,')
out.push('   `artifacts.page`, `suggestions.list`, `suggestions.page`, `ai.rollbackStatus`, `engineeringIntelligence.report`.')
out.push('   The rest of the P3 rows (project index files, symbols, imports, exports, stack, exclusions; change diffs;')
out.push('   `providers.list`; `recovery.list`; `evidence.list`) follow after their bounds are set.')
out.push('3. **P4, database writes.** Still to port: `models.setBindings`, `ai.saveAssistant`,')
out.push('   `changeControl.editHunk`. The deletes come last, each in its own commit: `conversations.delete`, `workspace.remove`,')
out.push('   `brain.delete`, `artifacts.delete`. Each one tests that a failed write rolls back and that unknown fields are rejected.')
out.push('4. **P5, observability.** `diagnostics.copy`, `diagnostics.export`, `diagnostics.rendererError`, `diagnostics.rendererStats`,')
out.push('   and the operation observer for the commands already ported.')
out.push('5. **Not in the current queue.** Every row marked `later`, `P8`, or `stop`. Those need their own decision, and')
out.push('   the `stop` rows are stop conditions of the plan (vault and secrets, provider network, updater, signing).')
out.push('')
out.push('## Not channels')
out.push('')
out.push('- `nocturne.onboarding.completed` appears in `src/App.tsx`, but it is a `localStorage` key, not an IPC call.')
out.push('')
out.push('## Channels')
out.push('')
out.push('Caller is the `window.nocturne` method in `electron/preload.ts`. Input validation is what the handler')
out.push('checks: a zod schema from `shared/ipc/schemas.ts`, an inline `.parse`, or none. Every handler runs through')
out.push('`safeIpcMain` (`electron/ipc/safeIpc.ts`), which checks the sender and the frame URL and applies a rate limit.')
out.push('')
out.push('| Channel | Caller | TypeScript contract | Input validation | Handler | Risk | Port | Note |')
out.push('| --- | --- | --- | --- | --- | --- | --- | --- |')
for (const row of rows) {
  out.push(`| \`${row.value}\` | \`${row.caller}\` | ${cell(`\`${row.contract}\``)} | ${cell(row.validation)} | \`${row.handler}\` | ${row.risk} | ${row.port} | ${cell(row.note)} |`)
}
out.push('')
process.stdout.write(out.join('\n'))
