# Changelog

## 2026.09.17.1

Full vendor-by-vendor resync: **all 34 vendor-regions** re-verified against each vendor's official
model and pricing pages on 2026-09-17 (first full sweep since 2026-08-13). Evidence timestamps
stamped to `2026-09-17T00:00:00Z`; catalog version bumped from `2026.09.06.1`.

### Catalog deltas

- **164 new model files** and **145 new pricing files**; **259 model files** and **259 pricing files**
  updated; 11 `families.json` touched; 44 `model-video-profiles/` added.
- **New families**: `dola-seed`, `seed-2-0`, `dreamina-seedance` (bytedance/global),
  `kling-image` (kuaishou cn/global), `minimax-video` (minimax/cn).

### Per-vendor highlights

- **openai/global** — 9 new (`gpt-5.6-cyber`, `gpt-5.5-cyber`, `gpt-5.3-codex`,
  `gpt-image-2.5-flare`, `gpt-image-2.5-sunburst`, `gpt-live-1`, `gpt-live-transcribe`,
  `gpt-transcribe`, `gpt-rosalind-research`); legacy transcribe/whisper/realtime-mini demoted;
  official short/long-context rate matrix (incl. cache writes) backfilled.
- **deepseek/cn + global** — `deepseek-flash` (DeepSeek-V4.1-Flash) added; `deepseek-v4-flash` and
  `deepseek-v4-flash-vision-exp` retired by the vendor (legacy ids stay callable and are demoted to
  `catalog_only`/`hidden`); Flash peak/off-peak rates lowered; `deepseek-v4-pro` unchanged.
- **google/global** — 12 new (Gemini 3.8 Live, 3.5 Transcribe/Translate, Nano Banana 2 Lite,
  Omni Flash Preview, Lyria 3.5/3 Clip/3 Pro, Robotics ER 2); Imagen 4.0 line and
  3.1 flash/lite previews retired; 20 rate rows corrected.
- **minimax/cn + global** — 15 + 10 new models; video pricing re-expressed on official
  `video_result` tiers; Hailuo 2.3 rate rows corrected.
- **kuaishou/cn + global** — 6 + 6 new models; sound-effect unit price cut; `kling-v3` /
  `kling-v3-omni` expanded from single rows to full per-tier matrices.
- **bytedance/global** — global catalogue re-keyed to the vendor's actual international ids
  (`dola-seed-*`, `seed-2-0-*`, `dreamina-seedance-*`); legacy `doubao-*` global entries demoted.
- **runway/global** — 28 new (17 video, 11 image) with official pricing; 78 rate rows rewritten.
- **vidu/cn + global** — 4 + 4 new (Viduq3 series); `vidu-s1` demoted after the S2 launch.
- **zhipu/cn** — 6 new (`glm-5`, `glm-5-turbo`, `glm-5v-turbo`, `glm-ocr`, `glm-image`,
  `embedding-2`); `glm-5.3-flash` cache-hit rate corrected.
- **tencent/cn** — 5 new Hunyuan translation/role models; source declaration moved to TokenHub.
- **alibaba/cn + global**, **xiaomi/cn + global**, **baidu/cn**, **anthropic/global**,
  **elevenlabs/global**, **suno/global**, **stability_ai/global**, **black_forest_labs/global**,
  **mureka/global**, **luma_ai/global**, **pixverse/cn + global**, **xai/global**,
  **moonshot/cn + global**, **stepfun/cn**, **meituan/cn** — model facts and/or rate rows updated
  (see `.workbuddy/reports/` for the per-vendor evidence trail).

### Evidence and release files

- `sources/vendor-sources.json`: all 34 `lastCheckedAt` refreshed; 15 previously undeclared but
  officially referenced URLs added to `official.additionalUrls`; Kuaishou source hosts moved to
  `klingai.com` / `kling.ai`.
- `sources/official-model-snapshots.json`: all 9 snapshots recomputed
  (`models` = required ∪ enabled ∪ default, refreshed `officialUrls`, `sourceSnapshotHash`).
