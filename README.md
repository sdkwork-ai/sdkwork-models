# sdkwork-models
repository-kind: foundation-dependency

`sdkwork-models` is the standalone model catalog project for Sdkwork products
and application integrations. It stores model vendor data, model facts, billing
meters, prices, source evidence, and optional ranking snapshots as versioned
JSON files.

The catalog is designed for two consumers:

- applications that want to load model and price information directly through a
  language SDK
- CloudRouter, which imports the same catalog into its canonical `ai_*` model
  and pricing tables during installation or catalog refresh

## Status

Production catalog repository for Sdkwork model intelligence. The catalog ships as
versioned JSON under `models/`, consumed by `@sdkwork/models` SDKs, the standalone
gateway, and CloudRouter catalog sync.

Current catalog version: **2026.09.17.1** (see `sdkwork-models.json`). TTS voice
(speaker) catalog uses `voices.json` and `model-voices/` bindings; video generation
profiles use `model-video-profiles/` per model. See `specs/voice-catalog.spec.json`
and `specs/video-generation-profile.spec.json`.

CloudRouter submodule mount point:

```text
data/sdkwork-models
```

Upstream repository:

```text
https://github.com/sdkwork-ai/sdkwork-models.git
```

## Standard

The authoritative SDKWork standards are:

```text
../sdkwork-specs/README.md
../sdkwork-specs/REGION_SPEC.md
specs/component.spec.json
docs/standards-alignment.md
docs/root-layout.md
```

Repository-specific catalog contracts remain documented in this README and under `schemas/`.

## Directory Layout

```text
sdkwork-models/
  README.md
  LICENSE
  CHANGELOG.md
  sdkwork-models.json
  schemas/
    catalog.schema.json
    index.schema.json
    official-model-snapshot.schema.json
    official-verification-policy.schema.json
    vendor-sources.schema.json
    meter.schema.json
    vendor.schema.json
    family.schema.json
    model.schema.json
    pricing.schema.json
    voice.schema.json
    model-voice.schema.json
    model-video-profiles.schema.json
    ranking.schema.json
    provider-overlay.schema.json
  models/
    index.json
    meters.json
    vendors.json
    <vendorCode>/
      <regionCode>/
        vendor.json
        families.json
        models/
          <modelId>.json
        pricing/
          <modelId>.json
        voices.json
        model-voices/
          <modelId>.json
        model-video-profiles/
          <modelId>.json
        rankings.json
  overlays/
    cloudrouter/
  sources/
    vendor-sources.json
    official-model-snapshots.json
    official-verification-policy.json
  tools/
  apps/
    sdkwork-models-pc/
  sdks/
    sdkwork-models-sdk/
      sdkwork-models-sdk-typescript/
      sdkwork-models-sdk-python/
      sdkwork-models-sdk-java/
      sdkwork-models-sdk-rust/
      sdkwork-models-sdk-flutter/
```


## Vendor and Region Identity

`vendorCode` is the stable model vendor identity and must not encode a product
line or operating region. `regionCode` identifies the operating market,
billing currency, legal jurisdiction, and platform scope for that vendor.

Examples:

- `models/minimax/cn`
- `models/minimax/global`
- `models/deepseek/cn`
- `models/deepseek/global`
- `models/moonshot/cn`
- `models/moonshot/global`
- `models/alibaba/cn`
- `models/kuaishou/cn`
- `models/kuaishou/global`

The same upstream `modelId` can appear in more than one region under the same
vendor. Application code and database imports must use `vendorCode/modelId` as
the stable model catalog key. `regionCode` stays explicit on vendor-region,
deployment, ranking, and pricing records, and every price row must use the
region's `billingCurrency`.

## Vendor Rule

Each model vendor has one isolated directory:

```text
models/openai/global/
models/anthropic/global/
models/google/global/
models/deepseek/cn/
models/deepseek/global/
```

A vendor is the model publisher, not the provider used to access it.

Examples:

- `openai` is a model vendor.
- `anthropic` is a model vendor.
- `google` is a model vendor.
- `openrouter`, `azure_openai`, and `aws_bedrock` are access providers and
  belong in overlays, not vendor directories.

## Model and Price Files

Each model is stored in its own file:

```text
models/openai/global/models/gpt-5.2.json
models/openai/global/pricing/gpt-5.2.json
```

Model files contain model facts only. Pricing files contain price rows only.
Provider/channel routing belongs in overlays.

`models/index.json` is the generated file manifest for all SDK loaders. Each
vendor-region entry declares `path`, `familiesPath`, `rankingsPath`,
`modelFiles`, `pricingFiles`, and optional `voicesPath` / `modelVoicesPath` /
`modelVoiceFiles` for TTS speaker catalogs. SDKs must use these file lists
instead of enumerating directories so the same catalog root works from a local
checkout, GitHub raw URLs, CDN/object storage, and application asset bundles.

