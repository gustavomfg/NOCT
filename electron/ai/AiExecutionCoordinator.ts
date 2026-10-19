import { randomUUID } from 'node:crypto'
import { turnOutcome } from '../../shared/turnOutcome'
import type { BrowserWindow } from 'electron'
import type { WorkspaceModelBindings } from '../../shared/ai/bindings'
import type { NormalizedTaskInput } from '../../shared/ai/task'
import type { AgentState } from '../../shared/agentState'
import { isTerminalAgentState, reduceAgentLifecycle, type AgentLifecycleDetails, type AgentLifecycleEvent, type AgentRunState } from '../../shared/agentLifecycle'
import type { AgentMode } from '../../shared/types'
import { PERSISTENCE_LIMITS } from '../../shared/constants'
import type { Logger } from '../logging/Logger'
import type { ModelRegistry } from './ModelRegistry'
import type { ProviderRegistry } from './ProviderRegistry'
import { startAiTurn } from './executeAiTurn'
import type { AgentToolRuntime } from './AIOrchestrator'
import type { ToolApprovalRequest } from './agent/WorkspaceToolbox'
import type { CompletedTurnSnapshot, PersistedTurn } from './TurnPersistence'
import { IPC_CHANNELS } from '../../shared/ipc/channels'

export type ApprovalDetails = Map<string, { command?: string; risk?: string }>

interface ActiveExecution {
  runId: string
  executionId: string | null
  conversationId: string
  workspace: string
  mode: AgentMode
  content: string
  diff: string
  files: string[]
  plan: unknown[]
  planExplanation: string
  finishing: boolean
  cancelRequested: boolean
  sequence: number
  lifecycle: AgentRunState | null
  /** Tool approvals waiting for a developer decision (Provider runs only). */
  pendingApprovals: Map<string, (accepted: boolean) => void>
  cancel(reason?: string): Promise<void>
}

export interface ProviderTurnOptions {
  executionId?: string
  mode?: AgentMode
  /** Builds the workspace tools for this run; omitted for plain chat completions. */
  createTools?: (bridge: ProviderToolBridge) => AgentToolRuntime
}

export interface ProviderToolBridge {
  emit(method: string, params: Record<string, unknown>): void
  requestApproval(request: ToolApprovalRequest): Promise<boolean>
}

export class AiExecutionCoordinator {
  private active: ActiveExecution | null = null
  private disposed = false
  private disposal: Promise<void> | undefined

  constructor(
    private readonly win: BrowserWindow,
    private readonly models: ModelRegistry,
    private readonly providers: ProviderRegistry,
    private readonly logger: Logger,
    private readonly approvalDetails: ApprovalDetails,
    private readonly finalizeTurn: (snapshot: CompletedTurnSnapshot) => PersistedTurn | Promise<PersistedTurn>,
    private readonly createRunId: () => string = randomUUID,
    private readonly now: () => Date = () => new Date(),
    private readonly persistExecutionState: (executionId: string, lifecycle: AgentRunState) => void = () => undefined,
  ) {}

