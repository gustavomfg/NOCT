import { randomUUID } from 'node:crypto'
import fs from 'node:fs'
import path from 'node:path'
import { z } from 'zod'
import type { AgentToolCall, AgentToolDefinition } from '../../../shared/ai/agentTools'
import { WORKSPACE_READ_LIMITS } from '../../../shared/constants'
import type { AgentMode } from '../../../shared/suggestions'
import { writeAtomicFile } from '../../persistence/AtomicFile'
import { isPotentiallySensitivePath } from '../../semantic-index/SemanticPrivacyPolicy'
import { assessCommand, readWorkspaceFile, resolveInsideWorkspace, type CommandRisk } from '../../security/ExecutionPolicy'
import type { ProcessRunner } from '../../validation/CancellableProcessRunner'
import {
  isIgnoredProjectDiscoveryRelativePath,
  isIgnoredWorkspaceRelativePath,
  PROJECT_DISCOVERY_IGNORED_DIRECTORIES,
  relativeWorkspacePath,
} from '../../workspaces/WorkspacePathPolicy'
import type { AgentToolRuntime } from '../AIOrchestrator'

export const WORKSPACE_TOOL_LIMITS = {
  listEntries: 500,
  readLines: 600,
  readCharacters: 30_000,
  writeBytes: WORKSPACE_READ_LIMITS.documentBytes,
  searchFiles: 4_000,
  searchFileBytes: 1_000_000,
  searchMatches: 100,
  searchLineCharacters: 300,
  commandCharacters: 500,
  commandArguments: 64,
  commandArgumentCharacters: 4_000,
  commandTimeoutMs: 120_000,
  commandOutputCharacters: 30_000,
} as const

export interface ToolApprovalRequest {
  kind: 'command' | 'file'
  detail: string
  command?: string
  risk?: CommandRisk
}

export interface WorkspaceToolboxOptions {
  workspace: string
  mode: AgentMode
  approvalPolicy: 'untrusted' | 'on-request'
  runner: ProcessRunner
  /** Publishes renderer-visible activity using the existing agent event vocabulary. */
  emit(method: string, params: Record<string, unknown>): void
  /** Resolves true only after an explicit developer decision. */
  requestApproval(request: ToolApprovalRequest): Promise<boolean>
  createId?: () => string
}

const pathSchema = z.string().trim().min(1).max(4_000)
const listSchema = z.object({ path: pathSchema.optional() }).strict()
const readSchema = z.object({
  path: pathSchema,
  start_line: z.number().int().min(1).optional(),
  line_count: z.number().int().min(1).max(WORKSPACE_TOOL_LIMITS.readLines).optional(),
}).strict()
const searchSchema = z.object({
  query: z.string().min(1).max(500),
  path: pathSchema.optional(),
}).strict()
const writeSchema = z.object({
  path: pathSchema,
  content: z.string().max(WORKSPACE_TOOL_LIMITS.writeBytes),
}).strict()
const editSchema = z.object({
  path: pathSchema,
  old_text: z.string().min(1).max(WORKSPACE_TOOL_LIMITS.writeBytes),
  new_text: z.string().max(WORKSPACE_TOOL_LIMITS.writeBytes),
  replace_all: z.boolean().optional(),
}).strict()
const commandSchema = z.object({
  command: z.string().trim().min(1).max(WORKSPACE_TOOL_LIMITS.commandCharacters),
  args: z.array(z.string().max(WORKSPACE_TOOL_LIMITS.commandArgumentCharacters)).max(WORKSPACE_TOOL_LIMITS.commandArguments).optional(),
}).strict()

