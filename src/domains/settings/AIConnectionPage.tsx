import { useEffect, useMemo, useState } from 'react'
import {
  Activity,
  ArrowLeft,
  Atom,
  Bot,
  Brain,
  Check,
  ChevronDown,
  Gem,
  Globe,
  Laptop,
  LoaderCircle,
  Network,
  Plus,
  Search,
  Server,
  Sparkles,
  Trash2,
  UserRound,
  Wind,
  Zap,
  type LucideIcon,
} from 'lucide-react'
import type { ProviderDiagnostic } from '../../../shared/ai/provider'
import type {
  ProviderConfigurationInput,
  ProviderConfigurationSummary,
} from '../../../shared/ai/providerConfiguration'
import type { ModelDescriptor, ModelReference } from '../../../shared/ai/model'
import type { WorkspaceModelBindings } from '../../../shared/ai/bindings'
import { providerCatalog, type ProviderCatalogId } from '../../../shared/ai/providerCatalog'
import { selectAiEngine, type AiEngine } from '../../../shared/ai/engine'
import type { CodexAccountStatus } from '../../../shared/types'
import type { CodexModel } from '../../../shared/codexModels'
import { errorMessage } from '../../shared/format'
import { useI18n } from '../../shared/i18n'
import type { AiSelection } from './useSettingsController'

type Step = 'list' | 'service' | 'auth' | 'model' | 'codex-model'

interface ServicePreset {
  id: ProviderCatalogId | 'other' | 'codex'
  name: string
  icon: LucideIcon
  baseUrl: string
  authType: 'account' | 'api-key' | 'local'
}

const catalogIcons: Record<ProviderCatalogId, LucideIcon> = {
  openai: Sparkles,
  gemini: Gem,
  openrouter: Network,
  deepseek: Brain,
  groq: Zap,
  mistral: Wind,
  xai: Atom,
  ollama: Laptop,
  'lm-studio': Server,
}

const keyPlaceholders: Partial<Record<ServicePreset['id'], string>> = {
  openai: 'sk-proj-…',
  deepseek: 'sk-…',
  openrouter: 'sk-or-v1-…',
  gemini: 'AIza…',
  groq: 'gsk_…',
  xai: 'xai-…',
}

const presets: ServicePreset[] = [
  ...providerCatalog.map((entry) => ({
    id: entry.id,
    name: entry.displayName,
    icon: catalogIcons[entry.id],
    baseUrl: entry.baseUrl ?? '',
    authType: entry.source === 'local' ? 'local' as const : 'api-key' as const,
  })),
  { id: 'other', name: 'Outro', icon: Globe, baseUrl: '', authType: 'api-key' },
  { id: 'codex', name: 'Conta ChatGPT', icon: UserRound, baseUrl: '', authType: 'account' },
]

const MODEL_FILTER_THRESHOLD = 8

interface AIConnectionPageProps {
  workspaceId: string
  activeEngine?: AiEngine
  codexModel: string
  onNotify(message: string): void
  onAiSelectionChange(selection: AiSelection): Promise<void>
}

