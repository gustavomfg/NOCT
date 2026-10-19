import { describe, expect, it, vi } from 'vitest'
import { AiExecutionCoordinator } from '../electron/ai/AiExecutionCoordinator'
import { AIOrchestrator, type AgentToolRuntime } from '../electron/ai/AIOrchestrator'
import { ModelRegistry } from '../electron/ai/ModelRegistry'
import { ModelResolver } from '../electron/ai/ModelResolver'
import { ProviderRegistry, type ProviderAdapter } from '../electron/ai/ProviderRegistry'
import { TaskBuilder } from '../electron/ai/TaskBuilder'
import { ProviderExecutionError } from '../electron/ai/ProviderExecutionError'
import type { WorkspaceModelBindings } from '../shared/ai/bindings'
import type { ModelDescriptor } from '../shared/ai/model'
import type { ProviderExecutionControl, ProviderExecutionRequest, ProviderExecutionResult } from '../shared/ai/providerExecution'
import type { NormalizedTaskInput } from '../shared/ai/task'
import { IPC_CHANNELS } from '../shared/ipc/channels'
import { providerDefinition } from './helpers/providerDefinition'

const descriptor: ModelDescriptor = { providerId: 'fake', modelId: 'model', displayName: 'Fake', source: 'local', capabilities: ['chat', 'streaming'], availability: 'available' }
const bindings: WorkspaceModelBindings = { workspaceId: '/workspace', defaultBinding: { providerId: 'fake', modelId: 'model' } }
const taskInput: NormalizedTaskInput = {
  workspace: { id: '/workspace', name: 'Workspace' }, intent: 'Corrija o bug.', mode: 'build', messages: [], context: [], constraints: [],
  requirements: ['chat', 'streaming'], selection: { type: 'workspace-default' }, output: { format: 'markdown' }, permissions: { workspaceAccess: 'workspace-write' }, tools: [],
}

type Step = (request: ProviderExecutionRequest, control: ProviderExecutionControl) => ProviderExecutionResult | Promise<ProviderExecutionResult>

class ScriptedAdapter implements ProviderAdapter {
  readonly definition = { ...providerDefinition('fake', 'local'), displayName: 'Scripted' }
  readonly requests: ProviderExecutionRequest[] = []
  constructor(private readonly steps: Step[]) {}
  getAvailability() { return { status: 'available' as const } }
  listModels() { return [descriptor] }
  async execute(request: ProviderExecutionRequest, control: ProviderExecutionControl) {
    this.requests.push(structuredClone(request))
    const step = this.steps[this.requests.length - 1]
    if (!step) throw new Error('Etapa inesperada.')
    return step(request, control)
  }
}

function setup(adapter: ProviderAdapter) {
  const models = new ModelRegistry(); models.register(descriptor)
  const providers = new ProviderRegistry(); providers.register(adapter)
  return { models, providers, orchestrator: new AIOrchestrator(new ModelResolver(models, providers), providers) }
}

const tools = (execute: AgentToolRuntime['execute']): AgentToolRuntime => ({
  definitions: [{ name: 'read_file', description: 'Lê.', parameters: { type: 'object' } }],
  execute,
})

