# SDKWork Models — Standards Alignment

Status: active  
Updated: 2026-07-31
Authority: `../sdkwork-specs/README.md`

This document records the **verified** alignment posture for `sdkwork-models`. It must match evidence from `pnpm run verify`, `cargo test --workspace`, and topology validation.

## Repository Identity

| Item | Value |
| --- | --- |
| Application key | `sdkwork-models` |
| Component | `@sdkwork/models-catalog` |
| Domain / capability | `intelligence` / `catalog` |
| Archetype | `composed-product-module` with standalone HTTP server |
| PC root | `apps/sdkwork-models-pc/` |
| HTTP composition | `crates/sdkwork-api-models-standalone-gateway/` |
| Standalone binary | `sdkwork-api-models-standalone-gateway` |
| Database module | `database/` (`sdkwork.database.module`) |

## Framework Integration

| Framework | Status | Evidence |
| --- | --- | --- |
| `sdkwork-specs` | Aligned | `AGENTS.md`, `specs/`, envelope check in `pnpm run check` |
| `sdkwork-web-framework` | Aligned | Route manifests + `WebFrameworkLayer`; handlers use `WebRequestContext` + `finish_success` / `problem_for` |
| `sdkwork-database` | Aligned | `sdkwork-models-database-host` bootstraps lifecycle from `database.manifest.json` |
| `sdkwork-iam-web-adapter` | Aligned | `web_bootstrap.rs` in backend and app route crates |
| `sdkwork-utils` | Aligned | Rust handlers use `SdkWorkApiResponse`; tools/PC use `@sdkwork/utils` |
| `sdkwork-sdk-generator` | Aligned | `pnpm run api:materialize:openapi` + `sdk:generate` |
| `sdkwork-discovery` | N/A | No RPC services in this repository |
| `sdkwork-drive` | N/A | No file-upload surfaces in this product |

## HTTP API Envelope

| Requirement | Status | Evidence |
| --- | --- | --- |
| Success `{ code: 0, data, traceId }` | Aligned | `catalog-service/src/api/response.rs` (`ApiResponse`, `finish_success`) |
| Errors `application/problem+json` | Aligned | `problem_for(SdkWorkResultCode, …)` + `ProblemDetail`; request-scoped `traceId` |
| OpenAPI authority | Aligned | `apis/*/intelligence/openapi.json` migrated via `migrateOpenApiDocument` |
| Envelope gate | Aligned | `check-api-response-envelope.mjs` in `pnpm run check` |

## Production Readiness

| Area | Status | Notes |
| --- | --- | --- |
| Catalog JSON authority | Aligned | Catalog version 2026.07.05.3; voice + video generation profile catalogs validated |
| TTS voice catalog | Aligned | `specs/voice-catalog.spec.json`; DB import sync; app + backend read APIs (`voices.list`, `modelVoices.list`) with `intelligence.models.read` on backend |
| Video generation profiles | Aligned | `specs/video-generation-profile.spec.json`; full coverage for all `primaryCapability: video` models; DB import sync (`ai_model_video_profile`); SDK helpers all languages; app + backend read APIs |
| Voice SDK parity | Aligned | `listVoices`, `listVoicesForModel`, `listModelsForVoice` in Rust/TS/Python/Java/Flutter; Rust + TS + Python tests cover bundled voice catalog |
| Video profile SDK parity | Aligned | `listVideoProfiles`, `listVideoProfilesForModel`, `findVideoProfile` in Rust/TS/Python/Java/Flutter; Rust + TS + Python tests cover bundled video profiles |
| Catalog sync observability | Aligned | `models.sync` returns `voiceCount`, `voiceBindingCount`, and `videoProfileCount` in `ModelCatalogSyncResult` |
| Backend admin API | Aligned | IAM permissions on all backend routes; SDKWork offset envelopes; SQL-backed pagination with default `20` and max `200` |
| Resource-group capacity | Aligned | Manual groups are capped at `512` members; page hydration reads only resource codes selected by the current page |
| Member mutation transaction | Aligned | PostgreSQL group-row lock / SQLite write lock; membership, audit, and routing config change commit atomically; repeated delete emits no false event |
| App read API | Aligned | App catalog list routes require dual-token authentication; voice list uses `SdkWorkPageData` list envelope |
| Database module | Aligned | PostgreSQL is the authoritative server engine; SQLite remains a local adapter; baseline DDL includes voice and video profile tables |
| Readiness probe | Aligned | `/healthz` and `/readyz` are infra probes (not business API envelope); `/readyz` probes DB |
| Gateway production template | Aligned | Restricted CORS via `SDKWORK_MODELS_CORS_ALLOWED_ORIGINS`; cloud gateway configs validated |
| CI dependency closure | Aligned | `.github/workflows/verify.yml` checks out platform crates |
| Observability | Aligned | Request-scoped `traceId` on success and error via `WebRequestContext`; structured tracing on admin model list |
| Supply-chain SBOM | Deferred | `security.sbomRequired: false` in `agent.workflow.json` until release gate enables evidence |
| PC browser catalog | Aligned | `apps/sdkwork-models-pc/` standalone catalog explorer via `@sdkwork/models` |
| PC admin UI | Composed | Admin packages (`sdkwork-models-pc-admin-*`) mount in Cloud Router host per `APP_PC_ARCHITECTURE_SPEC` |
| PostgreSQL contention evidence | Open | Requires an isolated `SDKWORK_DATABASE_URL` to retain concurrent writer, retryable SQLSTATE, deadlock/serialization, and pool saturation evidence |

## Composed Host Integration

Cloud Router mounts catalog routes locally. Admin PC packages (`sdkwork-models-pc-admin-*`) are **composed workspaces** requiring `@sdkwork/cloudrouter-pc-commons` at runtime.

Standalone installs use `.npmrc` (`auto-install-peers=false`) so peer packages resolve from the composed host.

## Verification

```powershell
pnpm install
pnpm run api:check:route-manifest
pnpm run db:validate
pnpm run topology:validate
cargo check -p sdkwork-api-models-standalone-gateway
cargo test -p sdkwork-models-catalog-repository-sqlx
node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
pnpm run verify
```

Expected: route manifests match handlers, database module validates, Rust workspace compiles, repository tests pass (`21` passed and the isolated PostgreSQL numeric test may remain ignored without `SDKWORK_DATABASE_URL`), catalog checks pass, API and pagination gates pass, and `pnpm run verify` exits 0. Commercial release additionally requires the open PostgreSQL contention evidence above.
