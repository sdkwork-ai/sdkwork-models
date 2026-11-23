#!/usr/bin/env node
/**
 * Materializes peak/off-peak time-window schedules into pricing documents.
 *
 * Vendors publish peak/off-peak pricing and statutory holiday calendars
 * separately from the pricing rows they affect. Those windows and calendars are
 * authored in `config/pricing/time-window-schedules.json` as the single source
 * of truth, and this tool rewrites the `schedule` of every `time_window` price
 * row that matches an enforcement:
 *
 * - peak rows exclude the calendar's holiday dates (holidays are off-peak);
 * - off-peak rows mark the calendar's holiday dates as `holidayDates`;
 * - when `deriveOffPeakPrice` is configured, the off-peak `unitPrice` is
 *   recomputed from the matching peak row as an exact decimal product.
 *
 * Rewrites are idempotent and complete: the whole `schedule` is replaced on
 * every run, keys are emitted in alphabetical order, and any touched row gets a
 * `rateHash` recomputed with the exact payload `tools/migrate-pricing-v2.mjs`
 * uses, so that tool stays at zero pending migrations.
 *
 * Usage:
 *   node tools/materialize-time-window-schedules.mjs --write
 *   node tools/materialize-time-window-schedules.mjs --check
 */
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const args = new Set(process.argv.slice(2));
const write = args.has("--write");
const check = args.has("--check");

const DECIMAL_PLACES = 6;

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

/** Deep copy whose object keys are serialized alphabetically, matching file style. */
function sortedClone(value) {
  if (Array.isArray(value)) return value.map(sortedClone);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, sortedClone(value[key])]),
    );
  }
  return value;
}

function parseDecimal(value) {
  const match = /^(-?)(\d+)(?:\.(\d+))?$/.exec(String(value).trim());
  if (!match) return null;
  const [, sign, integer, fraction = ""] = match;
  const digits = `${integer}${fraction}`.replace(/^0+(?=\d)/, "");
  return { negative: sign === "-", units: BigInt(digits), scale: fraction.length };
}

/**
 * Exact decimal multiplication on authored decimal strings, rounded half-up and
 * formatted with `DECIMAL_PLACES` decimals. Never goes through binary floats.
 */
function multiplyDecimal(left, right) {
  const a = parseDecimal(left);
  const b = parseDecimal(right);
  if (!a || !b) return null;
  let units = a.units * b.units;
  let scale = a.scale + b.scale;
  if (scale > DECIMAL_PLACES) {
    const divisor = 10n ** BigInt(scale - DECIMAL_PLACES);
    const quotient = units / divisor;
    const remainder = units % divisor;
    units = remainder * 2n >= divisor ? quotient + 1n : quotient;
    scale = DECIMAL_PLACES;
  }
  if (scale < DECIMAL_PLACES) {
    units *= 10n ** BigInt(DECIMAL_PLACES - scale);
    scale = DECIMAL_PLACES;
  }
  const digits = units.toString().padStart(scale + 1, "0");
  const integer = digits.slice(0, digits.length - scale);
  const fraction = digits.slice(digits.length - scale);
  const sign = a.negative !== b.negative && units !== 0n ? "-" : "";
  return `${sign}${integer}.${fraction}`;
}

const authored = json(join(root, "config", "pricing", "time-window-schedules.json"));
const calendars = authored.calendars ?? {};
const schedules = authored.schedules ?? {};
const enforcements = authored.enforcements ?? [];

const warnings = [];
/** pricing path -> { path, document, touched: Set<price> } */
const documents = new Map();
let rowCount = 0;

function pricingPaths(vendorCode, regionCode) {
  const pricingRoot = join(root, "models", vendorCode, regionCode, "pricing");
  if (!existsSync(pricingRoot)) {
    warnings.push(`pricing directory not found for ${vendorCode}/${regionCode}`);
    return [];
  }
  return readdirSync(pricingRoot)
    .filter((name) => name.endsWith(".json"))
    .sort()
    .map((name) => join(pricingRoot, name));
}

function documentEntry(path) {
  let entry = documents.get(path);
  if (!entry) {
    entry = { path, document: json(path), touched: new Set() };
    documents.set(path, entry);
  }
  return entry;
}

function windowTier(price) {
  const windows = price.schedule?.weeklyWindows ?? [];
  if (windows.length === 0) return null;
  const codes = windows.map((window) => String(window.windowCode ?? ""));
  if (codes.every((code) => code.startsWith("peak"))) return "peak";
  if (codes.every((code) => code.startsWith("off_peak"))) return "off_peak";
  return null;
}

