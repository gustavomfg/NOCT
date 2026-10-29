import type { RefObject } from 'react'
import { Brain, Code2, GitBranch, HelpCircle, Menu, PanelRight, Settings, Terminal } from 'lucide-react'
import type { GitInfo } from '../../types'
import { useI18n } from '../../shared/i18n'

interface WorkspaceTopbarProps {
  title: string
  /** Set when a project is open; enables the tools that act on it. */
  hasProject: boolean
  gitInfo: GitInfo | null
  sidebarOpen: boolean
  inspectorOpen: boolean
  compact: boolean
  hasMemory: boolean
  sidebarTriggerRef: RefObject<HTMLButtonElement | null>
  inspectorTriggerRef: RefObject<HTMLButtonElement | null>
  onOpenSidebar(): void
  onOpenTool(tool: 'editor' | 'terminal'): void
  onMemory(): void
  onSettings(): void
  onHelp(): void
  onToggleInspector(): void
}

// The project name lives in the sidebar and the agent state in the progress panel,
// so the header only carries the task and the tools that act on it.
export function WorkspaceTopbar({ title, hasProject, gitInfo, sidebarOpen, inspectorOpen, compact, hasMemory, sidebarTriggerRef, inspectorTriggerRef, onOpenSidebar, onOpenTool, onMemory, onSettings, onHelp, onToggleInspector }: WorkspaceTopbarProps) {
  const { t } = useI18n()
  return <header className="topbar">
    {!sidebarOpen && <button ref={sidebarTriggerRef} className="icon-button" aria-label={t('nav.openSidebar')} title={t('nav.openSidebar')} aria-controls="workspace-sidebar" aria-expanded={sidebarOpen} onClick={onOpenSidebar}><Menu size={18}/></button>}
    <div className="title-block"><h1 title={title}>{title}</h1></div>
    <div className="top-actions">
      {gitInfo && <span className="branch-pill top-action-branch"><GitBranch size={12}/>{gitInfo.branch}</span>}
      {hasProject && <>
        <button className="icon-button" aria-label={t('topbar.openWebstorm')} title={t('topbar.openWebstorm')} onClick={() => onOpenTool('editor')}><Code2 size={16}/></button>
        <button className="icon-button" aria-label={t('topbar.openTerminal')} title={t('topbar.openTerminal')} onClick={() => onOpenTool('terminal')}><Terminal size={16}/></button>
      </>}
      <button className={`icon-button ${hasMemory ? 'has-memory' : ''}`} aria-label={t('topbar.workspaceMemory')} title={t('topbar.workspaceMemory')} onClick={onMemory}><Brain size={17}/></button>
      <button className="icon-button" aria-label={t('nav.openSettings')} title={t('nav.openSettings')} onClick={onSettings}><Settings size={17}/></button>
      <button className="icon-button" aria-label={t('topbar.openHelp')} title={t('topbar.openHelp')} onClick={onHelp}><HelpCircle size={17}/></button>
      {(!compact || !inspectorOpen) && <button ref={inspectorTriggerRef} className={`icon-button ${inspectorOpen ? 'selected' : ''}`} aria-label={inspectorOpen ? t('topbar.hideAgent') : t('topbar.showAgent')} title={inspectorOpen ? t('topbar.hideAgent') : t('topbar.showAgent')} aria-controls="agent-inspector" aria-expanded={inspectorOpen} onClick={onToggleInspector}><PanelRight size={18}/></button>}
    </div>
  </header>
}
