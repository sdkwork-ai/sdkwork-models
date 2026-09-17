import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const write = process.argv.includes("--write");

function json(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function directories(path) {
  return readdirSync(path, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
}

function operationCode(capability, meterCode) {
  if (meterCode === "tts_input_character") return "speech.synthesize";
  if (meterCode === "stt_audio_minute") return "audio.transcribe";
  if (meterCode === "sfx_result") return "sound.generate";
  if (meterCode === "music_output_second") return "music.generate";
  return {
    audio: "audio.generate",
    chat: "inference.generate",
    code: "inference.generate",
    embedding: "embedding.create",
    image: "image.generate",
    music: "music.generate",
    reasoning: "inference.generate",
    sfx: "sound.generate",
    streaming: "video.generate",
    video: "video.generate",
  }[capability] ?? "model.invoke";
}

function chargeTiming(meterCode) {
  if (meterCode === "api_request") return "request_accepted";
  if (
    meterCode.endsWith("_result")
    || meterCode.endsWith("_second")
    || meterCode.endsWith("_minute")
    || meterCode === "image_megapixel"
  ) {
    return "successful_result";
  }
  return "usage_reported";
}

function conditions(price) {
  const result = [];
  if (price.thresholdTokens !== undefined) {
    result.push({ dimensionCode: "context_tokens", operator: "gt", value: String(price.thresholdTokens) });
  }
  for (const [field, dimensionCode] of [
    ["tierCode", "tier_code"],
    ["mediaDirection", "media_direction"],
    ["mediaType", "media_type"],
    ["inputType", "input_type"],
    ["outputType", "output_type"],
    // Vendor quality modes whose price differs by level but which are not a resolution or a
    // modality. Kling's digital human is the first: `mode` is `std`/`pro`, the official price
    // table quotes 0.4 and 0.8 per second for the two levels, and the router reads the value
    // off `mode` into the `quality` dimension. Appended last so the generated `conditions`
    // array of every existing rate keeps its exact previous bytes and rateHash.
    ["quality", "quality"],
  ]) {
    if (price[field] !== undefined) {
      result.push({ dimensionCode, operator: "eq", value: price[field] });
    }
  }
  return result;
}

function rateHash(pricing, price) {
  const payload = JSON.stringify({
    vendorCode: pricing.vendorCode,
    regionCode: pricing.regionCode,
    catalogKey: pricing.catalogKey,
    priceId: price.priceId,
    priceSide: price.priceSide,
    billability: price.billability,
    chargeTiming: price.chargeTiming,
    calculationMode: price.calculationMode,
    quantityAggregation: price.quantityAggregation,
    meterCode: price.meterCode,
    unitSize: price.unitSize,
    unitPrice: price.unitPrice,
    minimumQuantity: price.minimumQuantity,
    quantityStep: price.quantityStep ?? null,
    currency: price.currency ?? pricing.currency,
    effectiveFrom: price.effectiveFrom,
    effectiveTo: price.effectiveTo ?? null,
    priority: price.priority,
    rateVariant: price.rateVariant,
    schedule: price.schedule ?? null,
    conditions: price.conditions,
    tiers: price.tiers ?? [],
    formula: price.formula ?? null,
  });
  return createHash("sha256").update(payload).digest("hex");
}

let changedFiles = 0;
let changedRates = 0;
for (const vendorCode of directories(join(root, "models"))) {
  for (const regionCode of directories(join(root, "models", vendorCode))) {
    const regionRoot = join(root, "models", vendorCode, regionCode);
    const modelsRoot = join(regionRoot, "models");
    const pricingRoot = join(regionRoot, "pricing");
    if (!existsSync(modelsRoot) || !existsSync(pricingRoot)) continue;
    const models = new Map(
      readdirSync(modelsRoot)
        .filter((name) => name.endsWith(".json"))
        .map((name) => {
          const model = json(join(modelsRoot, name));
          return [model.modelId, model];
        }),
    );
    for (const name of readdirSync(pricingRoot).filter((entry) => entry.endsWith(".json")).sort()) {
      const path = join(pricingRoot, name);
      const pricing = json(path);
      const model = models.get(pricing.modelId);
      if (!model) throw new Error(`pricing model is missing: ${path}`);
      let fileChanged = pricing.schemaVersion !== "2.0.0";
      pricing.schemaVersion = "2.0.0";
      for (const price of pricing.prices ?? []) {
        const before = JSON.stringify(price);
        price.priceBookCode = `models.${vendorCode}.${regionCode}.${price.priceSide}`;
        price.productCode = `models.${vendorCode}.${model.primaryCapability ?? "model"}`;
        price.operationCode = operationCode(model.primaryCapability, price.meterCode);
        price.billability = Number(price.unitPrice) > 0 ? "chargeable" : "unknown";
        price.chargeTiming = chargeTiming(price.meterCode);
        price.calculationMode = "per_unit";
        price.quantityAggregation = price.meterCode === "api_request" ? "distinct_invocation" : "sum";
        price.conditions = conditions(price);
        price.priority = Number.isInteger(price.priority) && price.priority >= 0 ? price.priority : 100;
        price.rateVariant = price.rateVariant ?? "standard";
        price.schedule = price.rateVariant === "time_window" ? (price.schedule ?? null) : null;
        price.rateHash = rateHash(pricing, price);
        if (JSON.stringify(price) !== before) {
          changedRates += 1;
          fileChanged = true;
        }
      }
      if (fileChanged) {
        changedFiles += 1;
        if (write) writeFileSync(path, `${JSON.stringify(pricing, null, 2)}\n`, "utf8");
      }
    }
  }
}

console.log(`${write ? "Migrated" : "Would migrate"} ${changedRates} rates in ${changedFiles} pricing files`);
if (!write && changedFiles > 0) process.exitCode = 1;
