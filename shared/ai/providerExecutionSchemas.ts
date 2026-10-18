import { z } from 'zod'
import { AGENT_TOOL_LIMITS } from './agentTools'
import { executionIdSchema, normalizedProviderErrorSchema } from './executionSchemas'
import { modelDescriptorSchema } from './modelSchemas'
import { normalizedTaskSchema } from './taskSchemas'

export const providerStreamPayloadSchema = z.discriminatedUnion('type', [
  z.object({
    type: z.literal('message.delta'),
    messageId: z.string().trim().min(1).max(512),
    delta: z.string().min(1).max(100_000),
  }).strict(),
  z.object({
    type: z.literal('usage.updated'),
    usage: z.object({
      inputTokens: z.number().int().nonnegative().optional(),
      cachedInputTokens: z.number().int().nonnegative().optional(),
      outputTokens: z.number().int().nonnegative().optional(),
    }).strict().refine(
      (usage) => Object.values(usage).some((value) => value !== undefined),
      'Informe ao menos um contador de uso.',
    ),
  }).strict(),
])

export const agentToolDefinitionSchema = z.object({
  name: z.string().regex(/^[a-zA-Z0-9_-]+$/).min(1).max(AGENT_TOOL_LIMITS.toolNameCharacters),
  description: z.string().trim().min(1).max(AGENT_TOOL_LIMITS.toolDescriptionCharacters),
  parameters: z.record(z.string(), z.unknown()),
}).strict()

export const agentToolCallSchema = z.object({
  id: z.string().trim().min(1).max(AGENT_TOOL_LIMITS.toolCallIdCharacters),
  name: z.string().min(1).max(AGENT_TOOL_LIMITS.toolNameCharacters),
  arguments: z.string().max(AGENT_TOOL_LIMITS.toolArgumentCharacters),
}).strict()

export const agentTranscriptEntrySchema = z.discriminatedUnion('role', [
  z.object({
    role: z.literal('assistant'),
    content: z.string().max(AGENT_TOOL_LIMITS.transcriptCharacters),
    toolCalls: z.array(agentToolCallSchema).min(1).max(AGENT_TOOL_LIMITS.toolCallsPerStep),
  }).strict(),
  z.object({
    role: z.literal('tool'),
    toolCallId: z.string().trim().min(1).max(AGENT_TOOL_LIMITS.toolCallIdCharacters),
    content: z.string().max(AGENT_TOOL_LIMITS.toolResultCharacters),
  }).strict(),
])

export const providerExecutionRequestSchema = z.object({
  executionId: executionIdSchema,
  task: normalizedTaskSchema,
  model: modelDescriptorSchema,
  tools: z.array(agentToolDefinitionSchema).max(AGENT_TOOL_LIMITS.toolsPerRequest).optional(),
  transcript: z.array(agentTranscriptEntrySchema).max(AGENT_TOOL_LIMITS.transcriptEntries).optional(),
}).strict().superRefine((request, context) => {
  const names = new Set<string>()
  for (const [index, tool] of (request.tools ?? []).entries()) {
    if (names.has(tool.name)) {
      context.addIssue({ code: 'custom', path: ['tools', index, 'name'], message: 'Ferramentas duplicadas não são permitidas.' })
    }
    names.add(tool.name)
  }
  const characters = (request.transcript ?? []).reduce((total, entry) => total + entry.content.length
    + (entry.role === 'assistant' ? entry.toolCalls.reduce((sum, call) => sum + call.arguments.length, 0) : 0), 0)
  if (characters > AGENT_TOOL_LIMITS.transcriptCharacters) {
    context.addIssue({ code: 'custom', path: ['transcript'], message: 'O histórico de ferramentas excede o limite permitido.' })
  }
})

export const providerExecutionResultSchema = z.object({
  finishReason: z.enum(['stop', 'length', 'tool-calls', 'unknown']),
  toolCalls: z.array(agentToolCallSchema).max(AGENT_TOOL_LIMITS.toolCallsPerStep).optional(),
}).strict()

export const providerExecutionErrorSchema = normalizedProviderErrorSchema
