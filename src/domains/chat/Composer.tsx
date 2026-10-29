import { useLayoutEffect, type FormEvent, type KeyboardEvent, type RefObject } from 'react'
import { Cpu, Paperclip, Send, ShieldCheck, Square, X } from 'lucide-react'
import type { AgentMode, Attachment } from '../../types'
import type { AgentState } from '../../../shared/agentState'
import { isBusy } from '../../shared/format'
import { useI18n } from '../../shared/i18n'
import { useRendererRenderCounter } from '../../shared/rendererDiagnostics'

interface ComposerProps {
  agentMode: AgentMode; attachments: Attachment[]; prompt: string; status: AgentState; finalizing: boolean; pendingApprovals: number; composerRef: RefObject<HTMLTextAreaElement | null>;
  /** "Service · model" of the selected AI, or null when the user still has to choose one. */
  aiLabel: string | null; onOpenProviderSettings(): void;
  onOpenApprovals(): void; onMode(mode: AgentMode): void; onPrompt(value: string): void; onRemoveAttachment(path: string): void; onAttach(): void; onCancel(): void; onSubmit(event: FormEvent<HTMLFormElement>): void
}

// One name per mode. The description under the switch states what the mode does to the project.
const modes: Array<{ id: AgentMode; actionKey: string; descriptionKey: string; placeholderKey: string }> = [
  { id: 'review', actionKey: 'front.analyze', descriptionKey: 'composer.reviewDescription', placeholderKey: 'composer.placeholderReview' },
  { id: 'build', actionKey: 'front.implement', descriptionKey: 'composer.buildDescription', placeholderKey: 'composer.placeholderBuild' },
  { id: 'docs', actionKey: 'front.document', descriptionKey: 'composer.docsDescription', placeholderKey: 'composer.placeholderDocs' },
]

export function Composer({ agentMode, attachments, prompt, status, finalizing, pendingApprovals, onOpenApprovals, composerRef, aiLabel, onOpenProviderSettings, onMode, onPrompt, onRemoveAttachment, onAttach, onCancel, onSubmit }: ComposerProps) {
  useRendererRenderCounter('composer')
  const { t } = useI18n()
  const busy = isBusy(status) || finalizing
  const selected = modes.find((mode) => mode.id === agentMode) ?? modes[0]
  useLayoutEffect(() => {
    const textarea = composerRef.current
    if (!textarea) return
    if (!prompt) { textarea.style.height = ''; textarea.style.overflowY = 'hidden'; return }
    textarea.style.height = 'auto'
    const maximum = 220
    textarea.style.height = `${Math.min(textarea.scrollHeight, maximum)}px`
    textarea.style.overflowY = textarea.scrollHeight > maximum ? 'auto' : 'hidden'
  }, [composerRef, prompt])
  const moveModeFocus = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
    event.preventDefault()
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? modes.length - 1 : (index + (event.key === 'ArrowRight' ? 1 : -1) + modes.length) % modes.length
    onMode(modes[next].id)
    event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="radio"]')[next]?.focus()
  }
  return <div className="composer-wrap">
    {pendingApprovals > 0 && <div className="approval-notice" role="status"><ShieldCheck size={15}/><span><strong>{t('composer.pendingApproval')}</strong><small>{t('composer.openAgentPanel')}</small></span><button type="button" onClick={onOpenApprovals}>{t('front.reviewApprovals')}</button></div>}
    <div className="agent-mode-switch" role="radiogroup" aria-label={t('composer.agentMode')}>
      {modes.map((mode, index) => <button key={mode.id} type="button" role="radio" aria-checked={agentMode === mode.id} aria-describedby="composer-mode-description" tabIndex={agentMode === mode.id ? 0 : -1} className={agentMode === mode.id ? 'active' : ''} onKeyDown={(event) => moveModeFocus(event, index)} onClick={() => onMode(mode.id)}>{t(mode.actionKey)}</button>)}
    </div>
    <p id="composer-mode-description" className="mode-description">{t(selected.descriptionKey)}</p>
    <form className="composer" onSubmit={onSubmit}>
      <label className="sr-only" htmlFor="prompt-composer">{t('composer.messageLabel')}</label>
      {!!attachments.length && <div className="attachment-list">{attachments.map((item) => <span key={item.path}><Paperclip size={13}/>{item.name}<button type="button" aria-label={`${t('composer.removeAttachment')} ${item.name}`} title={t('composer.removeAttachment')} onClick={() => onRemoveAttachment(item.path)}><X size={13}/></button></span>)}</div>}
      <textarea id="prompt-composer" aria-label={t('composer.messageLabel')} ref={composerRef} value={prompt} onChange={(event) => onPrompt(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); event.currentTarget.form?.requestSubmit() } }} placeholder={t(selected.placeholderKey)} rows={1}/>
      <div className="composer-bottom">
        <button type="button" className="composer-attach" aria-label={t('composer.attachFile')} title={busy ? t('composer.waitExecution') : t('composer.attachFile')} onClick={onAttach} disabled={busy}><Paperclip size={16}/></button>
        <button type="button" className={`composer-ai ${aiLabel ? '' : 'missing'}`} onClick={onOpenProviderSettings} aria-label={aiLabel ? t('composer.activeAi', { name: aiLabel }) : t('composer.noAiHint')} title={aiLabel ?? t('composer.noAiHint')}><Cpu size={14} aria-hidden="true"/><span>{aiLabel ?? t('composer.chooseAi')}</span></button>
        <button type={busy ? 'button' : 'submit'} aria-label={finalizing ? t('composer.savingResponse') : busy ? t('composer.cancelExecution') : t('composer.sendMessage')} title={finalizing ? `${t('composer.savingResponse')}…` : busy ? t('composer.cancelExecution') : t('composer.sendMessage')} onClick={busy && !finalizing && status !== 'cancelling' ? onCancel : undefined} className={`send-button ${busy && !finalizing ? 'stop' : ''}`} disabled={finalizing || status === 'cancelling' || (!prompt.trim() && !busy)}>{busy && !finalizing ? <Square size={14} fill="currentColor"/> : <Send size={15}/>}<span>{finalizing ? t('composer.savingResponse') : busy ? t('front.stop') : t('composer.sendMessage')}</span></button>
      </div>
    </form>
    <p className="composer-hint">{t('composer.enterHint')}</p>
  </div>
}
