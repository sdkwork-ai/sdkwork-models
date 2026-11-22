/**
 * Catalog-to-surface alignment for the front-end model configuration.
 *
 * The admin page edits which catalog models a product surface exposes. The
 * catalog itself is import-owned and refreshed by `models.sync` at any time, so
 * a surface can drift: a model is removed by a vendor, a new model appears, or
 * a row still carries a stale int64 model id. These pure helpers reconcile a
 * configuration document with one catalog snapshot so the operator's
 * enable/disable intent survives while the exposed keys stay resolvable.
 *
 * Nothing here mutates its inputs; the caller decides when to persist the
 * returned document. Every helper is deterministic and side-effect free so it
 * can be unit tested without React or the network.
 *
 * @module @sdkwork/models-pc-admin-catalog/frontendCatalogAlignment
 */

import type {
  FrontendChannelConfig,
  FrontendChannelModelConfig,
  FrontendConfigDocument,
} from './frontendConfigService';
import type { Model } from './modelService';

/** Model types a product surface can serve. */
export type ChannelModelType = Model['type'];

/** Backend hard limit mirrored so a merge can never build an unsaveable document. */
export const MAX_CHANNEL_MODELS = 2_000;

/** Default sort-order step used when appending rows, matching the page's increments. */
const SORT_ORDER_STEP = 10;

/** Capability code to the model types it serves. */
const CAPABILITY_MODEL_TYPES: Readonly<Record<string, readonly ChannelModelType[]>> = {
  chat: ['Chat'],
  responses: ['Chat'],
  completion: ['Chat'],
  tool_calling: ['Chat'],
  json_schema: ['Chat'],
  batch: ['Chat'],
  realtime: ['Chat'],
  moderation: ['Chat'],
  computer_use: ['Chat'],
  web_search: ['Chat'],
  file_search: ['Chat'],
  code_interpreter: ['Chat'],
  image_generation: ['Image'],
  image_editing: ['Image'],
  vision: ['Image'],
  video_generation: ['Video'],
  audio_input: ['Audio'],
  audio_output: ['Audio'],
  speech_to_text: ['Audio'],
  text_to_speech: ['Audio'],
  music_generation: ['Music'],
  sound_effect_generation: ['SoundEffect'],
  embedding: ['Embedding'],
  rerank: ['Embedding'],
};

/**
 * Map a channel's catalog capability codes to the model types the surface can
 * serve. An empty set means "no capability binding" and offers every type.
 * @param capabilityCodes - the channel's bound capability codes.
 * @returns the model types the channel accepts.
 */
export function capabilityModelTypes(capabilityCodes: readonly string[]): Set<ChannelModelType> {
  const types = new Set<ChannelModelType>();
  for (const code of capabilityCodes) {
    const mapped = CAPABILITY_MODEL_TYPES[code];
    if (mapped === undefined) continue;
    for (const type of mapped) types.add(type);
  }
  return types;
}

/**
 * Stable catalog identity of one model: `<vendorCode>/<model>`.
 * @param model - any row carrying a vendor code and a vendor-local model id.
 * @returns the catalog key used by the front-end configuration.
 */
export function catalogKeyOf(model: Pick<Model, 'vendorCode' | 'model'>): string {
  return `${model.vendorCode}/${model.model}`;
}

/**
 * Index catalog keys to the int64 model id (decimal string) persisted on a
 * channel row. The id never becomes a JavaScript number (API_SPEC 13.6).
 * @param models - the catalog models.
 * @returns catalog key to decimal id string.
 */
export function modelIdsByKey(models: readonly Model[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const model of models) {
    map.set(catalogKeyOf(model), model.id);
  }
  return map;
}

/** One catalog model referenced by an alignment report. */
export interface FrontendAlignmentModelRef {
  /** Channel the reference belongs to. */
  channelCode: string;
  /** Stable catalog key. */
  catalogKey: string;
  /** Catalog display name, or the key when the catalog no longer has it. */
  displayName: string;
  /** Catalog model type, when the catalog still describes the model. */
  modelType: ChannelModelType | null;
}

