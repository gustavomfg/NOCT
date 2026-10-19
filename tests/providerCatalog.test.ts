import { describe, expect, it } from 'vitest'
import {
  configurationFromProviderCatalog,
  providerCatalog,
} from '../shared/ai/providerCatalog'
import { providerConfigurationInputSchema } from '../shared/ai/providerConfigurationSchemas'

describe('providerCatalog', () => {
  it('mantém empresas e métodos de conexão com identidades únicas', () => {
    expect(new Set(providerCatalog.map(({ id }) => id)).size).toBe(providerCatalog.length)
    expect(providerCatalog.map(({ id }) => id)).not.toContain('openai')
    for (const provider of providerCatalog) {
      expect(provider.connectionMethods.length).toBeGreaterThan(0)
      expect(new Set(provider.connectionMethods.map(({ kind }) => kind)).size)
        .toBe(provider.connectionMethods.length)
    }
  })

  it('gera somente configurações aceitas pelo transporte disponível', () => {
    for (const provider of providerCatalog) {
      const configuration = configurationFromProviderCatalog(provider.id)
      if (provider.integrationStatus === 'adapter-required') {
        expect(configuration).toBeUndefined()
        continue
      }
      expect(providerConfigurationInputSchema.parse(configuration)).toEqual(configuration)
    }
  })

  it('não oferece login por assinatura: somente chaves de API e runtimes locais', () => {
    for (const provider of providerCatalog) {
      expect(provider.connectionMethods.map(({ kind }) => kind).every((kind) => kind === 'api-key' || kind === 'local')).toBe(true)
    }
  })

  it('restringe HTTP sem TLS aos presets locais em loopback', () => {
    for (const provider of providerCatalog) {
      if (!provider.baseUrl) continue
      const url = new URL(provider.baseUrl)
      if (provider.source === 'remote') {
        expect(url.protocol).toBe('https:')
      } else {
        expect(url.protocol).toBe('http:')
        expect(url.hostname).toBe('127.0.0.1')
      }
    }
  })
})
