import { lazy, Suspense } from 'react'
import type { AppSettings, FilePreview, Workspace, WorkspaceMemory } from '../../types'
import { loadSettingsDialog } from './loadSettingsDialog'
import type { AiSelection } from './useSettingsController'

const SettingsDialog = lazy(loadSettingsDialog)
const MemoryDialog = lazy(() => import('./Dialogs').then((module) => ({ default: module.MemoryDialog })))
const OnboardingDialog = lazy(() => import('./Dialogs').then((module) => ({ default: module.OnboardingDialog })))
const PreviewDialog = lazy(() => import('./Dialogs').then((module) => ({ default: module.PreviewDialog })))
const ShortcutsDialog = lazy(() => import('./Dialogs').then((module) => ({ default: module.ShortcutsDialog })))

interface AppOverlaysProps {
  settingsOpen: boolean
  settings: AppSettings
  workspaces: Workspace[]
  memoryOpen: boolean
  memory: WorkspaceMemory
  preview: FilePreview | null
  onboardingOpen: boolean
  helpOpen: boolean
  activeId: string | null
  workspace: string
  onSettingsClose(): void
  onSaveSettings(value: AppSettings): Promise<void>
  onAiSelectionChange(selection: AiSelection): Promise<void>
  onNotify(message: string): void
  onOpenOnboarding(): void
  onMemoryClose(): void
  onOpenBrain(): void
  onSaveMemory(content: string, rules: string): Promise<void>
  onPreviewClose(): void
  onError(message: string | null): void
  onWorkspace(): Promise<void>
  onOpenSettings(): void
  onDismissOnboarding(): void
  onCompleteOnboarding(): void
  onHelpClose(): void
}

export function AppOverlays({ settingsOpen, settings, workspaces, memoryOpen, memory, preview, onboardingOpen, helpOpen, activeId, workspace, onSettingsClose, onSaveSettings, onAiSelectionChange, onNotify, onOpenOnboarding, onMemoryClose, onOpenBrain, onSaveMemory, onPreviewClose, onError, onWorkspace, onOpenSettings, onDismissOnboarding, onCompleteOnboarding, onHelpClose }: AppOverlaysProps) {
  return <Suspense fallback={null}>
    {settingsOpen && <SettingsDialog value={settings} workspace={workspace} workspaces={workspaces} onClose={onSettingsClose} onSave={onSaveSettings} onAiSelectionChange={onAiSelectionChange} onNotify={onNotify} onOnboarding={onOpenOnboarding}/>}
    {memoryOpen && <MemoryDialog value={memory} onClose={onMemoryClose} onOpenBrain={onOpenBrain} onSave={onSaveMemory}/>}
    {preview && <PreviewDialog preview={preview} activeId={activeId} onClose={onPreviewClose} onError={onError} onNotify={onNotify}/>}
    {onboardingOpen && <OnboardingDialog workspace={workspace} onWorkspace={onWorkspace} onSettings={onOpenSettings} onDismiss={onDismissOnboarding} onComplete={onCompleteOnboarding}/>}
    {helpOpen && <ShortcutsDialog onClose={onHelpClose}/>}
  </Suspense>
}
