#!/usr/bin/env node
import { existsSync, readdirSync, statSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
import {
  buildCatalogIndex,
  buildVendorList,
  catalogKey,
  collectRegionalCatalogDirectories,
  isDecimalString,
  issue,
  loadManifest,
  loadMeters,
  loadVendorBundle,
  modelIdPath,
  projectRootFromTool,
  readJsonFile,
  stableJson,
  videoProfileCatalogKey,
} from "./catalog-lib.mjs";

const GENERATION_MODE_INPUTS = {
  text_to_video: ["text"],
  image_to_video: ["image"],
  reference_to_video: ["video", "image"],
  start_end_frame: ["image"],
  video_extension: ["video"],
  video_edit: ["video"],
  multi_shot: ["text"],
  // Digital human (数字人): a still portrait is animated by a driving audio
  // track. Kling's avatar endpoint takes `image` + `audio`; classifying it as
  // `image_to_video` (its historical label) hid the audio input and made the
  // mode indistinguishable from ordinary image-to-video, so its own pricing
  // tiers (`motion_res_*` for motion control, the audio tiers for avatar)
  // could never be selected.
  avatar: ["image", "audio"],
  // Motion mimicry (动作模仿): a reference performance video drives the person
  // in a still image. Kling's `motion-control` endpoint takes `image` + `video`.
  motion_control: ["image", "video"],
};

const DURATION_TIER_CODES = new Set([
  "dur_5s",
  "dur_6s",
  "dur_8s",
  "dur_10s",
  "dur_15s",
  "dur_30s",
  "dur_60s",
]);

const USAGE_SCOPES = new Set(["coding", "chat", "agent"]);

/**
 * Meters whose rates the Cloud Router bills against a `tier_code` condition for
 * video generation. `tier_code` is the only dimension the router resolves from
 * `ai_model_video_profile`, so a profile whose declared tiers do not intersect
 * this vocabulary can never be priced.
 */
const VIDEO_PRICING_METERS = new Set(["video_output_second", "video_result", "video_input_second"]);

/**
 * Of `VIDEO_PRICING_METERS`, the ones that bill the generation *output*. The
 * ambiguity rule only compares tiers inside a single meter: `video_input_second`
 * prices the source clip (`input_res_720p`), so mixing it in would flag a
 * correctly declared `res_720p` as if the model also quoted it dearer.
 */
const VIDEO_OUTPUT_PRICING_METERS = new Set(["video_output_second", "video_result"]);

/**
 * A rate keyed by resolution alone (`res_720p`). `res_4k_native` and `res_480p_720p` are
 * excluded: their extra segment carries a vendor distinction a resolution token cannot
 * express, so demanding a profile for them would be a guess. `tools/validate-catalog.mjs`
 * reports those through the `tier.unreachable` rule instead.
 */
const PLAIN_RESOLUTION_TIER_CODE = /^res_([^_]+)$/;

function compareDecimalStrings(left, right) {
  if (!isDecimalString(left) || !isDecimalString(right)) {
    throw new TypeError("decimal comparison requires canonical non-negative decimal strings");
  }
  const [leftWhole, leftFraction = ""] = left.split(".");
  const [rightWhole, rightFraction = ""] = right.split(".");
  if (leftWhole.length !== rightWhole.length) {
    return leftWhole.length < rightWhole.length ? -1 : 1;
  }
  const wholeOrder = leftWhole.localeCompare(rightWhole);
  if (wholeOrder !== 0) {
    return wholeOrder;
  }
  const width = Math.max(leftFraction.length, rightFraction.length);
  return leftFraction.padEnd(width, "0").localeCompare(rightFraction.padEnd(width, "0"));
}

function isZeroDecimal(value) {
  return isDecimalString(value) && compareDecimalStrings(value, "0") === 0;
}

function isPositiveDecimal(value) {
  return isDecimalString(value) && compareDecimalStrings(value, "0") > 0;
}

/**
 * Largest member of a set of unit-price strings, or `null` when none of them is
 * a comparable decimal. Comparison goes through `compareDecimalStrings` so the
 * ranking never depends on float rounding.
 */
function maxDecimalString(values) {
  let best = null;
  for (const value of values) {
    if (!isDecimalString(value)) {
      continue;
    }
    if (best === null || compareDecimalStrings(value, best) > 0) {
      best = value;
    }
  }
  return best;
}

function validatePriceSchedule(price, pricingPath, index, issues) {
  const path = `${pricingPath}#/prices/${index}`;
  const variant = price.rateVariant ?? "standard";
  if (!Number.isInteger(price.priority) || price.priority < 0) {
    issues.push(issue("price.priority.invalid", `${path}/priority`, "priority must be a non-negative integer"));
  }
  if (!["standard", "time_window"].includes(variant)) {
    issues.push(issue("price.rate_variant.invalid", `${path}/rateVariant`, "rateVariant must be standard or time_window"));
  }
  if (variant === "standard" && price.schedule != null) {
    issues.push(issue("price.schedule.unexpected", `${path}/schedule`, "standard rates must not define a schedule"));
    return;
  }
  if (variant === "time_window" && (!price.schedule || typeof price.schedule !== "object")) {
    issues.push(issue("price.schedule.missing", `${path}/schedule`, "time_window rates require a schedule"));
    return;
  }
  if (variant !== "time_window") return;
  const schedule = price.schedule;
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: schedule.timeZone }).format();
  } catch {
    issues.push(issue("price.schedule.time_zone.invalid", `${path}/schedule/timeZone`, "timeZone must be an IANA time-zone identifier"));
  }
  const windows = schedule.weeklyWindows;
  if (!Array.isArray(windows) || windows.length === 0) {
    issues.push(issue("price.schedule.windows.missing", `${path}/schedule/weeklyWindows`, "weeklyWindows must contain at least one window"));
    return;
  }
  const codes = new Set();
  for (const [windowIndex, window] of windows.entries()) {
    const windowPath = `${path}/schedule/weeklyWindows/${windowIndex}`;
    if (typeof window.windowCode !== "string" || !window.windowCode.trim() || codes.has(window.windowCode)) {
      issues.push(issue("price.schedule.window_code.invalid", `${windowPath}/windowCode`, "windowCode must be non-empty and unique"));
    }
    codes.add(window.windowCode);
    const days = window.daysOfWeek;
    if (!Array.isArray(days) || days.length === 0 || new Set(days).size !== days.length || days.some((day) => !Number.isInteger(day) || day < 1 || day > 7)) {
      issues.push(issue("price.schedule.days.invalid", `${windowPath}/daysOfWeek`, "daysOfWeek must contain unique ISO weekdays from 1 through 7"));
    }
    const timePattern = /^(?:[01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]$/;
    if (!timePattern.test(window.startTime) || !timePattern.test(window.endTime)) {
      issues.push(issue("price.schedule.time.invalid", windowPath, "startTime and endTime must use HH:mm:ss"));
    } else {
      const start = window.startTime;
      const end = window.endTime;
      if (![0, 1].includes(window.endDayOffset) || (window.endDayOffset === 0 && end <= start) || (window.endDayOffset === 1 && end >= start)) {
        issues.push(issue("price.schedule.range.invalid", windowPath, "same-day windows require endTime after startTime; cross-midnight windows require endDayOffset 1 and endTime before startTime"));
      }
    }
  }
  for (const field of ["includeDates", "excludeDates"]) {
    if (schedule[field] === undefined) continue;
    if (!Array.isArray(schedule[field]) || new Set(schedule[field]).size !== schedule[field].length || schedule[field].some((value) => !/^\d{4}-\d{2}-\d{2}$/.test(value) || Number.isNaN(Date.parse(`${value}T00:00:00Z`)))) {
      issues.push(issue("price.schedule.date.invalid", `${path}/schedule/${field}`, `${field} must contain unique ISO dates`));
    }
  }
  if (Array.isArray(schedule.includeDates) && Array.isArray(schedule.excludeDates) && schedule.includeDates.some((value) => schedule.excludeDates.includes(value))) {
    issues.push(issue("price.schedule.date_conflict", `${path}/schedule`, "a date cannot be both included and excluded"));
  }
}

/**
 * A `time_window` rate must not also carry a `tier_code` condition.
 *
 * The Cloud Router selects a `time_window` rate purely through
 * `PricingSchedule.matched_window_code()`: `PricingRateMetadata::matches_at`
 * takes the `TimeWindow` arm and never consults the request dimensions for the
 * window. The only `tier_code` producer in the runtime is the video path, which
 * intersects `ai_model_video_profile.pricingTierCodes` with the model's priced
 * tiers - and no video profile exists for an LLM/audio/image meter.
 *
 * So a rate carrying both keys can never be selected: the schedule already
 * encodes the tier as its window-code prefix (`peak_weekday_morning` ->
 * `peak`), and the redundant condition filters the rate out instead. Leaving
 * both in place is not a harmless belt-and-braces overlap; it is a hard outage.
 * All four DeepSeek models published on 2026-09-17 were unrouteable this way.
 *
 * See `ROUTING_PRICING_SPEC.md` section 5.4.
 */
function validateTimeWindowTierRedundancy(price, pricingPath, index, issues) {
  const variant = price.rateVariant ?? "standard";
  if (variant !== "time_window") return;
  const tierCondition = (price.conditions ?? []).find(
    (condition) => (condition.dimensionCode ?? "").toLowerCase() === "tier_code",
  );
  if (!tierCondition) return;
  const path = `${pricingPath}#/prices/${index}`;
  issues.push(
    issue(
      "price.time_window.tier_code.redundant",
      `${path}/conditions`,
      "a time_window rate is selected by its schedule's window codes and must not also "
        + `condition on tier_code (found tier_code eq ${JSON.stringify(tierCondition.value)}); `
        + "remove the condition - the runtime has no tier_code producer for this rate",
    ),
  );
}

