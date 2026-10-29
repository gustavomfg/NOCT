import { memo } from 'react'
import { Brain, FolderOpen, Paperclip } from 'lucide-react'
import type { Message } from '../../types'
import { SafeMarkdown } from '../../shared/SafeMarkdown'
import { parseAwarenessSnapshot } from '../../../shared/awareness'
import { useI18n } from '../../shared/i18n'

export function Welcome({ workspaceReady, onWorkspace }: { workspaceReady: boolean; onWorkspace(): void }) {
  const { t } = useI18n()
  return <div className="welcome">
    <h2>{t('welcome.title')}</h2>
    <p>{t('welcome.subtitle')}</p>
    {!workspaceReady && <button type="button" className="welcome-open" onClick={onWorkspace}><FolderOpen size={16}/>{t('welcome.openProject')}</button>}
  </div>
}

export const MessageBubble = memo(function MessageBubble({ message }: { message: Message }) {
  const { t } = useI18n()
  if (message.role !== 'user') return <AssistantMessage content={message.content}/>
  let attachments: string[] = []
  try { attachments = JSON.parse(message.metadata || '{}').attachments || [] } catch { attachments = [] }
  const awareness = parseAwarenessSnapshot(message.metadata)
  return <div className="user-row"><div className="user-message">{message.content}{!!attachments.length && <div className="message-attachments">{attachments.map((filePath) => <span key={filePath}><Paperclip size={10}/>{filePath.split(/[/\\]/).pop()}</span>)}</div>}{awareness && <details className="message-awareness"><summary><Brain size={12}/>{t('chat.usedContext')} · {awareness.selections.length}</summary><div>{awareness.selections.length ? awareness.selections.map((selection) => <p key={`${selection.source}-${selection.id}`}><strong>{selection.title} · {selection.relevance}%</strong><small>{selection.reason}</small></p>) : <p><small>{t('chat.noRelevantMemory')}</small></p>}</div></details>}</div><div className="mini-avatar">G</div></div>
})

export function AssistantMessage({ content, streaming }: { content: string; streaming?: boolean }) {
  const { t } = useI18n()
  const renderAsText = Boolean(streaming) || content.length > 300_000
  return <div className="assistant-row"><div className="assistant-avatar"><img src="./nocturne.png" alt="" width="28" height="28"/></div><div className="assistant-content"><div className="assistant-name">NOCT {streaming && <span>{t('chat.writing')}</span>}</div>{renderAsText ? <>{!streaming && <p>{t('chat.longResponse')}</p>}<pre className={`large-response${streaming ? ' streaming-response' : ''}`}>{streaming ? content : content.slice(-300_000)}</pre></> : <SafeMarkdown>{content}</SafeMarkdown>}{streaming && <span className="caret"/>}</div></div>
}
