/**
 * Execution backends. `provider` covers every configured API or local model
 * (OpenAI-compatible adapters); `codex` is the optional Codex CLI integration.
 */
export const aiEngines = ['provider', 'codex'] as const
export type AiEngine = typeof aiEngines[number]

export interface AiEngineSelectionInput {
  /** Explicit user choice persisted in settings, if any. */
  preference?: string
  /** The workspace has a default model whose Provider is enabled. */
  providerReady: boolean
  /** A Codex model was chosen before explicit engine selection existed. */
  legacyCodexModel?: string
}

/** Returns the engine for a run, or null when the user still has to choose an AI. */
export function selectAiEngine({ preference, providerReady, legacyCodexModel }: AiEngineSelectionInput): AiEngine | null {
  if (preference === 'codex') return 'codex'
  if (providerReady) return 'provider'
  // Installations from 1.0.x selected Codex implicitly by choosing its model.
  if (preference !== 'provider' && legacyCodexModel) return 'codex'
  return null
}
