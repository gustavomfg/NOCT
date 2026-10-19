import { useEffect, useMemo, useState } from 'react'
import {
  Activity,
  ArrowLeft,
  Atom,
  Brain,
  Check,
  ChevronDown,
  Cpu,
  Gem,
  Globe,
  Laptop,
  LoaderCircle,
  Network,
  Plus,
  Search,
  Server,
  Trash2,
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
import { errorMessage } from '../../shared/format'
import { useI18n } from '../../shared/i18n'

type Step = 'list' | 'service' | 'auth' | 'model'

interface ServicePreset {
  id: ProviderCatalogId | 'other'
  name: string
  icon: LucideIcon
  baseUrl: string
  authType: 'api-key' | 'local'
}

const catalogIcons: Record<ProviderCatalogId, LucideIcon> = {
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
  deepseek: 'sk-…',
  openrouter: 'sk-or-v1-…',
  gemini: 'AIza…',
  groq: 'gsk_…',
  xai: 'xai-…',
  mistral: '…',
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
]

const MODEL_FILTER_THRESHOLD = 8

interface AIConnectionPageProps {
  workspaceId: string
  onNotify(message: string): void
}

export function AIConnectionPage({ workspaceId, onNotify }: AIConnectionPageProps) {
  const { t } = useI18n()
  const [services, setServices] = useState<ProviderConfigurationSummary[]>([])
  const [catalog, setCatalog] = useState<ModelDescriptor[]>([])
  const [step, setStep] = useState<Step>('list')
  const [selectedPreset, setSelectedPreset] = useState<ServicePreset | null>(null)
  const [modelProvider, setModelProvider] = useState<{ id: string; name: string } | null>(null)
  const [credential, setCredential] = useState('')
  const [customUrl, setCustomUrl] = useState('')
  const [models, setModels] = useState<ModelDescriptor[]>([])
  const [modelFilter, setModelFilter] = useState('')
  const [selectedModel, setSelectedModel] = useState<ModelReference | null>(null)
  const [connecting, setConnecting] = useState(false)
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
    setShowAdvanced(false)
    setError(null)
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
        providerType: 'chat-completions',
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

  return <div className="ai-page">
    {error && <div className="provider-feedback error" role="alert">{error}</div>}

    {step === 'list' && <>
      <div className="ai-list-header">
        <h4 className="ai-list-heading">{t('ai.chooseYourAi')}</h4>
        <p className="ai-list-sub">{t('ai.chooseYourAiHint')}</p>
      </div>

      <section className={`ai-active ${boundService ? 'ready' : 'empty'}`} aria-live="polite" aria-label={t('ai.activeAi')}>
        <span className="ai-active-mark">{boundService ? <Check size={16}/> : <Cpu size={16}/>}</span>
        <span className="ai-active-copy">
          <small>{t('ai.activeAi')}</small>
          <strong>{boundService ? boundService.displayName : t('ai.noActiveAi')}</strong>
          <span>{boundService ? (boundModel?.displayName ?? defaultBinding?.modelId) : workspaceId ? t('ai.noActiveAiHint') : t('ai.selectWorkspace')}</span>
        </span>
      </section>

      {services.length > 0 && <div className="ai-list-connections">
        {services.map((service) => {
          const diagnostic = providerDiagnostics[service.id]
          const isActive = boundService?.id === service.id
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
                ><Cpu size={13}/></button>
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
        selectedPreset.authType === 'local'
          ? t('ai.connectLocal', { service: presetName(selectedPreset, t) })
          : selectedPreset.id === 'other'
            ? t('ai.enterKeyAndUrl')
            : t('ai.pasteProviderKey', { service: presetName(selectedPreset, t) })
      }</p>

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

      <button className="ai-connect-btn" disabled={connecting} onClick={() => void connect()}>
        {connecting ? <><LoaderCircle className="spin" size={15}/> {t('ai.connecting')}</> : t('ai.connect')}
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
  return preset.id === 'other' ? t('ai.other') : preset.name
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