export function validateCatalog(root) {
  const issues = [];

  function requireFile(rel) {
    const path = join(root, rel);
    if (!existsSync(path) || !statSync(path).isFile()) {
      issues.push(issue("file.missing", rel, `${rel} is required`));
    }
  }

  for (const rel of [
    "sdkwork-models.json",
    "models/meters.json",
    "models/protocols.json",
    "models/vendors.json",
    "models/index.json",
    "schemas/catalog.schema.json",
    "schemas/index.schema.json",
    "schemas/model.schema.json",
    "schemas/pricing.schema.json",
    "schemas/protocol.schema.json",
    "schemas/voice.schema.json",
    "schemas/model-voice.schema.json",
    "schemas/model-video-profiles.schema.json",
  ]) {
    requireFile(rel);
  }

  const manifest = loadManifest(root);
  const modelsRoot = join(root, manifest.modelsRoot);
  const meterCodes = new Set(loadMeters(root).map((meter) => meter.meterCode));
  const protocolFile = readJsonFile(join(root, manifest.modelsRoot, "protocols.json"));
  const protocolCodes = new Set();
  const protocolFamilyByCode = new Map();
  for (const [index, protocol] of (protocolFile.protocols ?? []).entries()) {
    if (typeof protocol.protocolCode !== "string" || protocol.protocolCode.length === 0) {
      issues.push(issue("protocol.code.invalid", `models/protocols.json#/protocols/${index}/protocolCode`, "protocolCode must be a non-empty string"));
      continue;
    }
    if (protocolCodes.has(protocol.protocolCode)) {
      issues.push(issue("protocol.code.duplicate", `models/protocols.json#/protocols/${index}/protocolCode`, `${protocol.protocolCode} is duplicated`));
    }
    protocolCodes.add(protocol.protocolCode);
    if (typeof protocol.family === "string") {
      protocolFamilyByCode.set(protocol.protocolCode, protocol.family);
    }
  }
  // 兼容族标准路径规律:familyCode -> 允许的 pathPrefix 集合(models/protocols.json#/families)
  const familyPathPrefixes = new Map();
  for (const family of (protocolFile.families ?? [])) {
    if (typeof family.familyCode !== "string") {
      continue;
    }
    familyPathPrefixes.set(family.familyCode, new Set(family.standardPathPrefixes ?? []));
  }
  const routingResourceIndex = loadRoutingResourceIndex(root);
  const seenVendorRegions = new Set();
  const seenModels = new Map();

  for (const regionDir of collectRegionalCatalogDirectories(modelsRoot)) {
    const bundle = loadVendorBundle(regionDir);
    const pathPrefix = `models/${bundle.vendorCode}/${bundle.regionCode}`;
    if (bundle.vendor.vendorCode !== bundle.vendorCode) {
      issues.push(
        issue(
          "vendor.directory.mismatch",
          `${pathPrefix}/vendor.json#/vendorCode`,
          `vendorCode ${bundle.vendor.vendorCode} must match directory ${bundle.vendorCode}`,
        ),
      );
    }
    if (bundle.vendor.regionCode !== bundle.regionCode) {
      issues.push(
        issue(
          "vendor.region_directory.mismatch",
          `${pathPrefix}/vendor.json#/regionCode`,
          `regionCode ${bundle.vendor.regionCode} must match directory ${bundle.regionCode}`,
        ),
      );
    }
    if (bundle.vendor.vendorCode && /_(cn|global)$/.test(bundle.vendor.vendorCode)) {
      issues.push(issue("vendor.code.region_suffix", `${pathPrefix}/vendor.json#/vendorCode`, "vendorCode must be the unique vendor identity; put operating context in regionCode"));
    }
    const vendorRegionKey = `${bundle.vendor.vendorCode}/${bundle.vendor.regionCode}`;
    if (seenVendorRegions.has(vendorRegionKey)) {
      issues.push(issue("vendor_region.duplicate", pathPrefix, `${vendorRegionKey} is duplicated`));
    }
    seenVendorRegions.add(vendorRegionKey);
    for (const field of ["regionCode", "marketScope", "billingCurrency", "billingJurisdiction", "operatingRegions"]) {
      const value = bundle.vendor[field];
      if (value === undefined || value === null || (Array.isArray(value) && value.length === 0)) {
        issues.push(issue("vendor.operating_context.missing", `${pathPrefix}/vendor.json#/${field}`, `${field} is required for vendor operating and billing context`));
      }
    }
    const supportedProtocols = new Set();
    if (!Array.isArray(bundle.vendor.supportedProtocols) || bundle.vendor.supportedProtocols.length === 0) {
      issues.push(issue("vendor.protocols.missing", `${pathPrefix}/vendor.json#/supportedProtocols`, "supportedProtocols must declare at least one protocolCode"));
    }
    for (const [index, protocolCode] of (bundle.vendor.supportedProtocols ?? []).entries()) {
      if (!protocolCodes.has(protocolCode)) {
        issues.push(issue("vendor.protocol.unknown", `${pathPrefix}/vendor.json#/supportedProtocols/${index}`, `${protocolCode} is not defined in models/protocols.json`));
      }
      supportedProtocols.add(protocolCode);
    }
    validateApiEndpoints({
      vendor: bundle.vendor,
      protocolFamilyByCode,
      familyPathPrefixes,
      pathPrefix,
      issues,
    });
    validateProtocolBaseUrls({
      vendor: bundle.vendor,
      protocolFamilyByCode,
      familyPathPrefixes,
      pathPrefix,
      issues,
    });
    validateClientApiCompatibility({
      vendor: bundle.vendor,
      protocolCodes,
      routingResourceIndex,
      pathPrefix,
      issues,
    });
    if (/\b(qwen|kling|hunyuan|bigmodel|seedance)\b/i.test(bundle.vendor.vendorCode.replace(/_/g, " "))) {
      issues.push(issue("vendor.code.product_line", `${pathPrefix}/vendor.json#/vendorCode`, "vendorCode must identify the unique vendor identity, not a product line"));
    }

    const familyCodes = new Set((bundle.families.families ?? []).map((family) => family.familyCode));
    if (bundle.families.vendorCode !== bundle.vendorCode) {
      issues.push(issue("family.vendor.mismatch", `${pathPrefix}/families.json`, "families vendorCode must match directory"));
    }
    if (bundle.families.regionCode !== bundle.regionCode) {
      issues.push(issue("family.region.mismatch", `${pathPrefix}/families.json`, "families regionCode must match directory"));
    }

    const modelsById = new Map(bundle.models.map((model) => [model.modelId, model]));
    const modelFilesById = new Map();
    for (const relPath of collectJsonFileRefsForDirectory(modelsRoot, join(regionDir, "models"))) {
      const model = readJsonFile(join(modelsRoot, relPath));
      if (typeof model.modelId === "string") {
        modelFilesById.set(model.modelId, `models/${relPath}`);
      }
    }
    const pricingFilesById = new Map();
    for (const relPath of collectJsonFileRefsForDirectory(modelsRoot, join(regionDir, "pricing"))) {
      const pricing = readJsonFile(join(modelsRoot, relPath));
      if (typeof pricing.modelId === "string") {
        pricingFilesById.set(pricing.modelId, `models/${relPath}`);
      }
    }
    const pricedModelIds = new Set(bundle.pricing.map((pricing) => pricing.modelId));
    for (const [index, family] of (bundle.families.families ?? []).entries()) {
      if (!family.defaultModel) {
        continue;
      }
      const defaultModel = modelsById.get(family.defaultModel);
      if (!defaultModel) {
        issues.push(issue("family.default_model.missing", `${pathPrefix}/families.json#/families/${index}/defaultModel`, `${family.defaultModel} is not defined for ${bundle.vendorCode}/${bundle.regionCode}`));
        continue;
      }
      if (defaultModel.routingState !== "enabled" || defaultModel.shelfState !== "listed") {
        issues.push(issue("family.default_model.not_routable", `${pathPrefix}/families.json#/families/${index}/defaultModel`, `${family.defaultModel} must be enabled and listed to be a family default`));
      }
      if (!pricedModelIds.has(family.defaultModel)) {
        issues.push(issue("family.default_model.pricing_missing", `${pathPrefix}/families.json#/families/${index}/defaultModel`, `${family.defaultModel} must have pricing to be a family default`));
      }
    }

    for (const model of bundle.models) {
      const modelPath = `${pathPrefix}/models/${safeModelIdPath(model.modelId, issues, `${pathPrefix}/models`) }.json`;
      const actualModelPath = modelFilesById.get(model.modelId);
      if (actualModelPath && actualModelPath !== modelPath) {
        issues.push(issue("model.path.mismatch", actualModelPath, `${model.modelId} must be stored at ${modelPath}`));
      }
      if (model.vendorCode !== bundle.vendorCode) {
        issues.push(issue("model.vendor.mismatch", `${modelPath}#/vendorCode`, "model vendorCode must match directory"));
      }
      if (model.regionCode !== bundle.regionCode) {
        issues.push(issue("model.region.mismatch", `${modelPath}#/regionCode`, "model regionCode must match directory"));
      }
      const expectedCatalogKey = catalogKey(bundle.vendorCode, model.modelId);
      if (model.catalogKey !== expectedCatalogKey) {
        issues.push(issue("model.catalog_key.mismatch", `${modelPath}#/catalogKey`, `catalogKey must be ${expectedCatalogKey}`));
      }
      if (!familyCodes.has(model.familyCode)) {
        issues.push(issue("model.family.missing", `${modelPath}#/familyCode`, `familyCode ${model.familyCode} is not defined`));
      }
      if (!model.source?.sourceUrl || !model.source?.observedAt) {
        issues.push(issue("model.source.missing", `${modelPath}#/source`, "model sourceUrl and observedAt are required"));
      }
      for (const capabilityField of ["supportsStreaming", "supportsTools", "supportsJsonSchema", "codingVisible"]) {
        if (typeof model[capabilityField] !== "boolean") {
          issues.push(
            issue(
              "model.capability_flag.missing",
              `${modelPath}#/${capabilityField}`,
              `${capabilityField} must be a boolean; run node tools/align-model-capabilities.mjs to backfill`,
            ),
          );
        }
      }
      if (!Array.isArray(model.usageScopes)) {
        issues.push(
          issue(
            "model.usage_scopes.missing",
            `${modelPath}#/usageScopes`,
            "usageScopes must be an array of product usage scopes; run node tools/align-model-capabilities.mjs to backfill",
          ),
        );
      } else {
        const unknownScopes = model.usageScopes.filter((scope) => !USAGE_SCOPES.has(scope));
        if (unknownScopes.length > 0) {
          issues.push(
            issue(
              "model.usage_scopes.unknown",
              `${modelPath}#/usageScopes`,
              `unknown usage scope(s): ${unknownScopes.join(", ")}; allowed: ${[...USAGE_SCOPES].sort().join(", ")}`,
            ),
          );
        }
      }
      if (!Array.isArray(model.inputModalities) || model.inputModalities.length === 0) {
        issues.push(issue("model.input_modalities.missing", `${modelPath}#/inputModalities`, "inputModalities must declare at least one modality"));
      }
      if (!Array.isArray(model.outputModalities) || model.outputModalities.length === 0) {
        issues.push(issue("model.output_modalities.missing", `${modelPath}#/outputModalities`, "outputModalities must declare at least one modality"));
      }
      if (!protocolCodes.has(model.apiFormat)) {
        issues.push(issue("model.protocol.unknown", `${modelPath}#/apiFormat`, `${model.apiFormat} is not defined in models/protocols.json`));
      } else if (!supportedProtocols.has(model.apiFormat)) {
        issues.push(issue("model.protocol.unsupported_by_vendor", `${modelPath}#/apiFormat`, `${bundle.vendorCode}/${bundle.regionCode} must include ${model.apiFormat} in supportedProtocols`));
      }
      const modelCatalogKey = expectedCatalogKey;
      if (seenModels.has(modelCatalogKey)) {
        const previous = seenModels.get(modelCatalogKey);
        const differences = modelIdentityDifferences(previous.model, model);
        if (differences.length > 0) {
          issues.push(
            issue(
              "model.identity_conflict",
              modelPath,
              `${modelCatalogKey} differs from ${previous.path} on ${differences.join(", ")}; region-specific data belongs in pricing, ranking, and provider endpoint resources`,
            ),
          );
        }
        continue;
      }
      seenModels.set(modelCatalogKey, { path: modelPath, model });
    }

    const vendorModelIds = new Set(bundle.models.map((model) => model.modelId));
    for (const model of bundle.models) {
      if (
        (model.routingState === "enabled" || model.shelfState === "listed" || model.releaseStage === "active") &&
        !pricedModelIds.has(model.modelId)
      ) {
        issues.push(
          issue(
            "model.pricing.required",
            `${pathPrefix}/models/${safeModelIdPath(model.modelId, issues, `${pathPrefix}/models`) }.json`,
            `${model.catalogKey} is enabled, listed, or active and must have a pricing file with billable rows`,
          ),
        );
      }
    }
    for (const pricing of bundle.pricing) {
      const pricingPath = `${pathPrefix}/pricing/${safeModelIdPath(pricing.modelId, issues, `${pathPrefix}/pricing`) }.json`;
      const actualPricingPath = pricingFilesById.get(pricing.modelId);
      if (actualPricingPath && actualPricingPath !== pricingPath) {
        issues.push(issue("pricing.path.mismatch", actualPricingPath, `${pricing.modelId} must be stored at ${pricingPath}`));
      }
      if (pricing.vendorCode !== bundle.vendorCode) {
        issues.push(issue("pricing.vendor.mismatch", `${pricingPath}#/vendorCode`, "pricing vendorCode must match directory"));
      }
      if (pricing.regionCode !== bundle.regionCode) {
        issues.push(issue("pricing.region.mismatch", `${pricingPath}#/regionCode`, "pricing regionCode must match directory"));
      }
      const expectedCatalogKey = catalogKey(bundle.vendorCode, pricing.modelId);
      if (pricing.catalogKey !== expectedCatalogKey) {
        issues.push(issue("pricing.catalog_key.mismatch", `${pricingPath}#/catalogKey`, `catalogKey must be ${expectedCatalogKey}`));
      }
      if (bundle.vendor.billingCurrency && pricing.currency !== bundle.vendor.billingCurrency) {
        issues.push(issue("pricing.currency.vendor_mismatch", `${pricingPath}#/currency`, `${pricing.modelId} currency must match vendor billingCurrency ${bundle.vendor.billingCurrency}`));
      }
      if (!vendorModelIds.has(pricing.modelId)) {
        issues.push(issue("pricing.model.missing", `${pricingPath}#/modelId`, `${pricing.modelId} is not defined for ${bundle.vendorCode}`));
      }
      const seenPriceIds = new Set();
      const seenPriceKeys = new Set();
      if (!Array.isArray(pricing.prices) || pricing.prices.length === 0) {
        issues.push(issue("pricing.prices.empty", `${pricingPath}#/prices`, `${pricing.modelId} pricing must contain at least one billable row`));
      }
      for (const [index, price] of (pricing.prices ?? []).entries()) {
        if (seenPriceIds.has(price.priceId)) {
          issues.push(issue("price.id.duplicate", `${pricingPath}#/prices/${index}/priceId`, `${price.priceId} is duplicated in ${pricing.modelId}`));
        }
        seenPriceIds.add(price.priceId);

        // The effective pricing key is what makes two rows mutually exclusive:
        // same book, same resource, same meter, same currency, same window.
        //
        // `rateVariant` and `schedule` belong in the key for exactly the reason
        // `conditions` does. A `time_window` rate is discriminated by *when* it
        // applies, not by a request dimension, so the peak/off-peak pair of a
        // model shares every other field and is separated only by its schedule.
        // Keying on `conditions` alone would report that legitimate pair as a
        // duplicate and, worse, would push an author toward re-adding a
        // `tier_code` condition to silence it - reintroducing the
        // unreachable-rate outage described in
        // `validateTimeWindowTierRedundancy`.
        const priceKey = [
          price.priceBookCode,
          price.productCode,
          price.operationCode,
          price.priceSide,
          price.pricingScope ?? "model",
          price.meterCode,
          price.mediaType ?? "",
          price.mediaDirection ?? "",
          price.inputType ?? "",
          price.outputType ?? "",
          price.tierCode ?? "",
          price.currency ?? pricing.currency,
          price.minimumQuantity ?? "0",
          price.effectiveFrom,
          price.rateVariant ?? "standard",
          stableJson(price.schedule ?? null),
          stableJson(price.conditions ?? []),
        ].join("|");
        if (seenPriceKeys.has(priceKey)) {
          issues.push(issue("price.key.duplicate", `${pricingPath}#/prices/${index}`, `${pricing.modelId} has duplicate effective pricing key ${priceKey}`));
        }
        seenPriceKeys.add(priceKey);

        for (const field of ["unitSize", "unitPrice", "minimumQuantity"]) {
          if (!isDecimalString(price[field])) {
            issues.push(issue("price.decimal.invalid", `${pricingPath}#/prices/${index}/${field}`, `${field} must be a decimal string`));
          }
        }
        if (isDecimalString(price.unitSize) && !isPositiveDecimal(price.unitSize)) {
          issues.push(issue("price.unit_size.invalid", `${pricingPath}#/prices/${index}/unitSize`, "unitSize must be positive"));
        }
        validatePriceSchedule(price, pricingPath, index, issues);
        validateTimeWindowTierRedundancy(price, pricingPath, index, issues);
        if (price.quantityStep !== undefined && (!isDecimalString(price.quantityStep) || !isPositiveDecimal(price.quantityStep))) {
          issues.push(issue("price.quantity_step.invalid", `${pricingPath}#/prices/${index}/quantityStep`, "quantityStep must be a positive decimal string"));
        }
        if (price.calculationMode === "flat" && isDecimalString(price.unitSize) && compareDecimalStrings(price.unitSize, "1") !== 0) {
          issues.push(issue("price.flat.unit_size.invalid", `${pricingPath}#/prices/${index}/unitSize`, "flat pricing unitSize must equal one"));
        }
        if (bundle.vendor.billingCurrency && (price.currency ?? pricing.currency) !== bundle.vendor.billingCurrency) {
          issues.push(issue("price.currency.vendor_mismatch", `${pricingPath}#/prices/${index}/currency`, `${price.priceId} currency must match vendor billingCurrency ${bundle.vendor.billingCurrency}`));
        }
        if (!meterCodes.has(price.meterCode)) {
          issues.push(issue("price.meter.missing", `${pricingPath}#/prices/${index}/meterCode`, `meterCode ${price.meterCode} is not defined`));
        }
        if (!price.source?.sourceUrl || !price.source?.observedAt || !price.effectiveFrom) {
          issues.push(issue("price.source.missing", `${pricingPath}#/prices/${index}`, "sourceUrl, observedAt, and effectiveFrom are required"));
        }
        for (const field of ["priceBookCode", "productCode", "operationCode", "rateHash"]) {
          if (typeof price[field] !== "string" || price[field].trim().length === 0) {
            issues.push(issue("price.identity.missing", `${pricingPath}#/prices/${index}/${field}`, `${field} must be a non-empty string`));
          }
        }
        if (!["chargeable", "free", "not_applicable", "unknown"].includes(price.billability)) {
          issues.push(issue("price.billability.invalid", `${pricingPath}#/prices/${index}/billability`, "billability must be explicit"));
        }
        const tieredCalculation = ["graduated", "volume"].includes(price.calculationMode);
        if (price.billability === "chargeable" && !tieredCalculation && isZeroDecimal(price.unitPrice)) {
          issues.push(issue("price.chargeable.zero_price", `${pricingPath}#/prices/${index}/unitPrice`, "zero price cannot be inferred as chargeable"));
        }
        if (["free", "not_applicable"].includes(price.billability) && isPositiveDecimal(price.unitPrice)) {
          issues.push(issue("price.non_chargeable.positive_price", `${pricingPath}#/prices/${index}/unitPrice`, "free or not-applicable rates cannot have a positive price"));
        }
        if (!["request_accepted", "successful_result", "usage_reported"].includes(price.chargeTiming)) {
          issues.push(issue("price.charge_timing.invalid", `${pricingPath}#/prices/${index}/chargeTiming`, "chargeTiming is invalid"));
        }
        if (!["per_unit", "flat", "graduated", "volume", "formula"].includes(price.calculationMode)) {
          issues.push(issue("price.calculation_mode.invalid", `${pricingPath}#/prices/${index}/calculationMode`, "calculationMode is invalid"));
        }
        const tiers = price.tiers ?? [];
        if (tieredCalculation && tiers.length === 0) {
          issues.push(issue("price.tiers.missing", `${pricingPath}#/prices/${index}/tiers`, "graduated and volume rates require at least one tier"));
        }
        if (!tieredCalculation && tiers.length > 0) {
          issues.push(issue("price.tiers.unexpected", `${pricingPath}#/prices/${index}/tiers`, "tiers are allowed only for graduated and volume rates"));
        }
        let expectedLowerBound = "0";
        const tierCodes = new Set();
        for (const [tierIndex, tier] of tiers.entries()) {
          const tierPath = `${pricingPath}#/prices/${index}/tiers/${tierIndex}`;
          for (const field of ["lowerBound", "unitSize", "unitPrice", "flatAmount"]) {
            if (tier[field] !== undefined && !isDecimalString(tier[field])) {
              issues.push(issue("price.tier.decimal.invalid", `${tierPath}/${field}`, `${field} must be a decimal string`));
            }
          }
          if (tier.upperBound !== undefined && tier.upperBound !== null && !isDecimalString(tier.upperBound)) {
            issues.push(issue("price.tier.decimal.invalid", `${tierPath}/upperBound`, "upperBound must be a decimal string or null"));
          }
          if (!tier.tierCode || tierCodes.has(tier.tierCode)) {
            issues.push(issue("price.tier.code.invalid", `${tierPath}/tierCode`, "tierCode must be non-empty and unique within the rate"));
          }
          tierCodes.add(tier.tierCode);
          const lowerBound = tier.lowerBound;
          const upperBound = tier.upperBound == null ? null : tier.upperBound;
          if (expectedLowerBound === null || (isDecimalString(lowerBound) && compareDecimalStrings(lowerBound, expectedLowerBound) !== 0)) {
            issues.push(issue("price.tier.range.gap", `${tierPath}/lowerBound`, "tier ranges must start at zero and remain contiguous"));
          }
          if (upperBound !== null && isDecimalString(lowerBound) && isDecimalString(upperBound) && compareDecimalStrings(upperBound, lowerBound) <= 0) {
            issues.push(issue("price.tier.range.invalid", `${tierPath}/upperBound`, "upperBound must be greater than lowerBound"));
          }
          if (upperBound === null && tierIndex !== tiers.length - 1) {
            issues.push(issue("price.tier.range.open", `${tierPath}/upperBound`, "only the final tier may have an open upper bound"));
          }
          if (isDecimalString(tier.unitSize) && !isPositiveDecimal(tier.unitSize)) {
            issues.push(issue("price.tier.unit_size.invalid", `${tierPath}/unitSize`, "tier unitSize must be positive"));
          }
          if (price.billability === "chargeable" && isZeroDecimal(tier.unitPrice ?? "0") && isZeroDecimal(tier.flatAmount ?? "0")) {
            issues.push(issue("price.tier.chargeable.zero_price", tierPath, "each chargeable tier must have a positive unitPrice or flatAmount"));
          }
          if (["free", "not_applicable"].includes(price.billability) && (isPositiveDecimal(tier.unitPrice ?? "0") || isPositiveDecimal(tier.flatAmount ?? "0"))) {
            issues.push(issue("price.tier.non_chargeable.positive_price", tierPath, "non-chargeable tiers cannot contain positive amounts"));
          }
          expectedLowerBound = upperBound;
        }
        if (tieredCalculation && tiers.length > 0 && tiers.at(-1)?.upperBound != null) {
          issues.push(issue("price.tier.range.unbounded", `${pricingPath}#/prices/${index}/tiers`, "the final tier must have a null upperBound"));
        }
        if (price.calculationMode === "formula" && !price.formula) {
          issues.push(issue("price.formula.missing", `${pricingPath}#/prices/${index}/formula`, "formula rates require a formula definition"));
        }
        if (price.calculationMode !== "formula" && price.formula) {
          issues.push(issue("price.formula.unexpected", `${pricingPath}#/prices/${index}/formula`, "formula is allowed only for formula rates"));
        }
        if (price.formula) {
          const formula = price.formula;
          for (const field of ["constantUnits", "quantityCoefficient", "minimumUnits", "maximumUnits"]) {
            if (formula[field] !== undefined && formula[field] !== null && !isDecimalString(formula[field])) {
              issues.push(issue("price.formula.decimal.invalid", `${pricingPath}#/prices/${index}/formula/${field}`, `${field} must be a decimal string`));
            }
          }
          if (formula.minimumUnits != null && formula.maximumUnits != null && isDecimalString(formula.minimumUnits) && isDecimalString(formula.maximumUnits) && compareDecimalStrings(formula.maximumUnits, formula.minimumUnits) < 0) {
            issues.push(issue("price.formula.bounds.invalid", `${pricingPath}#/prices/${index}/formula`, "maximumUnits must be greater than or equal to minimumUnits"));
          }
          const termCodes = new Set();
          const termDimensions = new Set();
          for (const [termIndex, term] of (formula.terms ?? []).entries()) {
            const termPath = `${pricingPath}#/prices/${index}/formula/terms/${termIndex}`;
            if (!term.termCode || termCodes.has(term.termCode) || !term.dimensionCode || termDimensions.has(term.dimensionCode)) {
              issues.push(issue("price.formula.term.invalid", termPath, "formula term codes and dimensions must be non-empty and unique"));
            }
            if (!isDecimalString(term.coefficient)) {
              issues.push(issue("price.formula.term.coefficient.invalid", `${termPath}/coefficient`, "formula coefficient must be a decimal string"));
            }
            termCodes.add(term.termCode);
            termDimensions.add(term.dimensionCode);
          }
        }
        if (!["sum", "maximum", "minimum", "last", "distinct_invocation"].includes(price.quantityAggregation)) {
          issues.push(issue("price.quantity_aggregation.invalid", `${pricingPath}#/prices/${index}/quantityAggregation`, "quantityAggregation is invalid"));
        }
        const dimensions = new Set();
        for (const [conditionIndex, condition] of (price.conditions ?? []).entries()) {
          const conditionPath = `${pricingPath}#/prices/${index}/conditions/${conditionIndex}`;
          if (typeof condition.dimensionCode !== "string" || condition.dimensionCode.trim().length === 0) {
            issues.push(issue("price.condition.dimension.invalid", `${conditionPath}/dimensionCode`, "condition dimensionCode is required"));
          } else if (dimensions.has(condition.dimensionCode)) {
            issues.push(issue("price.condition.dimension.duplicate", `${conditionPath}/dimensionCode`, `${condition.dimensionCode} is duplicated`));
          }
          dimensions.add(condition.dimensionCode);
          if (!["eq", "neq", "gt", "gte", "lt", "lte", "in", "not_in", "exists"].includes(condition.operator)) {
            issues.push(issue("price.condition.operator.invalid", `${conditionPath}/operator`, "condition operator is invalid"));
          }
        }
      }
    }

    for (const snapshot of bundle.rankings.snapshots ?? []) {
      for (const [index, item] of (snapshot.items ?? []).entries()) {
        if (!vendorModelIds.has(item.modelId)) {
          issues.push(issue("ranking.model.missing", `${pathPrefix}/rankings.json#/snapshots/${snapshot.snapshotDate}/items/${index}/modelId`, `${item.modelId} is not defined for ${bundle.vendorCode}`));
        }
      }
    }

    const voiceByKey = new Map();
    for (const [index, voice] of (bundle.voices ?? []).entries()) {
      const voicePath = `${pathPrefix}/voices.json#/voices/${index}`;
      const expectedVoiceKey = catalogKey(bundle.vendorCode, voice.voiceId);
      if (voice.catalogKey !== expectedVoiceKey) {
        issues.push(issue("voice.catalog_key.mismatch", `${voicePath}/catalogKey`, `catalogKey must be ${expectedVoiceKey}`));
      }
      if (voice.vendorCode !== bundle.vendorCode) {
        issues.push(issue("voice.vendor.mismatch", `${voicePath}/vendorCode`, "voice vendorCode must match directory"));
      }
      if (voice.regionCode !== bundle.regionCode) {
        issues.push(issue("voice.region.mismatch", `${voicePath}/regionCode`, "voice regionCode must match directory"));
      }
      if (!voice.source?.sourceUrl || !voice.source?.observedAt) {
        issues.push(issue("voice.source.missing", `${voicePath}/source`, "voice sourceUrl and observedAt are required"));
      }
      if (voiceByKey.has(voice.catalogKey)) {
        issues.push(issue("voice.catalog_key.duplicate", `${voicePath}/catalogKey`, `${voice.catalogKey} is duplicated in voices.json`));
      }
      voiceByKey.set(voice.catalogKey, voice);
    }

    for (const bindingFile of bundle.modelVoices ?? []) {
      const bindingPath = `${pathPrefix}/model-voices/${safeModelIdPath(bindingFile.modelId, issues, `${pathPrefix}/model-voices`) }.json`;
      if (bindingFile.vendorCode !== bundle.vendorCode) {
        issues.push(issue("model_voice.vendor.mismatch", `${bindingPath}#/vendorCode`, "model voice vendorCode must match directory"));
      }
      if (bindingFile.regionCode !== bundle.regionCode) {
        issues.push(issue("model_voice.region.mismatch", `${bindingPath}#/regionCode`, "model voice regionCode must match directory"));
      }
      const expectedModelKey = catalogKey(bundle.vendorCode, bindingFile.modelId);
      if (bindingFile.catalogKey !== expectedModelKey) {
        issues.push(issue("model_voice.catalog_key.mismatch", `${bindingPath}#/catalogKey`, `catalogKey must be ${expectedModelKey}`));
      }
      if (!vendorModelIds.has(bindingFile.modelId)) {
        issues.push(issue("model_voice.model.missing", `${bindingPath}#/modelId`, `${bindingFile.modelId} is not defined for ${bundle.vendorCode}`));
      }
      if (!bindingFile.source?.sourceUrl || !bindingFile.source?.observedAt) {
        issues.push(issue("model_voice.source.missing", `${bindingPath}#/source`, "sourceUrl and observedAt are required"));
      }
      let defaultCount = 0;
      for (const [index, binding] of (bindingFile.bindings ?? []).entries()) {
        if (!voiceByKey.has(binding.voiceKey)) {
          issues.push(issue("model_voice.binding.voice_missing", `${bindingPath}#/bindings/${index}/voiceKey`, `${binding.voiceKey} is not defined in ${pathPrefix}/voices.json`));
        }
        if (binding.voiceKey !== catalogKey(bundle.vendorCode, binding.voiceId)) {
          issues.push(issue("model_voice.binding.voice_key.mismatch", `${bindingPath}#/bindings/${index}/voiceKey`, `voiceKey must be ${catalogKey(bundle.vendorCode, binding.voiceId)}`));
        }
        if (binding.isDefault) {
          defaultCount += 1;
        }
      }
      if (defaultCount > 1) {
        issues.push(issue("model_voice.binding.default.duplicate", `${bindingPath}#/bindings`, "at most one binding may set isDefault"));
      }
    }

    const modelById = new Map(bundle.models.map((model) => [model.modelId, model]));
    const pricingTierCodesByModel = new Map();
    // modelId -> meterCode -> Map<tierCode, Set<unitPrice>>. Kept per meter
    // because the two rules below need different scopes: reachability unions the
    // meters, the ambiguity rule must stay inside one.
    const videoPricingByMeter = new Map();
    for (const pricing of bundle.pricing ?? []) {
      const tiers = new Set(
        (pricing.prices ?? [])
          .map((price) => price.tierCode)
          .filter((tierCode) => typeof tierCode === "string" && tierCode.length > 0),
      );
      pricingTierCodesByModel.set(pricing.modelId, tiers);
      const byMeter = videoPricingByMeter.get(pricing.modelId) ?? new Map();
      for (const price of pricing.prices ?? []) {
        if (!VIDEO_PRICING_METERS.has(price.meterCode)) {
          continue;
        }
        if (typeof price.tierCode !== "string" || price.tierCode.length === 0) {
          continue;
        }
        const meterTiers = byMeter.get(price.meterCode) ?? new Map();
        const prices = meterTiers.get(price.tierCode) ?? new Set();
        prices.add(String(price.unitPrice ?? ""));
        meterTiers.set(price.tierCode, prices);
        byMeter.set(price.meterCode, meterTiers);
      }
      videoPricingByMeter.set(pricing.modelId, byMeter);
    }
    // Union across the meters — the scope the reachability rule compares against.
    const videoPricingByModel = new Map(
      [...videoPricingByMeter].map(([modelId, byMeter]) => {
        const merged = new Map();
        for (const meterTiers of byMeter.values()) {
          for (const [tierCode, prices] of meterTiers) {
            const seen = merged.get(tierCode) ?? new Set();
            for (const price of prices) {
              seen.add(price);
            }
            merged.set(tierCode, seen);
          }
        }
        return [modelId, merged];
      }),
    );

    for (const profileFile of bundle.modelVideoProfiles ?? []) {
      const profilePath = `${pathPrefix}/model-video-profiles/${safeModelIdPath(profileFile.modelId, issues, `${pathPrefix}/model-video-profiles`)}.json`;
      if (profileFile.vendorCode !== bundle.vendorCode) {
        issues.push(issue("model_video_profile.vendor.mismatch", `${profilePath}#/vendorCode`, "model video profile vendorCode must match directory"));
      }
      if (profileFile.regionCode !== bundle.regionCode) {
        issues.push(issue("model_video_profile.region.mismatch", `${profilePath}#/regionCode`, "model video profile regionCode must match directory"));
      }
      const expectedModelKey = catalogKey(bundle.vendorCode, profileFile.modelId);
      if (profileFile.catalogKey !== expectedModelKey) {
        issues.push(issue("model_video_profile.catalog_key.mismatch", `${profilePath}#/catalogKey`, `catalogKey must be ${expectedModelKey}`));
      }
      if (!vendorModelIds.has(profileFile.modelId)) {
        issues.push(issue("model_video_profile.model.missing", `${profilePath}#/modelId`, `${profileFile.modelId} is not defined for ${bundle.vendorCode}`));
      }
      if (!profileFile.source?.sourceUrl || !profileFile.source?.observedAt) {
        issues.push(issue("model_video_profile.source.missing", `${profilePath}#/source`, "sourceUrl and observedAt are required"));
      }
      const model = modelById.get(profileFile.modelId);
      const profileCodes = new Set();
      let defaultCount = 0;
      for (const [index, profile] of (profileFile.profiles ?? []).entries()) {
        const itemPath = `${profilePath}#/profiles/${index}`;
        const expectedProfileKey = videoProfileCatalogKey(bundle.vendorCode, profileFile.modelId, profile.profileCode);
        if (profile.catalogKey !== expectedProfileKey) {
          issues.push(issue("model_video_profile.profile.catalog_key.mismatch", `${itemPath}/catalogKey`, `catalogKey must be ${expectedProfileKey}`));
        }
        if (profileCodes.has(profile.profileCode)) {
          issues.push(issue("model_video_profile.profile.duplicate", `${itemPath}/profileCode`, `${profile.profileCode} is duplicated`));
        }
        profileCodes.add(profile.profileCode);
        if (profile.isDefault) {
          defaultCount += 1;
        }
        if (model) {
          const requiredInputs = GENERATION_MODE_INPUTS[profile.generationMode] ?? [];
          if (
            requiredInputs.length > 0
            && !requiredInputs.some((modality) => (model.inputModalities ?? []).includes(modality))
          ) {
            issues.push(
              issue(
                "model_video_profile.generation_mode.input_mismatch",
                `${itemPath}/generationMode`,
                `${profile.generationMode} requires one of ${requiredInputs.join(", ")} input modalities`,
              ),
            );
          }
          if (model.primaryCapability !== "video" && !(model.capabilities ?? []).includes("video")) {
            issues.push(issue("model_video_profile.model.not_video", `${profilePath}#/modelId`, `${profileFile.modelId} is not a video model`));
          }
        }
        if (profile.durationPolicy === "fixed") {
          if (!Number.isInteger(profile.durationSeconds) || profile.durationSeconds <= 0) {
            issues.push(issue("model_video_profile.duration.fixed.missing", `${itemPath}/durationSeconds`, "durationSeconds is required for fixed durationPolicy"));
          }
          if (!DURATION_TIER_CODES.has(profile.durationTierCode)) {
            issues.push(issue("model_video_profile.duration.tier.invalid", `${itemPath}/durationTierCode`, "durationTierCode must use canonical dur_* vocabulary"));
          }
        } else if (profile.durationPolicy === "discrete") {
          if (!Array.isArray(profile.durationOptions) || profile.durationOptions.length === 0) {
            issues.push(issue("model_video_profile.duration.discrete.missing", `${itemPath}/durationOptions`, "durationOptions is required for discrete durationPolicy"));
          }
          if (!Array.isArray(profile.durationTierCodes) || profile.durationTierCodes.length !== profile.durationOptions?.length) {
            issues.push(issue("model_video_profile.duration.tier_codes.mismatch", `${itemPath}/durationTierCodes`, "durationTierCodes must align with durationOptions"));
          }
          for (const tierCode of profile.durationTierCodes ?? []) {
            if (!DURATION_TIER_CODES.has(tierCode)) {
              issues.push(issue("model_video_profile.duration.tier.invalid", `${itemPath}/durationTierCodes`, "durationTierCodes must use canonical dur_* vocabulary"));
            }
          }
        } else if (profile.durationPolicy === "range" || profile.durationPolicy === "continuous") {
          if (!Number.isInteger(profile.minDurationSeconds) || !Number.isInteger(profile.maxDurationSeconds)) {
            issues.push(issue("model_video_profile.duration.range.missing", `${itemPath}`, "minDurationSeconds and maxDurationSeconds are required for range/continuous durationPolicy"));
          } else if (profile.minDurationSeconds > profile.maxDurationSeconds) {
            issues.push(issue("model_video_profile.duration.range.invalid", `${itemPath}`, "minDurationSeconds must be <= maxDurationSeconds"));
          }
          if (!Number.isInteger(profile.durationStepSeconds) || profile.durationStepSeconds <= 0) {
            issues.push(issue("model_video_profile.duration.step.missing", `${itemPath}/durationStepSeconds`, "durationStepSeconds is required for range/continuous durationPolicy"));
          }
        }
        for (const tierCode of profile.pricingTierCodes ?? []) {
          const modelTiers = pricingTierCodesByModel.get(profileFile.modelId) ?? new Set();
          if (!modelTiers.has(tierCode)) {
            issues.push(issue("model_video_profile.pricing.tier.missing", `${itemPath}/pricingTierCodes`, `${tierCode} is not declared on ${expectedModelKey} pricing`));
          }
        }
        // The Cloud Router bills a video request against the rate whose
        // `tier_code` condition equals a tier the profile declares. When the
        // profile's declared tiers and the model's video pricing tiers do not
        // intersect, that profile is unreachable: the router reports a pricing
        // gap instead of guessing a tier.
        //
        // Severity follows what the catalog can actually do about it:
        //
        // * `error` — the price book quotes the same resolution under another
        //   spelling and every such code costs the same, so linking them is
        //   mechanical and leaving it undone is simply drift (bytedance quoted
        //   `720p` while every other vendor quoted `res_720p`).
        // * `warning` — the price book splits that resolution by a dimension the
        //   profile cannot express (`res_768p_dur_6s`, `audio` vs `no_audio`,
        //   `over_4mp`), or quotes no resolution token at all. Recording one
        //   would be a guess, so the mapping needs a product decision.
        //
        // A model whose video rates carry no `tier_code` at all is exempt —
        // there the rate applies unconditionally.
        const pricedByTier = videoPricingByModel.get(profileFile.modelId) ?? new Map();
        if (pricedByTier.size > 0) {
          const declaredTiers = [
            profile.resolutionTierCode,
            profile.durationTierCode,
            ...(profile.durationTierCodes ?? []),
            ...(profile.pricingTierCodes ?? []),
          ].filter((tierCode) => typeof tierCode === "string" && tierCode.length > 0);
          if (!declaredTiers.some((tierCode) => pricedByTier.has(tierCode))) {
            const token = (profile.resolutionTierCode ?? "").startsWith("res_")
              ? profile.resolutionTierCode.slice(4)
              : profile.resolution ?? "";
            const candidates = [...pricedByTier.keys()].filter(
              (tierCode) => token.length > 0 && tierCode.split("_").includes(token),
            );
            const candidatePrices = new Set(
              candidates.map((tierCode) => [...pricedByTier.get(tierCode)].sort().join("|")),
            );
            const mechanical = candidates.length > 0 && candidatePrices.size === 1;
            issues.push(
              issue(
                "model_video_profile.pricing.tier.unreachable",
                `${itemPath}`,
                `${expectedModelKey} bills ${[...pricedByTier.keys()].sort().join(", ")} but this profile declares `
                  + `${declaredTiers.length > 0 ? declaredTiers.join(", ") : "no tier code"}`
                  + (mechanical
                    ? `; set pricingTierCodes to ${candidates.sort().join(", ")}`
                    : "; the price book splits this tier by a dimension the profile cannot express"),
                mechanical ? "error" : "warning",
              ),
            );
          } else if ((profile.pricingTierCodes ?? []).length === 0) {
            // The declared tier resolves, so `unreachable` stays silent — but the
            // model may still quote *dearer* variants of the same resolution on
            // the same output meter. With no `pricingTierCodes` the router settles
            // on the cheapest match, which under-bills whenever the request
            // carries the dimension the profile cannot name. Measured case:
            // `kuaishou/kling-v3` declares `res_1080p` while `audio_res_1080p` /
            // `motion_res_1080p` cost 1.5x more, and `kuaishou/kling-v2-6`
            // declares `res_1080p` while `audio_voice_1080p` costs 2.4x more.
            //
            // Kling publishes those variants as first-class tiers — the vendor
            // price page reads `无声` (0.8), `有声 x 未指定音色` (1.2) and
            // `动作控制` (1.2) for 1080p on `kling-v3` — so the cheapest tier is
            // *correct* for a silent request and wrong for an audio one. The
            // dimension therefore cannot be decided here: it belongs to the
            // request, and the runtime reads no such fact out of the
            // vendor-native body. Report the gap instead of pinning a tier.
            const declaredResolution = profile.resolutionTierCode;
            if (typeof declaredResolution === "string" && declaredResolution.startsWith("res_")) {
              const token = declaredResolution.slice(4);
              const byMeter = videoPricingByMeter.get(profileFile.modelId) ?? new Map();
              const declaredScopes = [];
              const dearerVariants = [];
              for (const [meterCode, meterTiers] of byMeter) {
                if (!VIDEO_OUTPUT_PRICING_METERS.has(meterCode)) {
                  continue;
                }
                const declaredMax = maxDecimalString(meterTiers.get(declaredResolution) ?? new Set());
                if (declaredMax === null) {
                  continue;
                }
                declaredScopes.push(`${declaredResolution} = ${declaredMax} on ${meterCode}`);
                for (const [tierCode, prices] of meterTiers) {
                  if (tierCode === declaredResolution || !tierCode.split("_").includes(token)) {
                    continue;
                  }
                  const variantMax = maxDecimalString(prices);
                  if (variantMax === null || compareDecimalStrings(variantMax, declaredMax) <= 0) {
                    continue;
                  }
                  dearerVariants.push(`${tierCode} (${[...prices].sort().join(", ")} on ${meterCode})`);
                }
              }
              if (dearerVariants.length > 0) {
                const audioClaim = profile.outputAudio === true
                  ? " This profile also declares outputAudio, so the catalog claims audio output while the "
                    + "only tier the router can reach is the silent one."
                  : "";
                issues.push(
                  issue(
                    "model_video_profile.pricing.tier.ambiguous",
                    `${itemPath}`,
                    `${expectedModelKey} prices ${declaredScopes.sort().join(", ")} and also quotes a dearer variant `
                      + `for the same resolution — ${dearerVariants.sort().join(", ")}; with no pricingTierCodes the `
                      + "router settles on the cheapest tier."
                      + audioClaim
                      + " Pinning pricingTierCodes is only right when every routed request shares one variant; "
                      + "otherwise the variant must come from the request instead of the cheapest tier.",
                    "warning",
                  ),
                );
              }
            }
          }
        }
      }
      if (defaultCount > 1) {
        issues.push(issue("model_video_profile.default.duplicate", `${profilePath}#/profiles`, "at most one profile may set isDefault"));
      }

      // Coverage rather than expressiveness. A plain `res_<token>` rate the book carries is
      // declarable in principle, and when no profile of the model declares it the request can
      // never be priced: `decide_video_pricing_tier` filters the profile set by the requested
      // resolution *before* it looks for a tier, so naming an undeclared resolution yields an
      // empty candidate list and `DeclaredTierNotPriced` — refused before dispatch although
      // the rate sits in the book. Reported once per file; `unreachable` above covers the other
      // direction (a profile declaring something the book cannot price).
      const pricedResolutionTokens = new Set();
      const byOutputMeter = videoPricingByMeter.get(profileFile.modelId) ?? new Map();
      for (const [meterCode, meterTiers] of byOutputMeter) {
        if (!VIDEO_OUTPUT_PRICING_METERS.has(meterCode)) {
          continue;
        }
        for (const tierCode of meterTiers.keys()) {
          const match = PLAIN_RESOLUTION_TIER_CODE.exec(tierCode);
          if (match) {
            pricedResolutionTokens.add(match[1]);
          }
        }
      }
      const declaredResolutionTokens = new Set();
      for (const profile of profileFile.profiles ?? []) {
        if (typeof profile.resolution === "string" && profile.resolution.length > 0) {
          declaredResolutionTokens.add(profile.resolution);
        }
        for (const code of [
          profile.resolutionTierCode,
          ...(profile.pricingTierCodes ?? []),
          profile.durationTierCode,
          ...(profile.durationTierCodes ?? []),
        ]) {
          const match = typeof code === "string" ? PLAIN_RESOLUTION_TIER_CODE.exec(code) : null;
          if (match) {
            declaredResolutionTokens.add(match[1]);
          }
        }
      }
      const undeclaredResolutions = [...pricedResolutionTokens]
        .filter((token) => !declaredResolutionTokens.has(token))
        .sort();
      if (undeclaredResolutions.length > 0) {
        issues.push(
          issue(
            "model_video_profile.pricing.tier.undeclared_resolution",
            `${profilePath}#/profiles`,
            `${expectedModelKey} can be billed at ${undeclaredResolutions.join(", ")} but no profile declares `
              + `${undeclaredResolutions.length === 1 ? "that resolution" : "those resolutions"}; a request naming `
              + "one of them matches no profile and is refused before dispatch. Run "
              + "`node tools/sync-video-profile-resolutions.mjs --write` to add them.",
          ),
        );
      }
    }

    const profileModelIds = new Set((bundle.modelVideoProfiles ?? []).map((file) => file.modelId));
    for (const model of bundle.models) {
      if (model.primaryCapability === "video" && !profileModelIds.has(model.modelId)) {
        issues.push(
          issue(
            "model_video_profile.model.required",
            `${pathPrefix}/model-video-profiles/${model.modelId}.json`,
            `${model.modelId} is a video model and requires a model-video-profiles file`,
          ),
        );
      }
    }
  }

  const expectedIndex = buildCatalogIndex(root);
  const expectedVendors = buildVendorList(root);
  const currentIndex = readJsonFile(join(root, "models", "index.json"));
  const currentVendors = readJsonFile(join(root, "models", "vendors.json"));

  const currentIndexVendors = new Map(
    (currentIndex.vendors ?? []).map((vendor, index) => [
      `${vendor.vendorCode}/${vendor.regionCode}`,
      { vendor, index },
    ]),
  );
  const expectedIndexVendors = new Map(
    (expectedIndex.vendors ?? []).map((vendor) => [`${vendor.vendorCode}/${vendor.regionCode}`, vendor]),
  );
  for (const [vendorRegionKey, expectedVendor] of expectedIndexVendors) {
    const current = currentIndexVendors.get(vendorRegionKey);
    if (!current) {
      issues.push(issue("index.vendor_region.missing", "models/index.json#/vendors", `${vendorRegionKey} is missing from models/index.json`));
      continue;
    }
    const path = `models/index.json#/vendors/${current.index}`;
    const checks = [
      ["path", "index.path.mismatch"],
      ["familiesPath", "index.families_path.mismatch"],
      ["modelsPath", "index.models_path.mismatch"],
      ["pricingPath", "index.pricing_path.mismatch"],
      ["rankingsPath", "index.rankings_path.mismatch"],
      ["catalogKeyPrefix", "index.catalog_key_prefix.mismatch"],
      ["modelCount", "index.model_count.mismatch"],
      ["pricingFileCount", "index.pricing_file_count.mismatch"],
      ["familyCount", "index.family_count.mismatch"],
      ["rankingSnapshotCount", "index.ranking_snapshot_count.mismatch"],
      ["sha256", "index.sha256.mismatch"],
    ];
    for (const [field, code] of checks) {
      if (stableJson(current.vendor[field]) !== stableJson(expectedVendor[field])) {
        issues.push(issue(code, `${path}/${field}`, `${vendorRegionKey} ${field} must match generated catalog index`));
      }
    }
    for (const [field, code] of [
      ["modelFiles", "index.model_files.mismatch"],
      ["pricingFiles", "index.pricing_files.mismatch"],
    ]) {
      if (stableJson(current.vendor[field]) !== stableJson(expectedVendor[field])) {
        issues.push(issue(code, `${path}/${field}`, `${vendorRegionKey} ${field} must exactly match published JSON files`));
      }
    }
  }
  for (const [vendorRegionKey, current] of currentIndexVendors) {
    if (!expectedIndexVendors.has(vendorRegionKey)) {
      issues.push(issue("index.vendor_region.extra", `models/index.json#/vendors/${current.index}`, `${vendorRegionKey} is not backed by a catalog directory`));
    }
  }
  for (const vendor of currentIndex.vendors ?? []) {
    const pathPrefix = `models/${vendor.vendorCode}/${vendor.regionCode}`;
    for (const [field, directory] of [
      ["modelFiles", "models"],
      ["pricingFiles", "pricing"],
    ]) {
      const files = vendor[field];
      if (!Array.isArray(files)) {
        issues.push(issue(`index.${field}.invalid`, `models/index.json#/${vendor.vendorCode}/${vendor.regionCode}/${field}`, `${field} must be an array`));
        continue;
      }
      for (const [index, relPath] of files.entries()) {
        if (typeof relPath !== "string") {
          issues.push(issue(`index.${field}.invalid`, `models/index.json#/${vendor.vendorCode}/${vendor.regionCode}/${field}/${index}`, `${field} entries must be strings`));
          continue;
        }
        if (!relPath.startsWith(`${vendor.vendorCode}/${vendor.regionCode}/${directory}/`)) {
          issues.push(issue(`index.${field}.path_scope`, `models/index.json#/${vendor.vendorCode}/${vendor.regionCode}/${field}/${index}`, `${relPath} must stay inside ${pathPrefix}/${directory}`));
        }
      }
    }
    if (vendor.catalogKeyPrefix !== `${vendor.vendorCode}/`) {
      issues.push(issue("index.catalog_key_prefix.identity", `models/index.json#/${vendor.vendorCode}/${vendor.regionCode}/catalogKeyPrefix`, "catalogKeyPrefix must use vendor/model identity and must not include regionCode"));
    }
  }

  if (stableJson(currentIndex) !== stableJson(expectedIndex)) {
    issues.push(issue("index.stale", "models/index.json", "models/index.json must be regenerated with tools/build-index.mjs"));
  }
  if (stableJson(currentVendors) !== stableJson(expectedVendors)) {
    issues.push(issue("vendors.stale", "models/vendors.json", "models/vendors.json must be regenerated with tools/build-index.mjs"));
  }

  return {
    ok: issues.every((item) => item.severity !== "error"),
    issues,
  };
}

