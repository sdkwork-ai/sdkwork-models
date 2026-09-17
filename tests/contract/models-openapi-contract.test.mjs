#!/usr/bin/env node
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();

function readJson(relativePath) {
  return JSON.parse(readFileSync(join(root, relativePath), 'utf8'));
}

function responseDataSchemaRef(document, operation) {
  const responseSchema = operation?.responses?.['200']?.content?.['application/json']?.schema;
  const componentPrefix = '#/components/schemas/';
  const envelope = responseSchema?.$ref?.startsWith(componentPrefix)
    ? document.components?.schemas?.[responseSchema.$ref.slice(componentPrefix.length)]
    : responseSchema;
  return envelope?.allOf
    ?.find((part) => part?.properties?.data)
    ?.properties?.data?.allOf?.[0]?.$ref;
}

const backendOpenApi = readJson('apis/backend-api/intelligence/openapi.json');
const appOpenApi = readJson('apis/app-api/intelligence/openapi.json');
const resolveOperation = backendOpenApi.paths?.['/backend/v3/api/ai/model_mappings/resolve']?.post;
const modelMappingsListOperation = backendOpenApi.paths?.['/backend/v3/api/ai/model_mappings']?.get;
const appModelsListOperation = appOpenApi.paths?.['/app/v3/api/ai/models']?.get;
const appModelAccessChannelsListOperation = appOpenApi.paths?.['/app/v3/api/ai/model_access_channels']?.get;
const backendModelsListOperation = backendOpenApi.paths?.['/backend/v3/api/ai/models']?.get;
const aiResourceListOperations = [
  [
    'resources.list',
    '/backend/v3/api/ai/resources',
    '#/components/schemas/AiResourcesPage',
    '#/components/schemas/AdminAiResourceItem',
  ],
  [
    'resourceGroups.list',
    '/backend/v3/api/ai/resource_groups',
    '#/components/schemas/AiResourceGroupsPage',
    '#/components/schemas/AdminAiResourceGroupItem',
  ],
  [
    'resourceGroups.resources.list',
    '/backend/v3/api/ai/resource_groups/{groupIdOrCode}/resources',
    '#/components/schemas/AiResourceGroupResourcesPage',
    '#/components/schemas/AdminAiResourceGroupResourceItem',
  ],
];

const APP_API_METHODS = ['get', 'post', 'put', 'patch', 'delete'];

/**
 * app-api operations that serve public reference data. `API_SPEC.md` §366 scopes
 * `dual-token` to *protected* app-api routes, while §367 / §2168 require public
 * SDK-generated operations to materialize `security: []` together with
 * `x-sdkwork-auth-mode: anonymous` so generated clients skip credential
 * injection. The list is pinned instead of derived: an operation silently
 * gaining or losing public status must fail here rather than slide through.
 *
 * `x-route-scope` is not the discriminator — every operation in this authority
 * declares `public` while still being dual-token.
 */
const publicAppApiOperations = [
  'GET /app/v3/api/ai/model_access_channel_presets',
  'GET /app/v3/api/ai/model_access_channels',
  'GET /app/v3/api/ai/model_rankings',
  'GET /app/v3/api/ai/model_vendors',
  'GET /app/v3/api/ai/models',
  'GET /app/v3/api/ai/models/{modelId}/video_profiles',
  'GET /app/v3/api/ai/models/{modelId}/voices',
  'GET /app/v3/api/ai/video_profiles',
  'GET /app/v3/api/ai/voices',
];

const observedPublicAppApiOperations = [];
for (const [path, pathItem] of Object.entries(appOpenApi.paths ?? {})) {
  for (const [method, operation] of Object.entries(pathItem ?? {})) {
    if (!APP_API_METHODS.includes(method)) {
      continue;
    }
    const operationKey = `${method.toUpperCase()} ${path}`;
    if (operation['x-sdkwork-auth-mode'] === 'anonymous') {
      assert.deepEqual(
        operation.security,
        [],
        `${operationKey} is public app-api and must materialize security: [] so SDKs skip credential injection`,
      );
      observedPublicAppApiOperations.push(operationKey);
      continue;
    }
    assert.equal(
      operation['x-sdkwork-auth-mode'],
      'dual-token',
      `${operationKey} must be dual-token: only the pinned reference-data operations may be public`,
    );
    assert.deepEqual(
      operation.security,
      [{ AccessToken: [], AuthToken: [] }],
      `${operationKey} must require AuthToken and AccessToken together`,
    );
  }
}
assert.deepEqual(
  observedPublicAppApiOperations.sort(),
  [...publicAppApiOperations].sort(),
  'the public app-api surface must match the pinned reference-data list',
);