`schemas/index.schema.json` defines the machine-readable index contract. The
validator additionally checks the semantic contract: the declared `modelFiles`,
`pricingFiles`, and voice binding files must exactly match the generated file
lists for each `vendorCode/regionCode`, declared paths must stay under the same
vendor-region directory, and counts plus hashes must match `tools/build-index.mjs`.

## TTS Voice Catalog

TTS speakers are first-class catalog entities, separate from model facts but
bound to TTS models through many-to-many `model-voices/{modelId}.json` files.
Contract authority: `specs/voice-catalog.spec.json`.

Storage per vendor region:

```text
models/openai/global/voices.json
models/openai/global/model-voices/gpt-4o-mini-tts.json
```

Current seeded vendors: OpenAI, Google Gemini TTS, ElevenLabs (dynamic voice
library), MiniMax (cn/global), and ByteDance Seed TTS (cn).

SDK query helpers (all language SDKs): `listVoices`, `listVoicesForModel`,
`listModelsForVoice`.

HTTP read surfaces (SdkWork v3 list envelope: `data.items` + `data.pageInfo`):

| Surface | Route | Auth |
| --- | --- | --- |
| App | `GET /app/v3/api/ai/voices` | Public |
| App | `GET /app/v3/api/ai/models/{modelId}/voices` | Public |
| Backend | `GET /backend/v3/api/ai/voices` | `intelligence.models.read` |
| Backend | `GET /backend/v3/api/ai/models/{modelId}/voices` | `intelligence.models.read` |

Query parameters: `vendor_code`, `region_code`, `locale`, `model_id`,
`catalog_key`, `q`.

Database tables `ai_model_voice` and `ai_model_voice_binding` are defined in
baseline DDL (`database/ddl/baseline/*/0001_sdkwork-models_baseline.sql`) and
populated during catalog sync import.

## Video Generation Profiles

Video models declare supported generation modes, duration policies, and wire
mappings in per-model profile files. Contract authority:
`specs/video-generation-profile.spec.json`.

Storage:

```text
models/kuaishou/global/model-video-profiles/kling-v3.json
```

Each profile describes one executable configuration (for example t2v + 5s + 720p,
or i2v + 3–15s range). Duration buckets use canonical tier codes (`dur_5s`,
`dur_10s`, …) cross-referenced from pricing `tierCode` when billing is per result.

Every catalog model with `primaryCapability: video` must have a
`model-video-profiles/{modelId}.json` file. Use `node tools/seed-video-profiles.mjs`
to scaffold missing profiles from vendor defaults.

Database table `ai_model_video_profile` is populated during catalog sync import.

SDK query helpers: `listVideoProfiles`, `listVideoProfilesForModel`,
`findVideoProfile`.

HTTP read surfaces (SdkWork v3 list envelope):

| Surface | Route | Auth |
| --- | --- | --- |
| App | `GET /app/v3/api/ai/video_profiles` | Public |
| App | `GET /app/v3/api/ai/models/{modelId}/video_profiles` | Public |
| Backend | `GET /backend/v3/api/ai/video_profiles` | `intelligence.models.read` |
| Backend | `GET /backend/v3/api/ai/models/{modelId}/video_profiles` | `intelligence.models.read` |

Query parameters: `vendor_code`, `region_code`, `model_id`, `catalog_key`,
`generation_mode`, `duration_tier_code`, `resolution`.

## Source Evidence

Every release must maintain:

```text
sources/vendor-sources.json
sources/official-model-snapshots.json
sources/official-verification-policy.json
```

This manifest declares each vendor's official model and pricing URLs, local
cross-check references, and `requiredModels`. `requiredModels` is the current
sync contract: every listed model must have both a model JSON file and a
pricing JSON file. Deprecated models may remain in the catalog, but they should
be hidden or `catalog_only` and point at a replacement model when one exists.

Commercial availability is stricter than model discovery. A model may be kept
for discovery with `routingState: "catalog_only"` and `shelfState: "hidden"`
when official pricing is not confirmed. Any model that is enabled, listed, or
active must have a matching pricing file with at least one billable row, and a
family `defaultModel` must always point to an enabled, listed, priced model.