function collectJsonFileRefsForDirectory(modelsRoot, root) {
  return collectNestedJsonFiles(root).map((path) => path.slice(modelsRoot.length + 1).replace(/\\/g, "/"));
}

function collectNestedJsonFiles(root) {
  const files = [];
  const entries = existsSync(root) ? readdirSync(root, { withFileTypes: true }) : [];
  for (const entry of entries) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectNestedJsonFiles(path));
      continue;
    }
    if (entry.isFile() && entry.name.endsWith(".json")) {
      files.push(path);
    }
  }
  return files.sort((left, right) => left.localeCompare(right));
}

function safeModelIdPath(modelId, issues, path) {
  try {
    return modelIdPath(modelId);
  } catch {
    issues.push(issue("model_id.path.invalid", path, `${modelId} must be a safe relative modelId path`));
    return String(modelId ?? "__invalid__").replace(/\\/g, "/");
  }
}

function modelIdentityDifferences(left, right) {
  const fields = [
    "catalogKey",
    "modelId",
    "displayName",
    "vendorCode",
    "familyCode",
    "primaryCapability",
    "capabilities",
    "inputModalities",
    "outputModalities",
    "apiFormat",
    "contextTokens",
    "maxInputTokens",
    "maxOutputTokens",
    "supportsStreaming",
    "supportsTools",
    "supportsJsonSchema",
    "usageScopes",
    "codingVisible",
    "replacementModel",
  ];
  return fields.filter((field) => stableJson(left[field] ?? null) !== stableJson(right[field] ?? null));
}

