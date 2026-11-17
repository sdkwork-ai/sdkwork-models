import {
  ensureSdkworkApiSuccess,
  isRecord,
  readApiRecord,
  readBoolean,
  readNullableString,
  readNumber,
  readRequiredApiItems,
  readRequiredApiItem,
  readRequiredString,
  readString,
  readStringArray,
  type ApiRecord,
} from '@sdkwork/cloudroutes-pc-commons/api-result';
import { createIdempotencyParams } from '@sdkwork/cloudroutes-pc-commons/idempotency';
import { getModelsBackendSdkClient } from '@sdkwork/cloudroutes-pc-commons/sdk-clients';
import { requiredSafePathSegment } from '@sdkwork/cloudroutes-pc-commons/sdk-request-boundary';
import type {
  AdminAiModelCreateRequest,
  AdminAiModelUpdateRequest,
  AdminModelCatalogSyncRequest,
  AdminModelMappingCreateRequest,
  AdminModelMappingResolveRequest,
  AdminModelMappingRuleBindingInput,
  AdminModelMappingRuleItemInput,
  AdminModelMappingUpdateRequest,
  AdminModelVendorCreateRequest,
  ModelCatalogSyncResult,
  ModelRankingRefreshTriggerRequest,
  ModelRankingRefreshTriggerResponse,
} from '@sdkwork/models-backend-sdk';

type AdminAiModelType = NonNullable<AdminAiModelCreateRequest['type']>;
type ModelRankingMetricItem = { name: string; requests: string; baseVolume: string };
type ModelRankingRefreshStatusValue = 'ready' | 'empty' | 'unavailable';
type ModelMetadataType = Model['type'] | AdminAiModelType;

export interface Vendor {
  id: string;
  vendorCode: string;
  name: string;
  status: 'active' | 'inactive';
  color: string;
  description: string;
  /** Catalog models the operator can manage under this vendor. */
  modelCount: number;
}

export interface Model {
  id: string;
  vendorId: string;
  vendorCode: string;
  model: string;
  displayName: string;
  name: string;
  type: 'Chat' | 'Image' | 'Audio' | 'Embedding' | 'Music' | 'SoundEffect' | 'Video';
  regionPrices: ModelRegionPriceInput[];
  status: 'active' | 'inactive';
  calls: string;
  description: string | null;
  modalities: string[];
  inputModalities: string[];
  outputModalities: string[];
  apiFormat: string | null;
  capabilityIntro: string | null;
  limitations: string[];
  supportedLanguages: string[];
  useCases: string[];
  trainingDataCutoff: string | null;
  contextTokens: number | null;
  maxOutputTokens: number | null;
  supportsStreaming: boolean;
  supportsTools: boolean;
  supportsJsonSchema: boolean;
  releaseStage: number | null;
  shelfState: number | null;
  routingState: number | null;
  replacementModel: string | null;
}

export type ModelCatalogSyncReport = {
  acceptedCount: number;
  capabilityCount: number;
  catalogRoot: string | null;
  catalogVersion: string;
  dryRun: boolean;
  familyCount: number;
  meterCount: number;
  mode: ModelCatalogSyncResult['mode'];
  modelCount: number;
  priceCount: number;
  rankingCount: number;
  voiceCount: number;
  voiceBindingCount: number;
  videoProfileCount: number;
  requestedCatalogVersion: string | null;
  snapshotId: string | null;
  source: string;
  sourceHash: string;
  syncRunId: string | null;
  synced: boolean;
  vendorCodes: string[];
  vendorCount: number;
  vendors: Vendor[];
  models: Model[];
};

export type InitializedModelCatalog = {
  initialized: boolean;
  vendors: Vendor[];
  models: Model[];
};

export interface ModelMappingModelOption {
  id: string;
  vendorId: string;
  vendorCode: string;
  model: string;
  displayName: string;
  name: string;
  type: Model['type'];
  status: Model['status'];
}

export type ModelMappingModelOptionsPage = {
  items: ModelMappingModelOption[];
  totalCount: number;
  hasMore: boolean;
};

export type ModelRankingRefreshStatusView = {
  cacheMaxAgeSeconds: number;
  generatedAt: string;
  generatedCount: number;
  latestJob: ModelRankingRefreshJobView | null;
  nextRefreshAt: string;
  organizationId: string;
  rankScope: string;
  refreshIntervalSeconds: number;
  snapshotDate: string;
  snapshotPeriod: string;
  sourceCount: number;
  sourceTables: string[];
  status: ModelRankingRefreshStatusValue;
  tenantId: string;
  windowEnd: string;
  windowStart: string;
};

type ModelRankingRefreshJobStatus = 'succeeded' | 'failed' | 'empty' | 'skipped' | 'running';

type ModelRankingRefreshJobView = {
  durationMs: number;
  endedAt: string;
  failureCount: number;
  failureReason: string | null;
  generatedCount: number;
  id: string;
  jobName: string;
  nextRefreshAt: string;
  organizationId: string;
  rankScope: string;
  snapshotDate: string;
  snapshotPeriod: string;
  sourceCount: number;
  startedAt: string;
  status: ModelRankingRefreshJobStatus;
  successCount: number;
  tenantId: string;
  windowEnd: string;
  windowStart: string;
};

type ModelRankingRefreshJobHistoryView = {
  items: ModelRankingRefreshJobView[];
};

type ModelRankingRefreshTriggerView = {
  cacheMaxAgeSeconds: number;
  generatedCount: number;
  nextRefreshAt: string;
  organizationId: string;
  rankScope: string;
  refreshIntervalSeconds: number;
  snapshotDate: string;
  snapshotPeriod: 'hourly' | 'daily' | 'weekly' | 'monthly';
  sourceCount: number;
  status: 'succeeded' | 'empty';
  tenantId: string;
  triggered: boolean;
  windowEnd: string;
  windowStart: string;
};

export type VendorCreateInput = {
  name: string;
  status: Vendor['status'];
  color: string;
  description: string;
};

export type ModelRegionPriceInput = {
  regionCode: string;
  currency: string;
  priceIn: string;
  priceOut: string;
  cacheReadPrice?: string;
  cacheWritePrice?: string;
};

export type ModelCreateInput = {
  vendorId: string;
  model?: string;
  displayName?: string | null;
  type: Model['type'];
  regionPrices: ModelRegionPriceInput[];
  contextTokens: string;
  maxOutputTokens?: number | null;
  description?: string | null;
  capabilityIntro?: string | null;
  limitations?: string[];
  supportedLanguages?: string[];
  useCases?: string[];
  supportsStreaming?: boolean;
  supportsTools?: boolean;
  supportsJsonSchema?: boolean;
};

export type ModelUpdateInput = ModelCreateInput & {
  currentType?: Model['type'];
};

type ModelPatchInput = Partial<ModelUpdateInput> & Pick<AdminAiModelUpdateRequest, 'status'>;


export interface ModelMappingRule {
  id: string;
  bindingType: ModelMappingBindingType;
  sourceVendorId: string | null;
  sourceVendorCode: string;
  targetVendorId: string | null;
  targetVendorCode: string;
  mappingMode: 'alias';
  matchType: 'exact';
  enabled: boolean;
  bindings: ModelMappingRuleBinding[];
  mappingItems: ModelMappingRuleItem[];
  createdAt: string | null;
  updatedAt: string | null;
}

export type ModelMappingBindingType =
  | 'global'
  | 'vendor'
  | 'channel_group'
  | 'channel'
  | 'provider_account'
  | 'site'
  | 'site_service';

export interface ModelMappingRuleBinding {
  id: string;
  bindingType: ModelMappingBindingType;
  bindingId?: string | null;
  bindingCode?: string | null;
  bindingName?: string | null;
  sortOrder: number;
  enabled: boolean;
  createdAt?: string | null;
  updatedAt?: string | null;
}

export interface ModelMappingRuleItem {
  id: string;
  sourceModel: string;
  sourceCatalogKey?: string | null;
  targetModel: string;
  targetCatalogKey?: string | null;
  targetProviderModel?: string | null;
  targetProviderNativeModel?: string | null;
  sortOrder: number;
  enabled: boolean;
  createdAt?: string | null;
  updatedAt?: string | null;
}