/** Alignment facts for one channel. */
export interface FrontendAlignmentChannelReport {
  /** Channel code. */
  channelCode: string;
  /** Referenced keys the catalog no longer describes that this pass disabled. */
  staleModels: FrontendAlignmentModelRef[];
  /** Referenced keys whose persisted model id was missing or stale; rebound. */
  reboundModelKeys: string[];
  /** Active catalog models matching the binding that the channel does not expose. */
  availableModels: FrontendAlignmentModelRef[];
  /** Total available models before the report list was capped. */
  availableModelCount: number;
  /** Duplicate exposure rows removed from the channel. */
  duplicateModelCount: number;
}

/** The complete alignment report shown to the operator. */
export interface FrontendCatalogAlignmentReport {
  /** Whether the returned document differs from the input document. */
  changed: boolean;
  /** Number of catalog models considered. */
  catalogModelCount: number;
  /** Total stale model rows disabled by this pass (already disabled rows are not repeated). */
  staleModelCount: number;
  /** Total model rows whose int64 model id was rebound. */
  reboundModelIdCount: number;
  /** Total catalog models that are available but not exposed. */
  availableModelCount: number;
  /** Total duplicate exposure rows removed. */
  duplicateModelCount: number;
  /** Per-channel facts, in document order. */
  channels: FrontendAlignmentChannelReport[];
}

/** Options for {@link alignFrontendDocumentWithCatalog}. */
export interface AlignFrontendDocumentOptions {
  /** Hide stale exposure rows instead of leaving them visible (default `true`). */
  hideStaleModels?: boolean;
  /** Cap on the available models listed in the report per channel (default `100`). */
  maxReportedAvailableModels?: number;
}

/**
 * Point one channel's default at a visible exposed model.
 *
 * A document written before the backend enforced this invariant can carry a
 * default that is missing or hidden; normalizing it here keeps the save from
 * failing on a rule the operator cannot see.
 * @param channel - the channel to normalize.
 * @returns the channel, or the same reference when the default is already valid.
 */
export function normalizeFrontendChannelDefault(
  channel: FrontendChannelConfig,
): FrontendChannelConfig {
  const current = channel.defaultCatalogKey;
  const valid = current !== null
    && channel.models.some((model) => model.catalogKey === current && model.visible);
  if (valid) return channel;
  const fallback = channel.models.find((model) => model.visible)?.catalogKey ?? null;
  return fallback === current ? channel : { ...channel, defaultCatalogKey: fallback };
}

/**
 * Normalize every channel's default model so the document satisfies the
 * backend's "default must be a visible exposed model" invariant.
 * @param document - the configuration document.
 * @returns the document, with only the channels that needed a change replaced.
 */
export function normalizeFrontendDocumentDefaults(
  document: FrontendConfigDocument,
): FrontendConfigDocument {
  let changed = false;
  const channels = document.channels.map((channel) => {
    const next = normalizeFrontendChannelDefault(channel);
    if (next !== channel) changed = true;
    return next;
  });
  return changed ? { ...document, channels } : document;
}

/**
 * Append catalog models to a channel without duplicating an exposed key,
 * keeping the channel's sort order monotonic and never exceeding the backend's
 * row limit.
 * @param channel - the channel being edited.
 * @param keys - catalog keys to add, already filtered to served types.
 * @param catalogIdsByKey - catalog key to int64 model id (decimal string).
 * @param options - visibility of the appended rows (default `true`).
 * @returns the next channel plus how many rows were added.
 */
export function mergeCatalogModels(
  channel: FrontendChannelConfig,
  keys: readonly string[],
  catalogIdsByKey: ReadonlyMap<string, string>,
  options: { visible?: boolean } = {},
): { channel: FrontendChannelConfig; added: number } {
  const visible = options.visible ?? true;
  const existing = new Set(channel.models.map((model) => model.catalogKey));
  const baseOrder = channel.models.reduce((max, model) => Math.max(max, model.sortOrder), 0);
  const additions: FrontendChannelModelConfig[] = [];
  for (const key of keys) {
    if (existing.has(key)) continue;
    if (channel.models.length + additions.length >= MAX_CHANNEL_MODELS) break;
    existing.add(key);
    additions.push({
      catalogKey: key,
      modelId: catalogIdsByKey.get(key) ?? null,
      visible,
      featured: false,
      sortOrder: baseOrder + (additions.length + 1) * SORT_ORDER_STEP,
      badge: null,
      config: {},
    });
  }
  if (additions.length === 0) {
    return { channel, added: 0 };
  }
  return { channel: { ...channel, models: [...channel.models, ...additions] }, added: additions.length };
}

