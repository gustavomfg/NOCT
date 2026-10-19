import type { NormalizedErrorCode } from '../../../../shared/ai/execution'
import { ProviderExecutionError } from '../../ProviderExecutionError'

export function providerError(code: NormalizedErrorCode, message: string, retryable: boolean) {
  return new ProviderExecutionError({ code, message, retryable })
}

export function errorFromStatus(status: number) {
  if (status === 401 || status === 403) return providerError('authentication-failed', 'A autenticação do Provider foi recusada.', false)
  if (status === 402) return providerError('insufficient-credits', 'A API não possui créditos ou cota disponível. Adicione créditos na plataforma do Provider ou escolha outra conexão.', false)
  if (status === 404) return providerError('model-unavailable', 'O modelo solicitado não está disponível.', false)
  if (status === 408 || status === 504) return providerError('timeout', 'O Provider excedeu o tempo permitido.', true)
  if (status === 429) return providerError('rate-limited', 'O limite temporário do Provider foi atingido.', true)
  return providerError(
    status >= 500 ? 'provider-unavailable' : 'invalid-response',
    status >= 500 ? 'O Provider está temporariamente indisponível.' : 'O Provider recusou a solicitação.',
    status >= 500,
  )
}

/** Inspect native errors only to classify them; never expose their text or metadata. */
export function errorFromPayload(value: unknown, status: number, toolsOffered = false) {
  const body = asRecord(value)
  const error = asRecord(body?.error)
  const nativeCode = error?.code
  const code = typeof nativeCode === 'number' && Number.isInteger(nativeCode) && nativeCode >= 400 && nativeCode <= 599
    ? nativeCode : status
  const identifiers = [body?.code, body?.type, error?.code, error?.type]
  if (identifiers.some((item) => typeof item === 'string' && /insufficient[_-]?quota|billing[_-]?(?:hard[_-]?limit|not[_-]?active)|usage[_-]?limit|credit/i.test(item))) {
    return errorFromStatus(402)
  }
  const unsupportedTools = toolsOffered && (
    code === 400 || code === 422 || (
      code === 404 && typeof error?.message === 'string'
      && /support.{0,60}tool|tool.{0,60}support|function.calling/i.test(error.message)
    )
  )
  if (unsupportedTools) return new ProviderExecutionError({
    code: 'invalid-response',
    message: 'O Provider recusou a solicitação. O modelo selecionado pode não suportar ferramentas (function calling), necessárias para o Build; escolha outro modelo.',
    retryable: false,
  }, true)
  return errorFromStatus(code)
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined
}