export type ModelMappingBindingInput = AdminModelMappingRuleBindingInput;
export type ModelMappingRuleItemInput = AdminModelMappingRuleItemInput;
export type ModelMappingCreateInput = AdminModelMappingCreateRequest;
export type ModelMappingUpdateInput = AdminModelMappingUpdateRequest;
export type ModelMappingResolveInput = AdminModelMappingResolveRequest;


export interface ModelMappingResolveResult {
  matched: boolean;
  matchedBindingType: ModelMappingRule['bindingType'] | null;
  sourceModel: string;
  targetModel: string;
  targetCatalogKey: string | null;
  targetVendorCode: string | null;
  targetProviderModel: string | null;
  targetProviderNativeModel: string | null;
  rule: ModelMappingRule | null;
}

export const KNOWN_VENDORS = [
  { id: 'v_openai', name: 'OpenAI', desc: 'Industry leading LLMs inclusive of GPT-4 and DALL-E.' },
  { id: 'v_anthropic', name: 'Anthropic', desc: 'Claude models focused on safety and high context windows.' },
  { id: 'v_google', name: 'Google', desc: 'Gemini models with native multimodal capabilities.' },
  { id: 'v_meituan', name: 'Meituan', desc: 'LongCat (Longmao) models focused on Chinese reasoning and enterprise workflows.' },
  { id: 'v_mureka', name: 'Mureka', desc: 'Kunlun Tech Mureka music generation models for song and instrumental creation.' },
  { id: 'v_runway', name: 'Runway', desc: 'Runway Gen-4 video generation models for text-to-video and image-to-video.' },
  { id: 'v_luma_ai', name: 'Luma AI', desc: 'Luma Dream Machine Ray video generation models.' },
  { id: 'v_vidu', name: 'Vidu', desc: 'ShengShu Technology Vidu Q3 video generation models.' },
  { id: 'v_pixverse', name: 'PixVerse', desc: 'AIsphere PixVerse V6/C1 video generation (拍我AI).' },
  { id: 'v_meta', name: 'Meta', desc: 'Llama series open source models.' },
  { id: 'v_deepseek', name: 'DeepSeek', desc: 'DeepSeek models focus on reasoning and coding.' },
  { id: 'v_mistral', name: 'Mistral AI', desc: 'High-performance open-weight models from Europe.' },
  { id: 'v_cohere', name: 'Cohere', desc: 'Enterprise focused LLMs and advanced RAG embeddings.' },
  { id: 'custom', name: 'Custom', desc: '' },
];

export function selectPreferredModelVendorId(
  vendors: readonly Vendor[],
  currentVendorId?: string,
): string {
  if (currentVendorId && vendors.some((vendor) => vendor.id === currentVendorId)) {
    return currentVendorId;
  }
  return vendors.find((vendor) => vendor.name.toLowerCase() === 'openai')?.id ?? vendors[0]?.id ?? '';
}

export type ModelListQuery = {
  vendorCode?: string;
  q?: string;
  modelTypes?: string;
  page?: number;
  pageSize?: number;
};

export type ModelListPage = {
  items: Model[];
  totalCount: number;
  hasMore: boolean;
};

export type FetchModelsPageOptions = {
  enrichRankingCalls?: boolean;
};

async function listModelsRaw(query: ModelListQuery = {}): Promise<unknown> {
  const params: {
    page?: number;
    pageSize?: number;
    q?: string;
    modelTypes?: string;
    vendorCodes?: string[];
  } = {};
  if (query.page !== undefined) {
    params.page = query.page;
  }
  if (query.pageSize !== undefined) {
    params.pageSize = query.pageSize;
  }
  if (query.vendorCode) {
    params.vendorCodes = [query.vendorCode];
  }
  if (query.q) {
    params.q = query.q;
  }
  if (query.modelTypes) {
    params.modelTypes = query.modelTypes;
  }
  return Object.keys(params).length === 0
    ? getModelsBackendSdkClient().ai.models.list()
    : getModelsBackendSdkClient().ai.models.list(params);
}

function readModelListPage(result: unknown, errorMessage: string): ModelListPage {
  ensureSdkworkApiSuccess(result, errorMessage);
  const record = readApiRecord(result);
  const items = readRequiredApiItems(record, errorMessage).map(normalizeModel);
  const pageInfo = isRecord(record.pageInfo) ? record.pageInfo : {};
  const totalCount = readPageTotalItems(pageInfo, items.length);
  const hasMore = readBoolean(pageInfo, 'hasMore', false);
  return { items, totalCount, hasMore };
}

async function enrichModelsWithRankingCalls(models: Model[]): Promise<Model[]> {
  const rankingCalls = modelCallsByName(await fetchModelRankingCallStats());
  if (rankingCalls.size === 0) {
    return models;
  }
  return models.map((model) => ({
    ...model,
    calls: rankingCalls.get(model.displayName) ?? rankingCalls.get(model.model) ?? model.calls,
  }));
}

let rankingCallStatsCache: Promise<ModelRankingMetricItem[]> | null = null;

function fetchModelRankingCallStats(): Promise<ModelRankingMetricItem[]> {
  if (!rankingCallStatsCache) {
    rankingCallStatsCache = ModelService.fetchModelRankings().catch(() => []);
  }
  return rankingCallStatsCache;
}

export class ModelService {
  static resetRankingCallStatsCache(): void {
    rankingCallStatsCache = null;
  }

  static async fetchModelsPage(
    query: ModelListQuery = {},
    options: FetchModelsPageOptions = {},
  ): Promise<ModelListPage> {
    const result = await listModelsRaw(query);
    const page = readModelListPage(result, 'Failed to fetch models');
    if (options.enrichRankingCalls === false || page.items.length === 0) {
      return page;
    }
    return {
      items: await enrichModelsWithRankingCalls(page.items),
      totalCount: page.totalCount,
      hasMore: page.hasMore,
    };
  }

  static async fetchVendors(): Promise<Vendor[]> {
    const result = await getModelsBackendSdkClient().ai.modelVendors.list();
    ensureSdkworkApiSuccess(result, 'Failed to fetch vendors');
    return readRequiredApiItems(result, 'Failed to fetch vendors')
      .map(normalizeVendor);
  }

  static async fetchInitializedCatalog(): Promise<InitializedModelCatalog> {
    const vendors = await ModelService.fetchVendors();
    if (vendors.length > 0) {
      return {
        initialized: true,
        vendors,
        models: [],
      };
    }
    const synced = await ModelService.syncVendorsAndModels();
    return {
      initialized: true,
      vendors: synced.vendors,
      models: synced.models,
    };
  }

  /**
   * Reads the whole catalog model list through the generated backend SDK.
   *
   * The list endpoint is paginated, so a single fixed-size page silently hides
   * models from the admin authoring surfaces (the front-end model picker and the
   * "import from catalog" action). Pages are read until `hasMore` is false, with
   * a hard page cap so a misreported page count cannot loop forever.
   * @returns every catalog model.
   */
  static async fetchAllModels(): Promise<Model[]> {
    const pageSize = 200;
    const maxPages = 100;
    const models: Model[] = [];
    for (let page = 1; page <= maxPages; page += 1) {
      const result = await ModelService.fetchModelsPage(
        { page, pageSize },
        { enrichRankingCalls: false },
      );
      models.push(...result.items);
      if (!result.hasMore || result.items.length === 0 || models.length >= result.totalCount) {
        break;
      }
    }
    return models;
  }

  static async fetchModelRankings(): Promise<ModelRankingMetricItem[]> {
    const result = await getModelsBackendSdkClient().ai.modelRankings.list({ pageSize: 200 });
    ensureSdkworkApiSuccess(result, 'Failed to fetch model rankings');
    return readRequiredApiItems(readApiRecord(result), 'Failed to fetch model rankings', ['items'])
      .map(normalizeRankingItem)
      .filter((item): item is ModelRankingMetricItem => item !== null);
  }