export function AIConnectionPage({
  workspaceId,
  activeEngine,
  codexModel,
  onNotify,
  onAiSelectionChange,
}: AIConnectionPageProps) {
  const { t } = useI18n()
  const [services, setServices] = useState<ProviderConfigurationSummary[]>([])
  const [codexAccount, setCodexAccount] = useState<CodexAccountStatus | null>(null)
  const [codexModels, setCodexModels] = useState<CodexModel[]>([])
  const [selectedCodexModel, setSelectedCodexModel] = useState<CodexModel | null>(null)
  const [step, setStep] = useState<Step>('list')
  const [selectedPreset, setSelectedPreset] = useState<ServicePreset | null>(null)
  const [modelProvider, setModelProvider] = useState<{ id: string; name: string } | null>(null)
  const [credential, setCredential] = useState('')
  const [customUrl, setCustomUrl] = useState('')
  const [catalog, setCatalog] = useState<ModelDescriptor[]>([])
  const [models, setModels] = useState<ModelDescriptor[]>([])
  const [modelFilter, setModelFilter] = useState('')
  const [selectedModel, setSelectedModel] = useState<ModelReference | null>(null)
  const [connecting, setConnecting] = useState(false)
  const [loadingCodexModels, setLoadingCodexModels] = useState(false)
  const [saving, setSaving] = useState(false)
  const [removingId, setRemovingId] = useState<string | null>(null)
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null)
  const [diagnosingId, setDiagnosingId] = useState<string | null>(null)
  const [providerDiagnostics, setProviderDiagnostics] = useState<Record<string, ProviderDiagnostic>>({})
  const [workspaceBindings, setWorkspaceBindings] = useState<WorkspaceModelBindings | null>(null)
  const [selectedEmbeddingModel, setSelectedEmbeddingModel] = useState<ModelReference | null>(null)
  const [remoteEmbeddingAllowed, setRemoteEmbeddingAllowed] = useState(false)
  const [embeddingSaving, setEmbeddingSaving] = useState(false)
  const [showAdvanced, setShowAdvanced] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true
    void window.nocturne.providers.list()
      .then((items) => { if (active) setServices(items) })
      .catch((failure) => { if (active) setError(errorMessage(failure)) })
    // Codex is optional: its status is informative and never blocks the page.
    void window.nocturne.codex.status()
      .then((account) => { if (active) setCodexAccount(account) })
      .catch(() => undefined)
    return () => { active = false }
  }, [])

  useEffect(() => {
    let active = true
    if (!workspaceId) {
      setWorkspaceBindings(null)
      setSelectedEmbeddingModel(null)
      setRemoteEmbeddingAllowed(false)
      return () => { active = false }
    }
    void Promise.all([window.nocturne.models.list(), window.nocturne.models.bindings(workspaceId)])
      .then(([items, bindings]) => {
        if (!active) return
        setCatalog(items)
        setWorkspaceBindings(bindings)
        setSelectedEmbeddingModel(bindings?.embeddingBinding ?? null)
        setRemoteEmbeddingAllowed(bindings?.remoteEmbeddingAllowed === true)
      })
      .catch((failure) => { if (active) setError(errorMessage(failure)) })
    return () => { active = false }
  }, [workspaceId])

  const defaultBinding = workspaceBindings?.defaultBinding
  const boundService = services.find((service) => service.id === defaultBinding?.providerId && service.enabled)
  const engine = selectAiEngine({ preference: activeEngine, providerReady: Boolean(boundService), legacyCodexModel: codexModel })
  const boundModel = defaultBinding ? catalog.find((model) => model.providerId === defaultBinding.providerId && model.modelId === defaultBinding.modelId) : undefined
  const embeddingModels = catalog.filter((model) => model.capabilities.includes('embeddings') && model.availability === 'available')
  const filteredModels = useMemo(() => {
    const query = modelFilter.trim().toLowerCase()
    if (!query) return models
    return models.filter((model) => model.modelId.toLowerCase().includes(query) || model.displayName.toLowerCase().includes(query))
  }, [modelFilter, models])

  const resetWizard = () => {
    setStep('list')
    setSelectedPreset(null)
    setModelProvider(null)
    setCredential('')
    setCustomUrl('')
    setModels([])
    setModelFilter('')
    setSelectedModel(null)
    setSelectedCodexModel(null)
    setShowAdvanced(false)
    setError(null)
  }

  const openCodexModels = async () => {
    if (loadingCodexModels) return
    setStep('codex-model')
    setLoadingCodexModels(true)
    setError(null)
    try {
      const available = await window.nocturne.codex.models()
      setCodexModels(available)
      setSelectedCodexModel(
        available.find((model) => model.model === codexModel)
        ?? available.find((model) => model.isDefault)
        ?? available[0]
        ?? null,
      )
      if (available.length === 0) setError(t('ai.noAccountModels'))
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setLoadingCodexModels(false)
    }
  }

  const loadProviderModels = async (providerId: string) => {
    await window.nocturne.models.refresh(providerId)
    const items = await window.nocturne.models.list()
    setCatalog(items)
    const available = items.filter((model) => model.providerId === providerId && !model.capabilities.includes('embeddings'))
    setModels(available)
    const current = defaultBinding?.providerId === providerId
      ? available.find((model) => model.modelId === defaultBinding.modelId)
      : undefined
    const preferred = current ?? available.find((model) => model.availability === 'available')
    setSelectedModel(preferred ? { providerId, modelId: preferred.modelId } : null)
  }

  const openProviderModels = async (service: ProviderConfigurationSummary) => {
    if (connecting) return
    setModelProvider({ id: service.id, name: service.displayName })
    setModels([])
    setModelFilter('')
    setStep('model')
    setConnecting(true)
    setError(null)
    try {
      await loadProviderModels(service.id)
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setConnecting(false)
    }
  }

  const pickService = (preset: ServicePreset) => {
    setSelectedPreset(preset)
    setCustomUrl('')
    setCredential('')
    setShowAdvanced(false)
    setStep('auth')
    setError(null)
  }

  const connect = async () => {
    if (!selectedPreset || connecting) return
    setConnecting(true)
    setError(null)
    try {
      if (selectedPreset.authType === 'account') {
        const account = await window.nocturne.codex.login()
        setCodexAccount(account)
        onNotify(t('ai.connectedChatGpt'))
        setConnecting(false)
        await openCodexModels()
        return
      }
      const effectiveUrl = customUrl.trim() || selectedPreset.baseUrl
      if (selectedPreset.id === 'other' && !effectiveUrl) {
        setError(t('ai.enterServiceUrl'))
        return
      }
      // Custom endpoints may be local servers (vLLM, llama.cpp…) that need no key.
      const keyRequired = selectedPreset.authType === 'api-key' && selectedPreset.id !== 'other'
      if (keyRequired && !credential.trim()) {
        setError(t('ai.enterApiKey'))
        return
      }
      const local = selectedPreset.authType === 'local' || (selectedPreset.id === 'other' && isLoopbackUrl(effectiveUrl))
      const config: ProviderConfigurationInput = {
        providerType: 'openai-compatible',
        displayName: selectedPreset.id === 'other' ? hostLabel(effectiveUrl) : selectedPreset.name,
        source: local ? 'local' : 'remote',
        baseUrl: effectiveUrl,
        enabled: true,
        requiresAuthentication: keyRequired || Boolean(credential.trim()),
        timeoutMs: local ? 120_000 : 60_000,
      }
      const saved = await window.nocturne.providers.create(config, credential.trim() || undefined)
      setServices((current) => [saved, ...current.filter((item) => item.id !== saved.id)])
      setModelProvider({ id: saved.id, name: saved.displayName })
      setModels([])
      setModelFilter('')
      setStep('model')
      await loadProviderModels(saved.id)
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setConnecting(false)
    }
  }

  const disconnectCodex = async () => {
    if (connecting) return
    setConnecting(true)
    setError(null)
    try {
      setCodexAccount(await window.nocturne.codex.logout())
      if (engine === 'codex') await onAiSelectionChange({ aiEngine: 'provider' })
      onNotify(t('ai.disconnectedChatGpt'))
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setConnecting(false)
    }
  }

  const saveAndBind = async () => {
    if (!modelProvider || !selectedModel || saving) return
    if (!workspaceId) {
      setError(t('ai.selectWorkspace'))
      return
    }
    setSaving(true)
    setError(null)
    try {
      const nextBindings: WorkspaceModelBindings = {
        ...(workspaceBindings ?? { workspaceId }),
        workspaceId,
        defaultBinding: selectedModel,
      }
      const saved = await window.nocturne.models.setBindings(nextBindings)
      setWorkspaceBindings(saved)
      await onAiSelectionChange({ aiEngine: 'provider' })
      onNotify(t('ai.usingModel', { model: selectedModel.modelId }))
      resetWizard()
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setSaving(false)
    }
  }

  const saveEmbeddingBinding = async () => {
    if (!workspaceId || embeddingSaving) return
    setEmbeddingSaving(true)
    setError(null)
    try {
      const selectedEmbeddingDescriptor = selectedEmbeddingModel
        ? embeddingModels.find((model) => model.providerId === selectedEmbeddingModel.providerId && model.modelId === selectedEmbeddingModel.modelId)
        : undefined
      const nextBindings: WorkspaceModelBindings = {
        workspaceId,
        ...(workspaceBindings?.defaultBinding ? { defaultBinding: workspaceBindings.defaultBinding } : {}),
        ...(selectedEmbeddingModel && selectedEmbeddingDescriptor ? {
          embeddingBinding: selectedEmbeddingModel,
          remoteEmbeddingAllowed: remoteEmbeddingAllowed && selectedEmbeddingDescriptor.source === 'remote',
        } : { remoteEmbeddingAllowed: false }),
      }
      const saved = await window.nocturne.models.setBindings(nextBindings)
      setWorkspaceBindings(saved)
      setRemoteEmbeddingAllowed(saved.remoteEmbeddingAllowed === true)
      onNotify(t('ai.embeddingSaved'))
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setEmbeddingSaving(false)
    }
  }

  const saveCodexModel = async () => {
    if (!selectedCodexModel || saving) return
    setSaving(true)
    setError(null)
    try {
      await onAiSelectionChange({ aiEngine: 'codex', model: selectedCodexModel.model })
      onNotify(t('ai.usingCodexModel', { model: selectedCodexModel.displayName }))
      resetWizard()
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setSaving(false)
    }
  }

  const remove = async (id: string) => {
    if (confirmRemove !== id) { setConfirmRemove(id); return }
    setRemovingId(id)
    setError(null)
    try {
      await window.nocturne.providers.remove(id)
      setServices((current) => current.filter((item) => item.id !== id))
      setConfirmRemove(null)
      onNotify(t('ai.connectionRemoved'))
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setRemovingId(null)
    }
  }

  const diagnose = async (id: string) => {
    if (diagnosingId) return
    setDiagnosingId(id)
    setError(null)
    try {
      const diagnostic = await window.nocturne.providers.diagnose(id)
      setProviderDiagnostics((current) => ({ ...current, [id]: diagnostic }))
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setDiagnosingId(null)
    }
  }

  const activeSummary = engine === 'codex'
    ? { name: t('ai.chatGptAccount'), detail: codexModel ? `${codexModel} · Codex CLI` : 'Codex CLI' }
    : engine === 'provider' && boundService && defaultBinding
      ? { name: boundService.displayName, detail: boundModel?.displayName ?? defaultBinding.modelId }
      : null

  return <div className="ai-page">
    {error && <div className="provider-feedback error" role="alert">{error}</div>}

    {step === 'list' && <>
      <div className="ai-list-header">
        <h4 className="ai-list-heading">{t('ai.chooseYourAi')}</h4>
        <p className="ai-list-sub">{t('ai.chooseYourAiHint')}</p>
      </div>

      <section className={`ai-active ${activeSummary ? 'ready' : 'empty'}`} aria-live="polite" aria-label={t('ai.activeAi')}>
        <span className="ai-active-mark">{activeSummary ? <Check size={16}/> : <Bot size={16}/>}</span>
        <span className="ai-active-copy">
          <small>{t('ai.activeAi')}</small>
          <strong>{activeSummary ? activeSummary.name : t('ai.noActiveAi')}</strong>
          <span>{activeSummary ? activeSummary.detail : workspaceId ? t('ai.noActiveAiHint') : t('ai.selectWorkspace')}</span>
        </span>
      </section>

      {(codexAccount?.authenticated || services.length > 0) && <div className="ai-list-connections">
        {services.map((service) => {
          const diagnostic = providerDiagnostics[service.id]
          const isActive = engine === 'provider' && boundService?.id === service.id
          return <div key={service.id} className="ai-list-provider">
            <div className="ai-list-row">
              <div className="ai-list-row-info">
                <span className={`ai-list-dot ${diagnostic?.availability.status ?? ''}`}/>
                <span><strong>{service.displayName}</strong><small>{isActive && defaultBinding ? defaultBinding.modelId : service.baseUrl}</small></span>
                {isActive && <em className="ai-active-badge">{t('ai.activeBadge')}</em>}
              </div>
              <div className="ai-list-row-actions">
                <button
                  className="ai-list-config"
                  aria-label={t('ai.chooseModelFor', { service: service.displayName })}
                  title={t('ai.chooseModel')}
                  disabled={connecting || !service.enabled}
                  onClick={() => void openProviderModels(service)}
                ><Bot size={13}/></button>
                <button
                  className="ai-list-config"
                  aria-label={`${t('ai.diagnose')} ${service.displayName}`}
                  title={t('ai.diagnose')}
                  disabled={Boolean(diagnosingId)}
                  onClick={() => void diagnose(service.id)}
                >{diagnosingId === service.id ? <LoaderCircle className="spin" size={13}/> : <Activity size={13}/>}</button>
                <button
                  className="ai-list-remove"
                  aria-label={`${t('common.remove')} ${service.displayName}`}
                  title={t('common.remove')}
                  disabled={removingId === service.id}
                  onClick={() => void remove(service.id)}
                >{removingId === service.id ? <LoaderCircle className="spin" size={13}/> : <Trash2 size={13}/>}</button>
              </div>
            </div>
            {diagnostic && <ProviderDiagnosticSummary value={diagnostic}/>}
          </div>
        })}
        {codexAccount?.authenticated && <div className="ai-list-provider"><div className="ai-list-row">
          <div className="ai-list-row-info">
            <span className="ai-list-dot"/>
            <span><strong>{t('ai.chatGptAccount')}</strong><small>Codex CLI {codexAccount.version}{codexModel ? ` · ${codexModel}` : ''}</small></span>
            {engine === 'codex' && <em className="ai-active-badge">{t('ai.activeBadge')}</em>}
          </div>
          <div className="ai-list-row-actions">
            <button
              className="ai-list-config"
              aria-label={t('ai.chooseCodexModel')}
              title={t('ai.chooseModel')}
              disabled={loadingCodexModels || connecting}
              onClick={() => void openCodexModels()}
            >{loadingCodexModels ? <LoaderCircle className="spin" size={13}/> : <Bot size={13}/>}</button>
            <button
              className="ai-list-remove"
              aria-label={t('ai.disconnectChatGpt')}
              title={t('ai.disconnectChatGpt')}
              disabled={connecting}
              onClick={() => void disconnectCodex()}
            >{connecting ? <LoaderCircle className="spin" size={13}/> : <Trash2 size={13}/>}</button>
          </div>
        </div></div>}
      </div>}

      <button className="ai-add-btn" onClick={() => setStep('service')}>
        <Plus size={16}/> {t('ai.addConnection')}
      </button>

      {workspaceId && embeddingModels.length > 0 && <section className="ai-embedding-binding">
        <div className="ai-list-header"><h4 className="ai-list-heading">{t('ai.embeddingTitle')}</h4><p className="ai-list-sub">{t('ai.embeddingHint')}</p></div>
        <label className="ai-field-label" htmlFor="ai-embedding-model"><span>{t('ai.embeddingModel')}</span><select id="ai-embedding-model" className="ai-input" value={selectedEmbeddingModel ? `${selectedEmbeddingModel.providerId}/${selectedEmbeddingModel.modelId}` : ''} onChange={(event) => { const [providerId, ...modelParts] = event.target.value.split('/'); const modelId = modelParts.join('/'); setSelectedEmbeddingModel(providerId && modelId ? { providerId, modelId } : null) }}><option value="">{t('ai.embeddingDisabled')}</option>{embeddingModels.map((model) => <option key={`${model.providerId}/${model.modelId}`} value={`${model.providerId}/${model.modelId}`}>{model.displayName}</option>)}</select></label>
        {selectedEmbeddingModel && embeddingModels.some((model) => model.providerId === selectedEmbeddingModel.providerId && model.modelId === selectedEmbeddingModel.modelId && model.source === 'remote') && <label className="ai-embedding-consent"><input type="checkbox" checked={remoteEmbeddingAllowed} onChange={(event) => setRemoteEmbeddingAllowed(event.target.checked)}/><span>{t('ai.embeddingRemoteConsent')}</span></label>}
        <button className="ai-use-btn" disabled={embeddingSaving} onClick={() => void saveEmbeddingBinding()}>{embeddingSaving ? t('settings.saving') : t('ai.saveEmbedding')}</button>
      </section>}
    </>}

    {step === 'service' && <div className="ai-step-box">
      <div className="ai-step-top">
        <button className="ai-step-back" aria-label={t('ai.back')} onClick={() => setStep('list')}><ArrowLeft size={16}/></button>
        <div className="ai-step-copy"><strong>{t('ai.chooseAccess')}</strong><small>{t('ai.chooseAccessHint')}</small></div>
      </div>
      <div className="ai-service-list">
        {presets.map((preset) => {
          const Icon = preset.icon
          return <button key={preset.id} className="ai-service-opt" aria-label={presetName(preset, t)} aria-describedby={`ai-preset-${preset.id}`} onClick={() => pickService(preset)}>
            <span className="ai-service-mark"><Icon size={17}/></span>
            <span className="ai-service-copy">
              <span className="ai-service-name">{presetName(preset, t)}</span>
              <small id={`ai-preset-${preset.id}`}>{t(`ai.preset.${preset.id}`)}</small>
            </span>
            <ArrowLeft className="ai-service-arrow" size={14}/>
          </button>
        })}
      </div>
    </div>}

    {step === 'auth' && selectedPreset && <div className="ai-auth">
      <button type="button" className="ai-auth-back" onClick={() => setStep('service')}>
        <ArrowLeft size={14}/> {presetName(selectedPreset, t)}
      </button>
      <p className="ai-auth-desc">{
        selectedPreset.authType === 'account'
          ? t('ai.accountBrowser')
          : selectedPreset.authType === 'local'
          ? t('ai.connectLocal', { service: presetName(selectedPreset, t) })
          : selectedPreset.id === 'other'
            ? t('ai.enterKeyAndUrl')
            : t('ai.pasteProviderKey', { service: presetName(selectedPreset, t) })
      }</p>

      {selectedPreset.authType === 'account' && codexAccount && <CodexStatusSummary value={codexAccount}/>}

      {selectedPreset.authType === 'api-key' && <>
        {selectedPreset.id === 'other' && (
          <label className="ai-field-label" htmlFor="ai-custom-url">
            <span>{t('ai.serviceUrl')}</span>
            <input
              id="ai-custom-url"
              className="ai-input"
              type="url"
              value={customUrl}
              onChange={(e) => setCustomUrl(e.target.value)}
              placeholder={t('ai.customUrlExample')}
            />
          </label>
        )}
        <label className="ai-field-label" htmlFor="ai-credential">
          <span>{selectedPreset.id === 'other' ? t('ai.optionalApiKey') : t('ai.apiKey')}</span>
          <input
            id="ai-credential"
            className="ai-input"
            type="password"
            autoComplete="new-password"
            value={credential}
            onChange={(e) => setCredential(e.target.value)}
            placeholder={keyPlaceholders[selectedPreset.id] ? `Ex.: ${keyPlaceholders[selectedPreset.id]}` : t('ai.pasteApiKey')}
          />
        </label>
        {selectedPreset.id !== 'other' && (
          <div className="ai-advanced">
            <button type="button" className="ai-advanced-toggle" aria-expanded={showAdvanced} onClick={() => setShowAdvanced(!showAdvanced)}>
              <ChevronDown size={11} className={`ai-chevron${showAdvanced ? ' open' : ''}`}/>
              {t('ai.advanced')}
            </button>
            {showAdvanced && (
              <label className="ai-field-label" htmlFor="ai-custom-url-advanced">
                <span>{t('ai.customUrl')}</span>
                <input
                  id="ai-custom-url-advanced"
                  className="ai-input"
                  type="url"
                  value={customUrl}
                  onChange={(e) => setCustomUrl(e.target.value)}
                  placeholder={selectedPreset.baseUrl || t('ai.customUrlExample')}
                />
              </label>
            )}
          </div>
        )}
      </>}

      {selectedPreset.authType === 'local' && (
        <p className="ai-local-note">
          {t('ai.localRunning', { service: presetName(selectedPreset, t) })}
        </p>
      )}

      <button
        className="ai-connect-btn"
        disabled={connecting || (selectedPreset.authType === 'account' && (!codexAccount?.installed || !codexAccount.compatible || codexAccount.protocolCompatible === false))}
        onClick={() => void connect()}
      >
        {connecting
          ? <><LoaderCircle className="spin" size={15}/> {t('ai.connecting')}</>
          : selectedPreset.authType === 'account' ? t('ai.signInChatGpt') : t('ai.connect')}
      </button>
    </div>}

    {step === 'model' && modelProvider && <div className="ai-step-box">
      <div className="ai-step-top">
        <button className="ai-step-back" aria-label={t('ai.back')} onClick={() => selectedPreset ? setStep('auth') : resetWizard()}><ArrowLeft size={16}/></button>
        <div className="ai-step-copy"><strong>{t('ai.chooseModel')}</strong><small>{t('ai.chooseModelHintFor', { service: modelProvider.name })}</small></div>
      </div>
      <div className="ai-step-body">
        {connecting
          ? <div className="ai-searching"><LoaderCircle className="spin" size={20}/><span>{t('ai.searchingModels')}</span></div>
          : <>
              {models.length > MODEL_FILTER_THRESHOLD && <label className="ai-model-filter" htmlFor="ai-model-filter">
                <Search size={14} aria-hidden="true"/>
                <span className="sr-only">{t('ai.filterModels')}</span>
                <input id="ai-model-filter" className="ai-input" type="search" value={modelFilter} onChange={(event) => setModelFilter(event.target.value)} placeholder={t('ai.filterModels')}/>
              </label>}
              {models.length === 0
                ? <p className="ai-no-models">{t('ai.noModels')}</p>
                : filteredModels.length === 0
                  ? <p className="ai-no-models">{t('ai.noMatchingModels')}</p>
                  : <div className="ai-model-list" role="group" aria-label={t('ai.chooseModel')}>{filteredModels.map((m) => (
                    <button
                      key={`${m.providerId}/${m.modelId}`}
                      aria-pressed={selectedModel?.modelId === m.modelId}
                      className={`ai-model-opt ${selectedModel?.modelId === m.modelId ? 'active' : ''}`}
                      disabled={m.availability !== 'available'}
                      onClick={() => setSelectedModel({ providerId: m.providerId, modelId: m.modelId })}
                    >
                      <Check size={14} className="ai-model-check"/>
                      <span className="ai-model-name">{m.displayName}</span>
                      {m.displayName !== m.modelId && <small>{m.modelId}</small>}
                    </button>
                  ))}</div>}
              <p className="ai-local-note">{t('ai.toolsHint')}</p>
            </>}
      </div>
      <div className="ai-step-foot">
        {!workspaceId && <span className="ai-workspace-required">{t('ai.selectWorkspace')}</span>}
        <button disabled={saving || connecting || !selectedModel || !workspaceId} className="ai-use-btn" onClick={() => void saveAndBind()}>
          {saving ? t('settings.saving') : t('ai.useModel')}
        </button>
      </div>
    </div>}

    {step === 'codex-model' && <div className="ai-step-box">
      <div className="ai-step-top">
        <button className="ai-step-back" aria-label={t('ai.back')} onClick={() => resetWizard()}><ArrowLeft size={16}/></button>
        <div className="ai-step-copy"><strong>{t('ai.codexModel')}</strong><small>{t('ai.codexModelHint')}</small></div>
      </div>
      <div className="ai-step-body">
        {loadingCodexModels
          ? <div className="ai-searching"><LoaderCircle className="spin" size={20}/><span>{t('ai.searchingModels')}</span></div>
          : codexModels.length === 0
            ? <p className="ai-no-models">{t('ai.noAccountModels')}</p>
            : <div className="ai-model-list" role="group" aria-label={t('ai.codexModel')}>{codexModels.map((model) => (
                <button
                  key={model.model}
                  aria-pressed={selectedCodexModel?.model === model.model}
                  className={`ai-model-opt ${selectedCodexModel?.model === model.model ? 'active' : ''}`}
                  onClick={() => setSelectedCodexModel(model)}
                >
                  <Check size={14} className="ai-model-check"/>
                  <span className="ai-model-name">{model.displayName}</span>
                  {model.isDefault && <small>{t('ai.recommended')}</small>}
                </button>
              ))}</div>}
      </div>
      <div className="ai-step-foot">
        <button disabled={saving || !selectedCodexModel} className="ai-use-btn" onClick={() => void saveCodexModel()}>
          {saving ? t('settings.saving') : t('ai.useModel')}
        </button>
      </div>
    </div>}

    {confirmRemove && <div className="modal-backdrop" onMouseDown={() => setConfirmRemove(null)}>
      <div className="confirm-dialog" role="alertdialog" aria-modal="true" aria-label={t('ai.confirmRemoval')} onMouseDown={(event) => event.stopPropagation()}>
        <p>{t('ai.confirmRemove')}</p>
        <div className="modal-actions">
          <button onClick={() => setConfirmRemove(null)}>{t('settings.cancel')}</button>
          <button className="danger" onClick={() => void remove(confirmRemove)}>{t('common.remove')}</button>
        </div>
      </div>
    </div>}
  </div>
}