const REQUIRED_CLIENT_APIS = {
  codex: {
    displayName: "Codex",
    defaultApiCode: "openai.codex.responses",
    defaultResourceCode: "api.openai.codex",
  },
  claude_code: {
    displayName: "Claude Code",
    defaultApiCode: "anthropic.claude_code",
    defaultResourceCode: "api.anthropic.claude_code",
  },
  gemini_cli: {
    displayName: "Gemini CLI",
    defaultApiCode: "gemini.generate_content",
    defaultResourceCode: "api.gemini.generate_content",
  },
};

function loadRoutingResourceIndex(root) {
  const resourcesRoot = join(root, "..", "ai-routing", "resources");
  const resourceCodes = new Set();
  const apiCodes = new Set();
  let available = false;
  for (const fileName of ["openai-resources.json", "vendor-native-resources.json"]) {
    const path = join(resourcesRoot, fileName);
    if (!existsSync(path)) {
      continue;
    }
    available = true;
    const payload = readJsonFile(path);
    for (const item of payload.items ?? []) {
      if (typeof item.resourceCode === "string") {
        resourceCodes.add(item.resourceCode);
      }
      if (typeof item.apiCode === "string") {
        apiCodes.add(item.apiCode);
      }
    }
  }
  return { apiCodes, resourceCodes, available };
}