  static async fetchModelRankingRefreshStatus(): Promise<ModelRankingRefreshStatusView> {
    const result = await getModelsBackendSdkClient().ai.modelRankings.status.retrieve();
    ensureSdkworkApiSuccess(result, 'Failed to fetch model ranking refresh status');
    return normalizeModelRankingRefreshStatus(readApiRecord(result));
  }

  static async fetchModelRankingRefreshJobs(): Promise<ModelRankingRefreshJobHistoryView> {
    const result = await getModelsBackendSdkClient().ai.modelRankings.jobs.list({ pageSize: 20 });
    ensureSdkworkApiSuccess(result, 'Failed to fetch model ranking refresh jobs');
    return {
      items: readRequiredApiItems(readApiRecord(result), 'Failed to fetch model ranking refresh jobs', ['items'])
        .map(normalizeModelRankingRefreshJob),
    };
  }

  static async triggerModelRankingRefresh(): Promise<ModelRankingRefreshTriggerView> {
    ModelService.resetRankingCallStatsCache();
    const result = await getModelsBackendSdkClient().ai.modelRankings.refresh(
      toModelRankingRefreshTriggerRequest(),
      createIdempotencyParams('model-ranking-refresh'),
    );
    ensureSdkworkApiSuccess(result, 'Failed to trigger model ranking refresh');
    return normalizeModelRankingRefreshTrigger(readApiRecord(result));
  }

  static async syncVendorsAndModels(): Promise<ModelCatalogSyncReport> {
    ModelService.resetRankingCallStatsCache();
    const result = await getModelsBackendSdkClient().ai.models.sync(
      toSyncCatalogRequest(),
    );
    ensureSdkworkApiSuccess(result, 'Failed to sync vendors and models');
    const data = readApiRecord(result);
    return {
      synced: readRequiredBoolean(data, 'synced', 'Model catalog sync response is missing synced flag'),
      source: readRequiredString(data, 'source', 'Model catalog sync response is missing source'),
      mode: readSyncMode(data),
      dryRun: readRequiredBoolean(data, 'dryRun', 'Model catalog sync response is missing dryRun flag'),
      catalogVersion: readRequiredString(data, 'catalogVersion', 'Model catalog sync response is missing catalogVersion'),
      requestedCatalogVersion: readNullableString(data, 'requestedCatalogVersion'),
      catalogRoot: readNullableString(data, 'catalogRoot'),
      vendorCodes: readStringArray(data, 'vendorCodes'),
      sourceHash: readSourceHash(data),
      meterCount: readRequiredNonNegativeInteger(data, 'meterCount', 'Model catalog sync response meter count'),
      vendorCount: readRequiredNonNegativeInteger(data, 'vendorCount', 'Model catalog sync response vendor count'),
      familyCount: readRequiredNonNegativeInteger(data, 'familyCount', 'Model catalog sync response family count'),
      modelCount: readRequiredNonNegativeInteger(data, 'modelCount', 'Model catalog sync response model count'),
      capabilityCount: readRequiredNonNegativeInteger(data, 'capabilityCount', 'Model catalog sync response capability count'),
      priceCount: readRequiredNonNegativeInteger(data, 'priceCount', 'Model catalog sync response price count'),
      rankingCount: readRequiredNonNegativeInteger(data, 'rankingCount', 'Model catalog sync response ranking count'),
      voiceCount: readRequiredNonNegativeInteger(data, 'voiceCount', 'Model catalog sync response voice count'),
      voiceBindingCount: readRequiredNonNegativeInteger(
        data,
        'voiceBindingCount',
        'Model catalog sync response voice binding count',
      ),
      videoProfileCount: readRequiredNonNegativeInteger(
        data,
        'videoProfileCount',
        'Model catalog sync response video profile count',
      ),
      acceptedCount: readRequiredNonNegativeInteger(data, 'acceptedCount', 'Model catalog sync response accepted count'),
      snapshotId: readNullableString(data, 'snapshotId'),
      syncRunId: readNullableString(data, 'syncRunId'),
      vendors: readRequiredApiItems(result, 'Failed to sync vendors and models', ['vendors'])
        .map(normalizeVendor),
      models: readRequiredApiItems(data, 'Failed to sync vendors and models', ['models'])
        .map(normalizeModel),
    };
  }

  static async addVendor(vendor: VendorCreateInput): Promise<Vendor> {
    const result = await getModelsBackendSdkClient().ai.modelVendors.create(
      toCreateVendorRequest(vendor),
    );
    ensureSdkworkApiSuccess(result, 'Failed to add vendor');
    return normalizeVendor(readRequiredApiItem(result, 'Created vendor response is missing data'));
  }

  static async addModel(model: ModelCreateInput): Promise<Model> {
    const result = await getModelsBackendSdkClient().ai.models.create(
      toCreateModelRequest(model),
    );
    ensureSdkworkApiSuccess(result, 'Failed to add model');
    return normalizeModel(readRequiredApiItem(result, 'Created model response is missing data'));
  }

  static async updateModel(id: string, model: ModelUpdateInput): Promise<Model> {
    const result = await getModelsBackendSdkClient().ai.models.update(
      requiredSafePathSegment(id, 'modelId'),
      toUpdateModelRequest(model),
    );
    ensureSdkworkApiSuccess(result, 'Failed to update model');
    return normalizeModel(readRequiredApiItem(result, 'Updated model response is missing data'));
  }

  static updateModelStatus(id: string, status: Model['status']): Promise<Model> {
    return updateModelPatch(id, { status }, 'Failed to update model status');
  }

  static async deleteModel(id: string): Promise<boolean> {
    const result = await getModelsBackendSdkClient().ai.models.delete(
      requiredSafePathSegment(id, 'modelId'),
    );
    if (result?.deleted !== true) {
      throw new Error('Model delete confirmation is required');
    }
    return true;
  }
}


export class ModelMappingService {
  static async fetchModelOptionsPage(
    query: ModelListQuery = {},
  ): Promise<ModelMappingModelOptionsPage> {
    const page = await ModelService.fetchModelsPage(query, { enrichRankingCalls: false });
    return {
      items: page.items.map(normalizeModelMappingModelOption),
      totalCount: page.totalCount,
      hasMore: page.hasMore,
    };
  }

  static async fetchModelMappings(params?: {
    bindingType?: ModelMappingRule['bindingType'] | 'all';
    vendorCode?: string | null;
    channelCode?: string | null;
    q?: string | null;
  }): Promise<ModelMappingRule[]> {
    const query = {
      bindingType: params?.bindingType && params.bindingType !== 'all' ? params.bindingType : undefined,
      vendorCode: params?.vendorCode || undefined,
      channelCode: params?.channelCode || undefined,
      q: params?.q || undefined,
    };
    const result = await getModelsBackendSdkClient().ai.modelMappings.list(query);
    ensureSdkworkApiSuccess(result, 'Failed to fetch model mappings');
    return readRequiredApiItems(result, 'Failed to fetch model mappings')
      .map(normalizeModelMappingRule);
  }

  static fetchMappings(params?: Parameters<typeof ModelMappingService.fetchModelMappings>[0]): Promise<ModelMappingRule[]> {
    return ModelMappingService.fetchModelMappings(params);
  }

  static async createModelMapping(input: ModelMappingCreateInput): Promise<ModelMappingRule> {
    const result = await getModelsBackendSdkClient().ai.modelMappings.create(input);
    ensureSdkworkApiSuccess(result, 'Failed to create model mapping');
    return normalizeModelMappingRule(readRequiredApiItem(result, 'Created model mapping response is missing item'));
  }

  static createMapping(input: ModelMappingCreateInput): Promise<ModelMappingRule> {
    return ModelMappingService.createModelMapping(input);
  }

  static async updateModelMapping(id: string, input: ModelMappingUpdateInput): Promise<ModelMappingRule> {
    const result = await getModelsBackendSdkClient().ai.modelMappings.update(
      requiredSafePathSegment(id, 'mappingId'),
      input,
    );
    ensureSdkworkApiSuccess(result, 'Failed to update model mapping');
    return normalizeModelMappingRule(readRequiredApiItem(result, 'Updated model mapping response is missing item'));
  }