- `releases/2026.09.17.1.json` generated (34 regions, 25 vendors, 6 official-verified regions).
- Models with no published official price are catalogued as `catalog_only` + `hidden`
  (`suno-v6*`, `xiaomi/mimo-v2.5-tts*`, `flux-2-dev`, `sdxl-1-0`) — no prices were invented.
- Verification: `validate-catalog` (0 issues), `catalog-audit` (0 issues), freshness report
  (0 stale), `release-catalog --check`, `build-index --check`, `migrate-pricing-v2` (0 pending),
  `generate-mainstream-agent-model-catalog --check`, API envelope and app-composition
  checks all pass. Two OpenAPI checks (`models_openapi_export --check`,
  `materialize-models-openapi --check`) and `models-openapi-contract.test.mjs` fail on a
  **pre-existing** drift — reproduced unchanged on the `2026.09.06.1` tree, so not addressed here.

### Alignment follow-up

A second regression sweep covered every reference outside `models/`:

- **Regenerated the frontend picker artifacts** — `mainstreamAgentModelCatalog.generated.ts` and
  `officialModelVendorPresets.generated.ts` were pinned at `2026.08.17.1` and still advertised
  ids this sync demoted (`deepseek-v4-flash`, `gemini-3.1-flash-lite-preview`, `longcat-1.0`,
  `mimo-v2.5-pro-fp4-dflash`, `qwen3.8-max-preview`). Both now regenerate clean from the catalog;
  97 catalog entries and 87 vendor presets were re-checked to be `enabled` + `listed` + priced.
- **Repaired unroutable client-model aliases** — `clientApiCompatibility.*.modelMapping.rules`
  pointed at models demoted to `catalog_only`/`hidden`: `deepseek-v4-flash` was repointed to its
  declared successor `deepseek-flash` (3 rules), and `mimo-v2.5-pro-fp4-dflash` / `mimo-v2-flash`
  were dropped where no routable successor exists (3 rules).
- **Converged the alias tables to per-vendor scope** — `alibaba/global`, `deepseek/global`, and
  `xiaomi/global` had been written with a byte-identical 8-rule list, so 51 entries named models the
  declaring vendor does not serve (e.g. `mimo-v2.5-pro` under Alibaba). Per the documented contract
  (`sourceModel` = vendor-model-id) each table now lists only that vendor's own routable chat models:
  alibaba 5/API (`qwen3.7-flash`, `qwen3.7-max`, `qwen3.7-plus`, `qwen3.8-flash`, `qwen3.8-max`),
  deepseek 2/API (`deepseek-flash`, `deepseek-v4-pro`), xiaomi 2/API (`mimo-v2.5`, `mimo-v2.5-pro`).
  Tier assignment (`strong` → `claude-sonnet-4-6` / `gpt-5.5` / `gemini-3.1-pro-preview`;
  `light` → `claude-haiku-4-5` / `gpt-5.4` / `gemini-3-flash-preview`) was reverse-engineered from the
  pre-existing rules and reproduces all 8 of their source→target pairs exactly.
- **Recomputed pricing hashes** — 48 rate `rateHash` values across 8 `deepseek/{cn,global}`
  pricing files were out of sync with their content.
- **Regenerated derived artifacts** — `models/index.json`, `models/vendors.json`, and
  `releases/2026.09.17.1.json` rebuilt after the above.
- **Added the missing architecture-doc generator** — `docs/vendor-model-architecture.md` and
  `docs/architecture/tech/TECH-vendor-model-architecture.md` were hand-written snapshots dated
  2026-06-13 covering only 18 vendors. New `tools/generate-vendor-model-architecture-doc.mjs`
  regenerates both from the catalog (now 25 vendors / 34 vendor-regions), documents its own
  column semantics, and supports `--check`.
- **Wired two previously ungated generators into `_sdkwork:check`** —
  `generate-vendor-model-architecture-doc --check` and `generate-mainstream-agent-model-catalog --check`,
  plus npm scripts `models:check:vendor-architecture-doc`, `models:generate:vendor-architecture-doc`,
  `models:check:picker-catalog`, `models:generate:picker-catalog`.
