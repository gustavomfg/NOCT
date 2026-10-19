import { lookup } from 'node:dns/promises'
import { request as httpsRequest } from 'node:https'
import type { LookupFunction } from 'node:net'
import { Readable } from 'node:stream'
import { EMBEDDING_LIMITS, type EmbeddingExecutionControl, type EmbeddingRequest, type EmbeddingResult } from '../../../../shared/ai/embedding'
import type { ModelDescriptor } from '../../../../shared/ai/model'
import type { ProviderAvailability, ProviderDefinition } from '../../../../shared/ai/provider'
import type {
  ProviderExecutionControl,
  ProviderExecutionRequest,
  ProviderExecutionResult,
} from '../../../../shared/ai/providerExecution'
import { modelDescriptorSchema } from '../../../../shared/ai/modelSchemas'
import { ProviderExecutionError } from '../../ProviderExecutionError'
import type { ProviderAdapter } from '../../ProviderRegistry'
import {
  CHAT_COMPLETIONS_LIMITS,
  isPublicRemoteAddress,
  parseChatCompletionsConfig,
  providerEndpoint,
  type ChatCompletionsConfig,
} from './config'
import { buildChatCompletionsRequest } from './request'
import { errorFromPayload, providerError } from './errors'
import {
  consumeChatCompletionsStream,
  ChatCompletionsProtocolError,
  readBoundedJson,
} from './stream'

export interface ChatCompletionsDependencies {
  config: unknown
  models: readonly unknown[]
  resolveCredential: () => string | undefined | Promise<string | undefined>
  fetch?: typeof fetch
}

interface ResolvedProviderAddress {
  address: string
  family: 4 | 6
}

type ProviderHostResolver = (
  hostname: string,
) => Promise<readonly ResolvedProviderAddress[]>

type PinnedProviderRequest = (
  url: URL,
  init: RequestInit | undefined,
  resolved: readonly ResolvedProviderAddress[],
) => Promise<Response>

export class ChatCompletionsProviderAdapter implements ProviderAdapter {
  readonly definition: ProviderDefinition
  private readonly config: ChatCompletionsConfig
  private readonly models: ModelDescriptor[]
  private readonly request: typeof fetch

  constructor(private readonly dependencies: ChatCompletionsDependencies) {
    this.config = parseChatCompletionsConfig(dependencies.config)
    this.definition = {
      id: this.config.id,
      displayName: this.config.displayName,
      source: this.config.source,
      protocol: 'Chat Completions',
      version: 'v1',
      capabilities: {
        modelDiscovery: true,
        embeddings: true,
        streaming: true,
        toolCalling: true,
        cancellation: true,
        authentication: this.config.requiresAuthentication ? 'required' : 'none',
      },
      limitations: {
        requestTimeoutMs: { minimum: 1_000, maximum: 120_000 },
        notes: ['O agente usa function calling Chat Completions; o Build exige um modelo que suporte ferramentas.'],
      },
    }
    this.models = dependencies.models.map((model) => {
      const parsed = modelDescriptorSchema.parse(model) as ModelDescriptor
      if (parsed.providerId !== this.config.id || parsed.source !== this.config.source) {
        throw new Error('O catálogo Chat Completions não pertence à configuração.')
      }
      return cloneModel(parsed)
    })
    this.request = dependencies.fetch ?? (
      this.config.source === 'remote'
        ? createRemoteProviderTransport()
        : globalThis.fetch
    )
  }

  async getAvailability(): Promise<ProviderAvailability> {
    if (!this.config.enabled) return { status: 'disabled' }
    try {
      const catalog = await this.requestModelCatalog()
      const discovered = new Set(catalog.map(({ modelId }) => modelId))
      if (this.models.some((model) => !discovered.has(model.modelId))) {
        return {
          status: 'degraded',
          message: 'Um ou mais modelos configurados não estão disponíveis.',
        }
      }
      return { status: 'available' }
    } catch (error) {
      if (error instanceof ProviderExecutionError) {
        return availabilityFromError(error)
      }
      return {
        status: 'offline',
        message: 'Não foi possível acessar o endpoint do Provider.',
      }
    }
  }

  async listModels(): Promise<ModelDescriptor[]> {
    return this.requestModelCatalog()
  }

