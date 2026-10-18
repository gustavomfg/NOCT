import path from 'node:path'
import type { ProjectIndexFile } from '../../shared/codeIntelligence'
import { isIgnoredProjectDiscoveryRelativePath } from '../workspaces/WorkspacePathPolicy'

export interface SemanticPrivacyDecision {
  allowed: boolean
  embeddingAllowed: boolean
  reason: string | null
}

const secretNames = new Set([
  '.env',
  '.env.local',
  '.env.development',
  '.env.production',
  'credentials.json',
  'secrets.json',
  'secrets.yaml',
  'secrets.yml',
])

const secretExtensions = new Set(['.pem', '.key', '.p12', '.pfx', '.crt', '.cer'])

/** Files whose content must not leave the machine through AI context or tools. */
export function isPotentiallySensitivePath(relativePath: string) {
  const baseName = path.posix.basename(relativePath.replace(/\\/g, '/')).toLowerCase()
  const extension = path.posix.extname(baseName)
  return secretNames.has(baseName)
    || baseName.includes('secret')
    || baseName.includes('credential')
    || baseName.includes('token')
    || secretExtensions.has(extension)
}

/** Applies semantic content rules after Project Index authorization and before any embedding call. */
export class SemanticPrivacyPolicy {
  evaluate(file: Pick<ProjectIndexFile, 'relativePath' | 'classification' | 'excluded' | 'exclusionReason'>, remoteEmbeddingAllowed: boolean): SemanticPrivacyDecision {
    const relativePath = file.relativePath.replace(/\\/g, '/')
    if (file.excluded || isIgnoredProjectDiscoveryRelativePath(relativePath)) {
      return { allowed: false, embeddingAllowed: false, reason: file.exclusionReason ?? 'Caminho excluído pelo Project Index.' }
    }
    if (isPotentiallySensitivePath(relativePath)) {
      return { allowed: false, embeddingAllowed: false, reason: 'Arquivo potencialmente sensível excluído da inteligência semântica.' }
    }
    if (file.classification === 'asset' || file.classification === 'lockfile') {
      return { allowed: false, embeddingAllowed: false, reason: 'Tipo de arquivo não textual ou excessivamente derivado.' }
    }
    if (file.classification === 'unknown') {
      return { allowed: false, embeddingAllowed: false, reason: 'Classificação de arquivo não suportada pela política semântica.' }
    }
    if (!remoteEmbeddingAllowed) {
      return { allowed: true, embeddingAllowed: false, reason: 'Embedding remoto não autorizado para este workspace.' }
    }
    return { allowed: true, embeddingAllowed: true, reason: null }
  }
}
