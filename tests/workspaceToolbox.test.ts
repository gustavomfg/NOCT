import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createWorkspaceToolbox, type WorkspaceToolboxOptions } from '../electron/ai/agent/WorkspaceToolbox'
import type { ProcessRunner } from '../electron/validation/CancellableProcessRunner'
import { selectAiEngine } from '../shared/ai/engine'

let workspace: string

beforeEach(() => {
  workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'nocturne-toolbox-'))
  fs.mkdirSync(path.join(workspace, 'src'))
  fs.mkdirSync(path.join(workspace, 'node_modules'))
  fs.writeFileSync(path.join(workspace, 'src', 'app.ts'), 'export const value = 1\nexport const other = 2\n')
  fs.writeFileSync(path.join(workspace, '.env'), 'API_KEY=secret')
  fs.writeFileSync(path.join(workspace, 'node_modules', 'lib.js'), 'value')
})

afterEach(() => {
  fs.rmSync(workspace, { recursive: true, force: true })
})

function toolbox(overrides: Partial<WorkspaceToolboxOptions> = {}) {
  const emit = vi.fn()
  const requestApproval = vi.fn(async () => true)
  const runner: ProcessRunner = {
    run: vi.fn(async () => ({ exitCode: 0, stdout: 'ok', stderr: '', durationMs: 1, cancelled: false, timedOut: false, truncated: false, error: null, terminationUncertain: false, terminationScope: null })),
  }
  const tools = createWorkspaceToolbox({ workspace, mode: 'build', approvalPolicy: 'on-request', runner, emit, requestApproval, ...overrides })
  const call = (name: string, args: unknown) => tools.execute({ id: 'call', name, arguments: JSON.stringify(args) }, new AbortController().signal)
  return { tools, call, emit, requestApproval, runner }
}

