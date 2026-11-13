#!/usr/bin/env node
/**
 * Shared capability and pricing alignment rules for sdkwork-models catalog sync.
 */

export const CHAT_LIKE_CAPABILITIES = new Set(["chat", "reasoning", "code", "tool"]);
export const GENERATIVE_MEDIA_CAPABILITIES = new Set(["image", "video", "music", "sfx", "streaming"]);

/** Meter modalities considered valid per primary capability (plus api/* fallback meters). */
export const ALLOWED_METER_MODALITIES_BY_CAPABILITY = {
  chat: new Set(["text", "tool", "image", "audio", "video"]),
  reasoning: new Set(["text", "tool", "image", "audio", "video"]),
  code: new Set(["text", "tool", "image", "audio", "video"]),
  tool: new Set(["text", "tool", "image", "audio", "video"]),
  embedding: new Set(["embedding", "text"]),
  image: new Set(["image", "text"]),
  audio: new Set(["audio", "text", "image", "video"]),
  music: new Set(["audio", "music", "api"]),
  sfx: new Set(["audio", "sfx", "api"]),
  video: new Set(["video", "api", "text", "image", "audio"]),
  streaming: new Set(["video", "audio", "text", "image", "api"]),
  rerank: new Set(["rerank", "text"]),
  decision: new Set(["text", "api"]),
};

export const UNIVERSAL_METER_CODES = new Set(["api_request", "api_result", "api_item", "unknown"]);

export function inferMissingModelCapabilities(model) {
  const inferred = {};
  const primary = model.primaryCapability ?? "";
  const capabilities = new Set(model.capabilities ?? []);
  const inputModalities = new Set(model.inputModalities ?? []);
  const outputModalities = new Set(model.outputModalities ?? []);
  const familyCode = String(model.familyCode ?? "");
  const modelId = String(model.modelId ?? "");

  const isChatLike =
    CHAT_LIKE_CAPABILITIES.has(primary)
    || [...capabilities].some((capability) => CHAT_LIKE_CAPABILITIES.has(capability));
  const isEmbedding = primary === "embedding" || capabilities.has("embedding");
  const isRerank = primary === "rerank" || capabilities.has("rerank");
  const isGenerativeMedia =
    GENERATIVE_MEDIA_CAPABILITIES.has(primary)
    || [...capabilities].some((capability) => GENERATIVE_MEDIA_CAPABILITIES.has(capability));
  const isRealtimeFamily = familyCode.includes("realtime") || modelId.includes("realtime");
  const isSpeechToText =
    (primary === "audio" || capabilities.has("audio"))
    && inputModalities.has("audio")
    && outputModalities.has("text")
    && !inputModalities.has("text");
  const isTextToSpeech =
    (primary === "audio" || capabilities.has("audio"))
    && inputModalities.has("text")
    && outputModalities.has("audio")
    && !outputModalities.has("text");
  const isRealtimeVoice =
    isRealtimeFamily
    && inputModalities.has("audio")
    && (outputModalities.has("audio") || outputModalities.has("text"));

  if (model.supportsStreaming === undefined) {
    if (isEmbedding || isRerank || isGenerativeMedia || isTextToSpeech) {
      inferred.supportsStreaming = false;
    } else if (isSpeechToText && !isRealtimeFamily) {
      inferred.supportsStreaming = false;
    } else if (isRealtimeFamily || isRealtimeVoice || isChatLike) {
      inferred.supportsStreaming = true;
    } else {
      inferred.supportsStreaming = false;
    }
  }

  if (model.supportsTools === undefined) {
    if (isChatLike && !isGenerativeMedia && !isEmbedding && !isRerank && !isSpeechToText && !isTextToSpeech) {
      inferred.supportsTools = true;
    } else if (isRealtimeVoice) {
      inferred.supportsTools = !modelId.includes("translate") && !modelId.includes("whisper");
    } else {
      inferred.supportsTools = false;
    }
  }

  if (model.supportsJsonSchema === undefined) {
    if (
      isChatLike
      && !isGenerativeMedia
      && !isEmbedding
      && !isRerank
      && !isRealtimeFamily
      && !isSpeechToText
      && !isTextToSpeech
    ) {
      inferred.supportsJsonSchema = true;
    } else {
      inferred.supportsJsonSchema = false;
    }
  }

  if (model.usageScopes === undefined) {
    const scopes = [];
    const strengths = model.strengths ?? [];
    const modelIdToken = String(modelId ?? "").toLowerCase();
    const isCodingNamed =
      /(^|[-_. ])code([-_. ]|$)/.test(modelIdToken)
      || /(^|[-_. ])coder([-_. ]|$)/.test(modelIdToken)
      || /(^|[-_. ])coding([-_. ]|$)/.test(modelIdToken);
    const isCodingStrengthened = strengths.some((strength) => /\bcod(?:e|ing)\b/i.test(String(strength)));
    if (primary === "code" || capabilities.has("code") || isCodingNamed || isCodingStrengthened) {
      scopes.push("coding");
    }
    if (isChatLike) {
      scopes.push("chat");
    }
    const supportsTools = inferred.supportsTools ?? model.supportsTools;
    if (supportsTools === true) {
      scopes.push("agent");
    }
    inferred.usageScopes = scopes;
  }

  if (model.codingVisible === undefined) {
    inferred.codingVisible = isChatLike;
  }

  return inferred;
}

export function inferPricingIdentityFixes(model, vendor, pricing) {
  const fixes = {};
  const expectedCatalogKey = `${vendor.vendorCode}/${model.modelId}`;
  if (pricing.catalogKey !== expectedCatalogKey) {
    fixes.catalogKey = expectedCatalogKey;
  }
  if (pricing.vendorCode !== vendor.vendorCode) {
    fixes.vendorCode = vendor.vendorCode;
  }
  if (pricing.regionCode !== vendor.regionCode) {
    fixes.regionCode = vendor.regionCode;
  }
  if (pricing.modelId !== model.modelId) {
    fixes.modelId = model.modelId;
  }
  if (vendor.billingCurrency && pricing.currency !== vendor.billingCurrency) {
    fixes.currency = vendor.billingCurrency;
  }
  if (!pricing.schemaVersion) {
    fixes.schemaVersion = "1.0.0";
  }
  return fixes;
}

export function pricingMeterAllowedForModel(model, meterCode, meterModalities) {
  if (UNIVERSAL_METER_CODES.has(meterCode)) {
    return true;
  }
  const modality = meterModalities.get(meterCode);
  if (!modality) {
    return false;
  }
  const allowed = ALLOWED_METER_MODALITIES_BY_CAPABILITY[model.primaryCapability];
  if (!allowed) {
    return true;
  }
  return allowed.has(modality);
}

export function isBillableModel(model) {
  return (
    model.routingState === "enabled"
    || model.shelfState === "listed"
    || model.releaseStage === "active"
  );
}
