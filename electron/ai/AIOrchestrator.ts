import { randomUUID } from 'node:crypto'
import { AGENT_TOOL_LIMITS, type AgentToolCall, type AgentToolDefinition, type AgentTranscriptEntry } from '../../shared/ai/agentTools'
import type { WorkspaceModelBindings } from '../../shared/ai/bindings'
import type { NormalizedExecutionEvent, NormalizedProviderError } from '../../shared/ai/execution'
import type {
  ExecutionOutcome,
  ProviderExecutionResult,
} from '../../shared/ai/providerExecution'
import {
  providerExecutionRequestSchema,
  providerExecutionResultSchema,
  providerStreamPayloadSchema,
} from '../../shared/ai/providerExecutionSchemas'
import type { NormalizedTask } from '../../shared/ai/task'
import { normalizedTaskSchema } from '../../shared/ai/taskSchemas'
import { ExecutionEventStream } from './ExecutionEventStream'
import { ModelResolver } from './ModelResolver'
import { ProviderExecutionError } from './ProviderExecutionError'
import { ProviderRegistry } from './ProviderRegistry'

/** Tools offered to the model. Execution stays in the main process. */
export interface AgentToolRuntime {
  definitions: AgentToolDefinition[]
  /** Returns the tool result shown to the model. Expected failures are results, not throws. */
  execute(call: AgentToolCall, signal: AbortSignal): Promise<string>
}

const OMITTED_TOOL_RESULT = '[resultado anterior omitido para respeitar o limite de contexto]'

export interface ExecutionHandle {
  executionId: string
  completion: Promise<ExecutionOutcome>
  cancel(reason?: string): boolean
}

export class AIOrchestrator {
  constructor(
    private readonly resolver: ModelResolver,
    private readonly providers: ProviderRegistry,
    private readonly createId: () => string = randomUUID,
    private readonly now: () => Date = () => new Date(),
  ) {}

  async start(
    taskInput: unknown,
    bindings: WorkspaceModelBindings,
    onEvent?: (event: NormalizedExecutionEvent) => void,
    tools?: AgentToolRuntime,
  ): Promise<ExecutionHandle> {
    const task = normalizedTaskSchema.parse(taskInput) as NormalizedTask
    const resolved = await this.resolver.resolve(task, bindings)
    const adapter = this.providers.resolve(resolved.model.providerId)
    const executionId = this.createId()
    const controller = new AbortController()
    const stream = new ExecutionEventStream(executionId, onEvent, this.now)
    stream.emit({
      type: 'execution.started',
      providerId: resolved.model.providerId,
      modelId: resolved.model.modelId,
    })

    let settled = false
    let cancellationReason = 'Execução cancelada pelo usuário.'
    const completion = this.run(
      executionId,
      task,
      resolved.model,
      adapter,
      controller.signal,
      stream,
      () => cancellationReason,
      tools,
    ).finally(() => {
      settled = true
    })

    return {
      executionId,
      completion,
      cancel(reason = cancellationReason) {
        if (settled || controller.signal.aborted) return false
        cancellationReason = reason
        controller.abort()
        return true
      },
    }
  }

