import { describe, expect, it } from 'vitest'
import { normalizeSettings } from '../src/domains/settings/useSettingsController'

describe('settings controller contracts', () => {
  it('normalizes persisted settings to the supported renderer values', () => {
    expect(normalizeSettings({
      approvalPolicy: 'on-request',
      diagnosticMode: false,
      theme: 'dark',
      language: 'pt-BR',
      ...({ approvalPolicy: 'invalid', theme: 'invalid', language: 'invalid', diagnosticMode: 1 } as unknown as Record<string, unknown>),
    })).toMatchObject({
      approvalPolicy: 'on-request',
      diagnosticMode: false,
      theme: 'dark',
      language: 'pt-BR',
    })
  })

  it('preserves optional settings returned by persistence', () => {
    expect(normalizeSettings({
      approvalPolicy: 'untrusted',
      diagnosticMode: true,
      theme: 'light',
      language: 'en',
      pandocVersion: '3.1.1',
    })).toEqual({
      approvalPolicy: 'untrusted',
      diagnosticMode: true,
      theme: 'light',
      language: 'en',
      pandocVersion: '3.1.1',
    })
  })
})