  async startProvider(
    conversationId: string,
    taskInput: NormalizedTaskInput,
    bindings: WorkspaceModelBindings,
    { executionId, mode = taskInput.mode, createTools }: ProviderTurnOptions = {},
  ) {
    const active = this.reserve(conversationId, taskInput.workspace.id, mode, executionId)
    const runId = active.runId
    try {
      const emit = (method: string, params: Record<string, unknown>) => this.forwardEvent(method, params, conversationId, runId)
      const tools = createTools?.({
        emit,
        requestApproval: (request) => this.requestToolApproval(active, request),
      })
      const turn = await startAiTurn(
        this.models,
        this.providers,
        taskInput,
        bindings,
        emit,
        tools,
      )
      if (this.active?.runId !== runId) {
        turn.cancel('A execução perdeu seu contexto ativo.')
        throw new Error('A execução foi cancelada antes de iniciar.')
      }
      this.active.cancel = async (reason = 'Execução cancelada pelo usuário.') => {
        active.cancelRequested = true
        this.settlePendingApprovals(active, false)
        turn.cancel(reason)
      }
      if (active.cancelRequested) turn.cancel('Execução cancelada pelo usuário.')
      // A fast tool step may already be waiting for approval; do not overwrite it.
      if (active.lifecycle?.state === 'planning') this.pushExecutionStatus(active, 'running')
      void turn.completion
        .then((outcome) => {
          return this.persistAndComplete(active, { turn: { id: turn.executionId, ...outcome } })
        })
        .catch((error: unknown) => {
          const message = error instanceof Error ? error.message : String(error)
          if (!this.isCurrentOrFinished(active)) return
          this.pushEvent('error', { message }, active)
          void this.persistAndComplete(active, { turn: { id: turn.executionId, status: 'failed', error: { message } } })
        })
    } catch (error) {
      if (this.active?.runId === runId) {
        this.pushExecutionStatus(active, 'failed', error instanceof Error ? error.message : String(error))
        this.active = null
      }
      throw error
    }
  }

  async cancel(conversationId: string) {
    const active = this.active
    if (!active || active.conversationId !== conversationId || active.finishing || (active.lifecycle && isTerminalAgentState(active.lifecycle.state))) {
      throw new Error('Nenhuma execução ativa nesta conversa.')
    }
    if (!this.applyLifecycle(active, {
      type: 'run.cancelRequested',
      reason: 'Execução cancelada pelo usuário.',
    })) return
    this.pushStatusForRun(active, 'cancelling')
    await active.cancel()
  }

  async resolveApproval(key: string, accepted: boolean) {
    const active = this.active
    if (!active || active.lifecycle?.state !== 'waiting-approval') {
      throw new Error('Não existe uma execução aguardando aprovação.')
    }
    const resolve = active.pendingApprovals.get(key)
    if (!resolve) throw new Error('A aprovação solicitada não pertence à execução ativa.')
    active.pendingApprovals.delete(key)
    if (active.pendingApprovals.size === 0) this.pushExecutionStatus(active, 'running')
    resolve(accepted)
  }

  private requestToolApproval(active: ActiveExecution, request: ToolApprovalRequest) {
    if (this.active !== active || active.cancelRequested || active.finishing) return Promise.resolve(false)
    const approvalKey = `tool-approval-${this.createRunId()}`
    this.approvalDetails.set(approvalKey, { command: request.command, risk: request.risk })
    const decision = new Promise<boolean>((resolve) => active.pendingApprovals.set(approvalKey, resolve))
    this.pushExecutionStatus(active, 'waiting-approval')
    this.forwardEvent(
      request.kind === 'command' ? 'item/commandExecution/requestApproval' : 'item/fileChange/requestApproval',
      {
        approvalKey,
        reason: request.detail,
        ...(request.command ? { command: request.command } : {}),
        ...(request.risk ? { commandAssessment: { risk: request.risk } } : {}),
      },
      active.conversationId,
      active.runId,
    )
    return decision
  }

  private settlePendingApprovals(active: ActiveExecution, accepted: boolean) {
    const pending = [...active.pendingApprovals.entries()]
    active.pendingApprovals.clear()
    for (const [key, resolve] of pending) {
      this.approvalDetails.delete(key)
      resolve(accepted)
    }
  }

  dispose() {
    if (this.disposed) return this.disposal
    this.disposed = true
    const active = this.active
    this.active = null
    if (active) {
      this.settlePendingApprovals(active, false)
      void active.cancel('Aplicação encerrando; execução interrompida.').catch((error: unknown) => {
        this.logger.warn('ai', 'O cancelamento do Provider durante shutdown falhou.', { error: error instanceof Error ? error.message : String(error) })
      })
    }
    this.approvalDetails.clear()
    this.disposal = Promise.resolve()
    return this.disposal
  }