  private async run(
    executionId: string,
    task: NormalizedTask,
    model: ExecutionOutcome['model'],
    adapter: ReturnType<ProviderRegistry['resolve']>,
    signal: AbortSignal,
    stream: ExecutionEventStream,
    cancellationReason: () => string,
    tools?: AgentToolRuntime,
  ): Promise<ExecutionOutcome> {
    const transcript: AgentTranscriptEntry[] = []
    let separatorPending = false
    let toolsOffered = Boolean(tools)
    try {
      for (let step = 1; ; step += 1) {
        const request = providerExecutionRequestSchema.parse({
          executionId,
          task,
          model,
          ...(tools && toolsOffered ? { tools: tools.definitions, transcript } : {}),
        })
        let stepContent = ''
        const execute = () => adapter.execute(request, {
          signal,
          emit(payload) {
            if (stream.status === 'terminal') return
            const normalized = providerStreamPayloadSchema.parse(payload)
            if (normalized.type === 'message.delta') {
              stepContent += normalized.delta
              if (separatorPending) {
                separatorPending = false
                stream.emit({ ...normalized, delta: `\n\n${normalized.delta}` })
                return
              }
            }
            stream.emit(normalized)
          },
        })
        let rawResult: Awaited<ReturnType<typeof execute>>
        try {
          rawResult = await execute()
        } catch (error) {
          // Read-only modes stay usable with chat-only models: answer from the selected context.
          if (!(error instanceof ProviderExecutionError) || !error.toolsRejected || step !== 1 || task.mode === 'build' || signal.aborted) throw error
          toolsOffered = false
          stream.emit({
            type: 'message.delta',
            messageId: `${executionId}:assistant`,
            delta: '_O modelo selecionado não aceitou ferramentas; esta resposta usa apenas o contexto selecionado pelo Nocturne._\n\n',
          })
          continue
        }
        if (signal.aborted) return cancelledOutcome(executionId, model, stream, cancellationReason())

        const result = providerExecutionResultSchema.parse(rawResult) as ProviderExecutionResult
        const toolCalls = result.toolCalls ?? []
        if (!tools || !toolsOffered || result.finishReason !== 'tool-calls' || toolCalls.length === 0) {
          return completedOutcome(executionId, model, stream, result.finishReason === 'tool-calls' ? 'unknown' : result.finishReason)
        }
        if (step >= AGENT_TOOL_LIMITS.maxSteps || transcript.length + toolCalls.length + 1 > AGENT_TOOL_LIMITS.transcriptEntries) {
          stream.emit({
            type: 'message.delta',
            messageId: `${executionId}:assistant`,
            delta: '\n\n_O agente atingiu o limite de etapas desta execução. Revise o progresso e envie uma nova mensagem para continuar._',
          })
          return completedOutcome(executionId, model, stream, 'length')
        }

        transcript.push({
          role: 'assistant',
          content: stepContent.slice(0, AGENT_TOOL_LIMITS.toolResultCharacters),
          toolCalls,
        })
        if (stepContent) separatorPending = true
        for (const call of toolCalls) {
          let content: string
          try {
            content = await tools.execute(call, signal)
          } catch (error) {
            content = `Falha ao executar a ferramenta: ${error instanceof Error ? error.message : String(error)}`
          }
          if (signal.aborted) return cancelledOutcome(executionId, model, stream, cancellationReason())
          transcript.push({
            role: 'tool',
            toolCallId: call.id,
            content: truncateToolResult(content),
          })
        }
        compactTranscript(transcript)
      }
    } catch (error) {
      if (signal.aborted) return cancelledOutcome(executionId, model, stream, cancellationReason())
      const normalized = normalizeExecutionError(error)
      stream.emit({ type: 'execution.failed', error: normalized })
      return {
        executionId,
        status: 'failed',
        model,
        error: normalized,
      }
    }
  }
}

function completedOutcome(
  executionId: string,
  model: ExecutionOutcome['model'],
  stream: ExecutionEventStream,
  finishReason: Exclude<ProviderExecutionResult['finishReason'], 'tool-calls'>,
): ExecutionOutcome {
  stream.emit({ type: 'execution.completed', finishReason })
  return { executionId, status: 'completed', model, finishReason }
}

function truncateToolResult(content: string) {
  const limit = AGENT_TOOL_LIMITS.toolResultCharacters
  if (content.length <= limit) return content
  const notice = '\n[saída truncada pelo Nocturne]'
  return `${content.slice(0, limit - notice.length)}${notice}`
}

/** Keeps the replayed transcript bounded by dropping the oldest tool outputs first. */
function compactTranscript(transcript: AgentTranscriptEntry[]) {
  const budget = Math.floor(AGENT_TOOL_LIMITS.transcriptCharacters * 0.8)
  let total = transcript.reduce((sum, entry) => sum + entry.content.length
    + (entry.role === 'assistant' ? entry.toolCalls.reduce((calls, call) => calls + call.arguments.length, 0) : 0), 0)
  for (let index = 0; index < transcript.length && total > budget; index += 1) {
    const entry = transcript[index]
    if (entry.role !== 'tool' || entry.content === OMITTED_TOOL_RESULT) continue
    total -= entry.content.length - OMITTED_TOOL_RESULT.length
    transcript[index] = { ...entry, content: OMITTED_TOOL_RESULT }
  }
}

function cancelledOutcome(
  executionId: string,
  model: ExecutionOutcome['model'],
  stream: ExecutionEventStream,
  reason: string,
): ExecutionOutcome {
  stream.emit({ type: 'execution.cancelled', reason })
  return { executionId, status: 'cancelled', model }
}

function normalizeExecutionError(error: unknown): NormalizedProviderError {
  if (error instanceof ProviderExecutionError) return error.normalized
  return {
    code: 'invalid-response',
    message: 'O Provider retornou uma resposta inválida.',
    retryable: false,
  }
}
