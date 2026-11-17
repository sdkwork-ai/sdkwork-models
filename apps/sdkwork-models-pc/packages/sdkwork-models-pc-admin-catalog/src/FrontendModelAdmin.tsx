import { useEffect, useMemo, useState } from 'react';
import {
  Check,
  Download,
  Loader2,
  Plus,
  RefreshCw,
  Save,
  Search,
  Star,
  Trash2,
  X,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { ModelService, type Model } from './modelService';
import {
  loadFrontendConfigDocument,
  saveFrontendConfigDocument,
  type FrontendChannelConfig,
  type FrontendChannelFieldConfig,
  type FrontendChannelModelConfig,
  type FrontendConfigDocument,
} from './frontendConfigService';

type ChannelModelType = Model['type'];

function emptyChannel(channelCode: string): FrontendChannelConfig {
  return {
    channelCode,
    displayName: channelCode,
    displayNameI18n: {},
    description: null,
    enabled: true,
    sortOrder: 0,
    defaultCatalogKey: null,
    capabilityCodes: [],
    config: {},
    schemaVersion: '1.0.0',
    revision: 0,
    models: [],
    fields: [],
  };
}

function emptyField(fieldKey: string): FrontendChannelFieldConfig {
  return { fieldKey, visible: true, sortOrder: 0, config: {} };
}

function emptyModel(catalogKey: string, modelId: string | null = null): FrontendChannelModelConfig {
  return {
    catalogKey,
    modelId,
    visible: true,
    featured: false,
    sortOrder: 0,
    badge: null,
    config: {},
  };
}

/**
 * Map a channel's catalog capability codes to the admin model types so the
 * model picker and the "import from catalog" action offer only models that can
 * serve the surface. An empty set means "no capability binding" and offers
 * every type.
 * @param capabilityCodes - the channel's bound capability codes.
 * @returns the model types the channel accepts.
 */
function capabilityModelTypes(capabilityCodes: readonly string[]): Set<ChannelModelType> {
  const types = new Set<ChannelModelType>();
  for (const code of capabilityCodes) {
    if (['chat', 'responses', 'completion', 'tool_calling', 'json_schema', 'batch', 'realtime', 'moderation', 'computer_use', 'web_search', 'file_search', 'code_interpreter'].includes(code)) types.add('Chat');
    else if (['image_generation', 'image_editing', 'vision'].includes(code)) types.add('Image');
    else if (code === 'video_generation') types.add('Video');
    else if (['audio_input', 'audio_output', 'speech_to_text', 'text_to_speech'].includes(code)) types.add('Audio');
    else if (code === 'music_generation') types.add('Music');
    else if (code === 'sound_effect_generation') types.add('SoundEffect');
    else if (['embedding', 'rerank'].includes(code)) types.add('Embedding');
  }
  return types;
}

function catalogKeyOf(model: Pick<Model, 'vendorCode' | 'model'>): string {
  return `${model.vendorCode}/${model.model}`;
}

/** Catalog key to the int64 model id (decimal string) for persistence. */
function modelIdsByKey(models: readonly Model[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const model of models) {
    map.set(catalogKeyOf(model), model.id);
  }
  return map;
}

/**
 * Point a channel's default at a visible exposed model.
 *
 * A document saved before the backend enforced this invariant can carry a
 * default that is missing or hidden; normalize it on load so the operator can
 * change and save the surface without hitting a validation error.
 */
function normalizeDefaultCatalogKey(channel: FrontendChannelConfig): FrontendChannelConfig {
  const current = channel.defaultCatalogKey;
  const valid = current !== null
    && channel.models.some((model) => model.catalogKey === current && model.visible);
  if (valid) {
    return channel;
  }
  const fallback = channel.models.find((model) => model.visible)?.catalogKey ?? null;
  return fallback === current ? channel : { ...channel, defaultCatalogKey: fallback };
}

function normalizeDocumentDefaults(document: FrontendConfigDocument): FrontendConfigDocument {
  return {
    ...document,
    channels: document.channels.map(normalizeDefaultCatalogKey),
  };
}

/** Parse a sort-order input without letting an empty field become `NaN`. */
function parseSortOrder(value: string, fallback = 0): number {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : fallback;
}

/**
 * Merge catalog model keys into a channel without duplicating an already
 * exposed key, keeping the channel's sort order monotonic.
 * @param channel - the channel being edited.
 * @param keys - catalog keys to add.
 * @param catalogIdsByKey - catalog key to int64 model id (decimal string).
 * @returns the next channel plus how many rows were added.
 */
function mergeCatalogModels(
  channel: FrontendChannelConfig,
  keys: readonly string[],
  catalogIdsByKey: ReadonlyMap<string, string>,
): { channel: FrontendChannelConfig; added: number } {
  const existing = new Set(channel.models.map((model) => model.catalogKey));
  const baseOrder = channel.models.reduce((max, model) => Math.max(max, model.sortOrder), 0);
  const additions: FrontendChannelModelConfig[] = [];
  for (const key of keys) {
    if (existing.has(key)) continue;
    existing.add(key);
    additions.push({
      ...emptyModel(key, catalogIdsByKey.get(key) ?? null),
      sortOrder: baseOrder + (additions.length + 1) * 10,
    });
  }
  if (additions.length === 0) {
    return { channel, added: 0 };
  }
  return { channel: { ...channel, models: [...channel.models, ...additions] }, added: additions.length };
}

/**
 * Admin authoring surface for the models-owned front-end configuration.
 *
 * One tab per product surface (LLM / video / image / audio / music / …). The
 * page reads and replaces the revisioned document served by the models backend;
 * catalog facts remain in the model tables, and this page only chooses which
 * models a surface exposes and which fields it shows.
 */
export function FrontendModelAdmin() {
  const { t, i18n } = useTranslation();
  const [document, setDocument] = useState<FrontendConfigDocument | null>(null);
  const [selectedCode, setSelectedCode] = useState('');
  const [newChannelCode, setNewChannelCode] = useState('');
  const [newFieldKey, setNewFieldKey] = useState('');
  const [catalogModels, setCatalogModels] = useState<Model[] | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerSearch, setPickerSearch] = useState('');
  const [pickerSelected, setPickerSelected] = useState<string[]>([]);
  const [manualCatalogKey, setManualCatalogKey] = useState('');
  const [manualCatalogKeyError, setManualCatalogKeyError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadCatalog = async (force = false) => {
    if (!force && catalogModels !== null && catalogModels.length > 0) return;
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      const catalog = await ModelService.fetchAllModels();
      setCatalogModels(catalog);
    } catch (loadError) {
      setCatalogError(loadError instanceof Error ? loadError.message : t('admin.model.frontend.catalogLoadError'));
    } finally {
      setCatalogLoading(false);
    }
  };

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const next = normalizeDocumentDefaults(await loadFrontendConfigDocument());
      setDocument(next);
      setSelectedCode((current) => (
        next.channels.some((channel) => channel.channelCode === current)
          ? current
          : next.channels[0]?.channelCode ?? ''
      ));
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : t('admin.model.frontend.loadError'));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const selected = document?.channels.find((channel) => channel.channelCode === selectedCode);

  const catalogByKey = useMemo(() => {
    const map = new Map<string, Model>();
    for (const model of catalogModels ?? []) {
      map.set(catalogKeyOf(model), model);
    }
    return map;
  }, [catalogModels]);

  const catalogIdsByKey = useMemo(
    () => modelIdsByKey(catalogModels ?? []),
    [catalogModels],
  );

  const pickerCandidates = useMemo(() => {
    if (!selected || catalogModels === null) return [];
    const types = capabilityModelTypes(selected.capabilityCodes);
    const existing = new Set(selected.models.map((model) => model.catalogKey));
    const seen = new Set<string>();
    const query = pickerSearch.trim().toLowerCase();
    return catalogModels
      .filter((model) => types.size === 0 || types.has(model.type))
      .filter((model) => {
        const key = catalogKeyOf(model);
        if (existing.has(key) || seen.has(key)) return false;
        if (query.length > 0) {
          const matches = key.toLowerCase().includes(query)
            || model.displayName.toLowerCase().includes(query)
            || model.vendorCode.toLowerCase().includes(query)
            || model.type.toLowerCase().includes(query);
          if (!matches) return false;
        }
        seen.add(key);
        return true;
      })
      .slice(0, 300);
  }, [selected, catalogModels, pickerSearch]);

  const updateChannel = (updater: (channel: FrontendChannelConfig) => FrontendChannelConfig) => {
    setDocument((current) => {
      if (current === null) return current;
      return {
        ...current,
        channels: current.channels.map((channel) => (
          channel.channelCode === selectedCode ? updater(channel) : channel
        )),
      };
    });
  };

  const updateField = (index: number, updater: (field: FrontendChannelFieldConfig) => FrontendChannelFieldConfig) => {
    updateChannel((channel) => ({
      ...channel,
      fields: channel.fields.map((field, fieldIndex) => (fieldIndex === index ? updater(field) : field)),
    }));
  };

  const updateModel = (index: number, updater: (model: FrontendChannelModelConfig) => FrontendChannelModelConfig) => {
    updateChannel((channel) => ({
      ...channel,
      models: channel.models.map((model, modelIndex) => (modelIndex === index ? updater(model) : model)),
    }));
  };

  /**
   * Hide or show one exposed model. A hidden model cannot serve as the
   * surface's default (the app read drops it), so hiding the default moves the
   * default to the next visible model.
   */
  const setModelVisible = (index: number, visible: boolean) => {
    updateChannel((channel) => {
      const models = channel.models.map((model, modelIndex) => (
        modelIndex === index ? { ...model, visible } : model
      ));
      const hiddenKey = channel.models[index]?.catalogKey;
      const defaultCatalogKey = !visible && channel.defaultCatalogKey === hiddenKey
        ? models.find((model) => model.visible)?.catalogKey ?? null
        : channel.defaultCatalogKey;
      return { ...channel, models, defaultCatalogKey };
    });
  };

  const addChannel = () => {
    const code = newChannelCode.trim();
    if (code.length === 0) {
      setNotice(null);
      setError(t('admin.model.frontend.channelCodeRequired'));
      return;
    }
    if (!/^[a-z][a-z0-9_]{0,63}$/u.test(code)) {
      setNotice(null);
      setError(t('admin.model.frontend.channelCodeInvalid'));
      return;
    }
    if (document?.channels.some((channel) => channel.channelCode === code)) {
      setNotice(null);
      setError(t('admin.model.frontend.channelCodeExists', { code }));
      return;
    }
    setError(null);
    const nextSortOrder = (document?.channels.reduce(
      (max, channel) => Math.max(max, channel.sortOrder),
      0,
    ) ?? 0) + 10;
    setDocument((current) => current === null ? current : {
      ...current,
      channels: [...current.channels, { ...emptyChannel(code), sortOrder: nextSortOrder }],
    });
    setSelectedCode(code);
    setNewChannelCode('');
    setNotice(t('admin.model.frontend.channelAdded', { code }));
  };

  const addField = () => {
    if (!selected) return;
    const key = newFieldKey.trim();
    if (key.length === 0) {
      setNotice(null);
      setError(t('admin.model.frontend.fieldKeyRequired'));
      return;
    }
    if (selected.fields.some((field) => field.fieldKey === key)) {
      setNotice(null);
      setError(t('admin.model.frontend.fieldKeyExists', { key }));
      return;
    }
    setError(null);
    const nextSortOrder = selected.fields.reduce(
      (max, field) => Math.max(max, field.sortOrder),
      0,
    ) + 10;
    updateChannel((channel) => ({
      ...channel,
      fields: [...channel.fields, { ...emptyField(key), sortOrder: nextSortOrder }],
    }));
    setNewFieldKey('');
  };

  const addCatalogModels = (
    keys: readonly string[],
    idsByKey: ReadonlyMap<string, string> = catalogIdsByKey,
  ) => {
    if (!selected) return;
    const merged = mergeCatalogModels(selected, keys, idsByKey);
    if (merged.added === 0) {
      setNotice(t('admin.model.frontend.importEmpty'));
      return;
    }
    updateChannel((channel) => mergeCatalogModels(channel, keys, idsByKey).channel);
    setNotice(t('admin.model.frontend.imported', { count: merged.added }));
  };

  const importFromCatalog = async () => {
    if (!selected) return;
    setImporting(true);
    setError(null);
    setNotice(null);
    try {
      // An empty cached catalog means the catalog was not imported yet; retry
      // instead of reporting "no models to add" forever.
      const cachedCatalog = catalogModels !== null && catalogModels.length > 0 ? catalogModels : null;
      const catalog = cachedCatalog ?? (await ModelService.fetchAllModels());
      if (cachedCatalog === null) {
        setCatalogModels(catalog);
      }
      if (catalog.length === 0) {
        setNotice(t('admin.model.frontend.catalogEmpty'));
        return;
      }
      const types = capabilityModelTypes(selected.capabilityCodes);
      const keys = catalog
        .filter((model) => types.size === 0 || types.has(model.type))
        .map(catalogKeyOf);
      addCatalogModels(keys, modelIdsByKey(catalog));
    } catch (importError) {
      setError(importError instanceof Error ? importError.message : t('admin.model.frontend.importError'));
    } finally {
      setImporting(false);
    }
  };

  const openPicker = () => {
    if (!selected) return;
    setPickerSelected([]);
    setPickerSearch('');
    setManualCatalogKey('');
    setManualCatalogKeyError(null);
    setPickerOpen(true);
    void loadCatalog();
  };

  /**
   * Add a catalog key that is not in the imported catalog (a custom or
   * not-yet-synced model). The key format matches the backend's catalog key.
   */
  const addManualCatalogModel = () => {
    const key = manualCatalogKey.trim();
    if (!/^[A-Za-z0-9._:/-]{1,256}$/u.test(key)) {
      setManualCatalogKeyError(t('admin.model.frontend.manualModelInvalid'));
      return;
    }
    if (selected?.models.some((model) => model.catalogKey === key)) {
      setManualCatalogKeyError(t('admin.model.frontend.manualModelExists', { key }));
      return;
    }
    setManualCatalogKeyError(null);
    addCatalogModels([key], catalogIdsByKey);
    setManualCatalogKey('');
    setPickerOpen(false);
  };

  const togglePickerModel = (key: string) => {
    setPickerSelected((current) => (
      current.includes(key)
        ? current.filter((candidate) => candidate !== key)
        : [...current, key]
    ));
  };

  const confirmPicker = () => {
    addCatalogModels(pickerSelected);
    setPickerOpen(false);
    setPickerSelected([]);
  };

  const save = async () => {
    if (document === null) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const next = normalizeDocumentDefaults(await saveFrontendConfigDocument(document));
      setDocument(next);
      setNotice(t('admin.model.frontend.saved'));
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : t('admin.model.frontend.saveError'));
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-sm text-slate-500 dark:text-slate-400">
        <Loader2 className="h-4 w-4 animate-spin" />
        {t('admin.model.frontend.loading')}
      </div>
    );
  }

  const modelLabel = (catalogKey: string) => {
    const model = catalogByKey.get(catalogKey);
    return model?.displayName ?? catalogKey;
  };

  const channelLabel = (channel: FrontendChannelConfig) => {
    const language = i18n.language || 'en-US';
    const base = language.split('-')[0];
    return channel.displayNameI18n[language]
      ?? Object.entries(channel.displayNameI18n).find(([locale]) => locale.split('-')[0] === base)?.[1]
      ?? (channel.displayName || channel.channelCode);
  };

  return (
    <div className="flex h-full min-h-0 w-full flex-col overflow-hidden rounded-xl border border-slate-200 bg-slate-50 shadow-sm dark:border-white/5 dark:bg-[#121212]" data-admin-frontend-model-config>
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-slate-200 bg-white px-4 py-3 dark:border-white/10 dark:bg-[#1a1a1a]">
        <div className="min-w-0">
          <h2 className="truncate text-base font-bold text-slate-900 dark:text-white">{t('admin.model.frontend.title')}</h2>
          <p className="truncate text-xs text-slate-500 dark:text-slate-400">{t('admin.model.frontend.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          {document ? (
            <span className="rounded-full bg-slate-100 px-2 py-0.5 text-xs text-slate-500 dark:bg-white/10 dark:text-slate-400">
              {t('admin.model.frontend.revision', { revision: document.revision })}
            </span>
          ) : null}
          <button type="button" onClick={() => { void load(); void loadCatalog(true); }} className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-slate-500 hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-white/10 dark:hover:text-slate-200">
            <RefreshCw className="h-3.5 w-3.5" />
            {t('admin.model.frontend.reload')}
          </button>
          <button type="button" onClick={() => void save()} disabled={saving || document === null} className="inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-60">
            {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
            {saving ? t('admin.model.frontend.saving') : t('admin.model.frontend.save')}
          </button>
        </div>
      </div>

      {error ? <div role="alert" className="shrink-0 border-b border-rose-200 bg-rose-50 px-4 py-2 text-sm text-rose-700 dark:border-rose-500/20 dark:bg-rose-500/10 dark:text-rose-300">{error}</div> : null}
      {notice ? <div role="status" className="shrink-0 border-b border-emerald-200 bg-emerald-50 px-4 py-2 text-sm text-emerald-700 dark:border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-300">{notice}</div> : null}

      <div className="flex min-h-0 flex-1 overflow-hidden">
        <aside className="flex w-60 shrink-0 flex-col border-r border-slate-200 bg-white dark:border-white/10 dark:bg-[#1a1a1a]">
          <div className="flex items-center gap-1 border-b border-slate-200 p-2 dark:border-white/10">
            <input
              value={newChannelCode}
              onChange={(event) => setNewChannelCode(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') addChannel();
              }}
              placeholder={t('admin.model.frontend.channelCodePlaceholder')}
              aria-label={t('admin.model.frontend.addChannel')}
              className="min-w-0 flex-1 rounded-md border border-slate-200 bg-white px-2 py-1 text-xs text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white"
            />
            <button type="button" onClick={addChannel} title={t('admin.model.frontend.addChannel')} aria-label={t('admin.model.frontend.addChannel')} className="rounded-md p-1.5 text-slate-400 hover:bg-indigo-50 hover:text-indigo-600 dark:hover:bg-indigo-500/10">
              <Plus className="h-4 w-4" />
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-2">
            {(document?.channels ?? []).map((channel) => (
              <button
                key={channel.channelCode}
                type="button"
                onClick={() => {
                  setSelectedCode(channel.channelCode);
                  setPickerOpen(false);
                  setPickerSelected([]);
                }}
                className={`mb-1 flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${channel.channelCode === selectedCode ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-300' : 'text-slate-700 hover:bg-slate-50 dark:text-slate-300 dark:hover:bg-white/5'}`}
              >
                <span className="min-w-0 truncate">{channelLabel(channel)}</span>
                <span className={`shrink-0 rounded-full px-1.5 py-0.5 text-[10px] ${channel.enabled ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300' : 'bg-slate-100 text-slate-500 dark:bg-white/10 dark:text-slate-400'}`}>
                  {channel.enabled ? t('admin.model.frontend.enabled') : t('admin.model.frontend.disabled')}
                </span>
              </button>
            ))}
          </div>
        </aside>

        <section className="min-w-0 flex-1 overflow-y-auto p-4">
          {selected ? (
            <div className="flex flex-col gap-5">
              <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
                <label className="flex flex-col gap-1 text-sm">
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.displayName')}</span>
                  <input value={selected.displayName} onChange={(event) => updateChannel((channel) => ({ ...channel, displayName: event.target.value }))} className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                </label>
                <label className="flex flex-col gap-1 text-sm">
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.sortOrder')}</span>
                  <input type="number" value={selected.sortOrder} onChange={(event) => updateChannel((channel) => ({ ...channel, sortOrder: parseSortOrder(event.target.value) }))} className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                </label>
                <label className="flex flex-col gap-1 text-sm">
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.defaultModel')}</span>
                  <select
                    value={selected.defaultCatalogKey ?? ''}
                    onChange={(event) => updateChannel((channel) => ({ ...channel, defaultCatalogKey: event.target.value || null }))}
                    className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white"
                  >
                    <option value="">{t('admin.model.frontend.noDefaultModel')}</option>
                    {selected.models.filter((model) => model.visible).map((model) => (
                      <option key={model.catalogKey} value={model.catalogKey}>{modelLabel(model.catalogKey)}</option>
                    ))}
                    {selected.defaultCatalogKey && !selected.models.some((model) => model.catalogKey === selected.defaultCatalogKey && model.visible) ? (
                      <option value={selected.defaultCatalogKey}>{selected.defaultCatalogKey}</option>
                    ) : null}
                  </select>
                </label>
                <label className="flex flex-col gap-1 text-sm">
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.capabilityCodes')}</span>
                  <input value={selected.capabilityCodes.join(', ')} onChange={(event) => updateChannel((channel) => ({ ...channel, capabilityCodes: event.target.value.split(',').map((code) => code.trim()).filter((code) => code.length > 0) }))} placeholder="chat, responses" className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={selected.enabled} onChange={(event) => updateChannel((channel) => ({ ...channel, enabled: event.target.checked }))} className="h-4 w-4 rounded border-slate-300 text-indigo-600 focus:ring-indigo-500" />
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.channelEnabled')}</span>
                </label>
              </div>

              <div className="rounded-lg border border-slate-200 bg-white p-3 dark:border-white/10 dark:bg-[#1a1a1a]">
                <div className="mb-2 flex items-center justify-between gap-2">
                  <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-200">{t('admin.model.frontend.fields')}</h3>
                  <div className="flex items-center gap-1">
                    <input value={newFieldKey} onChange={(event) => setNewFieldKey(event.target.value)} onKeyDown={(event) => { if (event.key === 'Enter') addField(); }} placeholder={t('admin.model.frontend.fieldKeyPlaceholder')} className="w-32 rounded-md border border-slate-200 bg-white px-2 py-1 text-xs text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                    <button type="button" onClick={addField} aria-label={t('admin.model.frontend.addField')} title={t('admin.model.frontend.addField')} className="rounded-md p-1 text-slate-400 hover:bg-indigo-50 hover:text-indigo-600 dark:hover:bg-indigo-500/10">
                      <Plus className="h-4 w-4" />
                    </button>
                  </div>
                </div>
                <div className="flex flex-wrap gap-2">
                  {selected.fields.map((field, index) => (
                    <div key={field.fieldKey} className="flex items-center gap-2 rounded-md border border-slate-200 px-2 py-1 text-xs dark:border-white/10">
                      <span className="font-medium text-slate-700 dark:text-slate-200">{field.fieldKey}</span>
                      <label className="flex items-center gap-1 text-slate-500 dark:text-slate-400">
                        <input type="checkbox" checked={field.visible} onChange={(event) => updateField(index, (current) => ({ ...current, visible: event.target.checked }))} className="h-3.5 w-3.5 rounded border-slate-300 text-indigo-600" />
                        {t('admin.model.frontend.visible')}
                      </label>
                      <button type="button" onClick={() => updateChannel((channel) => ({ ...channel, fields: channel.fields.filter((_, fieldIndex) => fieldIndex !== index) }))} className="text-rose-500 hover:text-rose-600" aria-label={`${t('common.actions.delete')} ${field.fieldKey}`}>
                        <Trash2 className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>

              <div className="rounded-lg border border-slate-200 bg-white p-3 dark:border-white/10 dark:bg-[#1a1a1a]">
                <div className="mb-2 flex items-center justify-between gap-2">
                  <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-200">{t('admin.model.frontend.models')}</h3>
                  <div className="flex items-center gap-1">
                    <button
                      type="button"
                      onClick={openPicker}
                      className="inline-flex items-center gap-1 rounded-md border border-slate-200 px-2 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 dark:border-white/10 dark:text-slate-300 dark:hover:bg-white/5"
                      title={t('admin.model.frontend.addModel')}
                    >
                      <Plus className="h-3.5 w-3.5" />
                      {t('admin.model.frontend.addModel')}
                    </button>
                    <button
                      type="button"
                      onClick={() => void importFromCatalog()}
                      disabled={importing || catalogLoading}
                      className="inline-flex items-center gap-1 rounded-md border border-slate-200 px-2 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60 dark:border-white/10 dark:text-slate-300 dark:hover:bg-white/5"
                    >
                      {importing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Download className="h-3.5 w-3.5" />}
                      {importing ? t('admin.model.frontend.importing') : t('admin.model.frontend.importFromCatalog')}
                    </button>
                  </div>
                </div>
                {catalogError ? (
                  <p className="mb-2 text-xs text-rose-600 dark:text-rose-300">{catalogError}</p>
                ) : null}
                {selected.models.length === 0 ? (
                  <p className="py-3 text-center text-xs text-slate-400 dark:text-slate-500">{t('admin.model.frontend.noModels')}</p>
                ) : (
                  <div className="flex flex-col divide-y divide-slate-100 dark:divide-white/5">
                    {selected.models.map((model, index) => {
                      const catalogModel = catalogByKey.get(model.catalogKey);
                      const isDefault = selected.defaultCatalogKey === model.catalogKey;
                      return (
                        <div key={model.catalogKey} className="flex flex-wrap items-center gap-2 py-2 text-sm">
                          <div className="flex min-w-0 flex-1 flex-col">
                            <span className="truncate font-medium text-slate-800 dark:text-slate-200">{catalogModel?.displayName ?? model.catalogKey}</span>
                            <span className="truncate text-xs text-slate-400 dark:text-slate-500">
                              {model.catalogKey}
                              {catalogModel ? ` · ${catalogModel.type}` : null}
                              {catalogModel === undefined && catalogModels !== null ? ` · ${t('admin.model.frontend.notInCatalog')}` : null}
                            </span>
                          </div>
                          <label className="flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400">
                            <input type="checkbox" checked={model.visible} onChange={(event) => setModelVisible(index, event.target.checked)} className="h-3.5 w-3.5 rounded border-slate-300 text-indigo-600" />
                            {t('admin.model.frontend.visible')}
                          </label>
                          <label className="flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400">
                            <input type="checkbox" checked={model.featured} onChange={(event) => updateModel(index, (current) => ({ ...current, featured: event.target.checked }))} className="h-3.5 w-3.5 rounded border-slate-300 text-indigo-600" />
                            {t('admin.model.frontend.featured')}
                          </label>
                          <label className="flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400">
                            <input
                              type="radio"
                              name={`frontend-default-${selected.channelCode}`}
                              checked={isDefault}
                              disabled={!model.visible}
                              title={!model.visible ? t('admin.model.frontend.defaultModelRequiresVisible') : undefined}
                              onChange={() => updateChannel((channel) => ({ ...channel, defaultCatalogKey: model.catalogKey }))}
                              className="h-3.5 w-3.5 border-slate-300 text-indigo-600 disabled:opacity-40"
                            />
                            <Star className={`h-3.5 w-3.5 ${isDefault ? 'fill-amber-400 text-amber-500' : ''}`} />
                            {t('admin.model.frontend.defaultModel')}
                          </label>
                          <label className="flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400">
                            {t('admin.model.frontend.sort')}
                            <input type="number" value={model.sortOrder} onChange={(event) => updateModel(index, (current) => ({ ...current, sortOrder: parseSortOrder(event.target.value, current.sortOrder) }))} className="w-16 rounded border border-slate-200 bg-white px-1 py-0.5 text-xs dark:border-white/10 dark:bg-white/5 dark:text-white" />
                          </label>
                          <button type="button" onClick={() => updateChannel((channel) => {
                            const remaining = channel.models.filter((_, modelIndex) => modelIndex !== index);
                            const defaultCatalogKey = channel.defaultCatalogKey === model.catalogKey
                              ? (remaining.find((candidate) => candidate.visible)?.catalogKey ?? null)
                              : channel.defaultCatalogKey;
                            return { ...channel, models: remaining, defaultCatalogKey };
                          })} className="text-rose-500 hover:text-rose-600" aria-label={`${t('common.actions.delete')} ${model.catalogKey}`}>
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          ) : (
            <p className="py-12 text-center text-sm text-slate-500 dark:text-slate-400">{t('admin.model.frontend.selectChannel')}</p>
          )}
        </section>
      </div>

      {pickerOpen && selected ? (
        <div className="fixed inset-0 z-[70] flex items-center justify-center bg-slate-950/50 p-4 backdrop-blur-sm" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) setPickerOpen(false); }}>
          <div
            role="dialog"
            aria-modal="true"
            aria-labelledby="frontend-model-picker-title"
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault();
                setPickerOpen(false);
              }
            }}
            className="flex h-[min(80vh,44rem)] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl dark:border-white/10 dark:bg-[#171719]"
          >
            <div className="flex shrink-0 items-center justify-between border-b border-slate-200 px-4 py-3 dark:border-white/10">
              <div className="min-w-0">
                <h3 id="frontend-model-picker-title" className="truncate text-sm font-semibold text-slate-900 dark:text-white">{t('admin.model.frontend.addModelTitle', { channel: channelLabel(selected) })}</h3>
                <p className="truncate text-xs text-slate-500 dark:text-slate-400">{t('admin.model.frontend.addModelHint')}</p>
              </div>
              <button type="button" onClick={() => setPickerOpen(false)} className="text-slate-400 hover:text-slate-600" aria-label={t('common.actions.cancel')}>
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="shrink-0 border-b border-slate-200 p-3 dark:border-white/10">
              <div className="relative">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
                <input
                  value={pickerSearch}
                  onChange={(event) => setPickerSearch(event.target.value)}
                  placeholder={t('admin.model.frontend.addModelSearch')}
                  aria-label={t('admin.model.frontend.addModelSearch')}
                  autoFocus
                  className="w-full rounded-lg border border-slate-200 bg-white py-2 pl-9 pr-3 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white"
                />
              </div>
              <div className="mt-2 flex items-center gap-2">
                <input
                  value={manualCatalogKey}
                  onChange={(event) => {
                    setManualCatalogKey(event.target.value);
                    setManualCatalogKeyError(null);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault();
                      addManualCatalogModel();
                    }
                  }}
                  placeholder={t('admin.model.frontend.manualModelPlaceholder')}
                  aria-label={t('admin.model.frontend.manualModelPlaceholder')}
                  aria-invalid={Boolean(manualCatalogKeyError)}
                  className={`min-w-0 flex-1 rounded-lg border bg-white px-2.5 py-1.5 text-xs font-mono text-slate-900 outline-none focus:border-indigo-500 dark:bg-white/5 dark:text-white ${manualCatalogKeyError ? 'border-rose-300 dark:border-rose-500/50' : 'border-slate-200 dark:border-white/10'}`}
                />
                <button type="button" onClick={addManualCatalogModel} className="shrink-0 rounded-lg border border-slate-200 px-2.5 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-50 dark:border-white/10 dark:text-slate-300 dark:hover:bg-white/5">
                  {t('admin.model.frontend.addManualModel')}
                </button>
              </div>
              {manualCatalogKeyError ? <p className="mt-1.5 text-xs text-rose-600 dark:text-rose-300">{manualCatalogKeyError}</p> : null}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto p-2">
              {catalogLoading ? (
                <div className="flex items-center justify-center gap-2 py-10 text-sm text-slate-500 dark:text-slate-400">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t('admin.model.frontend.catalogLoading')}
                </div>
              ) : catalogError ? (
                <div className="flex flex-col items-center gap-3 py-10 text-center text-sm text-slate-500 dark:text-slate-400">
                  <p className="text-rose-600 dark:text-rose-300">{catalogError}</p>
                  <button type="button" onClick={() => void loadCatalog(true)} className="inline-flex items-center gap-1 rounded-md border border-slate-200 px-2.5 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 dark:border-white/10 dark:text-slate-300 dark:hover:bg-white/5">
                    <RefreshCw className="h-3.5 w-3.5" />
                    {t('admin.model.frontend.reload')}
                  </button>
                </div>
              ) : pickerCandidates.length === 0 ? (
                <p className="py-10 text-center text-sm text-slate-500 dark:text-slate-400">{t('admin.model.frontend.addModelEmpty')}</p>
              ) : (
                pickerCandidates.map((model) => {
                  const key = catalogKeyOf(model);
                  const checked = pickerSelected.includes(key);
                  return (
                    <button
                      key={`${model.id}-${key}`}
                      type="button"
                      onClick={() => togglePickerModel(key)}
                      aria-pressed={checked}
                      className={`flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm transition ${checked ? 'bg-indigo-50 dark:bg-indigo-500/10' : 'hover:bg-slate-50 dark:hover:bg-white/5'}`}
                    >
                      <span className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${checked ? 'border-indigo-600 bg-indigo-600 text-white' : 'border-slate-300 dark:border-white/20'}`}>
                        {checked ? <Check className="h-3 w-3" /> : null}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-medium text-slate-800 dark:text-slate-200">{model.displayName}</span>
                        <span className="block truncate text-xs text-slate-400 dark:text-slate-500">{key} · {model.type}</span>
                      </span>
                    </button>
                  );
                })
              )}
            </div>
            <div className="flex shrink-0 items-center justify-between gap-3 border-t border-slate-200 px-4 py-3 dark:border-white/10">
              <span className="text-xs text-slate-500 dark:text-slate-400">{t('admin.model.frontend.addModelSelected', { count: pickerSelected.length })}</span>
              <div className="flex items-center gap-2">
                <button type="button" onClick={() => setPickerOpen(false)} className="rounded-lg border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-50 dark:border-white/10 dark:text-slate-300 dark:hover:bg-white/5">
                  {t('common.actions.cancel')}
                </button>
                <button type="button" onClick={confirmPicker} disabled={pickerSelected.length === 0} className="rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-indigo-700 disabled:cursor-not-allowed disabled:opacity-60">
                  {t('admin.model.frontend.addSelected')}
                </button>
              </div>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
