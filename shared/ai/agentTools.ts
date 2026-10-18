export const AGENT_TOOL_LIMITS = {
  toolNameCharacters: 64,
  toolDescriptionCharacters: 2_000,
  toolsPerRequest: 32,
  toolCallIdCharacters: 512,
  toolCallsPerStep: 16,
  toolArgumentCharacters: 200_000,
  toolResultCharacters: 40_000,
  transcriptEntries: 400,
  transcriptCharacters: 600_000,
  maxSteps: 30,
} as const

/** JSON Schema object describing a tool's arguments (OpenAI function format). */
export interface AgentToolDefinition {
  name: string
  description: string
  parameters: Record<string, unknown>
}

export interface AgentToolCall {
  id: string
  name: string
  /** Raw JSON text produced by the model. It is untrusted until parsed. */
  arguments: string
}

export type AgentTranscriptEntry =
  | {
    role: 'assistant'
    content: string
    toolCalls: AgentToolCall[]
  }
  | {
    role: 'tool'
    toolCallId: string
    content: string
  }