  static updateMapping(id: string, input: ModelMappingUpdateInput): Promise<ModelMappingRule> {
    return ModelMappingService.updateModelMapping(id, input);
  }

  static async deleteModelMapping(id: string): Promise<boolean> {
    const result = await getModelsBackendSdkClient().ai.modelMappings.delete(
      requiredSafePathSegment(id, 'mappingId'),
    );
    if (result?.deleted !== true) {
      throw new Error('Model mapping delete confirmation is required');
    }
    return true;
  }

  static deleteMapping(id: string): Promise<boolean> {
    return ModelMappingService.deleteModelMapping(id);
  }

  static async resolveModelMapping(input: ModelMappingResolveInput): Promise<ModelMappingResolveResult> {
    const result = await getModelsBackendSdkClient().ai.modelMappings.resolve(input);
    ensureSdkworkApiSuccess(result, 'Failed to resolve model mapping');
    return normalizeModelMappingResolveResult(result);
  }

  static resolveMapping(input: ModelMappingResolveInput): Promise<ModelMappingResolveResult> {
    return ModelMappingService.resolveModelMapping(input);
  }
}

async function updateModelPatch(
  id: string,
  model: ModelPatchInput,
  errorMessage: string,
): Promise<Model> {
  const result = await getModelsBackendSdkClient().ai.models.update(
    requiredSafePathSegment(id, 'modelId'),
    toUpdateModelRequest(model),
  );
  ensureSdkworkApiSuccess(result, errorMessage);
  return normalizeModel(readRequiredApiItem(result, 'Updated model response is missing data'));
}

function normalizeModelRankingRefreshStatus(value: ApiRecord): ModelRankingRefreshStatusView {
  const status = readRequiredString(value, 'status', 'Model ranking refresh status is required');
  if (status !== 'ready' && status !== 'empty' && status !== 'unavailable') {
    throw new Error(`Unsupported model ranking refresh status: ${status}`);
  }
  return {
    status,
    tenantId: readRequiredNonNegativeInt64String(value, 'tenantId', 'Model ranking refresh status tenant id'),
    organizationId: readRequiredNonNegativeInt64String(value, 'organizationId', 'Model ranking refresh status organization id'),
    rankScope: readRequiredString(value, 'rankScope', 'Model ranking refresh status is missing rankScope'),
    snapshotDate: readString(value, 'snapshotDate'),
    snapshotPeriod: readRequiredString(value, 'snapshotPeriod', 'Model ranking refresh status is missing snapshotPeriod'),
    windowStart: readString(value, 'windowStart'),
    windowEnd: readString(value, 'windowEnd'),
    generatedAt: readString(value, 'generatedAt'),
    refreshIntervalSeconds: readRequiredPositiveInteger(value, 'refreshIntervalSeconds', 'Model ranking refresh status refresh interval seconds'),
    nextRefreshAt: readString(value, 'nextRefreshAt'),
    cacheMaxAgeSeconds: readRequiredPositiveInteger(value, 'cacheMaxAgeSeconds', 'Model ranking refresh status cache max age seconds'),
    generatedCount: readRequiredNonNegativeInteger(value, 'generatedCount', 'Model ranking refresh status generated count'),
    sourceCount: readRequiredNonNegativeInteger(value, 'sourceCount', 'Model ranking refresh status source count'),
    sourceTables: readStringArray(value, 'sourceTables'),
    latestJob: isRecord(value.latestJob) ? normalizeModelRankingRefreshJob(value.latestJob) : null,
  };
}

function normalizeModelRankingRefreshJob(value: unknown): ModelRankingRefreshJobView {
  const item = readRequiredRecord(value, 'Model ranking refresh job record is required');
  const status = readRequiredString(item, 'status', 'Model ranking refresh job status is required');
  if (status !== 'succeeded' && status !== 'failed' && status !== 'empty' && status !== 'skipped' && status !== 'running') {
    throw new Error(`Unsupported model ranking refresh job status: ${status}`);
  }
  return {
    id: readRequiredString(item, 'id', 'Model ranking refresh job id is required'),
    jobName: readRequiredString(item, 'jobName', 'Model ranking refresh job name is required'),
    status,
    tenantId: readRequiredNonNegativeInt64String(item, 'tenantId', 'Model ranking refresh job tenant id'),
    organizationId: readRequiredNonNegativeInt64String(item, 'organizationId', 'Model ranking refresh job organization id'),
    rankScope: readRequiredString(item, 'rankScope', 'Model ranking refresh job is missing rankScope'),
    snapshotDate: readString(item, 'snapshotDate'),
    snapshotPeriod: readRequiredString(item, 'snapshotPeriod', 'Model ranking refresh job is missing snapshotPeriod'),
    windowStart: readString(item, 'windowStart'),
    windowEnd: readString(item, 'windowEnd'),
    startedAt: readString(item, 'startedAt'),
    endedAt: readString(item, 'endedAt'),
    durationMs: readRequiredNonNegativeInteger(item, 'durationMs', 'Model ranking refresh job duration ms'),
    generatedCount: readRequiredNonNegativeInteger(item, 'generatedCount', 'Model ranking refresh job generated count'),
    sourceCount: readRequiredNonNegativeInteger(item, 'sourceCount', 'Model ranking refresh job source count'),
    successCount: readRequiredNonNegativeInteger(item, 'successCount', 'Model ranking refresh job success count'),
    failureCount: readRequiredNonNegativeInteger(item, 'failureCount', 'Model ranking refresh job failure count'),
    nextRefreshAt: readString(item, 'nextRefreshAt'),
    failureReason: readNullableString(item, 'failureReason'),
  };
}

function normalizeModelRankingRefreshTrigger(value: ApiRecord): ModelRankingRefreshTriggerView {
  const status = readRequiredString(value, 'status', 'Model ranking refresh trigger status is required');
  if (status !== 'succeeded' && status !== 'empty') {
    throw new Error(`Unsupported model ranking refresh trigger status: ${status}`);
  }
  return {
    triggered: readRequiredBoolean(value, 'triggered', 'Model ranking refresh trigger response is missing triggered flag'),
    status,
    tenantId: readRequiredNonNegativeInt64String(value, 'tenantId', 'Model ranking refresh trigger tenant id'),
    organizationId: readRequiredNonNegativeInt64String(value, 'organizationId', 'Model ranking refresh trigger organization id'),
    rankScope: readRequiredString(value, 'rankScope', 'Model ranking refresh trigger response is missing rankScope'),
    snapshotDate: readRequiredString(value, 'snapshotDate', 'Model ranking refresh trigger response is missing snapshotDate'),
    snapshotPeriod: readSnapshotPeriod(value, 'snapshotPeriod', 'Model ranking refresh trigger response is missing snapshotPeriod'),
    windowStart: readRequiredString(value, 'windowStart', 'Model ranking refresh trigger response is missing windowStart'),
    windowEnd: readRequiredString(value, 'windowEnd', 'Model ranking refresh trigger response is missing windowEnd'),
    generatedCount: readRequiredNonNegativeInteger(value, 'generatedCount', 'Model ranking refresh trigger generated count'),
    sourceCount: readRequiredNonNegativeInteger(value, 'sourceCount', 'Model ranking refresh trigger source count'),
    refreshIntervalSeconds: readRequiredPositiveInteger(value, 'refreshIntervalSeconds', 'Model ranking refresh trigger refresh interval seconds'),
    cacheMaxAgeSeconds: readRequiredPositiveInteger(value, 'cacheMaxAgeSeconds', 'Model ranking refresh trigger cache max age seconds'),
    nextRefreshAt: readRequiredString(value, 'nextRefreshAt', 'Model ranking refresh trigger response is missing nextRefreshAt'),
  };
}

function toModelRankingRefreshTriggerRequest(): ModelRankingRefreshTriggerRequest {
  return {
    rankScope: 'commercial-default',
    snapshotPeriod: 'daily',
    limit: '200',
    lookbackDays: '7',
    refreshIntervalSeconds: '3600',
    cacheMaxAgeSeconds: '60',
  };
}