- **Ignored the local agent workspace** — `.workbuddy/` added to `.gitignore`, so resync evidence
  (per-vendor reports, raw official-page captures, review scripts) stays out of the tree and out of
  gate scans.
- Deliberately **not** changed, with reasons recorded in `.workbuddy/reports/SUMMARY-2026.09.17.1.md`:
  dated `rankings.json` snapshots (historical records whose `snapshotDate` makes retention correct),
  converter test fixtures, and doc example blocks that mention retired ids.
- Final gate state: 12 of 15 checks pass; the 3 failures (`models_openapi_export --check`,
  `materialize-models-openapi --check`, `models-openapi-contract.test.mjs`) are pre-existing and were
  reproduced unchanged on a `HEAD` worktree. `validate-catalog.mjs` still does not validate
  `modelMapping`; adding that rule is a recommended follow-up rather than done here.

## 2026.07.05.3

- Fixed video profile HTTP routes to use `lower_snake_case` path segments (`video_profiles`) per `API_SPEC.md`; regenerated OpenAPI and backend SDK types including `videoProfileCount` on `ModelCatalogSyncResult`.
- Synced `pnpm-workspace.yaml` with sdkwork-specs workspace registry.

## 2026.07.05.2

- Expanded video generation profiles to **all 35 video models** (`primaryCapability: video`); added `tools/seed-video-profiles.mjs` for vendor-default scaffolding.
- Added catalog validation requiring every video model to declare `model-video-profiles/{modelId}.json`.
- Added `ai_model_video_profile` baseline table, contract registry entry, and catalog import sync (SQLite + Postgres).
- Exposed `videoProfileCount` on admin `models.sync` result (`ModelCatalogSyncResult`); PC admin parser aligned.

## 2026.07.05.1

- Introduced **Video Generation Profile** catalog (`specs/video-generation-profile.spec.json`, `schemas/model-video-profiles.schema.json`, `model-video-profiles/{modelId}.json`) with canonical generation modes, duration policies, and `dur_*` tier codes linked to pricing `tierCode`.
- Seeded profiles for Kling v3, Sora 2, Vidu Q3 Pro (with `dur_5s` / `dur_10s` bucket pricing), MiniMax Hailuo 2.3, and ByteDance Seedance 2.0.
- Extended catalog index, validation, and all language SDK helpers: `listVideoProfiles`, `listVideoProfilesForModel`, `findVideoProfile`.
- Added app + backend read APIs (`GET .../video-profiles`, `GET .../models/{modelId}/video-profiles`) with `SdkWorkPageData` list envelope; OpenAPI export and route manifests updated.

## 2026.07.04.3

- Materialized L2 database contract from baseline DDL (`db:materialize:contract`); fixed `schema.yaml` format for drift-check; registered `ai_model_voice` / `ai_model_voice_binding` in table registry.
- Removed redundant `0002_ai_model_voice` forward migrations (voice tables live in baseline `0001`); updated README and architecture docs.

## 2026.07.04.2

- Catalog version bump to **2026.07.04.2**; release manifest `releases/2026.07.04.2.json`.
- Added backend voice catalog read routes (`GET /backend/v3/api/ai/voices`, `GET /backend/v3/api/ai/models/{modelId}/voices`) with IAM `intelligence.models.read`; refactored shared `voice_catalog` handlers for app and backend surfaces.
- Expanded TTS voice data for MiniMax (cn/global) and ByteDance Seed TTS (cn); added Java and Flutter `listVoices` / `listVoicesForModel` / `listModelsForVoice` query helpers; TypeScript and Python voice catalog tests.
- Consolidated architecture docs: `TECH-standards-alignment.md` defers to `docs/standards-alignment.md`; removed stale `.catalog-audit.json` artifact.

## 2026.07.04.1

- Production alignment: fixed voice API handlers to emit `SdkWorkPageData` list envelope; merged voice OpenAPI paths into app contract export.
- Updated README and standards-alignment for TTS voice catalog posture; removed stale catalog version references.

## 2026.07.03.3