function validateClientApiCompatibility({ vendor, protocolCodes, routingResourceIndex, pathPrefix, issues }) {
  const compatibility = vendor.clientApiCompatibility;
  if (!compatibility || typeof compatibility !== "object" || Array.isArray(compatibility)) {
    issues.push(issue("vendor.client_api_compatibility.missing", `${pathPrefix}/vendor.json#/clientApiCompatibility`, "clientApiCompatibility must declare codex, claude_code, and gemini_cli"));
    return;
  }
  for (const [clientApiCode, standard] of Object.entries(REQUIRED_CLIENT_APIS)) {
    const item = compatibility[clientApiCode];
    const itemPath = `${pathPrefix}/vendor.json#/clientApiCompatibility/${clientApiCode}`;
    if (!item || typeof item !== "object" || Array.isArray(item)) {
      issues.push(issue("vendor.client_api_compatibility.required", itemPath, `${clientApiCode} compatibility is required`));
      continue;
    }
    if (item.clientApiCode !== clientApiCode) {
      issues.push(issue("vendor.client_api_compatibility.code_mismatch", `${itemPath}/clientApiCode`, `clientApiCode must be ${clientApiCode}`));
    }
    if (item.displayName !== standard.displayName) {
      issues.push(issue("vendor.client_api_compatibility.display_name", `${itemPath}/displayName`, `${clientApiCode} displayName must be ${standard.displayName}`));
    }
    if (!["supported", "unsupported", "partial", "convert"].includes(item.supportStatus)) {
      issues.push(issue("vendor.client_api_compatibility.status", `${itemPath}/supportStatus`, "supportStatus must be supported, unsupported, partial, or convert"));
    }
    if (typeof item.notes !== "string" || item.notes.trim().length === 0) {
      issues.push(issue("vendor.client_api_compatibility.notes", `${itemPath}/notes`, "notes must explain the compatibility decision"));
    }
    if (!item.source?.sourceUrl || !item.source?.observedAt) {
      issues.push(issue("vendor.client_api_compatibility.source", `${itemPath}/source`, "sourceUrl and observedAt are required"));
    }
    for (const [field, codeSet, code] of [
      ["protocolCodes", protocolCodes, "vendor.client_api_compatibility.protocol_unknown"],
      ["apiCodes", routingResourceIndex.apiCodes, "vendor.client_api_compatibility.api_unknown"],
      ["resourceCodes", routingResourceIndex.resourceCodes, "vendor.client_api_compatibility.resource_unknown"],
    ]) {
      if (!Array.isArray(item[field])) {
        issues.push(issue("vendor.client_api_compatibility.array", `${itemPath}/${field}`, `${field} must be an array`));
        continue;
      }
      for (const [index, value] of item[field].entries()) {
        if (typeof value !== "string" || value.length === 0) {
          issues.push(issue("vendor.client_api_compatibility.value", `${itemPath}/${field}/${index}`, `${field} entries must be non-empty strings`));
        } else if ((field === "protocolCodes" || routingResourceIndex.available) && !codeSet.has(value)) {
          issues.push(issue(code, `${itemPath}/${field}/${index}`, `${value} is not defined`));
        }
      }
    }
    if (item.supportStatus === "supported" || item.supportStatus === "partial") {
      if (!item.apiCodes?.includes(standard.defaultApiCode)) {
        issues.push(issue("vendor.client_api_compatibility.default_api_missing", `${itemPath}/apiCodes`, `${clientApiCode} support must include ${standard.defaultApiCode}`));
      }
      if (!item.resourceCodes?.includes(standard.defaultResourceCode)) {
        issues.push(issue("vendor.client_api_compatibility.default_resource_missing", `${itemPath}/resourceCodes`, `${clientApiCode} support must include ${standard.defaultResourceCode}`));
      }
    }
  }
}