  private reserve(conversationId: string, workspace: string, mode: AgentMode, executionId?: string) {
    if (this.disposed) throw new Error('O coordenador de execuções já foi descartado.')
    if (this.active) {
      throw new Error('Já existe uma execução em andamento. Cancele-a antes de iniciar outra.')
    }
    const active: ActiveExecution = {
      runId: this.createRunId(),
      executionId: executionId ?? null,
      conversationId,
      workspace,
      mode,
      content: '',
      diff: '',
      files: [],
      plan: [],
      planExplanation: '',
      finishing: false,
      cancelRequested: false,
      sequence: 0,
      lifecycle: null,
      pendingApprovals: new Map(),
      cancel: async () => {
        active.cancelRequested = true
      },
    }
    this.active = active
    this.applyLifecycle(active, { type: 'run.started', workspace, mode, ...(executionId ? { executionId } : {}) })
    this.pushStatusForRun(active, 'planning')
    return active
  }

  private pushEvent(method: string, params: Record<string, unknown>, active: ActiveExecution) {
    const sequence = this.nextSequence(active)
    const timestamp = this.now().toISOString()
    if (!this.win.isDestroyed()) {
      this.win.webContents.send(IPC_CHANNELS.ai.event, {
        method,
        runId: active.runId,
        ...(active.executionId ? { executionId: active.executionId } : {}),
        sequence,
        timestamp,
        params: {
          ...params,
          conversationId: active.conversationId,
          runId: active.runId,
          ...(active.executionId ? { executionId: active.executionId } : {}),
          sequence,
          timestamp,
        },
      })
    }
  }

  private forwardEvent(method: string, params: Record<string, unknown>, conversationId: string, runId: string) {
    const active = this.active
    if (!active || active.runId !== runId || active.conversationId !== conversationId) return
    if (active.finishing || (active.lifecycle?.state === 'cancelling' && method !== 'turn/completed')) return
    if (method === 'item/agentMessage/delta') active.content = `${active.content}${String(params.delta ?? '')}`.slice(0, PERSISTENCE_LIMITS.assistantCharacters)
    if (method === 'turn/diff/updated') active.diff = String(params.diff ?? '').slice(-PERSISTENCE_LIMITS.metadataCharacters)
    if (method === 'turn/plan/updated') {
      active.plan = Array.isArray(params.plan) ? params.plan.slice(-100) : []
      active.planExplanation = String(params.explanation ?? '').slice(-20_000)
    }
    active.files = [...new Set([...active.files, ...eventFiles(method, params)])].slice(-300)
    if (method === 'turn/completed') {
      void this.persistAndComplete(active, params)
      return
    }
    this.pushEvent(method, params, active)
  }

  private async persistAndComplete(active: ActiveExecution, params: Record<string, unknown>) {
    if (this.active !== active || active.finishing) return
    active.finishing = true
    const rawTurn = params.turn && typeof params.turn === 'object' ? params.turn as Record<string, unknown> : {}
    const outcome = turnOutcome(rawTurn)
    params = { ...params, turn: { ...rawTurn, ...outcome } }
    try {
      const persisted = await this.finalizeTurn({
        ...(active.executionId ? { executionId: active.executionId } : {}),
        conversationId: active.conversationId,
        workspace: active.workspace,
        mode: active.mode,
        outcome: outcome.status,
        content: active.content,
        diff: active.diff,
        files: active.files,
        plan: active.plan,
        planExplanation: active.planExplanation,
      })
      this.applyCompletionLifecycle(active, params)
      this.pushEvent('turn/completed', { ...params, persistedMessage: persisted.message, persistenceWarning: persisted.warning }, active)
    } catch (error) {
      const warning = `A resposta não pôde ser persistida no processo principal: ${error instanceof Error ? error.message : String(error)}`
      this.logger.error('persistence', warning, error)
      this.applyCompletionLifecycle(active, params)
      this.pushEvent('turn/completed', { ...params, persistenceWarning: warning }, active)
    } finally {
      if (this.active === active) this.active = null
    }
  }