describe('agent tool loop', () => {
  it('executa ferramentas e reenvia o transcript até a resposta final', async () => {
    const adapter = new ScriptedAdapter([
      (_request, control) => {
        control.emit({ type: 'message.delta', messageId: 'm', delta: 'Lendo.' })
        return { finishReason: 'tool-calls', toolCalls: [{ id: 'call-1', name: 'read_file', arguments: '{"path":"a.ts"}' }] }
      },
      (_request, control) => {
        control.emit({ type: 'message.delta', messageId: 'm', delta: 'Pronto.' })
        return { finishReason: 'stop' }
      },
    ])
    const execute = vi.fn(async () => 'conteúdo de a.ts')
    const deltas: string[] = []
    const handle = await setup(adapter).orchestrator.start(new TaskBuilder().build(taskInput), bindings, (event) => {
      if (event.type === 'message.delta') deltas.push(event.delta)
    }, tools(execute))

    await expect(handle.completion).resolves.toMatchObject({ status: 'completed', finishReason: 'stop' })
    expect(execute).toHaveBeenCalledWith({ id: 'call-1', name: 'read_file', arguments: '{"path":"a.ts"}' }, expect.any(AbortSignal))
    expect(adapter.requests[0].tools?.map((tool) => tool.name)).toEqual(['read_file'])
    expect(adapter.requests[1].transcript).toEqual([
      { role: 'assistant', content: 'Lendo.', toolCalls: [{ id: 'call-1', name: 'read_file', arguments: '{"path":"a.ts"}' }] },
      { role: 'tool', toolCallId: 'call-1', content: 'conteúdo de a.ts' },
    ])
    expect(deltas.join('')).toBe('Lendo.\n\nPronto.')
  })

  it('converte falhas de ferramenta em resultado para o modelo', async () => {
    const adapter = new ScriptedAdapter([
      () => ({ finishReason: 'tool-calls', toolCalls: [{ id: 'call-1', name: 'read_file', arguments: '{}' }] }),
      () => ({ finishReason: 'stop' }),
    ])
    const handle = await setup(adapter).orchestrator.start(new TaskBuilder().build(taskInput), bindings, undefined, tools(async () => { throw new Error('disco indisponível') }))
    await expect(handle.completion).resolves.toMatchObject({ status: 'completed' })
    expect(adapter.requests[1].transcript?.[1]).toMatchObject({ role: 'tool', content: expect.stringContaining('disco indisponível') })
  })

  it('repete sem ferramentas em modos de leitura quando o modelo as recusa', async () => {
    const rejected = () => { throw new ProviderExecutionError({ code: 'invalid-response', message: 'sem ferramentas', retryable: false }, true) }
    const adapter = new ScriptedAdapter([rejected, (_request, control) => {
      control.emit({ type: 'message.delta', messageId: 'm', delta: 'Análise.' })
      return { finishReason: 'stop' }
    }])
    const deltas: string[] = []
    const handle = await setup(adapter).orchestrator.start(new TaskBuilder().build({ ...taskInput, mode: 'review', permissions: { workspaceAccess: 'read-only' } }), bindings, (event) => {
      if (event.type === 'message.delta') deltas.push(event.delta)
    }, tools(vi.fn()))
    await expect(handle.completion).resolves.toMatchObject({ status: 'completed' })
    expect(adapter.requests[0].tools).toHaveLength(1)
    expect(adapter.requests[1].tools).toBeUndefined()
    expect(deltas.join('')).toContain('Análise.')

    const build = new ScriptedAdapter([rejected])
    const failed = await setup(build).orchestrator.start(new TaskBuilder().build(taskInput), bindings, undefined, tools(vi.fn()))
    await expect(failed.completion).resolves.toMatchObject({ status: 'failed', error: { message: 'sem ferramentas' } })
    expect(build.requests).toHaveLength(1)
  })

  it('não executa tool calls quando nenhuma ferramenta foi oferecida', async () => {
    const adapter = new ScriptedAdapter([() => ({ finishReason: 'stop' })])
    const handle = await setup(adapter).orchestrator.start(new TaskBuilder().build(taskInput), bindings)
    await expect(handle.completion).resolves.toMatchObject({ status: 'completed' })
    expect(adapter.requests[0].tools).toBeUndefined()
    expect(adapter.requests[0].transcript).toBeUndefined()
  })
})