function presetName(preset: ServicePreset, t: (key: string) => string) {
  return preset.id === 'codex' ? t('ai.chatGptAccount') : preset.id === 'other' ? t('ai.other') : preset.name
}

function isLoopbackUrl(value: string) {
  try {
    const hostname = new URL(value).hostname.replace(/^\[(.*)]$/, '$1').toLowerCase()
    return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '::1'
  } catch {
    return false
  }
}

function hostLabel(value: string) {
  try {
    return new URL(value).host || value
  } catch {
    return value
  }
}

function CodexStatusSummary({ value }: { value: CodexAccountStatus }) {
  const { t } = useI18n()
  if (value.state === 'ready') return null
  const message = {
    'not-installed': t('ai.installCodex', { minimum: value.minimumVersion, recommended: value.recommendedVersion }),
    'not-authenticated': t('ai.codexNotAuthenticated'),
    incompatible: t('ai.codexIncompatible', { version: value.version ?? t('common.unknown'), minimum: value.minimumVersion }),
    'internal-error': value.error ?? t('ai.codexInternalError'),
  }[value.state]
  return <div
    className={`provider-feedback ${value.state === 'not-authenticated' ? '' : 'error'}`}
    role={value.state === 'internal-error' ? 'alert' : 'status'}
  >
    {message}
  </div>
}