const writeOperationRequestBodies = [
  ['modelVendors.create', 'post', '/backend/v3/api/ai/model_vendors', '#/components/schemas/AdminModelVendorCreateRequest'],
  ['models.create', 'post', '/backend/v3/api/ai/models', '#/components/schemas/AdminAiModelCreateRequest'],
  ['models.update', 'patch', '/backend/v3/api/ai/models/{modelId}', '#/components/schemas/AdminAiModelUpdateRequest'],
  ['models.sync', 'post', '/backend/v3/api/ai/models/sync', '#/components/schemas/AdminModelCatalogSyncRequest'],
  ['modelMappings.create', 'post', '/backend/v3/api/ai/model_mappings', '#/components/schemas/AdminModelMappingCreateRequest'],
  ['modelMappings.update', 'patch', '/backend/v3/api/ai/model_mappings/{mappingId}', '#/components/schemas/AdminModelMappingUpdateRequest'],
  ['modelMappings.resolve', 'post', '/backend/v3/api/ai/model_mappings/resolve', '#/components/schemas/AdminModelMappingResolveRequest'],
  ['modelRankings.refresh', 'post', '/backend/v3/api/ai/model_rankings/refresh', '#/components/schemas/ModelRankingRefreshTriggerRequest'],
  ['resources.create', 'post', '/backend/v3/api/ai/resources', '#/components/schemas/AdminAiResourceCreateRequest'],
  ['resources.update', 'put', '/backend/v3/api/ai/resources/{resourceId}', '#/components/schemas/AdminAiResourceUpdateRequest'],
  ['resourceGroups.create', 'post', '/backend/v3/api/ai/resource_groups', '#/components/schemas/AdminAiResourceGroupCreateRequest'],
  ['resourceGroups.update', 'patch', '/backend/v3/api/ai/resource_groups/{groupId}', '#/components/schemas/AdminAiResourceGroupUpdateRequest'],
  ['resourceGroups.resources.update', 'put', '/backend/v3/api/ai/resource_groups/{groupId}/resources/{resourceCode}', '#/components/schemas/AdminAiResourceGroupMemberUpdateRequest'],
];

