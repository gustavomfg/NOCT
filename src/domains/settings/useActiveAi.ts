import { useEffect, useState } from 'react'
import { selectAiEngine, type AiEngine } from '../../../shared/ai/engine'
import type { AppSettings } from '../../types'

export interface ActiveAiSummary {
  engine: AiEngine | null
  /** Short "service · model" label, or null when no AI is selected. */
  label: string | null
}

const NONE: ActiveAiSummary = { engine: null, label: null }

/**
 * Mirrors the main-process engine selection for display. It never probes the
 * Codex CLI, so rendering the composer does not spawn external processes.
 */
export function useActiveAi(workspace: string, settings: AppSettings, refreshKey: unknown): ActiveAiSummary {
  const [summary, setSummary] = useState<ActiveAiSummary>(NONE)
  const { aiEngine, model } = settings

  useEffect(() => {
    let active = true
    const load = async () => {
      const [providers, bindings] = await Promise.all([
        window.nocturne.providers.list(),
        workspace ? window.nocturne.models.bindings(workspace) : Promise.resolve(null),
      ])
      const binding = bindings?.defaultBinding
      const provider = binding ? providers.find((item) => item.id === binding.providerId && item.enabled) : undefined
      const engine = selectAiEngine({ preference: aiEngine, providerReady: Boolean(provider), legacyCodexModel: model })
      if (engine === 'codex') return { engine, label: model ? `ChatGPT · ${model}` : 'ChatGPT' }
      if (engine === 'provider' && provider && binding) {
        const descriptor = (await window.nocturne.models.list()).find((item) => item.providerId === binding.providerId && item.modelId === binding.modelId)
        return { engine, label: `${provider.displayName} · ${descriptor?.displayName ?? binding.modelId}` }
      }
      return NONE
    }
    void load()
      .then((next) => { if (active) setSummary(next) })
      .catch(() => { if (active) setSummary(NONE) })
    return () => { active = false }
  }, [aiEngine, model, refreshKey, workspace])

  return summary
}