describe('AiExecutionCoordinator tool approvals', () => {
  function coordinatorFor(adapter: ProviderAdapter) {
    const { models, providers } = setup(adapter)
    const sent: Array<{ channel: string; payload: Record<string, unknown> }> = []
    const win = { isDestroyed: () => false, webContents: { send: (channel: string, payload: Record<string, unknown>) => sent.push({ channel, payload }) } }
    const approvalDetails = new Map<string, { command?: string; risk?: string }>()
    const finalize = vi.fn(async () => ({ message: null }))
    const coordinator = new AiExecutionCoordinator(win as never, models, providers, { debug: vi.fn(), info: vi.fn(), warn: vi.fn(), error: vi.fn() } as never, approvalDetails, finalize)
    return { coordinator, sent, approvalDetails, finalize }
  }

  it('aguarda a decisão do desenvolvedor antes de continuar a ferramenta', async () => {
    const adapter = new ScriptedAdapter([
      () => ({ finishReason: 'tool-calls', toolCalls: [{ id: 'call-1', name: 'run_command', arguments: '{}' }] }),
      () => ({ finishReason: 'stop' }),
    ])
    const { coordinator, sent, approvalDetails, finalize } = coordinatorFor(adapter)
    let decision: Promise<boolean> | undefined
    await coordinator.startProvider('conversation-1', taskInput, bindings, {
      mode: 'build',
      createTools: (bridge) => ({
        definitions: [{ name: 'run_command', description: 'Executa.', parameters: { type: 'object' } }],
        execute: async () => {
          decision = bridge.requestApproval({ kind: 'command', detail: 'npm test', command: 'npm test', risk: 'safe' })
          return await decision ? 'ok' : 'recusado'
        },
      }),
    })
    await vi.waitFor(() => expect(sent.some(({ payload }) => payload.method === 'item/commandExecution/requestApproval')).toBe(true))
    const request = sent.find(({ payload }) => payload.method === 'item/commandExecution/requestApproval')!.payload
    const approvalKey = (request.params as Record<string, unknown>).approvalKey as string
    expect(sent.some(({ channel, payload }) => channel === IPC_CHANNELS.ai.status && payload.status === 'waiting-approval')).toBe(true)
    expect(approvalDetails.get(approvalKey)).toEqual({ command: 'npm test', risk: 'safe' })

    await coordinator.resolveApproval(approvalKey, true)
    await expect(decision).resolves.toBe(true)
    await vi.waitFor(() => expect(finalize).toHaveBeenCalled())
    expect(adapter.requests[1].transcript?.[1]).toMatchObject({ content: 'ok' })
    expect(sent.some(({ channel, payload }) => channel === IPC_CHANNELS.ai.status && payload.status === 'running')).toBe(true)
    await coordinator.dispose()
  })

  it('recusa aprovações pendentes ao cancelar', async () => {
    const adapter = new ScriptedAdapter([
      () => ({ finishReason: 'tool-calls', toolCalls: [{ id: 'call-1', name: 'run_command', arguments: '{}' }] }),
    ])
    const { coordinator, sent, finalize } = coordinatorFor(adapter)
    let decision: Promise<boolean> | undefined
    await coordinator.startProvider('conversation-1', taskInput, bindings, {
      mode: 'build',
      createTools: (bridge) => ({
        definitions: [{ name: 'run_command', description: 'Executa.', parameters: { type: 'object' } }],
        execute: async () => {
          decision = bridge.requestApproval({ kind: 'command', detail: 'npm test' })
          return await decision ? 'ok' : 'recusado'
        },
      }),
    })
    await vi.waitFor(() => expect(decision).toBeDefined())
    await coordinator.cancel('conversation-1')
    await expect(decision).resolves.toBe(false)
    await vi.waitFor(() => expect(finalize).toHaveBeenCalledWith(expect.objectContaining({ outcome: 'cancelled' })))
    expect(adapter.requests).toHaveLength(1)
    expect(sent.some(({ payload }) => payload.method === 'turn/completed')).toBe(true)
    await coordinator.dispose()
  })

  it('rejeita chaves de aprovação desconhecidas', async () => {
    const { coordinator } = coordinatorFor(new ScriptedAdapter([]))
    await expect(coordinator.resolveApproval('unknown', true)).rejects.toThrow(/aguardando aprovação/)
    await coordinator.dispose()
  })
})
