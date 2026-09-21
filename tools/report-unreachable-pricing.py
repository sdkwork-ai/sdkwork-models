#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Generate a report of catalog prices that the runtime cannot select.

These entries are structurally valid (schema + migrator stable) but their
discriminator dimension is not populated by the runtime, so the rate is
unreachable: the meter resolves no price and the request fails (or falls back
across regions). Each row carries a suggested remediation; none is applied
automatically because collapsing/merging prices is a product decision.

Two unreachability classes are reported, distinguished by the `reason` column:

1. `dimension_never_populated` - the runtime never supplies the dimension at
   all, so any rate conditioned on it is dead. This is the original class.
2. `time_window_tier_redundant` - a `time_window` rate that *also* conditions on
   `tier_code`. The runtime selects a `time_window` rate through
   `PricingSchedule.matched_window_code()` (see `ROUTING_PRICING_SPEC.md` §5.4),
   and the only `tier_code` producer is the video path resolving
   `ai_model_video_profile`. No video profile exists for an LLM meter, so the
   condition can never be satisfied: the schedule already encodes the tier as
   its window-code prefix, and carrying both is a self-contradicting pair.

Class 2 was invisible until 2026-09-20: `tier_code` is a legitimate runtime
dimension (for video), so the guard `elif dim in RUNTIME_DIMS: continue`
skipped these rows before they could be judged. That blind spot cost a real
production outage - all four DeepSeek models shipped on 2026-09-17 were
unrouteable because their `off_peak`/`peak` rates carried both keys.

Usage:
    python tools/report-unreachable-pricing.py            # rewrite the report
    python tools/report-unreachable-pricing.py --check    # fail if it drifted

