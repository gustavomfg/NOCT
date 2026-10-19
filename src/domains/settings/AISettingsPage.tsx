import { AIConnectionPage } from './AIConnectionPage'
import { useI18n } from '../../shared/i18n'

interface AISettingsPageProps {
  workspaceId: string
  onNotify(message: string): void
}

export function AISettingsPage({ workspaceId, onNotify }: AISettingsPageProps) {
  const { t } = useI18n()
  return <div className="ai-settings" role="region" aria-label={t('ai.configuration')}>
    <AIConnectionPage workspaceId={workspaceId} onNotify={onNotify}/>
  </div>
}
