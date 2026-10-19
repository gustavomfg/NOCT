import { describe, expect, it } from 'vitest'
import { startAiTurn } from '../electron/ai/executeAiTurn'
import { ContextAssemblyService } from '../electron/ai/ContextAssemblyService'
import { ModelRegistry } from '../electron/ai/ModelRegistry'
import { ProviderRegistry } from '../electron/ai/ProviderRegistry'
import type { WorkspaceModelBindings } from '../shared/ai/bindings'
import type { NormalizedTaskInput } from '../shared/ai/task'
import type { ModelDescriptor } from '../shared/ai/model'
import { FakeProviderAdapter } from './helpers/FakeProviderAdapter'

const descriptor: ModelDescriptor = {
  providerId: 'fake', modelId: 'model', displayName: 'Fake Model', source: 'local',
  capabilities: ['chat', 'streaming'], availability: 'available',
}
const bindings: WorkspaceModelBindings = { workspaceId: 'workspace-1', defaultBinding: { providerId: 'fake', modelId: 'model' } }
const task: NormalizedTaskInput = {
  workspace: { id: 'workspace-1', name: 'Workspace' }, intent: 'Analise.', mode: 'review',
  messages: [], context: [], constraints: [], requirements: ['chat', 'streaming'],
  selection: { type: 'workspace-default' }, output: { format: 'markdown' },
  permissions: { workspaceAccess: 'read-only' }, tools: [],
}

function setup(adapter: FakeProviderAdapter) {
  const models = new ModelRegistry(); models.register(descriptor)
  const providers = new ProviderRegistry(); providers.register(adapter)
  return { models, providers }
}

describe('startAiTurn', () => {
  it.each(['build', 'review', 'docs'] as const)('envia contexto montado ao Provider e conclui uma tarefa em %s', async (mode) => {
    const adapter = new FakeProviderAdapter([descriptor], {
      events: [{ type: 'message.delta', messageId: 'assistant', delta: 'Resposta com contexto.' }],
    })
    const dependencies = setup(adapter)
    const memory = {
      id: 'workspace-memory', type: 'memory' as const, title: 'Memória do workspace',
      content: '# Regras do projeto\nLeia antes de alterar.', scope: 'workspace', potentiallyOutdated: true,
    }
    const projectIndex = {
      id: 'project-index:run-1', type: 'project-index' as const, title: 'Índice estrutural do projeto',
      content: 'src/index.ts exporta o ponto de entrada.', scope: 'workspace', potentiallyOutdated: false,
      provenance: { sourcePath: 'src/index.ts', sourceHash: 'a'.repeat(64), validity: 'current' as const },
    }
    const context = new ContextAssemblyService().assemble({ sources: [memory, projectIndex] }).sources
    const events: Array<{ method: string; params: Record<string, unknown> }> = []

    const turn = await startAiTurn(dependencies.models, dependencies.providers, {
      ...task, mode, context,
      permissions: { workspaceAccess: mode === 'build' ? 'workspace-write' : 'read-only' },
    }, bindings, (method, params) => events.push({ method, params }))

    await expect(turn.completion).resolves.toMatchObject({ status: 'completed' })
    expect(adapter.requests).toHaveLength(1)
    expect(adapter.requests[0].task.context).toEqual([projectIndex, memory])
    expect(events).toContainEqual({ method: 'item/agentMessage/delta', params: { delta: 'Resposta com contexto.' } })
    expect(events[events.length - 1]).toMatchObject({ method: 'turn/completed', params: { turn: { status: 'completed' } } })
  })

  it('preserva o resultado terminal de falha', async () => {
    const dependencies = setup(new FakeProviderAdapter([descriptor], { error: { code: 'rate-limited', message: 'Limite temporário.', retryable: true } }))
    const events: Array<{ method: string; params: Record<string, unknown> }> = []
    const turn = await startAiTurn(dependencies.models, dependencies.providers, task, bindings, (method, params) => events.push({ method, params }))
    await expect(turn.completion).resolves.toMatchObject({ status: 'failed' })
    expect(events[events.length - 1]).toMatchObject({ method: 'turn/completed', params: { turn: { status: 'failed', error: { message: 'Limite temporário.' } } } })
  })

  it('preserva o resultado terminal de cancelamento', async () => {
    const dependencies = setup(new FakeProviderAdapter([descriptor], { waitForCancellation: true }))
    const events: Array<{ method: string; params: Record<string, unknown> }> = []
    const turn = await startAiTurn(dependencies.models, dependencies.providers, task, bindings, (method, params) => events.push({ method, params }))
    expect(turn.cancel()).toBe(true)
    await expect(turn.completion).resolves.toMatchObject({ status: 'cancelled' })
    expect(events[events.length - 1]).toMatchObject({ method: 'turn/completed', params: { turn: { status: 'cancelled' } } })
  })
})
