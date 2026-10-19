import { beforeEach, describe, expect, it, vi } from 'vitest'

const storage = vi.hoisted(() => ({
  isEncryptionAvailable: vi.fn(),
  getSelectedStorageBackend: vi.fn(),
  encryptString: vi.fn(),
  decryptString: vi.fn(),
}))
vi.mock('electron', () => ({ safeStorage: storage }))

import { ElectronCredentialEncryption } from '../electron/security/ElectronCredentialEncryption'

beforeEach(() => {
  vi.resetAllMocks()
  storage.isEncryptionAvailable.mockReturnValue(true)
  storage.getSelectedStorageBackend.mockReturnValue('kwallet6')
})

describe('Electron credential encryption security', () => {
  it('keeps unavailable keyrings blocked', () => {
    storage.isEncryptionAvailable.mockReturnValue(false)
    expect(new ElectronCredentialEncryption().isSecureStorageAvailable()).toBe(false)
    expect(storage.encryptString).not.toHaveBeenCalled()
  })

  it.runIf(process.platform === 'linux')('rejects basic_text even when Electron reports encryption available', () => {
    storage.getSelectedStorageBackend.mockReturnValue('basic_text')
    expect(new ElectronCredentialEncryption().isSecureStorageAvailable()).toBe(false)
  })

  it.runIf(process.platform === 'linux')('accepts available KWallet 6 after startup selection', () => {
    expect(new ElectronCredentialEncryption().isSecureStorageAvailable()).toBe(true)
  })

  it('fails closed when native availability detection throws', () => {
    storage.isEncryptionAvailable.mockImplementation(() => { throw new Error('keyring unavailable') })
    expect(new ElectronCredentialEncryption().isSecureStorageAvailable()).toBe(false)
  })
})
