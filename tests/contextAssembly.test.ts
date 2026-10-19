import { describe, expect, it } from 'vitest'
import { ContextAssemblyService } from '../electron/ai/ContextAssemblyService'
import { TaskBuilder } from '../electron/ai/TaskBuilder'

function source(id: string, type: 'explicit-user' | 'semantic-index' | 'memory', content: string, overrides: Record<string, unknown> = {}) {
  return {
    id,
    type,
    title: id,
    content,
    scope: 'workspace',
    potentiallyOutdated: type === 'memory',
    ...overrides,
  } as const
}

describe('ContextAssemblyService', () => {
  it('entrega fontes compatíveis com o contrato estrito da tarefa sem metadados de ranking', () => {
    const memory = source('workspace-memory', 'memory', '# Regras do projeto\nRevise antes de alterar.')
    const context = new ContextAssemblyService().assemble({ sources: [memory] }).sources

    const task = new TaskBuilder().build({
      workspace: { id: '/workspace', name: 'Project' },
      intent: 'Explique o projeto.',
      mode: 'review',
      messages: [],
      context,
      constraints: [],
      requirements: ['chat', 'streaming'],
      selection: { type: 'workspace-default' },
      output: { format: 'markdown' },
      permissions: { workspaceAccess: 'read-only' },
      tools: [],
    })
    expect(task.context).toEqual([memory])
  })

  it('prioriza contexto explicitamente selecionado', () => {
    const result = new ContextAssemblyService().assemble({
      sources: [source('semantic', 'semantic-index', 'semantic'), source('explicit', 'explicit-user', 'explicit')],
      explicitSourceIds: ['semantic'],
      tokenBudget: 10,
    })

    expect(result.sources[0].id).toBe('semantic')
  })

  it('rejeita stale, deduplica conteúdo e preserva as razões', () => {
    const result = new ContextAssemblyService().assemble({
      sources: [
        source('stale', 'semantic-index', 'old', { stale: true }),
        source('one', 'semantic-index', 'same'),
        source('two', 'semantic-index', 'same'),
      ],
    })

    expect(result.sources.map(({ id }) => id)).toEqual(['one'])
    expect(result.dropped).toEqual(expect.arrayContaining([
      { id: 'stale', reason: 'stale' },
      { id: 'two', reason: 'duplicate' },
    ]))
  })

  it('aplica orçamento sem ultrapassar o limite estimado', () => {
    const result = new ContextAssemblyService().assemble({
      sources: [source('large', 'explicit-user', 'a'.repeat(100))],
      tokenBudget: 5,
    })

    expect(result.estimatedTokens).toBeLessThanOrEqual(5)
    expect(result.sources[0].content.length).toBeLessThanOrEqual(20)
  })
})