function toSyncCatalogRequest(): AdminModelCatalogSyncRequest {
  return {
    source: 'sdkwork_models',
    mode: 'official_refresh',
    force: true,
  };
}

function toCreateVendorRequest(vendor: VendorCreateInput): AdminModelVendorCreateRequest {
  return {
    name: requiredText(vendor.name, 'name'),
    status: vendor.status,
    color: safeStyleToken(vendor.color || 'bg-slate-700'),
    description: optionalText(vendor.description, 'description', 512),
  };
}

function toCreateModelRequest(model: ModelCreateInput): AdminAiModelCreateRequest {
  const regionPrices = normalizedRegionPrices(model);
  const runtimeModel = resolveModelIdentifier(model);
  const request: AdminAiModelCreateRequest = {
    vendorId: requiredText(model.vendorId, 'vendorId'),
    model: modelName(runtimeModel),
    displayName: optionalNullableText(model.displayName, 'displayName', 128) ?? undefined,
    type: modelType(model.type),
    regionPrices: regionPrices.map((regionPrice) => ({
      regionCode: regionCode(regionPrice.regionCode, 'regionPrices.regionCode'),
      currency: currencyCode(regionPrice.currency, `regionPrices.${regionPrice.regionCode}.currency`),
      priceIn: decimalAmount(regionPrice.priceIn, `regionPrices.${regionPrice.regionCode}.priceIn`),
      priceOut: decimalAmount(regionPrice.priceOut, `regionPrices.${regionPrice.regionCode}.priceOut`),
      cacheReadPrice: optionalDecimalAmount(regionPrice.cacheReadPrice, `regionPrices.${regionPrice.regionCode}.cacheReadPrice`),
      cacheWritePrice: optionalDecimalAmount(regionPrice.cacheWritePrice, `regionPrices.${regionPrice.regionCode}.cacheWritePrice`),
    })),
    contextTokens: requiredText(model.contextTokens, 'contextTokens'),
    ...defaultModelCreateMetadata(model.type),
    ...modelCapabilityMetadata(model),
  };
  return removeUndefinedProperties(request);
}

function toUpdateModelRequest(model: ModelPatchInput): AdminAiModelUpdateRequest {
  const request: AdminAiModelUpdateRequest = {
    ...modelCapabilityMetadata(model),
  };
  if (model.vendorId !== undefined) {
    request.vendorId = requiredText(model.vendorId, 'vendorId');
  }
  if (model.model !== undefined) {
    request.model = modelName(resolveModelIdentifier(model));
  }
  if (model.displayName !== undefined) {
    request.displayName = optionalNullableText(model.displayName, 'displayName', 128) ?? undefined;
  }
  if (model.regionPrices !== undefined) {
    request.regionPrices = normalizedRegionPrices(model).map((regionPrice) => ({
      regionCode: regionCode(regionPrice.regionCode, 'regionPrices.regionCode'),
      currency: currencyCode(regionPrice.currency, `regionPrices.${regionPrice.regionCode}.currency`),
      priceIn: decimalAmount(regionPrice.priceIn, `regionPrices.${regionPrice.regionCode}.priceIn`),
      priceOut: decimalAmount(regionPrice.priceOut, `regionPrices.${regionPrice.regionCode}.priceOut`),
      cacheReadPrice: optionalDecimalAmount(regionPrice.cacheReadPrice, `regionPrices.${regionPrice.regionCode}.cacheReadPrice`),
      cacheWritePrice: optionalDecimalAmount(regionPrice.cacheWritePrice, `regionPrices.${regionPrice.regionCode}.cacheWritePrice`),
    }));
  }
  if (model.contextTokens !== undefined) {
    request.contextTokens = requiredText(model.contextTokens, 'contextTokens');
  }
  if (model.status !== undefined) {
    request.status = model.status;
  }
  if (model.type !== undefined) {
    const nextType = modelType(model.type);
    if (!model.currentType || modelType(model.currentType) !== nextType) {
      Object.assign(request, defaultModelCreateMetadata(nextType));
      request.type = nextType;
    }
  }
  return request;
}

function normalizedRegionPrices(model: {
  regionPrices?: ModelRegionPriceInput[];
}): ModelRegionPriceInput[] {
  if (Array.isArray(model.regionPrices) && model.regionPrices.length > 0) {
    return model.regionPrices;
  }
  throw new Error('regionPrices is required');
}

function requiredText(value: string, fieldName: string): string {
  const normalized = value.trim();
  if (!normalized) {
    throw new Error(`${fieldName} is required`);
  }
  return normalized;
}

function optionalText(value: string | undefined, fieldName: string, maxLength: number): string {
  if (value === undefined) {
    return '';
  }
  const normalized = value.trim();
  if (normalized.length > maxLength) {
    throw new Error(`${fieldName} must be at most ${maxLength} characters`);
  }
  return normalized;
}

function optionalNullableText(value: string | null | undefined, fieldName: string, maxLength: number): string | null {
  if (value === null) {
    return null;
  }
  const normalized = optionalText(value, fieldName, maxLength);
  return normalized || null;
}

function modelName(value: string): string {
  const normalized = requiredText(value, 'model');
  if (!/^[A-Za-z0-9._:/-]+$/.test(normalized)) {
    throw new Error('model must use ASCII letters, numbers, dot, underscore, colon, slash, or hyphen');
  }
  return normalized;
}

function resolveModelIdentifier(model: Pick<ModelCreateInput, 'model'>): string {
  const runtimeModel = model.model?.trim() || '';
  if (!runtimeModel) {
    throw new Error('model is required');
  }
  return runtimeModel;
}

function regionCode(value: string, fieldName: string): string {
  const normalized = requiredText(value, fieldName);
  if (!/^[a-z0-9][a-z0-9_-]{0,63}$/.test(normalized)) {
    throw new Error(`${fieldName} must be a lowercase region code`);
  }
  return normalized;
}

function currencyCode(value: string, fieldName: string): string {
  const normalized = requiredText(value, fieldName).toUpperCase();
  if (!/^[A-Z]{3}$/.test(normalized)) {
    throw new Error(`${fieldName} must be a 3-letter ISO currency code`);
  }
  return normalized;
}

function modelType(value: Model['type']): AdminAiModelType {
  switch (value) {
    case 'Chat':
      return 'chat';
    case 'Image':
      return 'image';
    case 'Audio':
    case 'Music':
    case 'SoundEffect':
      return 'audio';
    case 'Embedding':
      return 'embedding';
    case 'Video':
      return 'video';
    default:
      throw new Error(value ? `Unsupported model type: ${value}` : 'Model type is required');
  }
}

function decimalAmount(value: string, fieldName: string): string {
  const normalized = requiredText(value, fieldName).replace(/,/g, '');
  if (!/^[0-9]+(\.[0-9]{1,12})?$/.test(normalized)) {
    throw new Error(`${fieldName} must be a positive decimal amount`);
  }
  const numeric = Number(normalized);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    throw new Error(`${fieldName} must be greater than zero`);
  }
  return normalized;
}

function optionalDecimalAmount(value: string | undefined, fieldName: string): string | undefined {
  if (value === undefined) {
    return undefined;
  }
  const normalized = value.trim().replace(/,/g, '');
  if (!normalized) {
    return '';
  }
  if (!/^[0-9]+(\.[0-9]{1,12})?$/.test(normalized)) {
    throw new Error(`${fieldName} must be a positive decimal amount`);
  }
  const numeric = Number(normalized);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    throw new Error(`${fieldName} must be greater than zero`);
  }
  return normalized;
}

function removeUndefinedProperties<T extends object>(value: T): T {
  const record = value as Record<string, unknown>;
  for (const key of Object.keys(value)) {
    if (record[key] === undefined) {
      delete record[key];
    }
  }
  return value;
}

