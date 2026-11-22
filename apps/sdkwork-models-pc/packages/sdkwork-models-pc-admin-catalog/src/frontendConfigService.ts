import {
  ensureSdkworkApiSuccess,
  isRecord,
  readBoolean,
  readNumber,
  readString,
  readStringArray,
  type ApiRecord,
} from '@sdkwork/cloudroutes-pc-commons/api-result';
import { getModelsBackendSdkClient } from '@sdkwork/cloudroutes-pc-commons/sdk-clients';

/**
 * One model exposed by a front-end channel. `catalogKey` joins the presentation
 * row to `ai_model.catalog_key`; the models backend owns the persistence.
 */
export interface FrontendChannelModelConfig {
  catalogKey: string;
  /** Int64 model id as a decimal string (never a JavaScript number). */
  modelId?: string | null;
  visible: boolean;
  featured: boolean;
  sortOrder: number;
  badge?: string | null;
  config: Record<string, unknown>;
}

/** One field (price, context, …) shown or hidden by a channel. */
export interface FrontendChannelFieldConfig {
  fieldKey: string;
  visible: boolean;
  sortOrder: number;
  config: Record<string, unknown>;
}

/** One product surface configuration (LLM, video, image, audio, music, …). */
export interface FrontendChannelConfig {
  channelCode: string;
  displayName: string;
  displayNameI18n: Record<string, string>;
  description?: string | null;
  enabled: boolean;
  sortOrder: number;
  defaultCatalogKey?: string | null;
  capabilityCodes: string[];
  config: Record<string, unknown>;
  schemaVersion: string;
  revision: number;
  models: FrontendChannelModelConfig[];
  fields: FrontendChannelFieldConfig[];
}

/** The revisioned document served by the models backend. */
export interface FrontendConfigDocument {
  revision: number;
  channels: FrontendChannelConfig[];
}

const FRONTEND_CONFIG_PATH = '/backend/v3/api/ai/frontend_config';

/**
 * The document changed on the server between the read the operator edited and
 * the replace, so the optimistic-concurrency guard refused the write.
 *
 * The page reloads on this error instead of overwriting another operator's
 * revision.
 */
export class FrontendConfigRevisionConflictError extends Error {
  constructor() {
    super('frontend_config_revision_conflict');
    this.name = 'FrontendConfigRevisionConflictError';
  }
}

/** Record whose string/number fields can be inspected defensively. */
function isObjectRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** Whether one value carries the platform's HTTP 409 or numeric conflict code. */
function isConflictCode(field: string, value: unknown): boolean {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return field === 'status' ? value === 409 : value === 40901 || value === 409;
  }
  return typeof value === 'string' && /^(?:40901|409|conflict)$/iu.test(value.trim());
}

/**
 * Detect the backend's revision-conflict failure across the SDK error shapes:
 * a plain `ProblemDetail` value, an error carrying `problem`, or an error whose
 * `cause` wraps one.
 * @param error - the thrown value.
 * @returns whether the write was refused because the revision is stale.
 */
export function isFrontendConfigRevisionConflict(error: unknown): boolean {
  if (error instanceof FrontendConfigRevisionConflictError) return true;
  const candidates: unknown[] = [error];
  if (isObjectRecord(error)) {
    candidates.push(error.problem, error.cause, error.response);
  }
  for (const candidate of candidates) {
    if (!isObjectRecord(candidate)) continue;
    for (const field of ['status', 'httpStatus', 'code'] as const) {
      if (isConflictCode(field, candidate[field])) return true;
    }
  }
  return false;
}

function readJsonObject(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : {};
}

/**
 * Read an int64 wire value without losing precision.
 *
 * The contract is a decimal string; the numeric branch only keeps a
 * transitional server returning a plain number from being dropped, and refuses
 * values JavaScript cannot represent exactly (API_SPEC section 13.6).
 */
function readInt64String(value: unknown): string | null {
  if (typeof value === 'string' && /^-?\d+$/u.test(value.trim())) {
    return value.trim();
  }
  if (typeof value === 'number' && Number.isSafeInteger(value)) {
    return String(value);
  }
  return null;
}