/** Catalog lookups an alignment pass needs, derived once per document. */
interface CatalogIndex {
  byKey: Map<string, Model>;
  idsByKey: Map<string, string>;
}

function buildCatalogIndex(catalog: readonly Model[]): CatalogIndex {
  const byKey = new Map<string, Model>();
  const idsByKey = new Map<string, string>();
  for (const model of catalog) {
    const key = catalogKeyOf(model);
    if (!byKey.has(key)) byKey.set(key, model);
    idsByKey.set(key, model.id);
  }
  return { byKey, idsByKey };
}

/**
 * Reconcile one channel with the catalog.
 * @param channel - the channel to align.
 * @param index - catalog lookups.
 * @param options - alignment options.
 * @returns the next channel and its report.
 */
function alignChannel(
  channel: FrontendChannelConfig,
  index: CatalogIndex,
  options: Required<AlignFrontendDocumentOptions>,
): { channel: FrontendChannelConfig; report: FrontendAlignmentChannelReport } {
  const report: FrontendAlignmentChannelReport = {
    channelCode: channel.channelCode,
    staleModels: [],
    reboundModelKeys: [],
    availableModels: [],
    availableModelCount: 0,
    duplicateModelCount: 0,
  };
  const seen = new Set<string>();
  const keptModels: FrontendChannelModelConfig[] = [];
  let modelsChanged = false;
  for (const model of channel.models) {
    if (seen.has(model.catalogKey)) {
      report.duplicateModelCount += 1;
      modelsChanged = true;
      continue;
    }
    seen.add(model.catalogKey);
    const catalogModel = index.byKey.get(model.catalogKey);
    if (catalogModel === undefined) {
      const willDisable = options.hideStaleModels && model.visible;
      if (willDisable) {
        report.staleModels.push({
          channelCode: channel.channelCode,
          catalogKey: model.catalogKey,
          displayName: model.catalogKey,
          modelType: null,
        });
      }
      if (willDisable || model.modelId !== null) {
        keptModels.push({
          ...model,
          modelId: null,
          visible: options.hideStaleModels ? false : model.visible,
        });
        modelsChanged = true;
      } else {
        keptModels.push(model);
      }
      continue;
    }
    const catalogId = index.idsByKey.get(model.catalogKey) ?? null;
    if (model.modelId !== catalogId) {
      report.reboundModelKeys.push(model.catalogKey);
      keptModels.push({ ...model, modelId: catalogId });
      modelsChanged = true;
    } else {
      keptModels.push(model);
    }
  }

  const available = availableCatalogModels(channel, index, seen);
  report.availableModelCount = available.length;
  report.availableModels = options.maxReportedAvailableModels > 0
    ? available.slice(0, options.maxReportedAvailableModels)
    : [];

  const nextChannel: FrontendChannelConfig = modelsChanged
    ? { ...channel, models: keptModels }
    : channel;
  return { channel: normalizeFrontendChannelDefault(nextChannel), report };
}

/**
 * Active catalog models a channel can serve but does not expose yet.
 * @param channel - the channel whose capability binding decides the types.
 * @param index - catalog lookups.
 * @param exposedKeys - keys the channel already references.
 * @returns the available models, sorted by catalog key.
 */
function availableCatalogModels(
  channel: FrontendChannelConfig,
  index: CatalogIndex,
  exposedKeys: ReadonlySet<string>,
): FrontendAlignmentModelRef[] {
  const types = capabilityModelTypes(channel.capabilityCodes);
  const available: FrontendAlignmentModelRef[] = [];
  for (const model of index.byKey.values()) {
    const key = catalogKeyOf(model);
    if (exposedKeys.has(key)) continue;
    if (model.status !== 'active') continue;
    if (types.size > 0 && !types.has(model.type)) continue;
    available.push({
      channelCode: channel.channelCode,
      catalogKey: key,
      displayName: model.displayName,
      modelType: model.type,
    });
  }
  available.sort((left, right) => (
    left.catalogKey < right.catalogKey ? -1 : left.catalogKey > right.catalogKey ? 1 : 0
  ));
  return available;
}