const readTools: AgentToolDefinition[] = [
  {
    name: 'list_directory',
    description: 'Lista arquivos e pastas de um diretório do workspace. Diretórios gerados (node_modules, dist, .git…) são omitidos.',
    parameters: {
      type: 'object',
      properties: { path: { type: 'string', description: 'Caminho relativo à raiz do workspace. Padrão: raiz.' } },
      additionalProperties: false,
    },
  },
  {
    name: 'read_file',
    description: 'Lê um arquivo de texto do workspace com números de linha. Use start_line e line_count para arquivos grandes.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Caminho relativo à raiz do workspace.' },
        start_line: { type: 'integer', minimum: 1, description: 'Primeira linha (1 = início).' },
        line_count: { type: 'integer', minimum: 1, maximum: WORKSPACE_TOOL_LIMITS.readLines, description: 'Quantidade de linhas.' },
      },
      required: ['path'],
      additionalProperties: false,
    },
  },
  {
    name: 'search_text',
    description: 'Procura um texto literal (sem diferenciar maiúsculas) nos arquivos do workspace e retorna caminho:linha: trecho.',
    parameters: {
      type: 'object',
      properties: {
        query: { type: 'string', description: 'Texto literal a procurar.' },
        path: { type: 'string', description: 'Diretório relativo onde limitar a busca. Padrão: raiz.' },
      },
      required: ['query'],
      additionalProperties: false,
    },
  },
]

const buildTools: AgentToolDefinition[] = [
  {
    name: 'write_file',
    description: 'Cria ou substitui por completo um arquivo de texto do workspace. Prefira edit_file para alterações pontuais.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Caminho relativo à raiz do workspace.' },
        content: { type: 'string', description: 'Conteúdo completo do arquivo.' },
      },
      required: ['path', 'content'],
      additionalProperties: false,
    },
  },
  {
    name: 'edit_file',
    description: 'Substitui um trecho exato de um arquivo existente. old_text deve ocorrer uma única vez, a menos que replace_all seja true.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Caminho relativo à raiz do workspace.' },
        old_text: { type: 'string', description: 'Trecho atual, copiado exatamente.' },
        new_text: { type: 'string', description: 'Novo trecho.' },
        replace_all: { type: 'boolean', description: 'Substituir todas as ocorrências.' },
      },
      required: ['path', 'old_text', 'new_text'],
      additionalProperties: false,
    },
  },
  {
    name: 'run_command',
    description: 'Executa um programa na raiz do workspace, sem shell, após aprovação explícita do desenvolvedor. Use para testes, build e lint.',
    parameters: {
      type: 'object',
      properties: {
        command: { type: 'string', description: 'Programa a executar, por exemplo "npm".' },
        args: { type: 'array', items: { type: 'string' }, description: 'Argumentos, por exemplo ["test"].' },
      },
      required: ['command'],
      additionalProperties: false,
    },
  },
]

class ToolInputError extends Error {}

/**
 * Workspace tools offered to Provider models. Every path is constrained to the
 * authorized root and every command waits for an explicit developer decision.
 * Build Mode is the only mode that receives mutating tools.
 */
export function createWorkspaceToolbox(input: WorkspaceToolboxOptions): AgentToolRuntime {
  // Tool paths are canonicalized by resolveInsideWorkspace; compare them with a canonical root.
  const options = { ...input, workspace: fs.realpathSync.native(path.resolve(input.workspace)) }
  const createId = options.createId ?? randomUUID
  const writable = options.mode === 'build'
  const definitions = writable ? [...readTools, ...buildTools] : [...readTools]
  const available = new Set(definitions.map((tool) => tool.name))

  const handlers: Record<string, (input: unknown, signal: AbortSignal, itemId: string) => Promise<string>> = {
    list_directory: (input) => listDirectory(options.workspace, listSchema.parse(input)),
    read_file: (input) => readFile(options.workspace, readSchema.parse(input)),
    search_text: (input, signal) => searchText(options.workspace, searchSchema.parse(input), signal),
    write_file: (input, _signal, itemId) => {
      const value = writeSchema.parse(input)
      return applyFileChange(options, itemId, value.path, async () => value.content)
    },
    edit_file: (input, _signal, itemId) => {
      const value = editSchema.parse(input)
      return applyFileChange(options, itemId, value.path, async (current) => {
        if (current === null) throw new ToolInputError('O arquivo não existe. Use write_file para criá-lo.')
        const occurrences = current.split(value.old_text).length - 1
        if (occurrences === 0) throw new ToolInputError('old_text não foi encontrado. Leia o arquivo novamente e copie o trecho exato.')
        if (occurrences > 1 && !value.replace_all) throw new ToolInputError(`old_text ocorre ${occurrences} vezes. Inclua mais contexto ou use replace_all.`)
        return value.replace_all ? current.split(value.old_text).join(value.new_text) : current.replace(value.old_text, () => value.new_text)
      })
    },
    run_command: (input, signal, itemId) => runCommand(options, itemId, commandSchema.parse(input), signal),
  }

  return {
    definitions,
    async execute(call: AgentToolCall, signal: AbortSignal) {
      if (!available.has(call.name)) {
        return `A ferramenta ${call.name} não está disponível no modo ${options.mode}.`
      }
      let input: unknown
      try {
        input = call.arguments.trim() ? JSON.parse(call.arguments) : {}
      } catch {
        return 'Os argumentos da ferramenta não são um JSON válido.'
      }
      const itemId = `tool-${createId()}`
      const observable = call.name === 'list_directory' || call.name === 'read_file' || call.name === 'search_text'
      if (observable) options.emit('item/started', { item: { id: itemId, type: 'dynamicToolCall', tool: call.name, arguments: input } })
      try {
        const result = await handlers[call.name](input, signal, itemId)
        if (observable) options.emit('item/completed', { item: { id: itemId, type: 'dynamicToolCall', tool: call.name } })
        return result
      } catch (error) {
        const message = toolErrorMessage(error)
        if (observable) options.emit('item/completed', { item: { id: itemId, type: 'dynamicToolCall', tool: call.name, error: message } })
        return `Erro: ${message}`
      }
    },
  }
}