function readChannel(record: ApiRecord): FrontendChannelConfig {
  const code = readString(record, 'channelCode', '');
  return {
    channelCode: code,
    displayName: readString(record, 'displayName', code),
    displayNameI18n: Object.fromEntries(
      Object.entries(readJsonObject(record.displayNameI18n)).flatMap(([locale, text]) => (
        typeof text === 'string' ? [[locale, text]] : []
      )),
    ),
    description: typeof record.description === 'string' ? record.description : null,
    enabled: readBoolean(record, 'enabled', true),
    sortOrder: readNumber(record, 'sortOrder', 0),
    defaultCatalogKey: typeof record.defaultCatalogKey === 'string' ? record.defaultCatalogKey : null,
    capabilityCodes: readStringArray(record, 'capabilityCodes', []),
    config: readJsonObject(record.config),
    schemaVersion: readString(record, 'schemaVersion', '1.0.0'),
    revision: readNumber(record, 'revision', 0),
    models: Array.isArray(record.models)
      ? record.models.filter(isRecord).map((model) => ({
        catalogKey: readString(model, 'catalogKey', ''),
        modelId: readInt64String(model.modelId),
        visible: readBoolean(model, 'visible', true),
        featured: readBoolean(model, 'featured', false),
        sortOrder: readNumber(model, 'sortOrder', 0),
        badge: typeof model.badge === 'string' ? model.badge : null,
        config: readJsonObject(model.config),
      }))
      : [],
    fields: Array.isArray(record.fields)
      ? record.fields.filter(isRecord).map((field) => ({
        fieldKey: readString(field, 'fieldKey', ''),
        visible: readBoolean(field, 'visible', true),
        sortOrder: readNumber(field, 'sortOrder', 0),
        config: readJsonObject(field.config),
      }))
      : [],
  };
}

function readDocument(value: unknown): FrontendConfigDocument {
  const record = isRecord(value) ? value : {};
  return {
    revision: readNumber(record, 'revision', 0),
    channels: Array.isArray(record.channels)
      ? record.channels.filter(isRecord).map(readChannel)
      : [],
  };
}

/** Load the current front-end configuration document. */
export async function loadFrontendConfigDocument(): Promise<FrontendConfigDocument> {
  const client = getModelsBackendSdkClient();
  const result = await client.http.request<unknown>(FRONTEND_CONFIG_PATH, {
    method: 'GET',
    sdkworkUnwrapKind: 'data',
  });
  ensureSdkworkApiSuccess(result, 'Unable to load front-end model configuration');
  return readDocument(result);
}

/** How one replace call records itself in the published revision history. */
export interface SaveFrontendConfigOptions {
  /** Revision the editor loaded; the backend refuses the write when it moved. */
  expectedRevision?: number | null;
  /** Revision source recorded by `ai_model_frontend_revision` (default `admin`). */
  source?: string;
}

/**
 * Replace the complete front-end configuration in one transaction. The models
 * backend validates the document, retires the previous rows, writes the new
 * set, and returns the next revision.
 *
 * When `expectedRevision` is supplied, a server revision that moved since the
 * client read is refused with HTTP 409 instead of silently overwriting another
 * operator's edit; the rejection surfaces as
 * {@link FrontendConfigRevisionConflictError}.
 * @param document - the edited channels.
 * @param options - concurrency guard and revision source.
 */
export async function saveFrontendConfigDocument(
  document: FrontendConfigDocument,
  options: SaveFrontendConfigOptions = {},
): Promise<FrontendConfigDocument> {
  const client = getModelsBackendSdkClient();
  const expectedRevision = options.expectedRevision === undefined
    ? document.revision
    : options.expectedRevision;
  let result: unknown;
  try {
    result = await client.http.request<unknown>(FRONTEND_CONFIG_PATH, {
      method: 'PUT',
      body: {
        channels: document.channels.map((channel) => ({
          channelCode: channel.channelCode,
          displayName: channel.displayName,
          displayNameI18n: channel.displayNameI18n,
          description: channel.description ?? null,
          enabled: channel.enabled,
          sortOrder: channel.sortOrder,
          defaultCatalogKey: channel.defaultCatalogKey ?? null,
          capabilityCodes: channel.capabilityCodes,
          config: channel.config,
          schemaVersion: channel.schemaVersion,
          models: channel.models,
          fields: channel.fields,
        })),
        ...(expectedRevision === null ? {} : { expectedRevision }),
        source: options.source ?? 'admin',
      },
      contentType: 'application/json',
      sdkworkUnwrapKind: 'data',
    });
  } catch (writeError) {
    if (isFrontendConfigRevisionConflict(writeError)) {
      throw new FrontendConfigRevisionConflictError();
    }
    throw writeError;
  }
  ensureSdkworkApiSuccess(result, 'Unable to save front-end model configuration');
  return readDocument(result);
}