/**
 * Reconcile a configuration document with one catalog snapshot.
 *
 * Rules:
 * - A referenced but missing catalog key is a stale row: it is hidden (the app
 *   surface only serves visible rows) and its int64 id is cleared.
 * - A referenced key whose persisted id is missing or different is rebound to
 *   the catalog's id.
 * - An inactive catalog model is never offered as available.
 * - Available rows are reported, never inserted: the operator keeps control
 *   over what a surface exposes.
 * - Duplicate rows collapse to the first occurrence because the backend rejects
 *   a repeated catalog key.
 * - A default that no longer names a visible model moves to the first visible
 *   row.
 * @param document - the edited configuration document.
 * @param catalog - the synchronized catalog snapshot.
 * @param options - alignment options.
 * @returns the aligned document and the report describing what changed.
 */
export function alignFrontendDocumentWithCatalog(
  document: FrontendConfigDocument,
  catalog: readonly Model[],
  options: AlignFrontendDocumentOptions = {},
): { document: FrontendConfigDocument; report: FrontendCatalogAlignmentReport } {
  const resolved: Required<AlignFrontendDocumentOptions> = {
    hideStaleModels: options.hideStaleModels ?? true,
    maxReportedAvailableModels: options.maxReportedAvailableModels ?? 100,
  };
  const index = buildCatalogIndex(catalog);
  const channels: FrontendChannelConfig[] = [];
  const channelReports: FrontendAlignmentChannelReport[] = [];
  let changed = false;
  for (const channel of document.channels) {
    const aligned = alignChannel(channel, index, resolved);
    if (aligned.channel !== channel) changed = true;
    channels.push(aligned.channel);
    channelReports.push(aligned.report);
  }
  const nextDocument = changed ? { ...document, channels } : document;
  const report: FrontendCatalogAlignmentReport = {
    changed,
    catalogModelCount: catalog.length,
    staleModelCount: channelReports.reduce((sum, channel) => sum + channel.staleModels.length, 0),
    reboundModelIdCount: channelReports.reduce((sum, channel) => sum + channel.reboundModelKeys.length, 0),
    availableModelCount: channelReports.reduce((sum, channel) => sum + channel.availableModelCount, 0),
    duplicateModelCount: channelReports.reduce((sum, channel) => sum + channel.duplicateModelCount, 0),
    channels: channelReports,
  };
  return { document: nextDocument, report };
}

/**
 * Merge every available catalog model into its channel as a hidden row.
 *
 * Hidden is the deliberate default: a newly published vendor model becomes
 * discoverable and can be enabled explicitly, without silently appearing on a
 * production surface. Duplicate keys and the backend row limit are honored.
 * @param document - the document to extend.
 * @param catalog - the synchronized catalog snapshot.
 * @param options - visibility of the appended rows (default `false`).
 * @returns the extended document and how many rows were added.
 */
export function mergeAvailableFrontendModels(
  document: FrontendConfigDocument,
  catalog: readonly Model[],
  options: { visible?: boolean } = {},
): { document: FrontendConfigDocument; addedCount: number } {
  const index = buildCatalogIndex(catalog);
  let addedCount = 0;
  let changed = false;
  const channels = document.channels.map((channel) => {
    const exposed = new Set(channel.models.map((model) => model.catalogKey));
    const keys = availableCatalogModels(channel, index, exposed)
      .map((model) => model.catalogKey);
    if (keys.length === 0) return channel;
    const merged = mergeCatalogModels(channel, keys, index.idsByKey, {
      visible: options.visible ?? false,
    });
    if (merged.added === 0) return channel;
    addedCount += merged.added;
    changed = true;
    return normalizeFrontendChannelDefault(merged.channel);
  });
  return {
    document: changed ? { ...document, channels } : document,
    addedCount,
  };
}

/**
 * Set every exposed row's visibility in one channel, optionally limited to rows
 * the catalog still describes.
 * @param channel - the channel to update.
 * @param visible - the visibility to apply.
 * @param catalogKeys - keys eligible for the change; omitted means every row.
 * @returns the next channel plus how many rows actually changed.
 */
export function setChannelModelsVisibility(
  channel: FrontendChannelConfig,
  visible: boolean,
  catalogKeys?: ReadonlySet<string>,
): { channel: FrontendChannelConfig; changedCount: number } {
  let changedCount = 0;
  const models = channel.models.map((model) => {
    if (catalogKeys !== undefined && !catalogKeys.has(model.catalogKey)) return model;
    if (model.visible === visible) return model;
    changedCount += 1;
    return { ...model, visible };
  });
  if (changedCount === 0) return { channel, changedCount };
  return { channel: normalizeFrontendChannelDefault({ ...channel, models }), changedCount };
}