`sources/official-model-snapshots.json` is the independent evidence snapshot
for vendor-regions promoted to `verificationStatus: "official_verified"`.
It must use the same `schemaVersion` and `catalogVersion` as
`sdkwork-models.json`. Each snapshot vendor must map to one declared
`vendorCode/regionCode` in `sources/vendor-sources.json` and one catalog
directory under `models/<vendorCode>/<regionCode>`. Snapshot URLs must stay
inside that vendor-region's declared official `modelsUrl`, `pricingUrl`, or
`additionalUrls`; reference repositories are not valid official snapshot
URLs. Snapshot model IDs must be unique and must exist in the matching catalog
vendor-region. `schemas/official-model-snapshot.schema.json` is the static
file contract, and `tools/catalog-audit.mjs` enforces the semantic contract.
Each snapshot vendor must also carry `sourceSnapshotHash`, the SHA-256 of that
vendor snapshot after removing `sourceSnapshotHash` and serializing with the
catalog stable JSON order. `tools/catalog-audit.mjs` rejects mismatches, and
`tools/release-catalog.mjs` records the same values under
`sourceEvidenceSha256.officialSnapshotHashes` by `vendorCode/regionCode`.

`schemas/vendor-sources.schema.json` is the static contract for
`sources/vendor-sources.json`. The audit also enforces that each
`vendorCode/regionCode` appears only once so a later source declaration cannot
silently override an earlier official source boundary.

`sources/official-verification-policy.json` is the release gate for
vendor-regions that must remain `official_verified`. It must satisfy
`schemas/official-verification-policy.schema.json`, use the same
`schemaVersion` and `catalogVersion` as `sdkwork-models.json`, and declare
`requiredVerifiedVendorRegions`. Each listed `vendorCode/regionCode` must have
a matching catalog directory, a matching source declaration, verification
status `official_verified`, and an independent official snapshot for the same
catalog release. Duplicate policy entries are rejected. The gate is also
bidirectional: every vendor-region declared as `official_verified` in
`sources/vendor-sources.json` must appear in `requiredVerifiedVendorRegions` so
official verification cannot bypass release review.

## Decimal Rule

Prices and billable quantities are strings, never JSON numbers:

```json
{
  "unitSize": "1000000",
  "unitPrice": "1.750000",
  "minimumQuantity": "0"
}
```

This avoids binary floating-point drift across TypeScript, Python, Java, Rust,
and Flutter.

## SDKs

Each SDK must expose equivalent loading, validation, and query behavior:

```text
loadCatalog(pathOrUrl)
loadBundledCatalog()
loadVendorCatalog(pathOrUrl, vendorCode, regionCode)
validateCatalog(catalog)
listVendors(catalog)
listVendorRegions(catalog)
listModels(catalog, filter)
listAvailableModels(catalog)
findModel(catalog, catalogKey)
findModelByVendorRegion(catalog, vendorCode, regionCode, modelId)
getModelPrices(catalog, catalogKey)
getBestReferencePrice(catalog, catalogKey, meterCode)
listModelsByCapability(catalog, capability)
listModelsByModality(catalog, input, output)
listProtocols(catalog)
findProtocol(catalog, protocolCode)
listProtocolsByVendor(catalog, vendorCode)
listModelsByProtocol(catalog, protocolCode)
listMeters(catalog)
findMeter(catalog, meterCode)
```

`listModels` filters are part of the cross-language contract and must support:

- `vendorCode`
- `regionCode`
- `familyCode`
- `capability`
- `inputModality`
- `outputModality`
- `releaseStage`
- `shelfState`
- `routingState`
- `apiFormat`

`apiFormat` values are protocol codes from `models/protocols.json`. SDKs expose
protocol queries so applications can discover protocol metadata, inspect which
protocols a vendor supports, and list models that conform to a protocol.

`listAvailableModels` is the safe default for application selectors, routing
configuration, and pricing previews. It returns only enabled, listed models
that have at least one billable pricing row.

Planned package names:

- TypeScript: `@sdkwork/models`
- Python: `sdkwork-models`
- Java: `com.sdkwork.models:sdkwork-models`
- Rust: `sdkwork-models`
- Flutter/Dart: `sdkwork_models`

## Application Integration

Applications can use this catalog in three ways:

1. Copy or vendor the catalog project root and load it locally.
2. Set `SDKWORK_MODELS_CATALOG_ROOT` and call `loadBundledCatalog()`.
3. Load immutable catalog JSON from GitHub, CDN, object storage, or an internal
   artifact service with `catalogVersion` and `sha256` verification.

## CloudRouter Integration

CloudRouter imports this catalog into:

- `ai_billing_meter`
- `ai_model_vendor`
- `ai_model_family`
- `ai_model`
- `ai_model_capability`
- `ai_model_pricing`
- `ai_model_rank_snapshot`

CloudRouter-specific providers, channels, route rules, secrets, and tenant
policies remain outside the portable model catalog and belong in
`overlays/cloudrouter/` or CloudRouter runtime configuration.

Installer and refresh details are documented in:

```text
../../docs/33-sdkwork-models-install-flow.md
```

At runtime, CloudRouter resolves the catalog from `SDKWORK_MODELS_CATALOG_ROOT`
when set, then falls back to the bundled local Rust package catalog. The
environment variable must point at this project root.

