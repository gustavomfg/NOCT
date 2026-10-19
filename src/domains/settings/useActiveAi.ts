import { useEffect, useState } from 'react'

/**
 * "Service · model" of the AI bound to the workspace, or null when none is
 * selected. Re-read whenever `refreshKey` changes (the settings dialog closing).
 */
export function useActiveAi(workspace: string, refreshKey: unknown): string | null {
  const [label, setLabel] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    const load = async () => {
      if (!workspace) return null
      const bindings = await window.nocturne.models.bindings(workspace)
      const binding = bindings?.defaultBinding
      if (!binding) return null
      const [providers, models] = await Promise.all([window.nocturne.providers.list(), window.nocturne.models.list()])
      const provider = providers.find((item) => item.id === binding.providerId && item.enabled)
      if (!provider) return null
      const descriptor = models.find((item) => item.providerId === binding.providerId && item.modelId === binding.modelId)
      return `${provider.displayName} · ${descriptor?.displayName ?? binding.modelId}`
    }
    void load()
      .then((next) => { if (active) setLabel(next) })
      .catch(() => { if (active) setLabel(null) })
    return () => { active = false }
  }, [refreshKey, workspace])

  return label
}