The report is a generated artifact: it went stale once (47 rows committed while
the catalog already carried 472), so --check lets a caller detect that drift
instead of trusting a hand-committed file.
"""
import json, glob, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MODELS = os.path.join(ROOT, "models")

# Dimensions the runtime *can* populate for at least one rate family. Membership
# here is not a blanket pass: see `is_reachable` for the per-variant judgement.
RUNTIME_DIMS = {
    "api_code", "operation_id", "tier_code", "quality", "resolution",
    "duration_seconds", "result_count", "media_type", "input_type", "output_type",
    "context_tokens", "vendor_code", "provider_code", "region_code", "catalog_key",
    "model", "meter_code", "product_code", "operation_code", "occurred_at",
}

# Meters whose `tier_code` the runtime resolves from `ai_model_video_profile`.
# Mirrors `is_video_meter` in
# sdkwork-cloudrouter-router-service/src/application/upstream_route_selector.rs.
# `tier_code` outside this set has no producer at all.
VIDEO_TIER_METERS = {
    "video_output_second", "video_input_second",
    "video_output_token", "video_input_token", "video_result",
}


def dim_label(dim, reason):
    if reason == "time_window_tier_redundant":
        return ("time_window rate also conditioned on tier_code; the schedule's "
                "window codes already carry the tier, and no video profile exists "
                "for this meter - drop the tier_code condition")
    return {
        "tier_code": "request body tier (/tier_code,/service_tier,/tier) - chat never sends it",
        "media_direction": "not populated by the runtime at all",
    }.get(dim, f"not populated by the runtime ({dim})")


def classify(price, dim):
    """Return the unreachability reason for `dim` on `price`, or None if reachable.

    The ordering matters: `tier_code` is judged by rate variant and meter before
    the generic RUNTIME_DIMS membership test, because "the runtime has a
    tier_code producer" is only true inside the video family.
    """
    variant = price.get("rateVariant", "standard")
    meter = price.get("meterCode", "")
    if dim == "tier_code":
        if variant == "time_window":
            # Always unreachable: TimeWindow selection is schedule-only, and no
            # video profile backs an LLM/audio/image meter.
            return "time_window_tier_redundant"
        if meter in VIDEO_TIER_METERS:
            return None  # resolved from ai_model_video_profile by the router
        return "dimension_never_populated"
    if dim in RUNTIME_DIMS:
        return None
    return "dimension_never_populated"


rows = []
for path in sorted(glob.glob(os.path.join(MODELS, "*", "*", "pricing", "*.json"))):
    try:
        pricing = json.load(open(path, encoding="utf-8"))
    except Exception:
        continue
    for p in pricing.get("prices", []):
        for c in p.get("conditions", []):
            dim = c.get("dimensionCode")
            reason = classify(p, dim)
            if reason is None:
                continue
            tier = next((cc.get("value") for cc in p.get("conditions", [])
                         if cc.get("dimensionCode") == "tier_code"), None)
            rows.append({
                "file": os.path.relpath(path, MODELS).replace(os.sep, "/"),
                "priceId": p.get("priceId"),
                "meter": p.get("meterCode"),
                "unitPrice": p.get("unitPrice"),
                "currency": p.get("currency") or pricing.get("currency"),
                "unitSize": p.get("unitSize"),
                "dimension": dim,
                "dimensionNote": dim_label(dim, reason),
                "reason": reason,
                "tierValue": tier,
                "thresholdTokens": p.get("thresholdTokens"),
            })

rows.sort(key=lambda r: (r["file"], r["priceId"]))

by_reason = {}
for r in rows:
    by_reason.setdefault(r["reason"], 0)
    by_reason[r["reason"]] += 1

out = ["# Unreachable catalog prices (runtime dimension gaps)\n",
       "Generated by tools/report-unreachable-pricing.py. Structurally valid rates",
       "whose discriminator dimension the runtime never populates -> the meter",
       "resolves no price at runtime. Fixing requires either a runtime dimension",
       "or merging tiers - a product decision, so nothing here is auto-applied.\n",
       "| reason | count |",
       "|---|---|"]
for reason in sorted(by_reason):
    out.append(f"| {reason} | {by_reason[reason]} |")
out += ["",
       "`dimension_never_populated`: the runtime has no producer for the dimension at all.",
       "`time_window_tier_redundant`: a `time_window` rate conditioned on `tier_code`; the",
       "schedule's window codes already encode the tier, so the extra condition only makes",
       "the rate unselectable. See `ROUTING_PRICING_SPEC.md` section 5.4.\n",
       "| file | priceId | meter | price | currency | unitSize | dimension | reason | tier | thresholdTokens |",
       "|---|---|---|---|---|---|---|---|---|---|"]
for r in rows:
    out.append(f"| {r['file']} | {r['priceId']} | {r['meter']} | {r['unitPrice']} | "
               f"{r['currency']} | {r['unitSize']} | {r['dimension']} | {r['reason']} | "
               f"{r['tierValue'] or '-'} | "
               f"{r['thresholdTokens'] or '-'} |")

DOC_PATH = os.path.join(ROOT, "docs", "pricing-unreachable-rates.md")
next_text = "\n".join(out) + "\n"

if "--check" in sys.argv:
    current = None
    if os.path.isfile(DOC_PATH):
        with open(DOC_PATH, encoding="utf-8") as fh:
            current = fh.read()
    if current != next_text:
        print("drift docs/pricing-unreachable-rates.md")
        print(f"  committed: {0 if current is None else current.count(chr(10))} line(s)")
        print(f"  expected : {next_text.count(chr(10))} line(s) from {len(rows)} unreachable rate(s)")
        sys.exit(1)
    print(f"docs/pricing-unreachable-rates.md is current ({len(rows)} row(s))")
    sys.exit(0)

# newline="\n" pins LF: the default text-mode write translates "\n" to
# os.linesep, so on Windows this generated artifact would come out CRLF and
# every regeneration would show up as a whole-file diff against the LF
# convention the repo commits. The read side keeps default newline handling so
# the --check comparison tolerates a CRLF working tree.
with open(DOC_PATH, "w", encoding="utf-8", newline="\n") as fh:
    fh.write(next_text)
print(f"wrote docs/pricing-unreachable-rates.md with {len(rows)} rows")