// 标准兼容族:vendor apiEndpoints 仅允许这些 family 作为 key
const API_ENDPOINT_FAMILIES = new Set(["openai", "anthropic", "google"]);

/**
 * 校验 vendor apiEndpoints 配置(兼容接口 Base URL 原始配置):
 * - family key 必须是标准兼容族(openai/anthropic/google)
 * - family 必须被 supportedProtocols 中同族协议声明支持
 * - host 必须是纯小写域名(无协议、无路径)
 * - pathPrefix 必须是该 family 标准集合(models/protocols.json#/families)中的值
 */
function validateApiEndpoints({ vendor, protocolFamilyByCode, familyPathPrefixes, pathPrefix, issues }) {
  const endpoints = vendor.apiEndpoints;
  if (endpoints === undefined || endpoints === null) {
    // 允许缺失:部分 vendor 无标准兼容端点(如纯音频/视频厂商)
    return;
  }
  if (typeof endpoints !== "object" || Array.isArray(endpoints)) {
    issues.push(issue("vendor.api_endpoints.invalid", `${pathPrefix}/vendor.json#/apiEndpoints`, "apiEndpoints must be an object keyed by protocol family"));
    return;
  }
  const supportedFamilies = new Set();
  for (const protocolCode of vendor.supportedProtocols ?? []) {
    const family = protocolFamilyByCode.get(protocolCode);
    if (family) {
      supportedFamilies.add(family);
    }
  }
  for (const [family, endpoint] of Object.entries(endpoints)) {
    const itemPath = `${pathPrefix}/vendor.json#/apiEndpoints/${family}`;
    if (!API_ENDPOINT_FAMILIES.has(family)) {
      issues.push(issue("vendor.api_endpoints.family.unknown", itemPath, `family ${family} is not a standard compatibility family; allowed: ${[...API_ENDPOINT_FAMILIES].join(", ")}`));
      continue;
    }
    if (!supportedFamilies.has(family)) {
      issues.push(issue("vendor.api_endpoint.unsupported_family", itemPath, `family ${family} must be supported by a protocolCode of the same family in supportedProtocols`));
    }
    if (!endpoint || typeof endpoint !== "object" || Array.isArray(endpoint)) {
      issues.push(issue("vendor.api_endpoint.invalid", itemPath, "apiEndpoint must be an object with host and pathPrefix"));
      continue;
    }
    if (typeof endpoint.host !== "string" || !/^[a-z0-9][a-z0-9.-]*$/.test(endpoint.host)) {
      issues.push(issue("vendor.api_endpoint.host.invalid", `${itemPath}/host`, "host must be a lowercase domain without scheme or path"));
    }
    const standardPathPrefixes = familyPathPrefixes.get(family);
    if (typeof endpoint.pathPrefix !== "string" || (endpoint.pathPrefix !== "" && (!endpoint.pathPrefix.startsWith("/") || endpoint.pathPrefix.endsWith("/")))) {
      issues.push(issue("vendor.api_endpoint.path.invalid", `${itemPath}/pathPrefix`, "pathPrefix must be empty or start with '/' and not end with '/'"));
    } else if (!standardPathPrefixes?.has(endpoint.pathPrefix)) {
      const allowed = standardPathPrefixes ? [...standardPathPrefixes].map((value) => JSON.stringify(value)).join(", ") : "(family not declared in models/protocols.json)";
      issues.push(issue("vendor.api_endpoint.path.not_standard", `${itemPath}/pathPrefix`, `pathPrefix ${JSON.stringify(endpoint.pathPrefix)} is not in the ${family} family standard set: ${allowed}`));
    }
  }
}