  async execute(
    execution: ProviderExecutionRequest,
    control: ProviderExecutionControl,
  ): Promise<ProviderExecutionResult> {
    if (!this.config.enabled) {
      throw providerError('provider-unavailable', 'O Provider está desabilitado.', false)
    }
    if (execution.model.providerId !== this.config.id) {
      throw providerError(
        'model-unavailable',
        'O modelo selecionado não pertence a este Provider.',
        false,
      )
    }

    let credential: string | undefined
    try {
      credential = await this.resolveCredential()
    } catch {
      throw providerError(
        'authentication-failed',
        'A credencial do Provider não está disponível.',
        false,
      )
    }
    if (control.signal.aborted) throw cancelledError()

    const request = createRequestControl(control.signal, this.config.timeoutMs)
    try {
      const response = await this.request(
        providerEndpoint(this.config, 'chat/completions'),
        {
          method: 'POST',
          headers: requestHeaders(credential),
          body: JSON.stringify(buildChatCompletionsRequest(
            execution.task,
            execution.model.modelId,
            execution.tools,
            execution.transcript,
          )),
          redirect: 'error',
          signal: request.signal,
        },
      )
      if (!response.ok) throw await errorFromResponse(response, Boolean(execution.tools?.length))
      return await consumeChatCompletionsStream(
        response,
        execution.executionId,
        control,
        { toolsOffered: Boolean(execution.tools?.length) },
      )
    } catch (error) {
      if (error instanceof ProviderExecutionError) throw error
      if (control.signal.aborted) throw cancelledError()
      if (request.didTimeout()) {
        throw providerError('timeout', 'O Provider excedeu o tempo permitido.', true)
      }
      if (error instanceof ChatCompletionsProtocolError) {
        throw providerError(
          'invalid-response',
          'O Provider retornou uma resposta inválida.',
          false,
        )
      }
      throw providerError(
        'provider-unavailable',
        'Não foi possível concluir a chamada ao Provider.',
        true,
      )
    } finally {
      request.dispose()
    }
  }

  async embed(
    embedding: EmbeddingRequest,
    control: EmbeddingExecutionControl,
  ): Promise<EmbeddingResult> {
    if (!this.config.enabled) throw providerError('provider-unavailable', 'O Provider está desabilitado.', false)
    if (embedding.model.providerId !== this.config.id) throw providerError('model-unavailable', 'O modelo selecionado não pertence a este Provider.', false)
    if (!embedding.model.capabilities.includes('embeddings')) throw providerError('model-unavailable', 'O modelo selecionado não oferece embeddings.', false)
    validateEmbeddingInputs(embedding)

    let credential: string | undefined
    try {
      credential = await this.resolveCredential()
    } catch {
      throw providerError('authentication-failed', 'A credencial do Provider não está disponível.', false)
    }
    if (control.signal.aborted) throw cancelledError()

    const request = createRequestControl(control.signal, this.config.timeoutMs)
    try {
      const response = await this.request(
        providerEndpoint(this.config, 'embeddings'),
        {
          method: 'POST',
          headers: requestHeaders(credential),
          body: JSON.stringify({
            model: embedding.model.modelId,
            input: embedding.inputs,
            ...(embedding.dimensions ? { dimensions: embedding.dimensions } : {}),
          }),
          redirect: 'error',
          signal: request.signal,
        },
      )
      if (!response.ok) throw await errorFromResponse(response)
      return normalizeEmbeddingResponse(await readBoundedJson(response, CHAT_COMPLETIONS_LIMITS.embeddingsResponseBytes), embedding)
    } catch (error) {
      if (error instanceof ProviderExecutionError) throw error
      if (control.signal.aborted) throw cancelledError()
      if (request.didTimeout()) throw providerError('timeout', 'O Provider excedeu o tempo permitido.', true)
      if (error instanceof ChatCompletionsProtocolError) throw providerError('invalid-response', 'O Provider retornou embeddings inválidos.', false)
      throw providerError('provider-unavailable', 'Não foi possível concluir a chamada de embeddings ao Provider.', true)
    } finally {
      request.dispose()
    }
  }

  private async resolveCredential() {
    const credential = (await this.dependencies.resolveCredential())?.trim()
    if (this.config.requiresAuthentication && !credential) {
      throw new Error('Credencial ausente.')
    }
    return credential || undefined
  }

  private async requestModelCatalog(): Promise<ModelDescriptor[]> {
    let credential: string | undefined
    try {
      credential = await this.resolveCredential()
    } catch {
      throw providerError(
        'authentication-failed',
        'A credencial do Provider não está disponível.',
        false,
      )
    }

    const request = createRequestControl(undefined, this.config.timeoutMs)
    try {
      const response = await this.request(providerEndpoint(this.config, 'models'), {
        method: 'GET',
        headers: requestHeaders(credential),
        redirect: 'error',
        signal: request.signal,
      })
      if (!response.ok) throw await errorFromResponse(response)
      return normalizeModelCatalog(
        await readBoundedJson(response),
        this.config,
        this.models,
      )
    } catch (error) {
      if (error instanceof ProviderExecutionError) throw error
      if (request.didTimeout()) {
        throw providerError('timeout', 'O Provider excedeu o tempo permitido.', true)
      }
      if (error instanceof ChatCompletionsProtocolError) {
        throw providerError(
          'invalid-response',
          'O catálogo de modelos do Provider é inválido.',
          false,
        )
      }
      throw providerError(
        'provider-unavailable',
        'Não foi possível acessar o endpoint do Provider.',
        true,
      )
    } finally {
      request.dispose()
    }
  }
}

