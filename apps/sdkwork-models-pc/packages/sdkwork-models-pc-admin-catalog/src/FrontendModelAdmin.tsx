import { useEffect, useState } from 'react';
import { Download, Loader2, Plus, RefreshCw, Save, Trash2 } from 'lucide-react';
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

function emptyModel(catalogKey: string): FrontendChannelModelConfig {
  return {
    catalogKey,
    modelId: null,
    visible: true,
    featured: false,
    sortOrder: 0,
    badge: null,
    config: {},
  };
}

/**
 * Map a channel's catalog capability codes to the admin model types so the
 * "import from catalog" action offers only models that can serve the surface.
 * An empty set means "no capability binding" and imports every type.
 * @param capabilityCodes - the channel's bound capability codes.
 * @returns the model types the channel accepts.
 */
function capabilityModelTypes(capabilityCodes: readonly string[]): Set<Model['type']> {
  const types = new Set<Model['type']>();
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

/**
 * Admin authoring surface for the models-owned front-end configuration.
 *
 * One tab per product surface (LLM / video / image / audio / music / …). The
 * page reads and replaces the revisioned document served by the models backend;
 * catalog facts remain in the model tables, and this page only chooses which
 * models a surface exposes and which fields it shows.
 */
export function FrontendModelAdmin() {
  const { t } = useTranslation();
  const [document, setDocument] = useState<FrontendConfigDocument | null>(null);
  const [selectedCode, setSelectedCode] = useState('');
  const [newChannelCode, setNewChannelCode] = useState('');
  const [newFieldKey, setNewFieldKey] = useState('');
  const [newCatalogKey, setNewCatalogKey] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await loadFrontendConfigDocument();
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

  const addChannel = () => {
    const code = newChannelCode.trim();
    if (code.length === 0 || document?.channels.some((channel) => channel.channelCode === code)) {
      return;
    }
    setDocument((current) => current === null ? current : {
      ...current,
      channels: [...current.channels, emptyChannel(code)],
    });
    setSelectedCode(code);
    setNewChannelCode('');
  };

  const importFromCatalog = async () => {
    if (!selected) return;
    setImporting(true);
    setError(null);
    setNotice(null);
    try {
      const catalog = await ModelService.fetchInitializedCatalog();
      const types = capabilityModelTypes(selected.capabilityCodes);
      const existing = new Set(selected.models.map((model) => model.catalogKey));
      const candidates = catalog.models
        .filter((model) => types.size === 0 || types.has(model.type))
        .map((model) => `${model.vendorCode}/${model.model}`)
        .filter((catalogKey) => !existing.has(catalogKey));
      if (candidates.length === 0) {
        setNotice(t('admin.model.frontend.importEmpty'));
        return;
      }
      const baseOrder = selected.models.reduce((max, model) => Math.max(max, model.sortOrder), 0);
      const additions = candidates.map((catalogKey, index) => ({
        ...emptyModel(catalogKey),
        sortOrder: baseOrder + (index + 1) * 10,
      }));
      updateChannel((channel) => ({ ...channel, models: [...channel.models, ...additions] }));
      setNotice(t('admin.model.frontend.imported', { count: additions.length }));
    } catch (importError) {
      setError(importError instanceof Error ? importError.message : t('admin.model.frontend.importError'));
    } finally {
      setImporting(false);
    }
  };

  const save = async () => {
    if (document === null) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      const next = await saveFrontendConfigDocument(document);
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
          <button type="button" onClick={() => void load()} className="inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-slate-500 hover:bg-slate-100 hover:text-slate-700 dark:hover:bg-white/10 dark:hover:text-slate-200">
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
              placeholder={t('admin.model.frontend.channelCodePlaceholder')}
              className="min-w-0 flex-1 rounded-md border border-slate-200 bg-white px-2 py-1 text-xs text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white"
            />
            <button type="button" onClick={addChannel} title={t('admin.model.frontend.addChannel')} className="rounded-md p-1.5 text-slate-400 hover:bg-indigo-50 hover:text-indigo-600 dark:hover:bg-indigo-500/10">
              <Plus className="h-4 w-4" />
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-2">
            {(document?.channels ?? []).map((channel) => (
              <button
                key={channel.channelCode}
                type="button"
                onClick={() => setSelectedCode(channel.channelCode)}
                className={`mb-1 flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-left text-sm transition-colors ${channel.channelCode === selectedCode ? 'bg-indigo-50 text-indigo-700 dark:bg-indigo-500/10 dark:text-indigo-300' : 'text-slate-700 hover:bg-slate-50 dark:text-slate-300 dark:hover:bg-white/5'}`}
              >
                <span className="min-w-0 truncate">{channel.displayName || channel.channelCode}</span>
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
                  <input type="number" value={selected.sortOrder} onChange={(event) => updateChannel((channel) => ({ ...channel, sortOrder: Number(event.target.value) }))} className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                </label>
                <label className="flex flex-col gap-1 text-sm">
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.defaultModel')}</span>
                  <input value={selected.defaultCatalogKey ?? ''} onChange={(event) => updateChannel((channel) => ({ ...channel, defaultCatalogKey: event.target.value.trim() || null }))} placeholder="deepseek/deepseek-flash" className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                </label>
                <label className="flex flex-col gap-1 text-sm">
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.capabilityCodes')}</span>
                  <input value={selected.capabilityCodes.join(', ')} onChange={(event) => updateChannel((channel) => ({ ...channel, capabilityCodes: event.target.value.split(',').map((code) => code.trim()).filter((code) => code.length > 0) }))} placeholder="chat, responses" className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                </label>
                <label className="flex items-center gap-2 text-sm">
                  <input type="checkbox" checked={selected.enabled} onChange={(event) => updateChannel((channel) => ({ ...channel, enabled: event.target.checked }))} className="h-4 w-4 rounded border-slate-300 text-indigo-600 focus:ring-indigo-500" />
                  <span className="font-medium text-slate-700 dark:text-slate-200">{t('admin.model.frontend.enabled')}</span>
                </label>
              </div>

              <div className="rounded-lg border border-slate-200 bg-white p-3 dark:border-white/10 dark:bg-[#1a1a1a]">
                <div className="mb-2 flex items-center justify-between gap-2">
                  <h3 className="text-sm font-semibold text-slate-700 dark:text-slate-200">{t('admin.model.frontend.fields')}</h3>
                  <div className="flex items-center gap-1">
                    <input value={newFieldKey} onChange={(event) => setNewFieldKey(event.target.value)} placeholder={t('admin.model.frontend.fieldKeyPlaceholder')} className="w-32 rounded-md border border-slate-200 bg-white px-2 py-1 text-xs text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                    <button type="button" onClick={() => { const key = newFieldKey.trim(); if (key.length === 0) return; updateChannel((channel) => ({ ...channel, fields: [...channel.fields, emptyField(key)] })); setNewFieldKey(''); }} className="rounded-md p-1 text-slate-400 hover:bg-indigo-50 hover:text-indigo-600 dark:hover:bg-indigo-500/10">
                      <Plus className="h-4 w-4" />
                    </button>
                  </div>
                </div>
                <div className="flex flex-wrap gap-2">
                  {selected.fields.map((field, index) => (
                    <div key={`${field.fieldKey}-${index}`} className="flex items-center gap-2 rounded-md border border-slate-200 px-2 py-1 text-xs dark:border-white/10">
                      <span className="font-medium text-slate-700 dark:text-slate-200">{field.fieldKey}</span>
                      <label className="flex items-center gap-1 text-slate-500 dark:text-slate-400">
                        <input type="checkbox" checked={field.visible} onChange={(event) => updateField(index, (current) => ({ ...current, visible: event.target.checked }))} className="h-3.5 w-3.5 rounded border-slate-300 text-indigo-600" />
                        {t('admin.model.frontend.visible')}
                      </label>
                      <button type="button" onClick={() => updateChannel((channel) => ({ ...channel, fields: channel.fields.filter((_, fieldIndex) => fieldIndex !== index) }))} className="text-rose-500 hover:text-rose-600">
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
                      onClick={() => void importFromCatalog()}
                      disabled={importing}
                      className="inline-flex items-center gap-1 rounded-md border border-slate-200 px-2 py-1 text-xs font-medium text-slate-600 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-60 dark:border-white/10 dark:text-slate-300 dark:hover:bg-white/5"
                    >
                      {importing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Download className="h-3.5 w-3.5" />}
                      {importing ? t('admin.model.frontend.importing') : t('admin.model.frontend.importFromCatalog')}
                    </button>
                    <input value={newCatalogKey} onChange={(event) => setNewCatalogKey(event.target.value)} placeholder="vendor/model" className="w-44 rounded-md border border-slate-200 bg-white px-2 py-1 text-xs text-slate-900 outline-none focus:border-indigo-500 dark:border-white/10 dark:bg-white/5 dark:text-white" />
                    <button type="button" onClick={() => { const key = newCatalogKey.trim(); if (key.length === 0) return; updateChannel((channel) => ({ ...channel, models: [...channel.models, emptyModel(key)] })); setNewCatalogKey(''); }} className="rounded-md p-1 text-slate-400 hover:bg-indigo-50 hover:text-indigo-600 dark:hover:bg-indigo-500/10">
                      <Plus className="h-4 w-4" />
                    </button>
                  </div>
                </div>
                {selected.models.length === 0 ? (
                  <p className="py-3 text-center text-xs text-slate-400 dark:text-slate-500">{t('admin.model.frontend.noModels')}</p>
                ) : (
                  <div className="flex flex-col divide-y divide-slate-100 dark:divide-white/5">
                    {selected.models.map((model, index) => (
                      <div key={`${model.catalogKey}-${index}`} className="flex flex-wrap items-center gap-2 py-2 text-sm">
                        <span className="min-w-0 flex-1 truncate font-medium text-slate-800 dark:text-slate-200">{model.catalogKey}</span>
                        <label className="flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400">
                          <input type="checkbox" checked={model.visible} onChange={(event) => updateModel(index, (current) => ({ ...current, visible: event.target.checked }))} className="h-3.5 w-3.5 rounded border-slate-300 text-indigo-600" />
                          {t('admin.model.frontend.visible')}
                        </label>
                        <label className="flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400">
                          <input type="checkbox" checked={model.featured} onChange={(event) => updateModel(index, (current) => ({ ...current, featured: event.target.checked }))} className="h-3.5 w-3.5 rounded border-slate-300 text-indigo-600" />
                          {t('admin.model.frontend.featured')}
                        </label>
                        <label className="flex items-center gap-1 text-xs text-slate-500 dark:text-slate-400">
                          {t('admin.model.frontend.sort')}
                          <input type="number" value={model.sortOrder} onChange={(event) => updateModel(index, (current) => ({ ...current, sortOrder: Number(event.target.value) }))} className="w-16 rounded border border-slate-200 bg-white px-1 py-0.5 text-xs dark:border-white/10 dark:bg-white/5 dark:text-white" />
                        </label>
                        <button type="button" onClick={() => updateChannel((channel) => ({ ...channel, models: channel.models.filter((_, modelIndex) => modelIndex !== index) }))} className="text-rose-500 hover:text-rose-600">
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ) : (
            <p className="py-12 text-center text-sm text-slate-500 dark:text-slate-400">{t('admin.model.frontend.selectChannel')}</p>
          )}
        </section>
      </div>
    </div>
  );
}