const expectedGeneratedBodyMethods = [
  ['modelVendors.create', /async create\([^)]*body:\s*AdminModelVendorCreateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/model_vendors`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['models.create', /async create\([^)]*body:\s*AdminAiModelCreateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/models`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['models.update', /async update\([^)]*body:\s*AdminAiModelUpdateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/models\/\$\{serializePathParameter\(modelId,[^\n]*\}`\), \{[^\n]*method: 'PATCH' as any,[^\n]*\bbody\b/],
  ['models.sync', /async sync\([^)]*body:\s*AdminModelCatalogSyncRequest[^)]*\): Promise<ModelCatalogSyncResult>/, /this\.client\.request<ModelCatalogSyncResult>\(backendApiPath\(`\/ai\/models\/sync`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['modelMappings.create', /async create\([^)]*body:\s*AdminModelMappingCreateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/model_mappings`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['modelMappings.update', /async update\([^)]*body:\s*AdminModelMappingUpdateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/model_mappings\/\$\{serializePathParameter\(mappingId,[^\n]*\}`\), \{[^\n]*method: 'PATCH' as any,[^\n]*\bbody\b/],
  ['modelMappings.resolve', /async resolve\([^)]*body:\s*AdminModelMappingResolveRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/model_mappings\/resolve`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['modelRankings.refresh', /async refresh\([^)]*body:\s*ModelRankingRefreshTriggerRequest[^)]*\): Promise<ModelRankingRefreshTriggerResponse>/, /this\.client\.request<ModelRankingRefreshTriggerResponse>\(backendApiPath\(`\/ai\/model_rankings\/refresh`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['resources.create', /async create\([^)]*body:\s*AdminAiResourceCreateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/resources`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['resources.update', /async update\([^)]*body:\s*AdminAiResourceUpdateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/resources\/\$\{serializePathParameter\(resourceId,[^\n]*\}`\), \{[^\n]*method: 'PUT' as any,[^\n]*\bbody\b/],
  ['resourceGroups.create', /async create\([^)]*body:\s*AdminAiResourceGroupCreateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/resource_groups`\), \{[^\n]*method: 'POST' as any,[^\n]*\bbody\b/],
  ['resourceGroups.update', /async update\([^)]*body:\s*AdminAiResourceGroupUpdateRequest[^)]*\): Promise<Record<string, unknown>>/, /this\.client\.request<Record<string, unknown>>\(backendApiPath\(`\/ai\/resource_groups\/\$\{serializePathParameter\(groupId,[^\n]*\}`\), \{[^\n]*method: 'PATCH' as any,[^\n]*\bbody\b/],
  ['resourceGroups.resources.update', /async update\(groupId: string, resourceCode: string, body:\s*AdminAiResourceGroupMemberUpdateRequest[^)]*\): Promise<AdminAiResourceGroupResourceItem>/, /this\.client\.request<AdminAiResourceGroupResourceItem>\(backendApiPath\(`\/ai\/resource_groups\/\$\{serializePathParameter\(groupId,[^\n]*\/resources\/\$\{serializePathParameter\(resourceCode,[^\n]*\}`\), \{[^\n]*method: 'PUT' as any,[^\n]*\bbody\b/],
];

for (const [operationId, method, path, schemaRef] of writeOperationRequestBodies) {
  const operation = backendOpenApi.paths?.[path]?.[method];
  assert.ok(operation, `${operationId} operation must exist`);
  assert.equal(operation.operationId, operationId);
  assert.equal(
    operation.requestBody?.content?.['application/json']?.schema?.$ref,
    schemaRef,
    `${operationId} must accept ${schemaRef} as its JSON request body`,
  );
}

assert.ok(resolveOperation, 'model mapping resolve operation must exist');
assert.equal(resolveOperation.operationId, 'modelMappings.resolve');
assert.equal(
  resolveOperation.requestBody?.content?.['application/json']?.schema?.$ref,
  '#/components/schemas/AdminModelMappingResolveRequest',
  'modelMappings.resolve must accept the runtime resolve request body',
);

const resolveRequest = backendOpenApi.components?.schemas?.AdminModelMappingResolveRequest;
assert.ok(resolveRequest, 'AdminModelMappingResolveRequest schema must be present');
assert.equal(resolveRequest.type, 'object');
assert.equal(resolveRequest.additionalProperties, false);
assert.deepEqual(resolveRequest.required, ['sourceModel']);
assert.ok(resolveRequest.properties?.sourceModel, 'sourceModel property must be present');
assert.ok(resolveRequest.properties?.vendorCode, 'vendorCode property must be present');
assert.ok(resolveRequest.properties?.channelCode, 'channelCode property must be present');
assert.ok(resolveRequest.properties?.providerAccountCode, 'providerAccountCode property must be present');

const syncRequest = backendOpenApi.components?.schemas?.AdminModelCatalogSyncRequest;
assert.ok(syncRequest, 'AdminModelCatalogSyncRequest schema must be present');
assert.deepEqual(
  syncRequest.properties?.mode?.enum,
  ['official_refresh', 'vendor_refresh', 'catalog_version_refresh', 'dry_run'],
  'AdminModelCatalogSyncRequest.mode must match the runtime sync mode validator',
);

assert.ok(modelMappingsListOperation, 'model mappings list operation must exist');
assert.equal(modelMappingsListOperation.operationId, 'modelMappings.list');
const modelMappingQueryParams = new Map(
  (modelMappingsListOperation.parameters ?? []).map((parameter) => [parameter.name, parameter]),
);
for (const name of ['binding_type', 'vendor_code', 'channel_id', 'channel_code', 'q']) {
  assert.ok(modelMappingQueryParams.has(name), `modelMappings.list must expose ${name} query parameter`);
}
assert.equal(
  modelMappingQueryParams.get('channel_id')?.schema?.['x-sdkwork-int64-string'],
  true,
  'modelMappings.list channel_id must be generated as an int64 string query parameter',
);
assert.equal(
  modelMappingsListOperation.responses?.['200']?.content?.['application/json']?.schema?.allOf?.[1]?.properties?.data?.allOf?.[0]?.$ref,
  '#/components/schemas/ModelMappingsPage',
  'modelMappings.list must return the runtime items/pageInfo data shape',
);
const modelMappingsPage = backendOpenApi.components?.schemas?.ModelMappingsPage;
assert.ok(modelMappingsPage, 'ModelMappingsPage schema must be present');
assert.ok(modelMappingsPage.properties?.items, 'ModelMappingsPage.items schema must be present');
assert.ok(modelMappingsPage.properties?.pageInfo, 'ModelMappingsPage.pageInfo schema must be present');

for (const [surface, document, operation, expectedPageRef] of [
  ['app', appOpenApi, appModelsListOperation, '#/components/schemas/AppModelCatalogPage'],
  ['backend', backendOpenApi, backendModelsListOperation, '#/components/schemas/AdminAiModelPage'],
]) {
  assert.ok(operation, `${surface} models.list operation must exist`);
  const queryParams = new Map((operation.parameters ?? []).map((parameter) => [parameter.name, parameter]));
  assert.ok(queryParams.has('page'), `${surface} models.list must expose page`);
  assert.ok(queryParams.has('page_size'), `${surface} models.list must expose page_size`);
  assert.equal(queryParams.get('page_size')?.schema?.maximum, 200);
  for (const legacyName of ['pageSize', 'limit', 'page_no', 'pageNo', 'per_page', 'size', 'model_types']) {
    assert.equal(queryParams.has(legacyName), false, `${surface} models.list must reject ${legacyName}`);
  }
  assert.equal(
    responseDataSchemaRef(document, operation),
    expectedPageRef,
    `${surface} models.list must return a typed items/pageInfo page`,
  );
}

const appModelCatalogPage = appOpenApi.components?.schemas?.AppModelCatalogPage;
assert.ok(appModelCatalogPage?.properties?.items, 'AppModelCatalogPage.items must be typed');
assert.ok(appModelCatalogPage?.properties?.pageInfo, 'AppModelCatalogPage.pageInfo must be typed');
const appModelItemRef = appModelCatalogPage.properties.items.items?.$ref;
assert.equal(appModelItemRef, '#/components/schemas/AppModelCatalogItem');
const appModelItem = appOpenApi.components?.schemas?.AppModelCatalogItem;
assert.equal(
  appModelItem?.properties?.officialReferencePrices?.items?.$ref,
  '#/components/schemas/AppModelCatalogReferencePrice',
  'App model items must expose typed regional officialReferencePrices',
);
assert.ok(appModelItem?.properties?.providerCodes, 'App model items must expose providerCodes');
assert.ok(
  appModelItem?.properties?.supportedAgentProviderIds,
  'App model items must expose supported Agent Config SPI provider ids',
);
assert.deepEqual(
  appModelItem?.properties?.usageScopes?.items?.enum,
  ['coding', 'chat', 'agent'],
  'App model items must expose usageScopes with the coding/chat/agent enum',
);
assert.ok(
  appModelItem?.properties?.codingVisible?.type === 'boolean',
  'App model items must expose the codingVisible boolean',
);

assert.ok(appModelAccessChannelsListOperation, 'modelAccessChannels.list operation must exist');
assert.equal(appModelAccessChannelsListOperation.operationId, 'modelAccessChannels.list');
assert.equal(
  responseDataSchemaRef(appOpenApi, appModelAccessChannelsListOperation),
  '#/components/schemas/AppModelAccessChannelsPage',
);
const modelAccessChannelQueryParams = new Map(
  (appModelAccessChannelsListOperation.parameters ?? []).map((parameter) => [parameter.name, parameter]),
);
assert.deepEqual(
  [...modelAccessChannelQueryParams.keys()],
  ['page', 'page_size', 'q', 'kind', 'vendor_code', 'agent_provider_id'],
);
assert.deepEqual(modelAccessChannelQueryParams.get('kind')?.schema?.enum, ['official', 'relay', 'custom']);
assert.equal(modelAccessChannelQueryParams.get('page_size')?.schema?.maximum, 200);

const modelAccessChannelPage = appOpenApi.components?.schemas?.AppModelAccessChannelsPage;
assert.equal(
  modelAccessChannelPage?.properties?.items?.items?.$ref,
  '#/components/schemas/AppModelAccessChannelItem',
);
assert.equal(
  modelAccessChannelPage?.properties?.pageInfo?.allOf?.[0]?.$ref,
  '#/components/schemas/PageInfo',
);
const modelAccessChannelItem = appOpenApi.components?.schemas?.AppModelAccessChannelItem;
for (const field of [
  'id',
  'code',
  'name',
  'kind',
  'baseUrl',
  'supportedAgentProviderIds',
  'offerings',
  'vendorCount',
  'modelCount',
]) {
  assert.ok(modelAccessChannelItem?.properties?.[field], `AppModelAccessChannelItem.${field} must exist`);
}
assert.equal(modelAccessChannelItem?.properties?.apiKey, undefined);
assert.equal(
  appOpenApi.components?.schemas?.AppModelAccessChannelOffering?.properties?.models?.items?.$ref,
  '#/components/schemas/AppModelAccessChannelModel',
);

const adminModelPage = backendOpenApi.components?.schemas?.AdminAiModelPage;
assert.equal(adminModelPage?.properties?.items?.items?.$ref, '#/components/schemas/AdminAiModelItem');

for (const [operationId, path, expectedPageRef, expectedItemRef] of aiResourceListOperations) {
  const operation = backendOpenApi.paths?.[path]?.get;
  assert.ok(operation, `${operationId} operation must exist`);
  assert.equal(operation.operationId, operationId);
  assert.equal(operation['x-sdkwork-pagination-mode'], 'offset');
  const queryParams = new Map(
    (operation.parameters ?? [])
      .filter((parameter) => parameter.in === 'query')
      .map((parameter) => [parameter.name, parameter]),
  );
  const expectedQueryParams = operationId === 'resources.list'
    ? ['page', 'page_size', 'q', 'resource_type']
    : ['page', 'page_size', 'q'];
  assert.deepEqual([...queryParams.keys()], expectedQueryParams);
  assert.equal(queryParams.get('page')?.schema?.default, 1);
  assert.equal(queryParams.get('page_size')?.schema?.default, 20);
  assert.equal(queryParams.get('page_size')?.schema?.maximum, 200);
  assert.equal(queryParams.get('q')?.schema?.maxLength, 256);
  for (const legacyName of ['pageSize', 'limit', 'page_no', 'pageNo', 'per_page', 'size']) {
    assert.equal(queryParams.has(legacyName), false, `${operationId} must reject ${legacyName}`);
  }
  assert.equal(responseDataSchemaRef(backendOpenApi, operation), expectedPageRef);
  const pageName = expectedPageRef.slice('#/components/schemas/'.length);
  const pageSchema = backendOpenApi.components?.schemas?.[pageName];
  assert.equal(pageSchema?.properties?.items?.items?.$ref, expectedItemRef);
  assert.equal(
    pageSchema?.properties?.pageInfo?.allOf?.[0]?.$ref,
    '#/components/schemas/OffsetPageInfo',
  );
}

const offsetPageInfo = backendOpenApi.components?.schemas?.OffsetPageInfo;
assert.deepEqual(
  offsetPageInfo?.required,
  ['mode', 'page', 'pageSize', 'totalItems', 'totalPages', 'hasMore'],
);
assert.equal(offsetPageInfo?.properties?.mode?.enum?.[0], 'offset');
assert.equal(offsetPageInfo?.properties?.totalItems?.type, 'string');
assert.equal(offsetPageInfo?.properties?.totalItems?.format, 'int64');
assert.equal(offsetPageInfo?.properties?.totalItems?.['x-sdkwork-int64-string'], true);

for (const [schemaName, fields] of [
  ['AdminAiResourceItem', ['id', 'sortOrder']],
  ['AdminAiResourceGroupItem', ['id', 'sortOrder', 'resourceCount']],
  ['AdminAiResourceGroupResourceItem', ['id', 'sortOrder']],
]) {
  const schema = backendOpenApi.components?.schemas?.[schemaName];
  for (const field of fields) {
    const property = schema?.properties?.[field];
    assert.ok(property, `${schemaName}.${field} must exist`);
    assert.equal(property.format, 'int64');
    assert.equal(property['x-sdkwork-int64-string'], true);
  }
}

for (const schemaName of [
  'AdminAiResourceCreateRequest',
  'AdminAiResourceUpdateRequest',
  'AdminAiResourceItem',
]) {
  const schema = backendOpenApi.components?.schemas?.[schemaName];
  for (const field of [
    'accessChannelKind',
    'baseUrl',
    'supportedAgentProviderIds',
    'description',
  ]) {
    assert.ok(schema?.properties?.[field], `${schemaName}.${field} must exist`);
  }
  assert.equal(schema?.properties?.apiKey, undefined, `${schemaName} must never expose API keys`);
}

const memberPath = '/backend/v3/api/ai/resource_groups/{groupId}/resources/{resourceCode}';
const memberUpdateOperation = backendOpenApi.paths?.[memberPath]?.put;
const memberDeleteOperation = backendOpenApi.paths?.[memberPath]?.delete;
const memberUpdateRequest = backendOpenApi.components?.schemas?.AdminAiResourceGroupMemberUpdateRequest;
assert.equal(memberUpdateRequest?.type, 'object');
assert.equal(memberUpdateRequest?.additionalProperties, false);
assert.deepEqual(Object.keys(memberUpdateRequest?.properties ?? {}), ['itemRole', 'sortOrder']);
assert.equal(memberUpdateRequest?.properties?.sortOrder?.format, 'int64');
assert.equal(memberUpdateRequest?.properties?.sortOrder?.['x-sdkwork-int64-string'], true);
assert.equal(
  memberUpdateOperation?.responses?.['200']?.content?.['application/json']?.schema
    ?.allOf?.[1]?.properties?.data?.properties?.item?.$ref,
  '#/components/schemas/AdminAiResourceGroupResourceItem',
);
assert.equal(memberDeleteOperation?.responses?.['204']?.content, undefined);
assert.equal(memberDeleteOperation?.responses?.['200'], undefined);
for (const operation of [memberUpdateOperation, memberDeleteOperation]) {
  assert.equal(operation?.['x-sdkwork-permission'], 'intelligence.resources.manage');
}

const generatedApiSource = readFileSync(
  join(root, 'sdks/sdkwork-models-backend-sdk/sdkwork-models-backend-sdk-typescript/generated/server-openapi/src/api/ai.ts'),
  'utf8',
);
const generatedAppApiSource = readFileSync(
  join(root, 'sdks/sdkwork-models-app-sdk/sdkwork-models-app-sdk-typescript/generated/server-openapi/src/api/ai.ts'),
  'utf8',
);
const catalogServiceSource = readFileSync(
  join(root, 'apps/sdkwork-models-pc/packages/sdkwork-models-pc-admin-catalog/src/modelService.ts'),
  'utf8',
);
const resourceGroupServiceSource = readFileSync(
  join(root, 'apps/sdkwork-models-pc/packages/sdkwork-models-pc-admin-resource/src/resourceGroupService.ts'),
  'utf8',
);
const iamModuleManifest = readJson('specs/iam.module.manifest.json');

for (const [operationId, signaturePattern, transportPattern] of expectedGeneratedBodyMethods) {
  assert.match(
    generatedApiSource,
    signaturePattern,
    `generated TypeScript backend SDK must expose ${operationId}(body)`,
  );
  assert.match(
    generatedApiSource,
    transportPattern,
    `generated TypeScript backend SDK must send ${operationId} body to the backend API`,
  );
}

assert.match(
  generatedAppApiSource,
  /async list\([^)]*params\?: AiModelsListParams[^)]*\): Promise<AppModelCatalogPage>/,
  'generated app SDK must return the typed app model catalog page',
);
assert.match(
  generatedAppApiSource,
  /\{ name: 'page_size', value: params\?\.pageSize/,
  'generated app SDK must serialize pageSize as page_size',
);
assert.doesNotMatch(
  generatedAppApiSource,
  /\{ name: 'model_types'/,
  'generated app SDK must not serialize the unsupported model_types query parameter',
);
assert.match(
  generatedAppApiSource,
  /async list\([^)]*params\?: AiModelAccessChannelsListParams[^)]*\): Promise<AppModelAccessChannelsPage>/,
  'generated app SDK must expose modelAccessChannels.list(params)',
);
for (const [wireName, modelName] of [
  ['page_size', 'pageSize'],
  ['kind', 'kind'],
  ['vendor_code', 'vendorCode'],
  ['agent_provider_id', 'agentProviderId'],
]) {
  assert.match(
    generatedAppApiSource,
    new RegExp(`\\{ name: '${wireName}', value: params\\?\\.${modelName}`),
    `generated app SDK must serialize modelAccessChannels.list ${modelName} as ${wireName}`,
  );
}
assert.match(
  generatedApiSource,
  /async list\([^)]*params\?: AiModelsListParams[^)]*\): Promise<AdminAiModelPage>/,
  'generated backend SDK must return the typed admin model page',
);

for (const [paramsName, pageName] of [
  ['AiResourcesListParams', 'AiResourcesPage'],
  ['AiResourceGroupsListParams', 'AiResourceGroupsPage'],
]) {
  assert.match(
    generatedApiSource,
    new RegExp(`async list\\([^)]*params\\?: ${paramsName}[^)]*\\): Promise<${pageName}>`),
  );
}
assert.match(
  generatedApiSource,
  /async list\(groupIdOrCode: string, params\?: AiResourceGroupsResourcesListParams[^)]*\): Promise<AiResourceGroupResourcesPage>/,
);
for (const paramsName of [
  'AiResourcesListParams',
  'AiResourceGroupsListParams',
  'AiResourceGroupsResourcesListParams',
]) {
  const paramsBlock = generatedApiSource.match(
    new RegExp(`export interface ${paramsName} \\{([\\s\\S]*?)\\n\\}`),
  )?.[1];
  assert.ok(paramsBlock, `${paramsName} must be generated`);
  assert.match(paramsBlock, /page\?: number;/);
  assert.match(paramsBlock, /pageSize\?: number;/);
  assert.match(paramsBlock, /q\?: string;/);
}
const resourcesListParamsBlock = generatedApiSource.match(
  /export interface AiResourcesListParams \{([\s\S]*?)\n\}/,
)?.[1];
assert.ok(resourcesListParamsBlock, 'AiResourcesListParams must be generated');
assert.match(
  resourcesListParamsBlock,
  /resourceType\?: 'vendor' \| 'modality' \| 'api_endpoint' \| 'model' \| 'model_api' \| 'bundle' \| 'model_access_channel';/,
);
for (const paramsName of ['AiResourceGroupsListParams', 'AiResourceGroupsResourcesListParams']) {
  const paramsBlock = generatedApiSource.match(
    new RegExp(`export interface ${paramsName} \\{([\\s\\S]*?)\\n\\}`),
  )?.[1];
  assert.doesNotMatch(paramsBlock ?? '', /resourceType\?:/, `${paramsName} must not expose resourceType`);
}
assert.match(
  generatedApiSource,
  /\{ name: 'resource_type', value: params\?\.resourceType/,
  'generated backend SDK must serialize resourceType as resource_type',
);
assert.match(
  generatedApiSource,
  /async delete\(groupId: string, resourceCode: string,[^)]*\): Promise<void>/,
  'generated backend SDK must expose resourceGroups.resources.delete as a void operation',
);
assert.doesNotMatch(generatedApiSource, /Promise<NoData>/);
assert.doesNotMatch(resourceGroupServiceSource, /\.ai\.aiResourceGroups\b|\.ai\.aiResources\b/);
assert.doesNotMatch(resourceGroupServiceSource, /while\s*\(hasMore\)|fetchResourceGroupResourcesForUpdate/);
assert.match(
  generatedApiSource,
  /\{ name: 'page_size', value: params\?\.pageSize/,
  'generated backend SDK must serialize pageSize as page_size',
);
assert.doesNotMatch(
  generatedApiSource,
  /\{ name: 'model_types'/,
  'generated backend SDK must not serialize the unsupported model_types query parameter',
);

const modelMappingsListParamsBlock = generatedApiSource.match(
  /export interface AiModelMappingsListParams \{([\s\S]*?)\n\}/,
)?.[1];
assert.ok(
  modelMappingsListParamsBlock,
  'generated TypeScript backend SDK must expose modelMappings.list(params) filters',
);
for (const fieldPattern of [
  /page\?: number;/,
  /pageSize\?: number;/,
  /bindingType\?: 'global' \| 'vendor' \| 'channel_group' \| 'channel' \| 'provider_account' \| 'site' \| 'site_service';/,
  /vendorCode\?: string;/,
  /channelId\?: string;/,
  /channelCode\?: string;/,
  /q\?: string;/,
]) {
  assert.match(
    modelMappingsListParamsBlock,
    fieldPattern,
    'generated TypeScript backend SDK must expose modelMappings.list(params) filters',
  );
}
assert.match(
  generatedApiSource,
  /async list\([^)]*params\?: AiModelMappingsListParams[^)]*\): Promise<ModelMappingsPage>/,
  'generated TypeScript backend SDK must accept params and return ModelMappingsPage for modelMappings.list',
);
for (const [wireName, modelName] of [
  ['binding_type', 'bindingType'],
  ['vendor_code', 'vendorCode'],
  ['channel_id', 'channelId'],
  ['channel_code', 'channelCode'],
  ['q', 'q'],
]) {
  assert.match(
    generatedApiSource,
    new RegExp(`\\{ name: '${wireName}', value: params\\?\\.${modelName}`),
    `generated TypeScript backend SDK must serialize modelMappings.list ${modelName} as ${wireName}`,
  );
}

assert.doesNotMatch(
  catalogServiceSource,
  /modelMappings\.resolve\.create/,
  'catalog service must use the generated modelMappings.resolve(body) method',
);
assert.match(
  catalogServiceSource,
  /modelMappings\.resolve\(input\)/,
  'catalog service must call modelMappings.resolve(input)',
);
assert.doesNotMatch(
  catalogServiceSource,
  /ensureDeleteResult/,
  'catalog service must not expect delete operations to return a JSON confirmation body',
);
assert.doesNotMatch(
  catalogServiceSource,
  /readRequiredRecord\(readApiRecord\(result\)\.data, 'Model mapping delete response is missing data'\)/,
  'model mapping delete must treat 204 No Content as success',
);
assert.doesNotMatch(
  catalogServiceSource,
  /AdminModelCatalogSyncResponse/,
  'catalog service must consume the current ModelCatalogSyncResult generated SDK type',
);
assert.match(
  catalogServiceSource,
  /ModelCatalogSyncResult\['mode'\]/,
  'catalog service sync report must derive mode from ModelCatalogSyncResult',
);
assert.match(
  catalogServiceSource,
  /modelRankings\.list\(\{\s*pageSize: 200\s*\}\)/,
  'catalog service must use generated pageSize params for model rankings list',
);
assert.match(
  catalogServiceSource,
  /modelRankings\.jobs\.list\(\{\s*pageSize: 20\s*\}\)/,
  'catalog service must use generated pageSize params for model ranking jobs',
);
assert.doesNotMatch(
  catalogServiceSource,
  /modelRankings\.(?:jobs\.)?list\(\{\s*limit:/,
  'catalog service must not use legacy limit params for generated list methods',
);
assert.doesNotMatch(
  catalogServiceSource,
  /limit\?: number;\s*\n\s*offset\?: number;/,
  'catalog service model list query must use standard page/pageSize params',
);
assert.doesNotMatch(
  catalogServiceSource,
  /fetchModelsPage\(\{[^}]*\blimit:/,
  'catalog service callers must not use legacy limit params for model pages',
);
assert.match(
  catalogServiceSource,
  /function modelType\(value: Model\['type'\]\): AdminAiModelType/,
  'catalog service must map UI model type labels to backend API model type values',
);
assert.doesNotMatch(
  catalogServiceSource,
  /return value;\s*\n\s*\}\s*\n\s*throw new Error\(value \? `Unsupported model type:/,
  'catalog service must not send UI model type labels directly to the backend SDK',
);
assert.match(
  catalogServiceSource,
  /releaseStage: 1,\s*\n\s*shelfState: 1,\s*\n\s*routingState: 1,/,
  'catalog service default model metadata must use numeric enum values required by the backend SDK',
);
assert.doesNotMatch(
  resourceGroupServiceSource,
  /readBoolean\(readApiRecord\(result\), 'deleted', false\)/,
  'resource group delete must treat 204 No Content as success',
);
assert.match(
  resourceGroupServiceSource,
  /function toSdkListParams\(query: ResourceListQuery\): \{\s*page\?: number;\s*pageSize\?: number;\s*q\?: string;\s*resourceType\?: ResourceListQuery\['resourceType'\];\s*\}/,
  'resource group service must pass numeric pagination params to generated SDK list methods',
);
assert.doesNotMatch(
  resourceGroupServiceSource,
  /page:\s*'1'|pageSize:\s*'200'|String\(query\.page\)|String\(query\.pageSize\)/,
  'resource group service must not pass string pagination values to generated SDK list methods',
);

const backendPermissions = iamModuleManifest.permissions?.openapiAuthorities
  ?.find((authority) => authority.apiAuthority === 'sdkwork-models-backend-api')
  ?.operationPermissions ?? [];
const operationIds = new Set(backendPermissions.map((permission) => permission.operationId));
assert.ok(operationIds.has('modelMappings.resolve'), 'IAM manifest must grant modelMappings.resolve');
assert.ok(!operationIds.has('modelMappings.resolve.create'), 'IAM manifest must not keep stale modelMappings.resolve.create');
for (const operationId of ['resourceGroups.resources.update', 'resourceGroups.resources.delete']) {
  assert.equal(
    backendPermissions.find((permission) => permission.operationId === operationId)?.permission,
    'intelligence.resources.manage',
    `IAM manifest must grant ${operationId}`,
  );
}

process.stdout.write('models-openapi-contract.test.mjs passed\n');