describe('WorkspaceToolbox', () => {
  it('oferece somente ferramentas de leitura fora do Build Mode', async () => {
    for (const mode of ['review', 'docs'] as const) {
      const { tools, call } = toolbox({ mode })
      expect(tools.definitions.map((tool) => tool.name)).toEqual(['list_directory', 'read_file', 'search_text'])
      await expect(call('write_file', { path: 'x.ts', content: '' })).resolves.toContain('não está disponível')
    }
    expect(fs.existsSync(path.join(workspace, 'x.ts'))).toBe(false)
  })

  it('lista, lê e pesquisa sem expor diretórios gerados nem segredos', async () => {
    const { call } = toolbox()
    const listing = await call('list_directory', {})
    expect(listing).toContain('src/')
    expect(listing).not.toContain('node_modules')
    await expect(call('read_file', { path: 'src/app.ts', start_line: 2, line_count: 1 })).resolves.toContain('2\texport const other = 2')
    await expect(call('search_text', { query: 'VALUE' })).resolves.toContain('src/app.ts:1: export const value = 1')
    await expect(call('search_text', { query: 'API_KEY' })).resolves.toContain('Nenhuma ocorrência')
    await expect(call('read_file', { path: '.env' })).resolves.toMatch(/^Erro: Arquivo potencialmente sensível/)
  })

  it('bloqueia caminhos fora do workspace', async () => {
    const { call } = toolbox()
    await expect(call('read_file', { path: '../outside.txt' })).resolves.toMatch(/^Erro: Acesso bloqueado/)
    await expect(call('write_file', { path: '../outside.txt', content: 'x' })).resolves.toMatch(/^Erro: Acesso bloqueado/)
    expect(fs.existsSync(path.join(path.dirname(workspace), 'outside.txt'))).toBe(false)
  })

  it('escreve e edita arquivos visíveis ao controle de alterações', async () => {
    const { call, emit, requestApproval } = toolbox()
    await expect(call('write_file', { path: 'src/new/file.ts', content: 'a' })).resolves.toBe('Criado: src/new/file.ts')
    await expect(call('edit_file', { path: 'src/app.ts', old_text: 'value = 1', new_text: 'value = 3' })).resolves.toBe('Atualizado: src/app.ts')
    expect(fs.readFileSync(path.join(workspace, 'src', 'app.ts'), 'utf8')).toContain('value = 3')
    expect(requestApproval).not.toHaveBeenCalled()
    expect(emit).toHaveBeenCalledWith('item/completed', { item: expect.objectContaining({ type: 'fileChange', changes: [{ path: 'src/new/file.ts', kind: 'add' }] }) })
    expect(emit).toHaveBeenCalledWith('fs/changed', { changedPaths: ['src/app.ts'] })
    await expect(call('edit_file', { path: 'src/app.ts', old_text: 'export const', new_text: 'const' })).resolves.toContain('ocorre 2 vezes')
    await expect(call('write_file', { path: 'node_modules/lib.js', content: 'x' })).resolves.toContain('não pode ser alterado')
    await expect(call('write_file', { path: '.git/config', content: 'x' })).resolves.toContain('não pode ser alterado')
  })

  it('pede aprovação para escrita com política untrusted e respeita a recusa', async () => {
    const requestApproval = vi.fn(async () => false)
    const { call } = toolbox({ approvalPolicy: 'untrusted', requestApproval })
    await expect(call('write_file', { path: 'src/app.ts', content: 'x' })).resolves.toContain('recusou')
    expect(requestApproval).toHaveBeenCalledWith({ kind: 'file', detail: 'Alterar src/app.ts' })
    expect(fs.readFileSync(path.join(workspace, 'src', 'app.ts'), 'utf8')).toContain('value = 1')
  })

  it('executa comandos somente após aprovação e sem shell', async () => {
    const approved = toolbox()
    await expect(approved.call('run_command', { command: 'npm', args: ['test'] })).resolves.toContain('Código de saída: 0')
    expect(approved.requestApproval).toHaveBeenCalledWith(expect.objectContaining({ kind: 'command', command: 'npm test', risk: 'safe' }))
    expect(approved.runner.run).toHaveBeenCalledWith('npm', ['test'], expect.objectContaining({ cwd: fs.realpathSync(workspace) }))

    const declined = toolbox({ requestApproval: async () => false })
    await expect(declined.call('run_command', { command: 'npm', args: ['test'] })).resolves.toContain('recusou')
    expect(declined.runner.run).not.toHaveBeenCalled()

    const dangerous = toolbox()
    await expect(dangerous.call('run_command', { command: 'sudo', args: ['rm', '-rf', '/'] })).resolves.toContain('bloqueado')
    expect(dangerous.requestApproval).not.toHaveBeenCalled()
  })

  it('devolve erros de argumentos ao modelo em vez de lançar', async () => {
    const { tools } = toolbox()
    await expect(tools.execute({ id: 'c', name: 'read_file', arguments: '{' }, new AbortController().signal)).resolves.toContain('JSON válido')
    await expect(tools.execute({ id: 'c', name: 'read_file', arguments: '{"file":"a"}' }, new AbortController().signal)).resolves.toContain('Argumentos inválidos')
  })
})

describe('selectAiEngine', () => {
  it('prefere a escolha explícita e não exige o Codex', () => {
    expect(selectAiEngine({ preference: 'codex', providerReady: true })).toBe('codex')
    expect(selectAiEngine({ preference: 'provider', providerReady: true })).toBe('provider')
    expect(selectAiEngine({ providerReady: true, legacyCodexModel: 'gpt-5' })).toBe('provider')
    expect(selectAiEngine({ providerReady: false, legacyCodexModel: 'gpt-5' })).toBe('codex')
    expect(selectAiEngine({ preference: 'provider', providerReady: false, legacyCodexModel: 'gpt-5' })).toBeNull()
    expect(selectAiEngine({ providerReady: false })).toBeNull()
  })
})