async function listDirectory(workspace: string, input: z.infer<typeof listSchema>) {
  const target = resolveInsideWorkspace(input.path ?? '.', workspace)
  const relative = displayPath(workspace, target)
  const entries = await fs.promises.readdir(target, { withFileTypes: true })
  const visible = entries
    .filter((entry) => !PROJECT_DISCOVERY_IGNORED_DIRECTORIES.has(entry.name) && !entry.name.startsWith('.nocturne-rollback-'))
    .sort((left, right) => Number(right.isDirectory()) - Number(left.isDirectory()) || left.name.localeCompare(right.name))
  const lines = visible.slice(0, WORKSPACE_TOOL_LIMITS.listEntries).map((entry) => entry.isDirectory() ? `${entry.name}/` : entry.name)
  if (visible.length > lines.length) lines.push(`… ${visible.length - lines.length} entrada(s) omitida(s)`)
  return `${relative}/\n${lines.join('\n') || '(vazio)'}`
}

async function readFile(workspace: string, input: z.infer<typeof readSchema>) {
  const target = resolveInsideWorkspace(input.path, workspace)
  const relative = displayPath(workspace, target)
  assertShareable(relative)
  const { content } = await readWorkspaceFile(target, workspace, WORKSPACE_READ_LIMITS.documentBytes)
  if (content.subarray(0, 8_000).includes(0)) throw new ToolInputError('O arquivo parece ser binário.')
  const lines = content.toString('utf8').split(/\r?\n/)
  const start = Math.min(input.start_line ?? 1, Math.max(lines.length, 1))
  const count = input.line_count ?? WORKSPACE_TOOL_LIMITS.readLines
  const selected: string[] = []
  let characters = 0
  let index = start - 1
  for (; index < Math.min(lines.length, start - 1 + count); index += 1) {
    const line = `${index + 1}\t${lines[index]}`
    if (characters + line.length > WORKSPACE_TOOL_LIMITS.readCharacters) break
    selected.push(line)
    characters += line.length + 1
  }
  const remaining = lines.length - index
  return [
    `${relative} (${lines.length} linha(s))`,
    ...selected,
    ...(remaining > 0 ? [`… ${remaining} linha(s) restante(s); continue com start_line=${index + 1}.`] : []),
  ].join('\n')
}

