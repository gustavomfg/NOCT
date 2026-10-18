import type {
  ExecutionEventPayload,
  NormalizedFinishReason,
  NormalizedProviderError,
} from './execution'
import type { AgentToolCall, AgentToolDefinition, AgentTranscriptEntry } from './agentTools'
import type { ModelDescriptor } from './model'
import type { NormalizedTask } from './task'

export type ProviderStreamPayload = Extract<
  ExecutionEventPayload,
  { type: 'message.delta' | 'usage.updated' }
>

export interface ProviderExecutionRequest {
  executionId: string
  task: NormalizedTask
  model: ModelDescriptor
  /** Tools the model may call. Omitted for plain chat completions. */
  tools?: AgentToolDefinition[]
  /** Assistant tool calls and tool results produced after the user intent. */
  transcript?: AgentTranscriptEntry[]
}

export interface ProviderExecutionControl {
  signal: AbortSignal
  emit(payload: ProviderStreamPayload): void
}

export interface ProviderExecutionResult {
  finishReason: Extract<NormalizedFinishReason, 'stop' | 'length' | 'tool-calls' | 'unknown'>
  toolCalls?: AgentToolCall[]
}

export interface ExecutionOutcome {
  executionId: string
  status: 'completed' | 'failed' | 'cancelled'
  model: ModelDescriptor
  finishReason?: ProviderExecutionResult['finishReason']
  error?: NormalizedProviderError
}