export function createRemoteProviderTransport(
  resolveHost: ProviderHostResolver = resolveProviderHost,
  requestAddress: PinnedProviderRequest = requestPinnedAddress,
): typeof fetch {
  return async (input, init) => {
    const url = requestUrl(input)
    if (url.protocol !== 'https:') {
      throw new Error('O transporte remoto exige HTTPS.')
    }
    const addresses = await resolveHost(url.hostname)
    if (addresses.length === 0 || addresses.some(({ address }) => (
      !isPublicRemoteAddress(address)
    ))) {
      throw new Error('O endpoint remoto resolveu para um endereço não público.')
    }
    // Let the socket select a reachable address from this validated DNS snapshot.
    // A single pinned address breaks providers on networks without working IPv6.
    return requestAddress(url, init, addresses)
  }
}

export async function resolveProviderHost(
  hostname: string,
): Promise<readonly ResolvedProviderAddress[]> {
  const addresses = await lookup(hostname, { all: true, verbatim: true })
  return addresses.map(({ address, family }) => {
    if (family !== 4 && family !== 6) {
      throw new Error('O endpoint remoto resolveu para uma família de rede inválida.')
    }
    return { address, family }
  })
}

export function createPinnedLookup(
  resolved: readonly ResolvedProviderAddress[],
): LookupFunction {
  return (_hostname, options, callback) => {
    // DNS callbacks must run after the TLS socket has installed error handlers.
    queueMicrotask(() => {
      if (options.all) callback(null, resolved.map((address) => ({ ...address })))
      else callback(null, resolved[0].address, resolved[0].family)
    })
  }
}

function requestUrl(input: RequestInfo | URL) {
  if (input instanceof URL) return new URL(input)
  if (typeof input === 'string') return new URL(input)
  return new URL(input.url)
}

function requestPinnedAddress(
  url: URL,
  init: RequestInit | undefined,
  resolved: readonly ResolvedProviderAddress[],
) {
  return new Promise<Response>((resolve, reject) => {
    const connectionOptions = {
      method: init?.method,
      headers: Object.fromEntries(new Headers(init?.headers).entries()),
      signal: init?.signal ?? undefined,
      agent: false,
      autoSelectFamily: true,
      lookup: createPinnedLookup(resolved),
    }
    const request = httpsRequest(url, connectionOptions, (incoming) => {
      const headers = new Headers()
      for (const [name, value] of Object.entries(incoming.headers)) {
        if (Array.isArray(value)) {
          for (const item of value) headers.append(name, item)
        } else if (value !== undefined) {
          headers.set(name, value)
        }
      }
      resolve(new Response(
        Readable.toWeb(incoming) as ReadableStream<Uint8Array>,
        {
          status: incoming.statusCode ?? 500,
          statusText: incoming.statusMessage,
          headers,
        },
      ))
    })
    request.once('error', reject)
    if (init?.body === undefined || init.body === null) {
      request.end()
    } else if (
      typeof init.body === 'string'
      || init.body instanceof Uint8Array
      || init.body instanceof ArrayBuffer
    ) {
      request.end(init.body)
    } else {
      request.destroy(new TypeError('O corpo da requisição do Provider não é suportado.'))
    }
  })
}

function requestHeaders(credential: string | undefined) {
  return {
    accept: 'application/json, text/event-stream',
    'content-type': 'application/json',
    ...(credential ? { authorization: `Bearer ${credential}` } : {}),
  }
}

function createRequestControl(parent: AbortSignal | undefined, timeoutMs: number) {
  const controller = new AbortController()
  let timedOut = false
  const onAbort = () => controller.abort()
  if (parent?.aborted) controller.abort()
  else parent?.addEventListener('abort', onAbort, { once: true })
  const timer = setTimeout(() => {
    timedOut = true
    controller.abort()
  }, timeoutMs)
  return {
    signal: controller.signal,
    didTimeout: () => timedOut,
    dispose() {
      clearTimeout(timer)
      parent?.removeEventListener('abort', onAbort)
    },
  }
}

function availabilityFromError(error: ProviderExecutionError): ProviderAvailability {
  if (error.normalized.code === 'authentication-failed') {
    return { status: 'authentication-required', message: 'A credencial foi recusada.' }
  }
  if (error.normalized.code === 'model-unavailable') {
    return { status: 'incompatible', message: 'O endpoint de modelos não foi encontrado.' }
  }
  if (error.normalized.code === 'timeout') {
    return { status: 'offline', message: 'O teste de conexão excedeu o tempo permitido.' }
  }
  return {
    status: error.normalized.code === 'provider-unavailable' ? 'offline' : 'degraded',
    message: error.normalized.message,
  }
}