async function searchText(workspace: string, input: z.infer<typeof searchSchema>, signal: AbortSignal) {
  const root = resolveInsideWorkspace(input.path ?? '.', workspace)
  const needle = input.query.toLowerCase()
  const matches: string[] = []
  const pending = [root]
  let scanned = 0
  while (pending.length && scanned < WORKSPACE_TOOL_LIMITS.searchFiles && matches.length < WORKSPACE_TOOL_LIMITS.searchMatches) {
    if (signal.aborted) break
    const directory = pending.pop()!
    let entries: fs.Dirent[]
    try {
      entries = await fs.promises.readdir(directory, { withFileTypes: true })
    } catch {
      continue
    }
    for (const entry of entries) {
      if (scanned >= WORKSPACE_TOOL_LIMITS.searchFiles || matches.length >= WORKSPACE_TOOL_LIMITS.searchMatches) break
      const absolute = path.join(directory, entry.name)
      const relative = relativeWorkspacePath(workspace, absolute)
      if (!relative || isIgnoredProjectDiscoveryRelativePath(relative)) continue
      if (entry.isDirectory()) {
        pending.push(absolute)
        continue
      }
      if (!entry.isFile() || isSensitive(relative)) continue
      scanned += 1
      let content: Buffer
      try {
        content = (await readWorkspaceFile(absolute, workspace, WORKSPACE_TOOL_LIMITS.searchFileBytes)).content
      } catch {
        continue
      }
      if (content.subarray(0, 8_000).includes(0)) continue
      const lines = content.toString('utf8').split(/\r?\n/)
      for (let line = 0; line < lines.length && matches.length < WORKSPACE_TOOL_LIMITS.searchMatches; line += 1) {
        if (lines[line].toLowerCase().includes(needle)) {
          matches.push(`${relative}:${line + 1}: ${lines[line].trim().slice(0, WORKSPACE_TOOL_LIMITS.searchLineCharacters)}`)
        }
      }
    }
  }
  if (!matches.length) return `Nenhuma ocorrência de "${input.query}" em ${scanned} arquivo(s).`
  const limited = matches.length >= WORKSPACE_TOOL_LIMITS.searchMatches ? '\n… limite de resultados atingido; refine a busca.' : ''
  return `${matches.join('\n')}${limited}`
}

async function applyFileChange(
  options: WorkspaceToolboxOptions,
  itemId: string,
  candidate: string,
  produce: (current: string | null) => Promise<string>,
) {
  const target = resolveInsideWorkspace(candidate, options.workspace)
  const relative = relativeWorkspacePath(options.workspace, target)
  if (!relative) throw new ToolInputError('A raiz do workspace não pode ser substituída.')
  // Ignored paths are outside BEFORE/AFTER capture; writing there would hide changes from review.
  if (isIgnoredWorkspaceRelativePath(relative) || relative.split('/').includes('.nocturne')) {
    throw new ToolInputError('Este caminho é gerado ou interno e não pode ser alterado pelo agente.')
  }
  assertShareable(relative)
  const existing = await fs.promises.stat(target).catch((error: NodeJS.ErrnoException) => {
    if (error.code === 'ENOENT') return null
    throw error
  })
  if (existing && !existing.isFile()) throw new ToolInputError('O caminho não é um arquivo regular.')
  const current = existing
    ? (await readWorkspaceFile(target, options.workspace, WORKSPACE_TOOL_LIMITS.writeBytes)).content.toString('utf8')
    : null
  const next = await produce(current)
  if (Buffer.byteLength(next, 'utf8') > WORKSPACE_TOOL_LIMITS.writeBytes) throw new ToolInputError('O conteúdo excede o limite de escrita permitido.')
  if (current === next) return `${relative} já possui este conteúdo; nada foi alterado.`

  const kind = current === null ? 'add' : 'update'
  if (options.approvalPolicy === 'untrusted') {
    const approved = await options.requestApproval({ kind: 'file', detail: `${kind === 'add' ? 'Criar' : 'Alterar'} ${relative}` })
    if (!approved) return `O desenvolvedor recusou a alteração em ${relative}. Não tente novamente sem um novo pedido.`
  }
  options.emit('item/started', { item: { id: itemId, type: 'fileChange' } })
  try {
    await fs.promises.mkdir(path.dirname(target), { recursive: true })
    // Re-validate after mkdir: a directory created concurrently must not redirect the write.
    const verified = resolveInsideWorkspace(target, options.workspace)
    await writeAtomicFile(verified, next, existing ? existing.mode & 0o777 : 0o644)
  } catch (error) {
    options.emit('item/completed', { item: { id: itemId, type: 'fileChange', status: 'failed', changes: [{ path: relative, kind }] } })
    throw error
  }
  options.emit('item/completed', { item: { id: itemId, type: 'fileChange', status: 'completed', changes: [{ path: relative, kind }] } })
  options.emit('fs/changed', { changedPaths: [relative] })
  return `${kind === 'add' ? 'Criado' : 'Atualizado'}: ${relative}`
}

