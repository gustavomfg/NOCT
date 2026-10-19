import path from 'node:path'
import { describe, expect, it, vi } from 'vitest'
import { configureLinuxCredentialStorage } from '../electron/security/LinuxCredentialStorage'

const serviceDirectory = '/usr/share/dbus-1/services'

function setup(services: string[] = []) {
  const commandLine = { hasSwitch: vi.fn(() => false), appendSwitch: vi.fn() }
  const serviceExists = vi.fn((file: string) => services.some((service) => file === path.join(serviceDirectory, service)))
  const options = {
    platform: 'linux' as NodeJS.Platform,
    environment: { XDG_CURRENT_DESKTOP: 'Hyprland', XDG_DATA_HOME: '/user/data', XDG_DATA_DIRS: '/usr/share' },
    serviceExists,
  }
  return { commandLine, options }
}

describe('Linux secure credential storage selection', () => {
  it('selects installed KWallet 6 for Hyprland before Chromium initialization', () => {
    const { commandLine, options } = setup(['org.kde.kwalletd6.service'])
    expect(configureLinuxCredentialStorage(commandLine, options)).toBe('kwallet6')
    expect(commandLine.appendSwitch).toHaveBeenCalledWith('password-store', 'kwallet6')
  })

  it('prefers Secret Service on an unrecognized desktop when available', () => {
    const { commandLine, options } = setup(['org.freedesktop.secrets.service', 'org.kde.kwalletd6.service'])
    expect(configureLinuxCredentialStorage(commandLine, options)).toBe('gnome-libsecret')
    expect(commandLine.appendSwitch).toHaveBeenCalledOnce()
  })

  it.each(['GNOME', 'GNOME:Unity', 'KDE', 'XFCE', 'X-Cinnamon'])('preserves native backend selection for %s', (desktop) => {
    const { commandLine, options } = setup(['org.kde.kwalletd6.service'])
    options.environment.XDG_CURRENT_DESKTOP = desktop
    configureLinuxCredentialStorage(commandLine, options)
    expect(commandLine.appendSwitch).not.toHaveBeenCalled()
    expect(options.serviceExists).not.toHaveBeenCalled()
  })

  it('preserves explicit user switches and does not enable plaintext fallback', () => {
    const { commandLine, options } = setup(['org.kde.kwalletd6.service'])
    commandLine.hasSwitch.mockReturnValue(true)
    configureLinuxCredentialStorage(commandLine, options)
    expect(commandLine.hasSwitch).toHaveBeenCalledWith('password-store')
    expect(commandLine.appendSwitch).not.toHaveBeenCalled()
    expect(options.serviceExists).not.toHaveBeenCalled()
  })

  it.each(['darwin', 'win32'] as const)('does not change storage selection on %s', (platform) => {
    const { commandLine, options } = setup(['org.kde.kwalletd6.service'])
    configureLinuxCredentialStorage(commandLine, { ...options, platform })
    expect(commandLine.appendSwitch).not.toHaveBeenCalled()
    expect(options.serviceExists).not.toHaveBeenCalled()
  })

  it('keeps secure storage unavailable when no supported service is installed', () => {
    const { commandLine, options } = setup()
    configureLinuxCredentialStorage(commandLine, options)
    expect(commandLine.appendSwitch).not.toHaveBeenCalled()
  })

  it('discovers services in absolute XDG directories and ignores relative search paths', () => {
    const { commandLine, options } = setup()
    options.environment.XDG_DATA_DIRS = 'relative:/custom/data'
    options.serviceExists.mockImplementation((file) => file === path.join('/custom/data', 'dbus-1', 'services', 'org.kde.kwalletd5.service'))
    expect(configureLinuxCredentialStorage(commandLine, options)).toBe('kwallet5')
    expect(options.serviceExists.mock.calls.flat().every((file) => path.isAbsolute(file))).toBe(true)
  })
})
