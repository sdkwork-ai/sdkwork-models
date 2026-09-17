#!/usr/bin/env node
// Backfill `model-video-profiles` so every resolution the price book can bill is declared.
//
// The Cloud Router resolves a video request's `tier_code` by intersecting the tier codes a
// profile declares with the tier codes the model's video rates carry
// (`catalog.rs::decide_video_pricing_tier`). That function first filters the profile set by
// the *requested* resolution (`matches_resolution`), then takes the first declared tier code
// that also appears on the rate. When the request names a resolution no profile of that
// generation mode declares, the candidate list is empty, the resolver reports
// `DeclaredTierNotPriced` and the request is refused before dispatch — the rate is in the
// book but unreachable, which reads downstream as "no price for this model".
//
// `tools/seed-video-profiles.mjs` only creates missing *files* and hardcodes a single
// resolution per vendor branch, so a model whose price book prices `480p`/`720p`/`1080p`/`4k`
// ships with one resolution and silently cannot bill the others. This tool closes exactly
// that gap: for every plain `res_<token>` rate the model's price book carries, it clones each
// existing profile of the same generation mode at that resolution. Only resolutions the book
// already prices are added — the catalog's own rate decides, nothing is invented.
//
// Resolutions whose rate is wrapped in another dimension (`res_768p_dur_6s`, `audio`, …) are
// deliberately left alone: their tier cannot be named from a resolution alone, so pinning one
// would guess a price. `tools/validate-catalog.mjs` reports those separately.
//
// Usage:
//   node tools/sync-video-profile-resolutions.mjs           # report only, exit 0
//   node tools/sync-video-profile-resolutions.mjs --write    # apply
//   node tools/sync-video-profile-resolutions.mjs --check    # exit 1 when a gap remains
import { existsSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import {
  collectRegionalCatalogDirectories,
  loadManifest,
  loadVendorBundle,
  projectRootFromTool,
  stableJson,
  videoProfileCatalogKey,
} from "./catalog-lib.mjs";

/**
 * The meters whose rate a *generated output* is billed against. `video_input_second` prices
 * the source clip instead, so its tier names the input side and must not drive which
 * resolutions the catalog offers.
 */
const OUTPUT_PRICING_METERS = new Set(["video_output_second", "video_result"]);

/**
 * `res_720p` — a rate keyed by resolution alone. `res_4k_native` / `res_480p_720p` are
 * excluded on purpose: their extra segment carries a vendor distinction the tool cannot
 * resolve, and the resolution they do mention is already covered by a plain tier or is a
 * deliberate combined band.
 */
const PLAIN_RESOLUTION_TIER = /^res_([^_]+)$/;

/** Higher rank sorts first when ordering the resolutions added to a generation mode. */
const RESOLUTION_RANK = { "8k": 9, "4k": 8, "2k": 7, "1080p": 6, "768p": 5, "720p": 4, "540p": 3, "512p": 2, "480p": 1 };

function resolutionRank(token) {
  return RESOLUTION_RANK[token] ?? 0;
}

/** Every plain resolution token the model's output rates can be billed at. */
function pricedResolutionTokens(pricingRow) {
  const tokens = new Set();
  for (const price of pricingRow?.prices ?? []) {
    if (!OUTPUT_PRICING_METERS.has(price.meterCode)) {
      continue;
    }
    const match = PLAIN_RESOLUTION_TIER.exec(price.tierCode ?? "");
    if (match) {
      tokens.add(match[1]);
    }
  }
  return tokens;
}

/** Resolution tokens the profile file already declares, by any of its three spellings. */
function declaredResolutionTokens(profiles) {
  const tokens = new Set();
  for (const profile of profiles) {
    if (typeof profile.resolution === "string" && profile.resolution.length > 0) {
      tokens.add(profile.resolution);
    }
    for (const code of [
      profile.resolutionTierCode,
      ...(profile.pricingTierCodes ?? []),
      profile.durationTierCode,
      ...(profile.durationTierCodes ?? []),
    ]) {
      if (typeof code !== "string" || code.length === 0) {
        continue;
      }
      const match = PLAIN_RESOLUTION_TIER.exec(code);
      if (match) {
        tokens.add(match[1]);
      }
    }
  }
  return tokens;
}

/**
 * Rewrite the trailing resolution of a generated `profileCode` / `displayName`. Both are
 * built as `…_<resolution>` and `… · <resolution>` by `seed-video-profiles.mjs`, and the
 * curated multi-resolution files follow the same shape, so replacing the trailing token is
 * exact — the separator (`_` or ` · `) stays as it was found.
 */
function swapResolutionSuffix(text, fromResolution, toResolution) {
  if (typeof text !== "string" || fromResolution.length === 0 || !text.endsWith(fromResolution)) {
    return null;
  }
  return `${text.slice(0, text.length - fromResolution.length)}${toResolution}`;
}

/**
 * Display names use a human label for the native-resolution bands (`4K Native`), so the
 * label has to move with the token rather than the raw tier code.
 */
const DISPLAY_LABEL_BY_TOKEN = { "4k_native": "4K Native", "4k": "4K", "2k": "2K" };

function cloneProfileAtResolution(template, token, sortOrder, vendorCode, modelId) {
  const profileCode = swapResolutionSuffix(template.profileCode, template.resolution, token);
  const displayName = swapResolutionSuffix(template.displayName, template.resolution, token);
  if (profileCode === null || displayName === null) {
    return null;
  }
  return {
    ...template,
    profileCode,
    catalogKey: videoProfileCatalogKey(vendorCode, modelId, profileCode),
    displayName,
    resolution: token,
    resolutionTierCode: `res_${token}`,
    // The curated default stays the default: promoting a freshly added resolution would
    // change what every client offers without anyone asking for it.
    isDefault: false,
    sortOrder,
    wireParameters: { ...(template.wireParameters ?? {}), resolution: token },
  };
}

/** Append the missing resolutions to one profile file; returns null when nothing is missing. */
export function planProfileFile(profileFile, pricingRow) {
  const profiles = profileFile.profiles ?? [];
  const tokens = pricedResolutionTokens(pricingRow);
  const declared = declaredResolutionTokens(profiles);
  const missing = [...tokens].filter((token) => !declared.has(token)).sort(
    (left, right) => resolutionRank(right) - resolutionRank(left) || left.localeCompare(right),
  );
  if (missing.length === 0 || profiles.length === 0) {
    return null;
  }

  const added = [];
  const grouped = new Map();
  for (const profile of profiles) {
    if (!grouped.has(profile.generationMode)) {
      grouped.set(profile.generationMode, []);
    }
    grouped.get(profile.generationMode).push(profile);
  }

  const nextProfiles = [];
  const skipped = [];
  let sortOrder = 10;
  for (const [generationMode, members] of grouped) {
    for (const member of members) {
      nextProfiles.push({ ...member, sortOrder });
      sortOrder += 10;
    }
    // Clone every member rather than one per mode: a `fixed` duration policy carries one
    // profile per duration (`t2v_5s_720p`, `t2v_10s_720p`), and each needs its own row at the
    // added resolution or the request at that duration still finds no tier.
    for (const member of members) {
      for (const token of missing) {
        const clone = cloneProfileAtResolution(
          member,
          token,
          sortOrder,
          profileFile.vendorCode,
          profileFile.modelId,
        );
        if (clone === null) {
          skipped.push({ generationMode, token, profileCode: member.profileCode });
          continue;
        }
        nextProfiles.push(clone);
        added.push({ generationMode, token, profileCode: clone.profileCode });
        sortOrder += 10;
      }
    }
  }

  return { file: { ...profileFile, profiles: nextProfiles }, added, skipped };
}

export function syncVideoProfileResolutions(root, { write = false } = {}) {
  const manifest = loadManifest(root);
  const files = [];
  const skipped = [];
  for (const regionDir of collectRegionalCatalogDirectories(join(root, "models"))) {
    const bundle = loadVendorBundle(regionDir);
    const profilesDir = join(regionDir, "model-video-profiles");
    if (!existsSync(profilesDir)) {
      continue;
    }
    for (const profileFile of bundle.modelVideoProfiles ?? []) {
      const pricingRow = (bundle.pricing ?? []).find((row) => row.modelId === profileFile.modelId);
      const plan = planProfileFile(profileFile, pricingRow);
      if (plan === null) {
        continue;
      }
      const relPath = join(profilesDir, `${profileFile.modelId}.json`).replace(/\\/g, "/");
      files.push({ relPath, added: plan.added, skipped: plan.skipped });
      for (const entry of plan.skipped) {
        skipped.push({ relPath, ...entry });
      }
      if (write) {
        writeFileSync(
          join(profilesDir, `${profileFile.modelId}.json`),
          `${stableJson(plan.file)}\n`,
          "utf8",
        );
      }
    }
  }
  return { files, skipped, catalogVersion: manifest.catalogVersion };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const root = projectRootFromTool(import.meta.url);
  const write = process.argv.includes("--write");
  const check = process.argv.includes("--check");
  const { files, skipped } = syncVideoProfileResolutions(root, { write });
  const totalAdded = files.reduce((sum, file) => sum + file.added.length, 0);

  if (check) {
    if (totalAdded === 0) {
      console.log("sync-video-profile-resolutions: every priced resolution has a profile");
      process.exit(0);
    }
    console.log(
      `sync-video-profile-resolutions: ${totalAdded} priced resolution(s) across ${files.length} model profile file(s) cannot be selected because no profile declares them:`,
    );
    for (const file of files) {
      const resolutions = [...new Set(file.added.map((entry) => entry.token))].sort().join(", ");
      console.log(`  ${file.relPath}: ${resolutions}`);
    }
    console.log("run `node tools/sync-video-profile-resolutions.mjs --write` to add them");
    process.exit(1);
  }

  if (totalAdded === 0) {
    console.log("sync-video-profile-resolutions: every priced resolution has a profile");
    process.exit(0);
  }
  for (const file of files) {
    const resolutions = [...new Set(file.added.map((entry) => entry.token))].sort().join(", ");
    console.log(
      `${write ? "updated" : "would update"} ${file.relPath}: +${file.added.length} profile(s) at ${resolutions}`,
    );
  }
  for (const entry of skipped) {
    console.log(`  ! skipped ${entry.relPath} ${entry.profileCode} -> ${entry.token} (profileCode shape not recognised)`);
  }
  console.log(
    `${write ? "Updated" : "Would update"} ${files.length} file(s), ${totalAdded} profile(s)${write ? "" : " (dry-run)"}`,
  );
}