- Introduced industry-aligned TTS voice (speaker) catalog: first-class `voices.json` per vendor region, `model-voices/{modelId}.json` many-to-many bindings, and `specs/voice-catalog.spec.json` contract.
- Added JSON schemas `voice.schema.json` and `model-voice.schema.json`; extended catalog index with `voiceCount`, `modelVoiceFiles`, and validation for binding integrity.
- Seeded OpenAI and Google Gemini TTS voices plus ElevenLabs dynamic voice-library provisioning (`vendor_api` + official list endpoint).
- Extended Rust and TypeScript catalog SDKs with `listVoices`, `listVoicesForModel`, and `listModelsForVoice` helpers.
- Added app API `GET /app/v3/api/ai/voices` and `GET /app/v3/api/ai/models/{model_id}/voices`; database migration `0002_ai_model_voice.sql` for `ai_model_voice` and `ai_model_voice_binding`.

## 2026.07.03.2

- Expanded sound-effect (SFX) catalog coverage across four vendors with dedicated `sfx` capability and `sfx_result` billing meter.
- Added Kling Sound text-to-audio (`kling-sound-t2a`) and video-to-audio (`kling-sound-v2a`) for cn and global with official per-task pricing.
- Added Vidu Audio 1.0 text-to-audio (`audio1.0-text2audio`) and timeline-controlled timing-to-audio (`audio1.0-timing2audio`) for cn and global with tiered credit pricing.
- Added Stability AI Stable Audio 2.5 SFX (`stable-audio-2.5-sfx`) text-to-audio mode with official api_result pricing.
- Reclassified ElevenLabs Text-to-Sound v2 under `elevenlabs-sfx` family with `primaryCapability: sfx`.
- Extended schema enums (`model`, `vendor`, `family`) with `sfx` family type; updated vendor-sources with official SFX API documentation URLs.

## 2026.07.03.1

- Production alignment pass: bulk-refreshed catalog evidence timestamps and bumped catalog version.
- Added ByteDance global Doubao Seed 2.1 Pro/Turbo with official USD pricing; updated family default.
- Added Alibaba cn Wan 2.6 reference-to-video (`wan2.6-r2v`) with official CNY per-second pricing.
- Documented OpenAI Sora 2 / Sora 2 Pro API shutdown schedule (2026-09-24) in model metadata.
- Cleared catalog-audit blockers: approved source URLs in vendor-sources, expanded MiniMax official snapshots, aligned Kling v3.0 Preview cn/global routing semantics.
- Synced pnpm workspace registry, exported OpenAPI contracts, and wrote release manifest `releases/2026.07.03.1.json`.
- Updated README status and standards-alignment documentation to reflect current production posture.

## 2026.07.02.9

- Model freshness audit: added Claude Sonnet 5 (intro pricing) and GPT-5.6 Sol/Terra/Luna
  preview catalog entries; fixed Kuaishou cn Kling family defaultModel.

## 2026.07.02.8

- Added domestic video vendors Vidu (生数科技, Q3 Pro/Turbo, cn+global) and
  PixVerse (爱诗科技/拍我AI, V6/C1 cn + V6 global) with official per-second pricing.

## 2026.07.02.7

- Per-vendor capability audit: aligned vendor.json declarations with on-disk models across
  ByteDance global, Kuaishou, Stability AI, MiniMax cn, and Xiaomi.
- Added Runway Gen-4 Image and Gen-4 Image Turbo with official per-image API pricing.
- Updated vendor-sources and ByteDance global metadata for Doubao Seed + Seedance families.

## 2026.07.02.6

- Expanded video catalog with new vendors Runway (Gen-4 Turbo, Gen-4.5) and Luma AI (Ray 2, Ray 2 Flash).
- Added Zhipu cn CogVideoX-3, Alibaba cn Wan 2.6 image-to-video, and MiniMax global Hailuo 02/2.3/2.3 Fast.
- Registered Runway and Luma AI in catalog enums; full video vendor capability audit documented.

## 2026.07.02.5

- Comprehensive multimodal catalog polish: Gemini 2.5 Flash Image, OpenAI Sora 2/Pro,
  Alibaba Wan 2.6 image/video + text-embedding-v3, Zhipu CogView-4 + Embedding-3,
  MiniMax Image-01 (cn/global), Kling cn CNY pricing activation, and Seed Music GenSong V4.