async function errorFromResponse(response: Response, toolsOffered = false) {
  let body: unknown
  try {
    body = await readBoundedJson(response)
  } catch {
    // Keep HTTP classification even when an intermediary returns HTML or invalid JSON.
  }
  return errorFromPayload(body, response.status, toolsOffered)
}

function cancelledError() {
  return providerError('cancelled', 'A execução foi cancelada.', false)
}

function normalizeModelCatalog(
  value: unknown,
  config: ChatCompletionsConfig,
  knownModels: readonly ModelDescriptor[],
): ModelDescriptor[] {
  const body = asRecord(value)
  if (!body || !Array.isArray(body.data) || body.data.length > CHAT_COMPLETIONS_LIMITS.models) {
    throw new ChatCompletionsProtocolError()
  }
  const known = new Map(knownModels.map((model) => [model.modelId, model]))
  const identifiers = new Set<string>()
  return body.data.map((nativeModel) => {
    const record = asRecord(nativeModel)
    const modelId = typeof record?.id === 'string' ? record.id.trim() : ''
    if (!modelId || modelId.length > 512 || identifiers.has(modelId)) {
      throw new ChatCompletionsProtocolError()
    }
    identifiers.add(modelId)
    const current = known.get(modelId)
    if (current) return { ...cloneModel(current), availability: 'available' }
    const nativeName = typeof record?.name === 'string' ? record.name.trim() : ''
    return {
      providerId: config.id,
      modelId,
      displayName: nativeName && nativeName.length <= 500
        ? nativeName
        : modelId.slice(0, 500),
      source: config.source,
      capabilities: [],
      availability: 'available',
    }
  })
}

function cloneModel(model: ModelDescriptor): ModelDescriptor {
  return {
    ...model,
    capabilities: [...model.capabilities],
    pricing: model.pricing ? { ...model.pricing } : undefined,
  }
}

function validateEmbeddingInputs(request: EmbeddingRequest) {
  if (request.inputs.length === 0 || request.inputs.length > EMBEDDING_LIMITS.maxInputsPerRequest) throw providerError('invalid-response', 'A solicitação de embeddings excede o número permitido de entradas.', false)
  const totalCharacters = request.inputs.reduce((total, input) => total + input.length, 0)
  if (request.inputs.some((input) => input.length === 0 || input.length > EMBEDDING_LIMITS.maxInputCharacters) || totalCharacters > EMBEDDING_LIMITS.maxTotalCharacters) {
    throw providerError('invalid-response', 'A solicitação de embeddings excede o limite de conteúdo permitido.', false)
  }
  if (request.dimensions !== undefined && (!Number.isInteger(request.dimensions) || request.dimensions < 1 || request.dimensions > EMBEDDING_LIMITS.maxDimensions)) {
    throw providerError('invalid-response', 'As dimensões solicitadas para embeddings são inválidas.', false)
  }
}

function normalizeEmbeddingResponse(value: unknown, request: EmbeddingRequest): EmbeddingResult {
  const body = asRecord(value)
  if (!body || !Array.isArray(body.data) || body.data.length !== request.inputs.length) throw new ChatCompletionsProtocolError()
  const vectors = new Array<readonly number[]>(request.inputs.length)
  for (const item of body.data) {
    const record = asRecord(item)
    const index = record?.index
    const embedding = record?.embedding
    if (typeof index !== 'number' || !Number.isInteger(index) || index < 0 || index >= vectors.length || vectors[index] || !Array.isArray(embedding) || embedding.length === 0 || embedding.some((value) => typeof value !== 'number' || !Number.isFinite(value))) throw new ChatCompletionsProtocolError()
    vectors[index] = embedding
  }
  if (vectors.some((vector) => !vector)) throw new ChatCompletionsProtocolError()
  const dimensions = vectors[0]?.length ?? 0
  if (dimensions < 1 || dimensions > EMBEDDING_LIMITS.maxDimensions || vectors.some((vector) => vector.length !== dimensions)) throw new ChatCompletionsProtocolError()
  if (request.dimensions !== undefined && request.dimensions !== dimensions) throw new ChatCompletionsProtocolError()
  const usage = asRecord(body.usage)
  return {
    requestId: request.requestId,
    model: request.model,
    embeddings: vectors,
    dimensions,
    usage: usage && typeof usage.prompt_tokens === 'number'
      ? {
        inputTokens: Number.isInteger(usage.prompt_tokens) && usage.prompt_tokens >= 0 ? usage.prompt_tokens : undefined,
        totalTokens: typeof usage.total_tokens === 'number' && Number.isInteger(usage.total_tokens) && usage.total_tokens >= 0 ? usage.total_tokens : undefined,
      }
      : undefined,
  }
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined
}
