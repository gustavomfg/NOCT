import type { CommandLine } from 'electron'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

// Desktops with native Chromium keyring selection keep their existing backend.
const nativeDesktops = new Set([
  'gnome', 'unity', 'x-cinnamon', 'cinnamon', 'deepin', 'pantheon',
  'xfce', 'xfce4', 'ukui', 'kde', 'kde4', 'kde5', 'kde6', 'plasma',
])

const secureServices = [
  ['gnome-libsecret', 'org.freedesktop.secrets.service'],
  ['kwallet6', 'org.kde.kwalletd6.service'],
  ['kwallet5', 'org.kde.kwalletd5.service'],
  ['kwallet', 'org.kde.kwalletd.service'],
] as const

/** Must run before app.ready, while Chromium can still select its key store. */
export function configureLinuxCredentialStorage(
  commandLine: Pick<CommandLine, 'hasSwitch' | 'appendSwitch'>,
  options: {
    platform?: NodeJS.Platform
    environment?: Partial<NodeJS.ProcessEnv>
    serviceExists?: (file: string) => boolean
  } = {},
) {
  if ((options.platform ?? process.platform) !== 'linux' || commandLine.hasSwitch('password-store')) return
  const environment = options.environment ?? process.env
  const desktops = [environment.XDG_CURRENT_DESKTOP, environment.DESKTOP_SESSION]
    .flatMap((value) => (value ?? '').toLowerCase().split(/[:;]/))
  if (desktops.some((desktop) => nativeDesktops.has(desktop))) return

  const directories = [...new Set([
    environment.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share'),
    ...(environment.XDG_DATA_DIRS || '/usr/local/share:/usr/share').split(':'),
  ])].filter((directory) => path.isAbsolute(directory)).slice(0, 16)
  const serviceExists = options.serviceExists ?? isServiceFile
  for (const [backend, service] of secureServices) {
    if (directories.some((directory) => serviceExists(path.join(directory, 'dbus-1', 'services', service)))) {
      commandLine.appendSwitch('password-store', backend)
      return backend
    }
  }
}

function isServiceFile(file: string) {
  try {
    return fs.statSync(file).isFile()
  } catch {
    return false
  }
}