async function runCommand(
  options: WorkspaceToolboxOptions,
  itemId: string,
  input: z.infer<typeof commandSchema>,
  signal: AbortSignal,
) {
  const args = input.args ?? []
  if ([input.command, ...args].some((value) => value.includes('\0'))) throw new ToolInputError('O comando contém caracteres inválidos.')
  const commandLine = [input.command, ...args.map(quoteArgument)].join(' ')
  const assessment = assessCommand([input.command, ...args])
  if (assessment.blockedAutomatic) {
    return `Comando bloqueado pela política do Nocturne: ${assessment.reasons.join('; ')}. Peça ao desenvolvedor para executá-lo manualmente se for necessário.`
  }
  const approved = await options.requestApproval({ kind: 'command', detail: commandLine, command: commandLine, risk: assessment.risk })
  if (!approved) return `O desenvolvedor recusou o comando: ${commandLine}. Não tente novamente sem um novo pedido.`
  if (signal.aborted) return 'Execução cancelada.'

  options.emit('item/started', { item: { id: itemId, type: 'commandExecution', command: commandLine } })
  const result = await options.runner.run(input.command, args, {
    cwd: options.workspace,
    signal,
    timeoutMs: WORKSPACE_TOOL_LIMITS.commandTimeoutMs,
    maxOutputCharacters: WORKSPACE_TOOL_LIMITS.commandOutputCharacters,
  })
  const output = [result.stdout, result.stderr].filter(Boolean).join('\n').trim()
  const failed = result.error !== null || result.timedOut || result.cancelled || result.exitCode !== 0
  options.emit('item/completed', {
    item: {
      id: itemId,
      type: 'commandExecution',
      command: commandLine,
      aggregatedOutput: output.slice(-WORKSPACE_TOOL_LIMITS.commandOutputCharacters),
      exitCode: result.exitCode,
      status: failed ? 'failed' : 'completed',
    },
  })
  return [
    `$ ${commandLine}`,
    result.timedOut ? `Tempo limite de ${WORKSPACE_TOOL_LIMITS.commandTimeoutMs / 1000}s excedido.` : `Código de saída: ${result.exitCode ?? 'desconhecido'}`,
    ...(result.error ? [`Erro: ${result.error}`] : []),
    ...(result.truncated ? ['(saída truncada)'] : []),
    output || '(sem saída)',
  ].join('\n')
}

function assertShareable(relative: string) {
  if (isSensitive(relative)) throw new ToolInputError('Arquivo potencialmente sensível (credenciais ou segredos): acesso negado ao agente.')
}

function isSensitive(relative: string) {
  const baseName = path.posix.basename(relative).toLowerCase()
  return isPotentiallySensitivePath(relative) || (baseName.startsWith('.env') && baseName !== '.env.example')
}

function displayPath(workspace: string, target: string) {
  return relativeWorkspacePath(workspace, target) ?? '.'
}

function quoteArgument(value: string) {
  return /^[\w@%+=:,./-]+$/.test(value) ? value : `'${value.replace(/'/g, `'\\''`)}'`
}

function toolErrorMessage(error: unknown) {
  if (error instanceof z.ZodError) return `Argumentos inválidos: ${error.issues.map((issue) => `${issue.path.join('.') || 'entrada'} ${issue.message}`).join('; ')}`
  if ((error as NodeJS.ErrnoException)?.code === 'ENOENT') return 'O caminho não existe.'
  if ((error as NodeJS.ErrnoException)?.code === 'EFBIG') return 'O arquivo excede o limite de leitura permitido.'
  if ((error as NodeJS.ErrnoException)?.code === 'ENOTDIR') return 'O caminho não é um diretório.'
  return error instanceof Error ? error.message : String(error)
}