  private pushExecutionStatus(active: ActiveExecution, status: AgentState, error?: string, cancelled = false) {
    if (!this.applyStatusLifecycle(active, status, error, cancelled)) return
    this.pushStatusForRun(active, status, error)
  }

  private pushStatusForRun(active: ActiveExecution, status: AgentState, error?: string) {
    const sequence = this.nextSequence(active)
    const timestamp = this.now().toISOString()
    if (!this.win.isDestroyed()) {
      this.win.webContents.send(IPC_CHANNELS.ai.status, {
        status,
        conversationId: active.conversationId,
        runId: active.runId,
        ...(active.executionId ? { executionId: active.executionId } : {}),
        sequence,
        timestamp,
        ...(error ? { error } : {}),
      })
    }
  }

  private applyStatusLifecycle(active: ActiveExecution, status: AgentState, error?: string, cancelled = false) {
    if (status === 'planning' || status === 'running' || status === 'waiting-approval' || status === 'cancelling') {
      return this.applyLifecycle(active, { type: 'run.stateChanged', state: status })
    }
    if (status === 'completed') return this.applyLifecycle(active, { type: 'run.completed' })
    if (status === 'failed') return this.applyLifecycle(active, { type: 'run.failed', error: error ?? 'A execução do agente falhou.' })
    if (status === 'ready' && cancelled) return this.applyLifecycle(active, { type: 'run.cancelled', reason: error })
    return true
  }

  private applyLifecycle(active: ActiveExecution, details: AgentLifecycleDetails) {
    const event = {
      ...details,
      runId: active.runId,
      conversationId: active.conversationId,
      ...(active.executionId ? { executionId: active.executionId } : {}),
      sequence: this.nextSequence(active),
      timestamp: this.now().toISOString(),
    } as AgentLifecycleEvent
    const transition = reduceAgentLifecycle(active.lifecycle, event)
    if (!transition.accepted) {
      this.logger.warn('ai', 'Transição do lifecycle do agente ignorada.', {
        runId: active.runId,
        conversationId: active.conversationId,
        current: active.lifecycle?.state,
        event: event.type,
        reason: transition.reason,
      })
      return false
    }
    active.lifecycle = transition.state
    if (active.executionId) this.persistExecutionState(active.executionId, transition.state)
    return true
  }

  private isCurrentOrFinished(active: ActiveExecution) {
    return this.active === active
  }

  private applyCompletionLifecycle(active: ActiveExecution, params: Record<string, unknown>) {
    const turn = params.turn as Record<string, unknown> | undefined
    let accepted = false
    if (turn?.status === 'cancelled') {
      accepted = this.applyLifecycle(active, {
        type: 'run.cancelled',
        reason: 'Execução cancelada pelo usuário.',
      })
    } else if (turn?.status === 'failed' || turn?.error) {
      const error = turn.error as Record<string, unknown> | undefined
      accepted = this.applyLifecycle(active, {
        type: 'run.failed',
        error: typeof error?.message === 'string' ? error.message : 'A execução do agente falhou.',
      })
    } else {
      accepted = this.applyLifecycle(active, { type: 'run.completed' })
    }
    if (accepted) {
      if (turn?.status === 'cancelled') this.pushStatusForRun(active, 'ready')
      else if (turn?.status === 'failed' || turn?.error) this.pushStatusForRun(active, 'failed')
      else this.pushStatusForRun(active, 'completed')
    }
  }

  private nextSequence(active: ActiveExecution) {
    active.sequence += 1
    return active.sequence
  }

}

function eventFiles(method: string, params: Record<string, unknown>) {
  if (method === 'fs/changed' && Array.isArray(params.changedPaths)) return params.changedPaths.map(String)
  if (method !== 'item/completed') return []
  const item = params.item as Record<string, unknown> | undefined
  if (!item || !Array.isArray(item.changes)) return []
  return item.changes.flatMap((change) => {
    const filePath = change && typeof change === 'object' ? (change as Record<string, unknown>).path : undefined
    return typeof filePath === 'string' ? [filePath] : []
  })
}