- Fixed Google Lyria music pricing meters to per-song api_result rates.
- Aligned vendor capabilities, families, and vendor-sources across multimodal vendors.

## 2026.07.02.4

- Polished image generation catalog: added Imagen 4 Fast/Ultra, Gemini Image family fixes,
  xAI Grok Imagine Image/Quality/Video 1.5, Stability Stable Image Core, BFL FLUX.2 Flex,
  and ByteDance cn Seedream 4.0 with official pricing.
- Fixed Gemini 3.1 Flash/Pro Image pricing meters to use image_output_token rates.
- Added OpenAI text-embedding-3-large and aligned vendor-sources catalogVersion.

## 2026.07.02.3

- Expanded voice and speech catalog coverage across OpenAI (GPT-4o mini TTS, TTS-1 HD,
  GPT-4o transcribe diarize, Whisper-1), Google (Gemini 2.5 Flash/Pro TTS preview),
  ElevenLabs (Flash v2.5, Multilingual v2, Scribe v2 Realtime), MiniMax cn
  (Speech 2.8 Turbo/HD, Music 2.6), and ByteDance cn (Seed TTS 2.0 Expressive).
- Added gemini-tts and gpt-tts model families with updated vendor metadata and rankings.

## 2026.07.02.2

- Expanded music catalog coverage with MiniMax Music 2.6/Cover, Mureka V7.5/O1,
  Stability Stable Audio 2.5, and ByteDance cn Seed Music GenSong V4.
- Added MiniMax Speech 2.8 and ByteDance Seed TTS 2.0 standard voice models.
- Registered Mureka vendor in catalog enums and admin vendor presets.

## 2026.07.02.1

- Updated multimodal catalog coverage for video, music, and audio models across
  Kuaishou (Kling v3, Kling v3 Omni), ByteDance (Seedance 2.0 fast + cn region),
  xAI (Grok Imagine Video), and ElevenLabs (Eleven v3 TTS, Music v2, Scribe v2).
- Refreshed ElevenLabs sound effect pricing to official per-minute API rates.
- Regenerated catalog indexes for the new catalog version.

## 2026.06.24.4

- Aligned `vendor-sources.json` supportedModels with on-disk catalog entries for
  Baidu, Google, Moonshot, OpenAI, and Tencent orphan models.
- Added Alibaba Cloud cn `qwen3.7-max` with official CNY pricing and cache rows;
  set it as the cn Qwen default; moved `qwen3.6-max-preview` to supportedModels.

## 2026.06.24.3

- Added Doubao Seed 2.1 (`doubao-seed-2-1-pro-260628`, `doubao-seed-2-1-turbo-260628`)
  from the 2026-06-23 Volcengine Ark release with official CNY pricing and cache rows.
- Set Doubao Seed 2.1 Pro as the cn default model; Seed 2.0 family entries remain listed.

## 2026.06.24.2

- Added StepFun (`stepfun/cn`) with `step-3.5-flash`, `step-3.5-flash-2603`, and
  `step-3.7-flash` from official StepFun pricing docs.
- Expanded ByteDance Doubao (豆包) coverage with `doubao-seed-2-0-code-preview-260215`
  in cn/global and `doubao-seed-2-0-pro-260215` in global.
- Added Zhipu `glm-5.2` as the new GLM default model.
- Marked Anthropic `claude-fable-5` as catalog-only after the 2026-06-12 suspension
  and added `claude-mythos-5` as a hidden catalog-only entry.

## 2026.06.24.1

- Re-verified official pricing for all 18 vendor regions against current vendor
  documentation; confirmed no unit price changes were required for the synced
  release scope.
- Refreshed source evidence timestamps across model, pricing, vendor, and
  official snapshot manifests.
- Regenerated catalog indexes and release metadata for the new catalog version.

## 2026.05.07.1

- Introduced the vendor-scoped `sdkwork-models` catalog layout.
- Added canonical billing meters, model facts, pricing files, ranking snapshots,
  validation tools, freshness policy, release metadata, and SDK entrypoints.