In the local CloudRouter workspace, `pnpm.cmd dev` and `pnpm.cmd dev:server`
default `SDKWORK_MODELS_CATALOG_ROOT` to this checkout-local
`data/sdkwork-models` directory and run `refresh-catalog --force` before
starting the Rust services. Updating JSON files here is enough for the next dev
startup to update the SQLite dev database.

Installed CloudRouter databases can refresh from the catalog without a full
reinstall:

```powershell
sdkwork-cloudrouter-installer refresh-catalog
sdkwork-cloudrouter-installer refresh-catalog --vendor openai
sdkwork-cloudrouter-installer refresh-catalog --catalog-root <catalog-root> --catalog-version 2026.06.24.3
sdkwork-cloudrouter-installer refresh-catalog --vendor alibaba --dry-run
```

Installer commands emit one camelCase JSON object to stdout. Refresh output
includes `status`, `synced`, `catalogVersion`, `vendorCodes`, `meterCount`,
`vendorCount`, `familyCount`, `modelCount`, `capabilityCount`, `priceCount`,
`rankingCount`, `acceptedCount`, `snapshotId`, `syncRunId`, and
`lastCatalogRefreshStatus`; deployment scripts should parse those fields
instead of scraping text. `acceptedCount` is the total imported standard fact
count across shared meters, selected vendors, families, models, capabilities,
prices, and ranking items. Failures emit one camelCase JSON object to stderr
with `status`, `errorCode`, and `message`. Stable installer error codes are
`missing_database_url`, `invalid_argument`, `invalid_state`, `database_error`,
`catalog_error`, and `installer_error`. Installer argument validation is
independent from database initialization: invalid commands or refresh options
return `invalid_argument` even when `SDKWORK_DATABASE_URL` is absent, which
lets update tooling lint invocations without provisioning a database first.
Non-refresh commands reject unexpected extra arguments; only `refresh-catalog`
accepts refresh-specific options.

The backend admin API and generated `@sdkwork/cloudrouter-backend-sdk` expose
the same refresh report as `AdminModelCatalogSyncResponse`. Application-level
service wrappers must preserve the full report, including all count fields,
`snapshotId`, and `syncRunId`, while also normalizing the returned `vendors`
and `models`. This keeps UI integrations, deployment scripts, and monitoring
jobs on one shared sync contract.

`vendor_refresh` imports only the selected vendor directories; shared meters are
kept global. `dry_run` previews the selected vendor/model scope and records a
dry-run sync record without mutating catalog tables.
Non-dry-run refreshes commit catalog table upserts, pricing import snapshot,
sync-run row, and audit log in one transaction. If a later sync step fails, the
catalog tables remain at their previous values and a separate failed sync-run
row is written when possible.
Failed refresh attempts also create a failed sync-run audit row in the target
application database. When the catalog loads successfully but vendor selection
or sync execution fails, that row must keep the resolved `catalogVersion` plus
the requested `vendorCodes`; update tooling should preserve this trail for
diagnostics and repeatable rollouts. Failed-refresh audit persistence is
best-effort and must not mask the original refresh error returned to callers.

## Continuous Updates

Each data update must regenerate indexes, validate JSON contracts, check source
freshness, and create or verify release metadata:

```powershell
pnpm run check
```

CloudRouter deployments should pin a catalog tag, release artifact, or submodule
commit. Production installs must not depend on a floating branch head.

## Release Versioning

Schema versions use semantic versioning:

```text
1.0.0
```

Catalog data versions use date release numbering:

```text
YYYY.MM.DD.N
```

Example:

```text
2026.06.24.3
```

The authoritative value is always `models/index.json` → `catalogVersion`.

Recommended Git tag format:

```text
catalog-v2026.06.24.3
```


## SDKWork Documentation Contract

Domain: intelligence
Capability: model
Package type: node-package
Status: standardizing

### Public API

Public exports are declared in `specs/component.spec.json` under `contracts.publicExports`.

### Required SDK Surface

- None declared in `specs/component.spec.json`.

### Configuration

Configuration keys and runtime entrypoints are declared in `specs/component.spec.json`.

### SaaS/Private/Local Behavior

This module follows the canonical standards linked from `specs/component.spec.json`, including deployment and runtime configuration rules where applicable.

### Security

Do not add secrets, live tokens, manual auth headers, or app-local credential handling to this module.

### Extension Points

Extension points are limited to declared public exports, runtime entrypoints, SDK clients, events, and config keys.

### Verification

- `pnpm run check`
- `pnpm run verify`

### Owner And Status

Owner and lifecycle status are tracked in `specs/component.spec.json`.

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Application Roots

- [apps directory index](apps/README.md)