function peakKey(price) {
  return `${price.meterCode}|${JSON.stringify(price.conditions ?? [])}`;
}

for (const enforcement of enforcements) {
  const schedule = schedules[enforcement.scheduleCode];
  if (!schedule) {
    warnings.push(
      `schedule ${enforcement.scheduleCode} not found for enforcement ${enforcement.vendorCode}`,
    );
    continue;
  }
  const calendar = calendars[schedule.calendarCode];
  if (!calendar) {
    warnings.push(
      `calendar ${schedule.calendarCode} not found for schedule ${enforcement.scheduleCode}`,
    );
    continue;
  }
  if ((schedule.peakWindows ?? []).length === 0) {
    warnings.push(`schedule ${enforcement.scheduleCode} has no peakWindows`);
  }
  if ((schedule.offPeakWindows ?? []).length === 0) {
    warnings.push(`schedule ${enforcement.scheduleCode} has no offPeakWindows`);
  }
  const holidayDates = [...(calendar.holidayDates ?? [])];
  const modelIds = new Set(enforcement.modelIds ?? []);
  const wildcard = modelIds.has("*");

  for (const regionCode of enforcement.regionCodes ?? []) {
    let matchedDocuments = 0;
    for (const path of pricingPaths(enforcement.vendorCode, regionCode)) {
      const entry = documentEntry(path);
      const { document } = entry;
      if (!wildcard && !modelIds.has(document.modelId)) continue;
      matchedDocuments += 1;

      const peakRows = [];
      const offPeakRows = [];
      for (const price of document.prices ?? []) {
        if (price.rateVariant !== "time_window") continue;
        const tier = windowTier(price);
        if (!tier) {
          warnings.push(
            `unrecognized time_window windows for ${price.priceId} in ${path}; skipped`,
          );
          continue;
        }
        rowCount += 1;
        const nextSchedule =
          tier === "peak"
            ? {
                excludeDates: holidayDates,
                holidayDates: [],
                includeDates: [],
                timeZone: schedule.timeZone,
                weeklyWindows: sortedClone(schedule.peakWindows ?? []),
              }
            : {
                excludeDates: [],
                holidayDates,
                includeDates: [],
                timeZone: schedule.timeZone,
                weeklyWindows: sortedClone(schedule.offPeakWindows ?? []),
              };
        if (JSON.stringify(price.schedule) !== JSON.stringify(nextSchedule)) {
          price.schedule = nextSchedule;
          entry.touched.add(price);
        }
        if (tier === "peak") peakRows.push(price);
        else offPeakRows.push(price);
      }

      const derive = enforcement.deriveOffPeakPrice;
      if (!derive) continue;
      if (derive.from !== "peak") {
        warnings.push(
          `deriveOffPeakPrice.from=${derive.from} is unsupported for ${enforcement.vendorCode}`,
        );
        continue;
      }
      const peakByKey = new Map();
      for (const price of peakRows) peakByKey.set(peakKey(price), price);
      for (const price of offPeakRows) {
        const peak = peakByKey.get(peakKey(price));
        if (!peak) {
          warnings.push(
            `no peak row for off-peak ${price.priceId} in ${path}; price not derived`,
          );
          continue;
        }
        const unitPrice = multiplyDecimal(peak.unitPrice, derive.ratio);
        if (unitPrice === null) {
          warnings.push(
            `cannot multiply ${peak.unitPrice} by ${derive.ratio} for ${price.priceId} in ${path}`,
          );
          continue;
        }
        if (price.unitPrice !== unitPrice) {
          price.unitPrice = unitPrice;
          entry.touched.add(price);
        }
      }
    }
    if (matchedDocuments === 0) {
      warnings.push(
        `no pricing document matched ${enforcement.vendorCode}/${regionCode} (models: ${enforcement.modelIds?.join(", ")})`,
      );
    }
  }
}

let drift = 0;
for (const entry of documents.values()) {
  for (const price of entry.touched) price.rateHash = rateHash(entry.document, price);
  const next = `${JSON.stringify(entry.document, null, 2)}\n`;
  const current = readFileSync(entry.path, "utf8");
  if (next === current) continue;
  if (write) {
    writeFileSync(entry.path, next, "utf8");
    drift += 1;
    console.log(`updated: ${entry.path}`);
  } else {
    drift += 1;
    console.log(`drift: ${entry.path}`);
    if (check) process.exitCode = 1;
  }
}

for (const warning of warnings) console.log(`warn: ${warning}`);
console.log(
  `${write ? "materialized" : "checked"} time-window schedules: ${documents.size} documents, ${rowCount} rows, ${drift} drift`,
);