function optionalNonNegativeInteger(value: number | null | undefined, fieldName: string): number | null {
  if (value === null || value === undefined) {
    return null;
  }
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${fieldName} must be a non-negative integer`);
  }
  return value;
}

function optionalNonNegativeInt64String(value: number | null | undefined, fieldName: string): string | null {
  const normalized = optionalNonNegativeInteger(value, fieldName);
  return normalized === null ? null : String(normalized);
}

function boundedStringArray(values: string[] | undefined, fieldName: string, maxItems: number, maxLength: number): string[] {
  const normalizedValues = values ?? [];
  if (normalizedValues.length > maxItems) {
    throw new Error(`${fieldName} must contain at most ${maxItems} items`);
  }
  return normalizedValues.map((value) => {
    const normalized = requiredText(value, fieldName);
    if (normalized.length > maxLength) {
      throw new Error(`${fieldName} items must be at most ${maxLength} characters`);
    }
    return normalized;
  });
}

function modelCapabilityMetadata(model: Partial<ModelCreateInput>): Partial<Pick<
  AdminAiModelCreateRequest,
  | 'description'
  | 'capabilityIntro'
  | 'limitations'
  | 'supportedLanguages'
  | 'useCases'
  | 'maxOutputTokens'
  | 'supportsStreaming'
  | 'supportsTools'
  | 'supportsJsonSchema'
>> {
  const metadata: Partial<AdminAiModelCreateRequest> = {};
  if (model.description !== undefined) {
    metadata.description = optionalNullableText(model.description, 'description', 2048);
  }
  if (model.capabilityIntro !== undefined) {
    metadata.capabilityIntro = optionalNullableText(model.capabilityIntro, 'capabilityIntro', 4096);
  }
  if (model.limitations !== undefined) {
    metadata.limitations = boundedStringArray(model.limitations, 'limitations', 64, 512);
  }
  if (model.supportedLanguages !== undefined) {
    metadata.supportedLanguages = boundedStringArray(model.supportedLanguages, 'supportedLanguages', 128, 128);
  }
  if (model.useCases !== undefined) {
    metadata.useCases = boundedStringArray(model.useCases, 'useCases', 64, 256);
  }
  if (model.maxOutputTokens !== undefined) {
    metadata.maxOutputTokens = optionalNonNegativeInt64String(model.maxOutputTokens, 'maxOutputTokens');
  }
  if (typeof model.supportsStreaming === 'boolean') {
    metadata.supportsStreaming = model.supportsStreaming;
  }
  if (typeof model.supportsTools === 'boolean') {
    metadata.supportsTools = model.supportsTools;
  }
  if (typeof model.supportsJsonSchema === 'boolean') {
    metadata.supportsJsonSchema = model.supportsJsonSchema;
  }
  return metadata;
}

function safeStyleToken(value: string): string {
  const normalized = requiredText(value, 'color');
  if (!/^[A-Za-z0-9_:/#-]{1,64}$/.test(normalized)) {
    throw new Error('color must be a safe style token');
  }
  return normalized;
}


function normalizeVendor(value: unknown): Vendor {
  const item = readRequiredRecord(value, 'Vendor record is required');
  return {
    id: readRequiredString(item, 'id', 'Vendor id is required'),
    vendorCode: readRequiredString(item, 'vendorCode', 'Vendor code is required'),
    name: readRequiredString(item, 'name', 'Vendor name is required'),
    status: readVendorStatus(item),
    color: readRequiredString(item, 'color', 'Vendor color is required'),
    description: readRequiredString(item, 'description', 'Vendor description is required'),
    modelCount: readRequiredNonNegativeInteger(item, 'modelCount', 'Vendor model count'),
  };
}

function normalizeModel(value: unknown): Model {
  const item = readRequiredRecord(value, 'Model record is required');
  const runtimeModel = readModelIdentifier(item);
  const displayName = readModelDisplayName(item, runtimeModel);
  return {
    id: readRequiredString(item, 'id', 'Model id is required'),
    vendorId: readRequiredString(item, 'vendorId', 'Model vendor id is required'),
    vendorCode: readRequiredString(item, 'vendorCode', 'Model vendor code is required'),
    model: runtimeModel,
    displayName,
    name: displayName,
    type: readModelType(item),
    regionPrices: readModelRegionPrices(item),
    status: readModelStatus(item),
    calls: readRequiredString(item, 'calls', 'Model calls are required'),
    description: readRequiredNullableString(item, 'description', 'Model description field is required'),
    modalities: readRequiredStringArray(item, 'modalities', 'Model modalities are required'),
    inputModalities: readRequiredStringArray(item, 'inputModalities', 'Model input modalities are required'),
    outputModalities: readRequiredStringArray(item, 'outputModalities', 'Model output modalities are required'),
    apiFormat: readRequiredNullableString(item, 'apiFormat', 'Model API format field is required'),
    capabilityIntro: readRequiredNullableString(item, 'capabilityIntro', 'Model capability intro field is required'),
    limitations: readRequiredStringArray(item, 'limitations', 'Model limitations are required'),
    supportedLanguages: readRequiredStringArray(item, 'supportedLanguages', 'Model supported languages are required'),
    useCases: readRequiredStringArray(item, 'useCases', 'Model use cases are required'),
    trainingDataCutoff: readRequiredNullableString(item, 'trainingDataCutoff', 'Model training data cutoff field is required'),
    contextTokens: readRequiredContextTokens(item),
    maxOutputTokens: readRequiredNullableNonNegativeInteger(item, 'maxOutputTokens', 'Model max output tokens field is required', 'Model max output tokens'),
    supportsStreaming: readRequiredBoolean(item, 'supportsStreaming', 'Model streaming support flag is required'),
    supportsTools: readRequiredBoolean(item, 'supportsTools', 'Model tools support flag is required'),
    supportsJsonSchema: readRequiredBoolean(item, 'supportsJsonSchema', 'Model JSON schema support flag is required'),
    releaseStage: readRequiredNullableNumber(item, 'releaseStage', 'Model release stage field is required', 'Model release stage'),
    shelfState: readRequiredNullableNumber(item, 'shelfState', 'Model shelf state field is required', 'Model shelf state'),
    routingState: readRequiredNullableNumber(item, 'routingState', 'Model routing state field is required', 'Model routing state'),
    replacementModel: readRequiredNullableString(item, 'replacementModel', 'Model replacement model field is required'),
  };
}

function normalizeModelMappingModelOption(value: unknown): ModelMappingModelOption {
  const item = readRequiredRecord(value, 'Model mapping model option record is required');
  const runtimeModel = readModelIdentifier(item);
  const displayName = readModelDisplayName(item, runtimeModel);
  return {
    id: readRequiredString(item, 'id', 'Model option id is required'),
    vendorId: readRequiredString(item, 'vendorId', 'Model option vendor id is required'),
    vendorCode: readRequiredString(item, 'vendorCode', 'Model option vendor code is required'),
    model: runtimeModel,
    displayName,
    name: displayName,
    type: readModelType(item),
    status: readModelStatus(item),
  };
}

function readModelRegionPrices(item: ApiRecord): ModelRegionPriceInput[] {
  if (!('regionPrices' in item) || item.regionPrices === null || item.regionPrices === undefined) {
    throw new Error('Model region prices are required');
  }
  if (!Array.isArray(item.regionPrices)) {
    throw new Error('Model region prices must be an array');
  }
  if (item.regionPrices.length === 0) {
    return [];
  }
  return item.regionPrices.map((value) => {
    const regionPrice = readRequiredRecord(value, 'Model region price record is required');
    return {
      regionCode: readRequiredString(regionPrice, 'regionCode', 'Model region price region code is required'),
      currency: currencyCode(readRequiredString(regionPrice, 'currency', 'Model region price currency is required'), 'regionPrices.currency'),
      priceIn: readRequiredStringField(regionPrice, 'priceIn', 'Model region input price is required'),
      priceOut: readRequiredStringField(regionPrice, 'priceOut', 'Model region output price is required'),
      cacheReadPrice: readString(regionPrice, 'cacheReadPrice').trim(),
      cacheWritePrice: readString(regionPrice, 'cacheWritePrice').trim(),
    };
  });
}

function readModelIdentifier(item: ApiRecord): string {
  return readRequiredStringField(item, 'model', 'Model model is required');
}

function readModelDisplayName(item: ApiRecord, runtimeModel: string): string {
  const displayName = readString(item, 'displayName').trim();
  if (displayName) {
    return displayName;
  }
  const name = readString(item, 'name').trim();
  return name || runtimeModel;
}

function modelCallsByName(items: ModelRankingMetricItem[]): Map<string, string> {
  const callsByName = new Map<string, string>();
  items
    .forEach((item) => {
      const requests = Number(item.requests);
      const baseVolume = Number(item.baseVolume);
      const calls = Number.isFinite(requests) && requests > 0 ? requests : baseVolume;
      callsByName.set(item.name, formatCount(calls));
    });
  return callsByName;
}

function normalizeRankingItem(value: unknown): ModelRankingMetricItem | null {
  if (!isRecord(value)) {
    return null;
  }
  const name = readString(value, 'name').trim();
  if (!name) {
    return null;
  }
  return {
    name,
    requests: readRequiredNonNegativeInt64String(value, 'requests', 'Admin model ranking requests'),
    baseVolume: readRequiredNonNegativeInt64String(value, 'baseVolume', 'Admin model ranking base volume'),
  };
}

function readRequiredNonNegativeInteger(record: ApiRecord, key: string, label: string): number {
  const value = readNumber(record, key, Number.NaN);
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${label} must be a non-negative integer`);
  }
  return value;
}