function ProviderDiagnosticSummary({ value }: { value: ProviderDiagnostic }) {
  const { t } = useI18n()
  const authentication = {
    'not-required': t('ai.notRequired'),
    configured: t('ai.configured'),
    missing: t('ai.missing'),
    rejected: t('ai.rejected'),
  }[value.authentication]
  const compatibility = {
    compatible: t('ai.compatible'),
    incompatible: t('ai.incompatible'),
    unknown: t('ai.notChecked'),
  }[value.compatibility]
  return <div className="provider-diagnostic" role="status">
    <div className="provider-diagnostic-grid">
      <span><small>{t('ai.status')}</small><strong>{value.availability.status}</strong></span>
      <span><small>{t('ai.connectivity')}</small><strong>{value.connectivity === 'connected' ? t('ai.connected') : value.connectivity === 'unreachable' ? t('ai.unreachable') : t('ai.notChecked')}</strong></span>
      <span><small>{t('ai.authentication')}</small><strong>{authentication}</strong></span>
      <span><small>{t('ai.compatibility')}</small><strong>{compatibility}</strong></span>
      <span><small>{t('ai.protocol')}</small><strong>{value.definition.protocol} {value.definition.version ?? ''}</strong></span>
      <span><small>{t('ai.response')}</small><strong>{value.latencyMs} ms</strong></span>
    </div>
    <div className="provider-capabilities" aria-label={t('ai.providerCapabilities')}>
      {value.definition.capabilities.modelDiscovery && <small>{t('ai.models')}</small>}
      {value.definition.capabilities.streaming && <small>{t('ai.streaming')}</small>}
      {value.definition.capabilities.toolCalling && <small>{t('ai.tools')}</small>}
      {value.definition.capabilities.cancellation && <small>{t('ai.cancellation')}</small>}
    </div>
    {value.definition.limitations.notes.length > 0 && <p>{value.definition.limitations.notes.join(' ')}</p>}
    {value.recentErrors.length > 0 && <details><summary>{t('ai.recentErrors', { count: value.recentErrors.length })}</summary><ul>{value.recentErrors.map((error) => <li key={`${error.occurredAt}-${error.message}`}>{error.message}</li>)}</ul></details>}
  </div>
}
