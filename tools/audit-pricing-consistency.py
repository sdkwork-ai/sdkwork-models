#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Full consistency audit of the sdkwork-models official pricing catalog.

Authoritative against tools/migrate-pricing-v2.mjs: a price is "drifted" iff
the migrator would rewrite it. Checks:
  1. pricing.schema.json contract (fields, enums, patterns),
  2. runtime resolver semantics (rateVariant/schedule pairing, real ambiguity,
     dead tier_code conditions, unknown dimension codes),
  3. rateHash + derived-field drift vs the migrator normalization,
  4. model <-> pricing pairing and index.json counts.
"""
import json, glob, os, re, hashlib
from datetime import datetime

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODELS = os.path.join(ROOT, "models")

PRICE_REQ = ["priceId", "rateHash", "priceBookCode", "productCode", "operationCode",
             "priceSide", "billability", "chargeTiming", "calculationMode", "meterCode",
             "quantityAggregation", "unitSize", "unitPrice", "minimumQuantity",
             "conditions", "source", "effectiveFrom"]
FILE_REQ = ["schemaVersion", "vendorCode", "regionCode", "catalogKey", "modelId", "currency", "prices"]
SIDES = {"official", "reference", "upstream", "customer"}
BILL = {"chargeable", "free", "not_applicable", "unknown"}
CHARGE = {"request_accepted", "successful_result", "usage_reported"}
CALC = {"per_unit", "flat", "graduated", "volume", "formula"}
AGG = {"sum", "maximum", "minimum", "last", "distinct_invocation"}
SCOPE = {"model", "provider", "channel", "plan"}
OPS = {"eq", "neq", "gt", "gte", "lt", "lte", "in", "not_in", "exists"}
DECIMAL_RE = re.compile(r"^(0|[1-9][0-9]*)(\.[0-9]+)?$")
HASH_RE = re.compile(r"^[a-f0-9]{64}$")
KNOWN_DIMS = {
    "api_code", "operation_id", "tier_code", "quality", "resolution",
    "duration_seconds", "result_count", "media_type", "input_type", "output_type",
    "context_tokens", "vendor_code", "provider_code", "region_code", "catalog_key",
    "model", "meter_code", "product_code", "operation_code", "occurred_at",
}

def sha256hex(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()

def op_code(capability, meter):
    if meter == "tts_input_character": return "speech.synthesize"
    if meter == "stt_audio_minute": return "audio.transcribe"
    if meter == "sfx_result": return "sound.generate"
    if meter == "music_output_second": return "music.generate"
    return {"audio": "audio.generate", "chat": "inference.generate",
            "code": "inference.generate", "embedding": "embedding.create",
            "image": "image.generate", "music": "music.generate",
            "reasoning": "inference.generate", "sfx": "sound.generate",
            "streaming": "video.generate", "video": "video.generate"}.get(capability, "model.invoke")

def charge_timing(meter):
    if meter == "api_request": return "request_accepted"
    if (meter.endswith("_result") or meter.endswith("_second") or meter.endswith("_minute")
            or meter == "image_megapixel"):
        return "successful_result"
    return "usage_reported"

def conditions_of(price):
    result = []
    if price.get("thresholdTokens") is not None:
        result.append({"dimensionCode": "context_tokens", "operator": "gt", "value": str(price["thresholdTokens"])})
    for field, dim in [("tierCode", "tier_code"), ("mediaDirection", "media_direction"),
                       ("mediaType", "media_type"), ("inputType", "input_type"),
                       ("outputType", "output_type"), ("quality", "quality")]:
        if price.get(field) is not None:
            result.append({"dimensionCode": dim, "operator": "eq", "value": price[field]})
    return result

def normalized_price(pricing, price, capability):
    """Faithful port of migrate-pricing-v2.mjs normalization."""
    p = dict(price)
    p["priceBookCode"] = f"models.{pricing['vendorCode']}.{pricing['regionCode']}.{p['priceSide']}"
    p["productCode"] = f"models.{pricing['vendorCode']}.{capability or 'model'}"
    p["operationCode"] = op_code(capability, p["meterCode"])
    p["billability"] = "chargeable" if float(p["unitPrice"]) > 0 else "unknown"
    p["chargeTiming"] = charge_timing(p["meterCode"])
    p["calculationMode"] = "per_unit"
    p["quantityAggregation"] = "distinct_invocation" if p["meterCode"] == "api_request" else "sum"
    p["conditions"] = conditions_of(price)
    p["priority"] = p["priority"] if isinstance(p["priority"], int) and p["priority"] >= 0 else 100
    p["rateVariant"] = p.get("rateVariant") or "standard"
    p["schedule"] = p["schedule"] if p["rateVariant"] == "time_window" else None
    return p

def rate_hash(pricing, price):
    """Faithful port of migrate-pricing-v2.mjs rateHash() (key order matters)."""
    obj = {}
    for key in ("vendorCode", "regionCode", "catalogKey"):
        if pricing.get(key) is not None:
            obj[key] = pricing[key]
    for key in ("priceId", "priceSide", "billability", "chargeTiming", "calculationMode",
                "quantityAggregation", "meterCode", "unitSize", "unitPrice", "minimumQuantity",
                "quantityStep", "currency", "effectiveFrom", "effectiveTo", "priority"):
        v = price.get(key)
        if key == "currency":
            v = price.get("currency") if price.get("currency") is not None else pricing.get("currency")
        if key == "quantityStep":
            v = price.get("quantityStep")
        obj[key] = v
    obj["rateVariant"] = price["rateVariant"]
    obj["schedule"] = price["schedule"]
    obj["conditions"] = price.get("conditions", [])
    obj["tiers"] = price.get("tiers", [])
    obj["formula"] = price.get("formula")
    payload = json.dumps(obj, ensure_ascii=False, separators=(",", ":"))
    return sha256hex(payload)

def window_codes(price):
    if price.get("rateVariant") != "time_window":
        return ("standard",)
    sched = price.get("schedule") or {}
    return tuple(sorted(w.get("windowCode", "") for w in sched.get("weeklyWindows", [])))

# ---- load model capabilities ----
capability_by_model = {}
for mf in glob.glob(os.path.join(MODELS, "*", "*", "models", "*.json")):
    try:
        m = json.load(open(mf, encoding="utf-8"))
        capability_by_model.setdefault(m.get("modelId"), m.get("primaryCapability"))
    except Exception:
        pass

issues = []
def issue(path, msg):
    issues.append((path, msg))

for pricing_path in sorted(glob.glob(os.path.join(MODELS, "*", "*", "pricing", "*.json"))):
    rel = os.path.relpath(pricing_path, MODELS).replace(os.sep, "/")
    try:
        pricing = json.load(open(pricing_path, encoding="utf-8"))
    except Exception as e:
        issue(rel, f"JSON parse error: {e}")
        continue

    for k in FILE_REQ:
        if k not in pricing:
            issue(rel, f"missing file field {k}")

    file_currency = pricing.get("currency")
    if file_currency is not None and not re.match(r"^[A-Z]{3}$", str(file_currency)):
        issue(rel, f"file currency invalid: {file_currency!r}")

    capability = capability_by_model.get(pricing.get("modelId"))
    if capability is None:
        issue(rel, "model file missing or modelId not found for capability lookup")

    prices = pricing.get("prices") or []
    for pi, price in enumerate(prices):
        loc = f"{rel}#{price.get('priceId', pi)}"
        if not isinstance(price, dict):
            issue(rel, f"price #{pi} is not an object"); continue

        for k in PRICE_REQ:
            if k not in price:
                issue(loc, f"missing required field {k}")

        if price.get("priceSide") not in SIDES: issue(loc, f"priceSide={price.get('priceSide')!r}")
        if price.get("billability") not in BILL: issue(loc, f"billability={price.get('billability')!r}")
        if price.get("chargeTiming") not in CHARGE: issue(loc, f"chargeTiming={price.get('chargeTiming')!r}")
        if price.get("calculationMode") not in CALC: issue(loc, f"calculationMode={price.get('calculationMode')!r}")
        if price.get("quantityAggregation") not in AGG: issue(loc, f"quantityAggregation={price.get('quantityAggregation')!r}")
        if "pricingScope" in price and price.get("pricingScope") not in SCOPE:
            issue(loc, f"pricingScope={price.get('pricingScope')!r}")
        for k in ("unitSize", "unitPrice", "minimumQuantity", "quantityStep"):
            v = price.get(k)
            if v is not None and not DECIMAL_RE.match(str(v)):
                issue(loc, f"{k}={v!r} not a decimal string")

        stored = price.get("rateHash")
        if stored is not None and not HASH_RE.match(stored):
            issue(loc, "rateHash malformed")
        # migrator drift check
        try:
            norm = normalized_price(pricing, price, capability)
            recomputed = rate_hash(pricing, norm)
            if stored != recomputed:
                issue(loc, "rateHash drift (migrate-pricing-v2 would rewrite)")
            else:
                # derived fields must equal what the migrator would produce
                for k, want in (("priceBookCode", norm["priceBookCode"]),
                                ("productCode", norm["productCode"]),
                                ("operationCode", norm["operationCode"]),
                                ("billability", norm["billability"]),
                                ("chargeTiming", norm["chargeTiming"]),
                                ("calculationMode", norm["calculationMode"]),
                                ("quantityAggregation", norm["quantityAggregation"]),
                                ("rateVariant", norm["rateVariant"])):
                    if price.get(k) != want:
                        issue(loc, f"field {k}={price.get(k)!r} differs from canonical {want!r}")
                if price.get("schedule") != norm["schedule"]:
                    issue(loc, f"schedule differs from canonical (variant={norm['rateVariant']})")
                if price.get("conditions") != norm["conditions"]:
                    issue(loc, f"conditions differ from canonical {norm['conditions']!r}")
        except Exception as e:
            issue(loc, f"drift check error: {e}")

        variant = price.get("rateVariant", "standard")
        schedule = price.get("schedule")
        if variant not in ("standard", "time_window"):
            issue(loc, f"rateVariant={variant!r}")
        if variant == "standard" and schedule is not None:
            issue(loc, "standard rate carries a schedule")
        if variant == "time_window":
            if schedule is None:
                issue(loc, "time_window rate without a schedule")
            else:
                for k in ("timeZone", "weeklyWindows"):
                    if k not in schedule: issue(loc, f"schedule missing {k}")
                for w in schedule.get("weeklyWindows", []):
                    for k in ("windowCode", "daysOfWeek", "startTime", "endTime", "endDayOffset"):
                        if k not in w: issue(loc, f"weekly window missing {k}")
                    if w.get("endDayOffset") not in (0, 1): issue(loc, f"endDayOffset={w.get('endDayOffset')!r}")
                    if not w.get("daysOfWeek"): issue(loc, "weekly window has empty daysOfWeek (never matches)")

        conds = price.get("conditions", [])
        if not isinstance(conds, list):
            issue(loc, "conditions is not an array")
        else:
            for c in conds:
                if c.get("dimensionCode") not in KNOWN_DIMS:
                    issue(loc, f"condition dimension {c.get('dimensionCode')!r} not evaluable at runtime")
                if c.get("operator") not in OPS:
                    issue(loc, f"operator={c.get('operator')!r}")
                if c.get("dimensionCode") == "tier_code" and variant != "time_window":
                    issue(loc, "tier_code condition on a non-time_window rate (unreachable without a request tier)")

        ef, et = price.get("effectiveFrom"), price.get("effectiveTo")
        if ef is not None and et is not None:
            try:
                if datetime.fromisoformat(et.replace("Z", "+00:00")) <= datetime.fromisoformat(ef.replace("Z", "+00:00")):
                    issue(loc, "effectiveTo <= effectiveFrom")
            except Exception:
                pass
        src = price.get("source")
        if src is not None:
            for k in ("sourceUrl", "observedAt"):
                if k not in src: issue(loc, f"source missing {k}")

    # ---- REAL ambiguity: identical conditions + identical coverage, different hash ----
    for a in prices:
        for b in prices:
            if a is b or a.get("priceId") == b.get("priceId"):
                continue
            if a.get("priceId") > b.get("priceId"):
                continue
            if (a.get("meterCode"), a.get("priceSide"), a.get("currency") or file_currency) != \
               (b.get("meterCode"), b.get("priceSide"), b.get("currency") or file_currency):
                continue
            if (a.get("rateVariant", "standard") != b.get("rateVariant", "standard")
                    or a.get("priority", 100) != b.get("priority", 100)
                    or a.get("effectiveFrom") != b.get("effectiveFrom")
                    or a.get("conditions") != b.get("conditions")
                    or window_codes(a) != window_codes(b)):
                continue
            if a.get("rateHash") != b.get("rateHash"):
                issue(rel, f"AMBIGUOUS at runtime: {a.get('priceId')} vs {b.get('priceId')} (identical conditions/coverage, different hash)")

# ---- model <-> pricing pairing ----
# Mirrors validate-catalog.mjs' "model.pricing.required" rule: only a model that
# is routable, listable or active must carry a pricing file. A routingState of
# "catalog_only" (shelfState "hidden") is a deliberate display-only entry, so an
# absent pricing file is expected there rather than a defect. Without this
# distinction the check emits a permanent, unfixable false positive for every
# catalog-only entry, which devalues the whole report.
model_files = {os.path.relpath(p, MODELS).replace(os.sep, "/")
               for p in glob.glob(os.path.join(MODELS, "*", "*", "models", "*.json"))}
pricing_files = {os.path.relpath(p, MODELS).replace(os.sep, "/")
                 for p in glob.glob(os.path.join(MODELS, "*", "*", "pricing", "*.json"))}
catalog_only_unpriced = []
for mf in sorted(model_files):
    if mf.replace("/models/", "/pricing/") in pricing_files:
        continue
    try:
        model = json.load(open(os.path.join(MODELS, mf), encoding="utf-8"))
    except Exception as e:
        issue(mf, f"unreadable: {e}")
        continue
    if (model.get("routingState") == "enabled" or model.get("shelfState") == "listed"
            or model.get("releaseStage") == "active"):
        issue(mf, "model is enabled/listed/active but has no matching pricing file")
    else:
        catalog_only_unpriced.append(mf)
for pf in sorted(pricing_files):
    if pf.replace("/pricing/", "/models/") not in model_files:
        issue(pf, "pricing file has no matching model file")

# ---- index.json counts ----
try:
    index = json.load(open(os.path.join(MODELS, "index.json"), encoding="utf-8"))
except Exception as e:
    issue("index.json", f"unreadable: {e}")
else:
    for label, actual, declared in (
        ("modelCount", len(model_files), index.get("modelCount")),
        ("pricingFileCount", len(pricing_files), index.get("pricingFileCount")),
    ):
        if actual != declared:
            issue("index.json", f"{label} mismatch: directory={actual}, index={declared}")
    # index.json's regionCount counts "vendor x region catalog directories"
    # (catalog-lib.mjs: regionCount = vendors.length, one entry per region dir
    # that carries a vendor.json), NOT the number of distinct region names.
    # Comparing it against distinct names ("cn"/"global" -> 2) can never pass.
    actual_regions = sum(
        1 for p in glob.glob(os.path.join(MODELS, "*", "*"))
        if os.path.isfile(os.path.join(p, "vendor.json"))
    )
    if actual_regions != index.get("regionCount"):
        issue("index.json", f"regionCount mismatch: directory={actual_regions}, index={index.get('regionCount')}")

by_path = {}
for path, msg in issues:
    by_path.setdefault(path, []).append(msg)

print(f"TOTAL ISSUES: {len(issues)} in {len(by_path)} files\n")
if catalog_only_unpriced:
    print(f"NOTE: {len(catalog_only_unpriced)} catalog-only model(s) carry no pricing file by design "
          f"(routingState not enabled / shelfState not listed / releaseStage not active):")
    for mf in catalog_only_unpriced:
        print(f"    {mf}")
    print()
for path in sorted(by_path):
    print(f"--- {path} ({len(by_path[path])}) ---")
    for m in by_path[path]:
        print("   ", m)