function readPageTotalItems(pageInfo: ApiRecord, fallback: number): number {
  const value = pageInfo.totalItems;
  if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) {
    return value;
  }
  if (typeof value === 'string' && /^(0|[1-9]\d*)$/u.test(value)) {
    const parsed = Number(value);
    return Number.isSafeInteger(parsed) ? parsed : fallback;
  }
  return fallback;
}

function readRequiredNonNegativeInt64String(record: ApiRecord, key: string, label: string): string {
  const value = readString(record, key).trim();
  if (!/^(0|[1-9]\d*)$/u.test(value)) {
    throw new Error(`${label} must be a non-negative integer`);
  }
  return value;
}

function readRequiredPositiveInteger(record: ApiRecord, key: string, label: string): number {
  const value = readNumber(record, key, Number.NaN);
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${label} must be a positive integer`);
  }
  return value;
}

function readRequiredStringArray(item: ApiRecord, key: string, message: string): string[] {
  if (!Array.isArray(item[key])) {
    throw new Error(message);
  }
  return readStringArray(item, key);
}

function readRequiredNullableString(item: ApiRecord, key: string, message: string): string | null {
  if (!(key in item)) {
    throw new Error(message);
  }
  return readNullableString(item, key);
}

function readRequiredStringField(item: ApiRecord, key: string, message: string): string {
  if (!(key in item)) {
    throw new Error(message);
  }
  return readString(item, key).trim();
}

function readOptionalString(item: ApiRecord, key: string): string | undefined {
  if (!(key in item) || item[key] === null || item[key] === undefined) {
    return undefined;
  }
  const value = readString(item, key).trim();
  return value.length > 0 ? value : undefined;
}

function readRequiredNullableNumber(item: ApiRecord, key: string, message: string, label: string): number | null {
  if (!(key in item)) {
    throw new Error(message);
  }
  const value = item[key];
  if (value === null || value === '') {
    return null;
  }
  const parsed = readNumber(item, key, Number.NaN);
  if (!Number.isFinite(parsed)) {
    throw new Error(`${label} must be a number or null`);
  }
  return parsed;
}

function readRequiredContextTokens(item: ApiRecord): number | null {
  return readRequiredNullableNonNegativeInteger(
    item,
    'contextTokens',
    'Model context tokens field is required',
    'Model context tokens',
  );
}

function readRequiredNullableNonNegativeInteger(
  item: ApiRecord,
  key: string,
  missingMessage: string,
  label: string,
): number | null {
  if (!(key in item)) {
    throw new Error(missingMessage);
  }
  const value = item[key];
  if (value === null) {
    return null;
  }
  const parsed = readNumber(item, key, Number.NaN);
  if (!Number.isInteger(parsed) || parsed < 0) {
    throw new Error(`${label} must be a non-negative integer`);
  }
  return parsed;
}

function formatCount(value: number): string {
  if (!Number.isFinite(value) || value <= 0) {
    return '0';
  }
  if (value >= 1_000_000_000) {
    return `${trimDecimal(value / 1_000_000_000)}B`;
  }
  if (value >= 1_000_000) {
    return `${trimDecimal(value / 1_000_000)}M`;
  }
  if (value >= 1_000) {
    return `${trimDecimal(value / 1_000)}k`;
  }
  return Math.trunc(value).toLocaleString();
}

function trimDecimal(value: number): string {
  return value.toFixed(1).replace(/\.0$/u, '');
}

function readRequiredRecord(value: unknown, message: string): ApiRecord {
  if (!isRecord(value)) {
    throw new Error(message);
  }
  return value;
}


function normalizeModelMappingRule(value: unknown): ModelMappingRule {
  const item = readRequiredRecord(value, 'Model mapping rule must be an object');
  return {
    id: readRequiredString(item, 'id', 'Model mapping id is required'),
    bindingType: readModelMappingBindingType(item, 'bindingType'),
    sourceVendorId: readNullableString(item, 'sourceVendorId'),
    sourceVendorCode: readRequiredString(item, 'sourceVendorCode', 'Model mapping source vendor is required'),
    targetVendorId: readNullableString(item, 'targetVendorId'),
    targetVendorCode: readRequiredString(item, 'targetVendorCode', 'Model mapping target vendor is required'),
    mappingMode: readModelMappingMode(item),
    matchType: readModelMappingMatchType(item),
    enabled: readBoolean(item, 'enabled', true),
    bindings: readRequiredApiItems(item, 'Model mapping bindings are required', ['bindings'])
      .map(normalizeModelMappingBinding),
    mappingItems: readRequiredApiItems(item, 'Model mapping items are required', ['mappingItems'])
      .map(normalizeModelMappingItem),
    createdAt: readNullableString(item, 'createdAt'),
    updatedAt: readNullableString(item, 'updatedAt'),
  };
}

function normalizeModelMappingBinding(value: unknown): ModelMappingRuleBinding {
  const item = readRequiredRecord(value, 'Model mapping binding must be an object');
  return {
    id: readRequiredString(item, 'id', 'Model mapping binding id is required'),
    bindingType: readModelMappingBindingType(item, 'bindingType'),
    bindingId: readNullableString(item, 'bindingId'),
    bindingCode: readNullableString(item, 'bindingCode'),
    bindingName: readNullableString(item, 'bindingName'),
    sortOrder: readNonNegativeInteger(item, 'sortOrder', 100),
    enabled: readBoolean(item, 'enabled', true),
    createdAt: readNullableString(item, 'createdAt'),
    updatedAt: readNullableString(item, 'updatedAt'),
  };
}

function normalizeModelMappingItem(value: unknown): ModelMappingRuleItem {
  const item = readRequiredRecord(value, 'Model mapping item must be an object');
  return {
    id: readRequiredString(item, 'id', 'Model mapping item id is required'),
    sourceModel: readRequiredString(item, 'sourceModel', 'Model mapping source model is required'),
    sourceCatalogKey: readNullableString(item, 'sourceCatalogKey'),
    targetModel: readRequiredString(item, 'targetModel', 'Model mapping target model is required'),
    targetCatalogKey: readNullableString(item, 'targetCatalogKey'),
    targetProviderModel: readNullableString(item, 'targetProviderModel'),
    targetProviderNativeModel: readNullableString(item, 'targetProviderNativeModel'),
    sortOrder: readNonNegativeInteger(item, 'sortOrder', 100),
    enabled: readBoolean(item, 'enabled', true),
    createdAt: readNullableString(item, 'createdAt'),
    updatedAt: readNullableString(item, 'updatedAt'),
  };
}

function normalizeModelMappingResolveResult(value: unknown): ModelMappingResolveResult {
  const item = readRequiredRecord(value, 'Model mapping resolve response must be an object');
  const matchedBindingType = readNullableString(item, 'matchedBindingType');
  return {
    matched: readBoolean(item, 'matched', false),
    matchedBindingType: matchedBindingType ? readModelMappingBindingType({ matchedBindingType }, 'matchedBindingType') : null,
    sourceModel: readRequiredString(item, 'sourceModel', 'Model mapping resolve source model is required'),
    targetModel: readRequiredString(item, 'targetModel', 'Model mapping resolve target model is required'),
    targetCatalogKey: readNullableString(item, 'targetCatalogKey'),
    targetVendorCode: readNullableString(item, 'targetVendorCode'),
    targetProviderModel: readNullableString(item, 'targetProviderModel'),
    targetProviderNativeModel: readNullableString(item, 'targetProviderNativeModel'),
    rule: isRecord(item.rule) ? normalizeModelMappingRule(item.rule) : null,
  };
}



function readModelMappingBindingType(item: ApiRecord, key: string): ModelMappingBindingType {
  const value = readRequiredString(item, key, 'Model mapping binding type is required');
  if (
    value === 'global'
    || value === 'vendor'
    || value === 'channel_group'
    || value === 'channel'
    || value === 'provider_account'
    || value === 'site'
    || value === 'site_service'
  ) {
    return value;
  }
  throw new Error(`Unsupported model mapping binding type: ${value}`);
}

function readModelMappingMode(item: ApiRecord): ModelMappingRule['mappingMode'] {
  const value = readRequiredString(item, 'mappingMode', 'Model mapping mode is required');
  if (value === 'alias') {
    return value;
  }
  throw new Error(`Unsupported model mapping mode: ${value}`);
}

function readModelMappingMatchType(item: ApiRecord): ModelMappingRule['matchType'] {
  const value = readRequiredString(item, 'matchType', 'Model mapping match type is required');
  if (value === 'exact') {
    return value;
  }
  throw new Error(`Unsupported model mapping match type: ${value}`);
}

function readNonNegativeInteger(item: ApiRecord, key: string, fallback: number): number {
  const value = item[key];
  if (value === undefined || value === null || value === '') {
    return fallback;
  }
  const parsed = readNumber(item, key, Number.NaN);
  if (!Number.isInteger(parsed) || parsed < 0) {
    throw new Error(`${key} must be a non-negative integer`);
  }
  return parsed;
}

function readNullableNumberField(item: ApiRecord, key: string, label: string): number | null {
  const value = item[key];
  if (value === undefined || value === null || value === '') {
    return null;
  }
  const numeric = readNumber(item, key, Number.NaN);
  if (!Number.isFinite(numeric) || numeric < 0) {
    throw new Error(`${label} must be a non-negative number`);
  }
  return numeric;
}

function readRequiredBoolean(item: ApiRecord, key: string, message: string): boolean {
  const value = item[key];
  if (typeof value === 'boolean') {
    return value;
  }
  throw new Error(message);
}

function readVendorStatus(item: ApiRecord): Vendor['status'] {
  const status = readRequiredString(item, 'status', 'Vendor status is required');
  if (status === 'active' || status === 'inactive') {
    return status;
  }
  throw new Error(`Unsupported vendor status: ${status}`);
}

function readModelStatus(item: ApiRecord): Model['status'] {
  const status = readRequiredString(item, 'status', 'Model status is required');
  if (status === 'active' || status === 'inactive') {
    return status;
  }
  throw new Error(`Unsupported model status: ${status}`);
}

function readSyncMode(item: ApiRecord): ModelCatalogSyncResult['mode'] {
  const mode = readRequiredString(item, 'mode', 'Model catalog sync response is missing mode');
  if (mode === 'official_refresh' || mode === 'vendor_refresh' || mode === 'catalog_version_refresh' || mode === 'dry_run') {
    return mode;
  }
  throw new Error(`Unsupported model catalog sync mode: ${mode}`);
}

function readSourceHash(item: ApiRecord): string {
  const value = readRequiredString(item, 'sourceHash', 'Model catalog sync response is missing sourceHash');
  if (!/^[a-f0-9]{64}$/.test(value)) {
    throw new Error('Model catalog sync sourceHash must be a 64 character lowercase SHA-256 hex digest');
  }
  return value;
}

function readSnapshotPeriod(
  item: ApiRecord,
  key: string,
  message: string,
): ModelRankingRefreshTriggerResponse['snapshotPeriod'] {
  const period = readRequiredString(item, key, message);
  if (period === 'hourly' || period === 'daily' || period === 'weekly' || period === 'monthly') {
    return period;
  }
  throw new Error(`Unsupported model ranking snapshot period: ${period}`);
}

function readModelType(item: ApiRecord): Model['type'] {
  const type = readString(item, 'type');
  switch (type) {
    case 'Chat':
    case 'chat':
      return 'Chat';
    case 'Image':
    case 'image':
      return 'Image';
    case 'Audio':
    case 'audio':
      return 'Audio';
    case 'Embedding':
    case 'embedding':
      return 'Embedding';
    case 'Music':
      return 'Music';
    case 'SoundEffect':
      return 'SoundEffect';
    case 'Video':
    case 'video':
      return 'Video';
    default:
      throw new Error(type ? `Unsupported model type: ${type}` : 'Model type is required');
  }
}

function readNullableNumber(item: ApiRecord, key: string): number | null {
  const value = item[key];
  if (value === null || value === undefined || value === '') {
    return null;
  }
  const parsed = readNumber(item, key, Number.NaN);
  return Number.isFinite(parsed) ? parsed : null;
}

function defaultModelCreateMetadata(type: ModelMetadataType): Pick<
  AdminAiModelCreateRequest,
  | 'modalities'
  | 'inputModalities'
  | 'outputModalities'
  | 'apiFormat'
  | 'supportsStreaming'
  | 'supportsTools'
  | 'supportsJsonSchema'
  | 'releaseStage'
  | 'shelfState'
  | 'routingState'
> {
  const common = {
    releaseStage: 1,
    shelfState: 1,
    routingState: 1,
  };
  switch (type) {
    case 'Image':
    case 'image':
      return {
        ...common,
        modalities: ['image'],
        inputModalities: ['text', 'image'],
        outputModalities: ['image'],
        apiFormat: 'openai_compatible',
        supportsStreaming: false,
        supportsTools: false,
        supportsJsonSchema: false,
      };
    case 'Audio':
    case 'audio':
      return {
        ...common,
        modalities: ['audio'],
        inputModalities: ['audio', 'text'],
        outputModalities: ['audio', 'text'],
        apiFormat: 'openai_compatible',
        supportsStreaming: false,
        supportsTools: false,
        supportsJsonSchema: false,
      };
    case 'Embedding':
    case 'embedding':
      return {
        ...common,
        modalities: ['embedding'],
        inputModalities: ['text'],
        outputModalities: ['embedding'],
        apiFormat: 'openai_compatible',
        supportsStreaming: false,
        supportsTools: false,
        supportsJsonSchema: false,
      };
    case 'Music':
      return {
        ...common,
        modalities: ['music'],
        inputModalities: ['text', 'audio'],
        outputModalities: ['audio'],
        apiFormat: 'openai_compatible',
        supportsStreaming: false,
        supportsTools: false,
        supportsJsonSchema: false,
      };
    case 'SoundEffect':
      return {
        ...common,
        modalities: ['sfx'],
        inputModalities: ['text', 'audio'],
        outputModalities: ['audio'],
        apiFormat: 'openai_compatible',
        supportsStreaming: false,
        supportsTools: false,
        supportsJsonSchema: false,
      };
    case 'Video':
    case 'video':
      return {
        ...common,
        modalities: ['video'],
        inputModalities: ['text', 'image', 'video'],
        outputModalities: ['video'],
        apiFormat: 'openai_compatible',
        supportsStreaming: false,
        supportsTools: false,
        supportsJsonSchema: false,
      };
    default:
      return {
        ...common,
        modalities: ['text'],
        inputModalities: ['text', 'image'],
        outputModalities: ['text'],
        apiFormat: 'openai_responses',
        supportsStreaming: true,
        supportsTools: true,
        supportsJsonSchema: true,
      };
  }
}
