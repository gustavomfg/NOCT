import { describe, expect, it, vi } from 'vitest'
import {
  ModelCatalogService,
  type ModelCatalogStore,
} from '../electron/ai/ModelCatalogService'
import { ModelRegistry } from '../electron/ai/ModelRegistry'
import { ProviderRegistry, ProviderRegistryError, type ProviderAdapter } from '../electron/ai/ProviderRegistry'
import type { ModelDescriptor } from '../shared/ai/model'
import { providerDefinition } from './helpers/providerDefinition'

function model(providerId: string, modelId: string): ModelDescriptor {
  return {
    providerId,
    modelId,
    displayName: modelId,
    source: 'remote',
    capabilities: ['chat'],
    availability: 'available',
  }
}

function adapter(
  id: string,
  listModels: ProviderAdapter['listModels'],
): ProviderAdapter {
  return {
    definition: providerDefinition(id),
    getAvailability: () => ({ status: 'available' }),
    listModels,
    execute: () => ({ finishReason: 'stop' }),
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}

class MemoryModelCatalog implements ModelCatalogStore {
  records: ModelDescriptor[]
  failReplace = false

  constructor(records: ModelDescriptor[] = []) {
    this.records = [...records]
  }

  list(providerId?: string) {
    return this.records.filter(
      (descriptor) => providerId === undefined || descriptor.providerId === providerId,
    )
  }

  replaceProviderModels(providerId: string, inputs: readonly unknown[]) {
    if (this.failReplace) throw new Error('persistence failed')
    const descriptors = inputs as ModelDescriptor[]
    this.records = [
      ...this.records.filter((descriptor) => descriptor.providerId !== providerId),
      ...descriptors,
    ]
    return descriptors
  }
}

describe('ModelCatalogService', () => {
  it('descobre e substitui somente os modelos do provider solicitado', async () => {
    const providers = new ProviderRegistry()
    const models = new ModelRegistry()
    models.register(model('provider-a', 'old'))
    models.register(model('ollama', 'local'))
    providers.register(adapter('provider-a', async () => [model('provider-a', 'new')]))
    const catalog = new MemoryModelCatalog([
      model('provider-a', 'old'),
      model('ollama', 'local'),
    ])

    const result = await new ModelCatalogService(
      providers,
      models,
      catalog,
    ).refresh('provider-a')

    expect(result).toMatchObject({
      status: 'applied',
      models: [{ providerId: 'provider-a', modelId: 'new' }],
    })
    expect(models.list({ providerId: 'provider-a' }).map(({ modelId }) => modelId)).toEqual(['new'])
    expect(models.list({ providerId: 'ollama' }).map(({ modelId }) => modelId)).toEqual(['local'])
    expect(catalog.list('provider-a').map(({ modelId }) => modelId)).toEqual(['new'])
    expect(catalog.list('ollama').map(({ modelId }) => modelId)).toEqual(['local'])
  })

  it('preserva o catálogo anterior quando qualquer descriptor é inválido', async () => {
    const providers = new ProviderRegistry()
    const models = new ModelRegistry()
    models.register(model('provider-a', 'stable'))
    providers.register(adapter('provider-a', async () => [
      model('provider-a', 'valid'),
      { ...model('provider-a', 'native'), nativePayload: { id: 'unsafe' } },
    ]))

    const catalog = new MemoryModelCatalog([model('provider-a', 'stable')])
    await expect(new ModelCatalogService(providers, models, catalog).refresh('provider-a')).rejects.toMatchObject({
      code: 'invalid-model',
    })
    expect(models.list({ providerId: 'provider-a' }).map(({ modelId }) => modelId)).toEqual(['stable'])
    expect(catalog.list('provider-a').map(({ modelId }) => modelId)).toEqual(['stable'])
  })

  it('impede que uma resposta antiga sobrescreva uma descoberta mais recente', async () => {
    const older = deferred<readonly unknown[]>()
    const newer = deferred<readonly unknown[]>()
    const listModels = vi.fn()
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise)
    const providers = new ProviderRegistry()
    const models = new ModelRegistry()
    const catalog = new MemoryModelCatalog()
    providers.register(adapter('provider-a', listModels))
    const service = new ModelCatalogService(providers, models, catalog)

    const olderRefresh = service.refresh('provider-a')
    const newerRefresh = service.refresh('provider-a')
    newer.resolve([model('provider-a', 'newer')])
    await expect(newerRefresh).resolves.toMatchObject({ status: 'applied' })
    older.resolve([model('provider-a', 'older')])
    await expect(olderRefresh).resolves.toMatchObject({ status: 'superseded' })

    expect(models.list({ providerId: 'provider-a' }).map(({ modelId }) => modelId)).toEqual(['newer'])
    expect(catalog.list('provider-a').map(({ modelId }) => modelId)).toEqual(['newer'])
  })

  it('não aplica o catálogo se o adapter for removido durante a descoberta', async () => {
    const pending = deferred<readonly unknown[]>()
    const providers = new ProviderRegistry()
    const models = new ModelRegistry()
    const catalog = new MemoryModelCatalog([model('provider-a', 'stable')])
    models.register(model('provider-a', 'stable'))
    providers.register(adapter('provider-a', () => pending.promise))
    const refresh = new ModelCatalogService(providers, models, catalog).refresh('provider-a')

    await providers.unregister('provider-a')
    pending.resolve([model('provider-a', 'late')])

    await expect(refresh).rejects.toEqual(
      expect.objectContaining<Partial<ProviderRegistryError>>({
        code: 'provider-not-found',
      }),
    )
    expect(models.list({ providerId: 'provider-a' }).map(({ modelId }) => modelId)).toEqual(['stable'])
    expect(catalog.list('provider-a').map(({ modelId }) => modelId)).toEqual(['stable'])
  })

  it('hidrata o Registry com o último snapshot persistido', () => {
    const providers = new ProviderRegistry()
    const models = new ModelRegistry()
    const catalog = new MemoryModelCatalog([
      model('provider-a', 'generic-model'),
      model('ollama', 'llama'),
    ])

    expect(new ModelCatalogService(providers, models, catalog).initialize())
      .toEqual({ loaded: 2, providers: 2 })
    expect(models.list().map(({ providerId, modelId }) => `${providerId}/${modelId}`))
      .toEqual(['provider-a/generic-model', 'ollama/llama'])
  })

  it('restaura o Registry anterior quando a persistência falha', async () => {
    const providers = new ProviderRegistry()
    const models = new ModelRegistry()
    const catalog = new MemoryModelCatalog([model('provider-a', 'stable')])
    catalog.failReplace = true
    models.register(model('provider-a', 'stable'))
    providers.register(adapter('provider-a', () => [model('provider-a', 'new')]))

    await expect(new ModelCatalogService(
      providers,
      models,
      catalog,
    ).refresh('provider-a')).rejects.toThrow('persistence failed')
    expect(models.list({ providerId: 'provider-a' }).map(({ modelId }) => modelId))
      .toEqual(['stable'])
  })
})
