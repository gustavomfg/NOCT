import type { AiEngine } from '../../../shared/ai/engine'
import { AIConnectionPage } from './AIConnectionPage'
import type { AiSelection } from './useSettingsController'
import { useI18n } from '../../shared/i18n'

interface AISettingsPageProps {
  workspaceId: string
  activeEngine?: AiEngine
  codexModel: string
  onNotify(message: string): void
  onAiSelectionChange(selection: AiSelection): Promise<void>
}

export function AISettingsPage({
  workspaceId,
  activeEngine,
  codexModel,
  onNotify,
  onAiSelectionChange,
}: AISettingsPageProps) {
  const { t } = useI18n()
  return <div className="ai-settings" role="region" aria-label={t('ai.configuration')}>
    <AIConnectionPage
      workspaceId={workspaceId}
      activeEngine={activeEngine}
      codexModel={codexModel}
      onNotify={onNotify}
      onAiSelectionChange={onAiSelectionChange}
    />
  </div>
}