// LLM API 协议集:protocolBaseUrls 仅允许这些协议 code 作为 key
const LLM_PROTOCOL_CODES = new Set(["openai_compatible", "openai_responses", "anthropic_messages"]);

/**
 * 校验 vendor protocolBaseUrls 配置(按 LLM API 协议 code 的官方默认 Base URL):
 * - key 必须是 LLM 协议 code(openai_compatible/openai_responses/anthropic_messages)
 * - 协议必须被 supportedProtocols 声明支持
 * - host 必须是纯小写域名(无协议、无路径)
 * - pathPrefix 必须是该协议族标准集合(models/protocols.json#/families)中的值
 * - 未收录的协议(vendor_native、google_gemini、无官方地址的声明)直接省略
 */
function validateProtocolBaseUrls({ vendor, protocolFamilyByCode, familyPathPrefixes, pathPrefix, issues }) {
  const baseUrls = vendor.protocolBaseUrls;
  if (baseUrls === undefined || baseUrls === null) {
    // 允许缺失:非 LLM 协议厂商或无官方默认地址的厂商省略该字段
    return;
  }
  if (typeof baseUrls !== "object" || Array.isArray(baseUrls)) {
    issues.push(issue("vendor.protocol_base_urls.invalid", `${pathPrefix}/vendor.json#/protocolBaseUrls`, "protocolBaseUrls must be an object keyed by LLM protocol code"));
    return;
  }
  for (const [protocolCode, endpoint] of Object.entries(baseUrls)) {
    const itemPath = `${pathPrefix}/vendor.json#/protocolBaseUrls/${protocolCode}`;
    if (!LLM_PROTOCOL_CODES.has(protocolCode)) {
      issues.push(issue("vendor.protocol_base_urls.protocol.unknown", itemPath, `protocol ${protocolCode} is not an LLM protocol code; allowed: ${[...LLM_PROTOCOL_CODES].join(", ")}`));
      continue;
    }
    if (!(vendor.supportedProtocols ?? []).includes(protocolCode)) {
      issues.push(issue("vendor.protocol_base_urls.unsupported_protocol", itemPath, `protocol ${protocolCode} must be declared in supportedProtocols`));
    }
    if (!endpoint || typeof endpoint !== "object" || Array.isArray(endpoint)) {
      issues.push(issue("vendor.protocol_base_url.invalid", itemPath, "protocolBaseUrl must be an object with host and pathPrefix"));
      continue;
    }
    if (typeof endpoint.host !== "string" || !/^[a-z0-9][a-z0-9.-]*$/.test(endpoint.host)) {
      issues.push(issue("vendor.protocol_base_url.host.invalid", `${itemPath}/host`, "host must be a lowercase domain without scheme or path"));
    }
    const family = protocolFamilyByCode.get(protocolCode);
    const standardPathPrefixes = family ? familyPathPrefixes.get(family) : undefined;
    if (typeof endpoint.pathPrefix !== "string" || (endpoint.pathPrefix !== "" && (!endpoint.pathPrefix.startsWith("/") || endpoint.pathPrefix.endsWith("/")))) {
      issues.push(issue("vendor.protocol_base_url.path.invalid", `${itemPath}/pathPrefix`, "pathPrefix must be empty or start with '/' and not end with '/'"));
    } else if (!standardPathPrefixes?.has(endpoint.pathPrefix)) {
      const allowed = standardPathPrefixes ? [...standardPathPrefixes].map((value) => JSON.stringify(value)).join(", ") : "(family not declared in models/protocols.json)";
      issues.push(issue("vendor.protocol_base_url.path.not_standard", `${itemPath}/pathPrefix`, `pathPrefix ${JSON.stringify(endpoint.pathPrefix)} is not in the ${family} family standard set: ${allowed}`));
    }
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const payload = validateCatalog(projectRootFromTool(import.meta.url));
  console.log(JSON.stringify(payload, null, 2));
  if (!payload.ok) {
    process.exit(1);
  }
}
