#!/usr/bin/env node
/**
 * Materializes official batch (off-peak) rates into the catalog.
 *
 * Vendors publish batch pricing separately from standard pricing (OpenAI and
 * Anthropic quote 50% of standard). Those quotes are authored in
 * `config/pricing/batch-rates.json`, transcribed from the vendor pricing
 * reports, and this tool appends one extra price row per (model, meter) with a
 * `tier_code = batch` condition.
 *
 * The router only bills a completed batch at the batch rate when the catalog
 * carries that exact condition, so an absent row means "no discount", never
 * "unpriced call".
 *
 * Usage:
 *   node tools/materialize-batch-rates.mjs --write
 *   node tools/materialize-batch-rates.mjs --check
 */
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const args = new Set(process.argv.slice(2));
const write = args.has("--write");
const check = args.has("--check");

const BATCH_TIER_CODE = "batch";
const BATCH_SUFFIX = "-batch";

function json(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

/** Same payload as `tools/migrate-pricing-v2.mjs` so hashes stay comparable. */
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

function pricingPath(vendorCode, regionCode, modelId) {
  const pricingRoot = join(root, "models", vendorCode, regionCode, "pricing");
  if (!existsSync(pricingRoot)) return null;
  for (const name of readdirSync(pricingRoot).filter((entry) => entry.endsWith(".json"))) {
    const path = join(pricingRoot, name);
    if (json(path).modelId === modelId) return path;
  }
  return null;
}

function hasBatchCondition(price) {
  return (price.conditions ?? []).some(
    (condition) =>
      condition.dimensionCode === "tier_code" &&
      condition.operator === "eq" &&
      condition.value === BATCH_TIER_CODE,
  );
}

/** Base row the batch rate replaces: the unconditioned standard rate. */
function baseRowFor(prices, meterCode) {
  const candidates = prices.filter(
    (price) =>
      price.meterCode === meterCode &&
      price.priceSide === "official" &&
      price.rateVariant !== "time_window" &&
      (price.conditions ?? []).length === 0,
  );
  return candidates.length === 1 ? candidates[0] : null;
}

const authored = json(join(root, "config", "pricing", "batch-rates.json"));
const entries = authored.entries ?? [];
let added = 0;
let removed = 0;
const warnings = [];

/** model key -> pricing document, so one model's rows batch together. */
const documents = new Map();

for (const entry of entries) {
  const key = `${entry.vendorCode}/${entry.regionCode}/${entry.modelId}`;
  if (!documents.has(key)) {
    const path = pricingPath(entry.vendorCode, entry.regionCode, entry.modelId);
    if (!path) {
      warnings.push(`pricing file not found for ${key}`);
      continue;
    }
    documents.set(key, { path, document: json(path), changed: false });
  }
  const { document } = documents.get(key);
  const prices = (document.prices ??= []);
  const base = baseRowFor(prices, entry.meterCode);
  if (!base) {
    warnings.push(
      `no single unconditioned ${entry.meterCode} row for ${key}; batch rate not materialized`,
    );
    continue;
  }
  const priceId = `${base.priceId}${BATCH_SUFFIX}`;
  const existing = prices.find((price) => price.priceId === priceId);
  const unitPrice = Number(entry.unitPrice).toFixed(6);
  if (existing) {
    let touched = false;
    if (existing.unitPrice !== unitPrice) {
      existing.unitPrice = unitPrice;
      touched = true;
    }
    if (existing.rateVariant !== "standard") {
      existing.rateVariant = "standard";
      touched = true;
    }
    if (touched) {
      existing.rateHash = rateHash(document, existing);
      documents.get(key).changed = true;
    }
    continue;
  }
  const row = {
    ...structuredClone(base),
    priceId,
    unitPrice,
    unitSize: entry.unitSize ?? base.unitSize ?? "1000000",
    // `rateVariant` is a closed vocabulary owned by the contract crate
    // (`standard` / `time_window`), so a batch row keeps the standard variant and
    // carries its discount as a `tier_code = batch` condition — that condition is
    // what the router matches when it settles a completed batch.
    rateVariant: "standard",
    conditions: [
      ...(base.conditions ?? []),
      { dimensionCode: "tier_code", operator: "eq", value: BATCH_TIER_CODE },
    ],
    source: {
      observedAt: entry.observedAt ?? base.source?.observedAt,
      sourceUrl: entry.sourceUrl ?? base.source?.sourceUrl,
    },
    priority: base.priority ?? 100,
  };
  // Preserve the authored evidence so the discount is auditable.
  if (entry.evidence) row.evidence = entry.evidence;
  row.rateHash = rateHash(document, row);
  prices.push(row);
  documents.get(key).changed = true;
  added += 1;
}

// A batch row whose authored entry disappeared is stale; drop it so the catalog
// never carries a discount nobody validated.
for (const [key, entry] of documents) {
  const authoredMeters = new Set(
    entries
      .filter(
        (item) => `${item.vendorCode}/${item.regionCode}/${item.modelId}` === key,
      )
      .map((item) => item.meterCode),
  );
  const kept = (entry.document.prices ?? []).filter((price) => {
    if (!price.priceId.endsWith(BATCH_SUFFIX) || !hasBatchCondition(price)) return true;
    const keep = authoredMeters.has(price.meterCode);
    if (!keep) {
      removed += 1;
      entry.changed = true;
    }
    return keep;
  });
  if (entry.changed) entry.document.prices = kept;
}

for (const [, entry] of documents) {
  const next = JSON.stringify(entry.document, null, 2);
  const current = readFileSync(entry.path, "utf8");
  if (!write) {
    if (`${next}\n` !== current) {
      console.log(`drift: ${entry.path}`);
      if (check) process.exitCode = 1;
    }
    continue;
  }
  if (`${next}\n` !== current) {
    writeFileSync(entry.path, `${next}\n`, "utf8");
    console.log(`updated: ${entry.path}`);
  }
}

for (const warning of warnings) console.log(`warn: ${warning}`);
console.log(
  `${write ? "materialized" : "checked"} batch rates: ${added} added, ${removed} stale removed, ${warnings.length} warnings`,
);
