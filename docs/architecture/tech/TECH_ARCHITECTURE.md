# SDKWork Models Technical Architecture
Specs: ARCHITECTURE_DECISION_SPEC.md, DOCUMENTATION_SPEC.md

Status: active  
Owner: SDKWork maintainers  
Updated: 2026-07-31

## 1. Overview

`sdkwork-models` is a **composed product module** that can run standalone or mount into Cloud Router. It owns intelligence catalog domain data, HTTP route crates, database module, and SDK families.

## 2. Layering

```
models/ releases/ schemas/     → Catalog JSON authority
tools/                         → Validation, index, OpenAPI export
crates/
  contract-service             → Store traits, domain errors
  catalog-repository-sqlx      → PostgreSQL server repository; SQLite local adapter
  catalog-service              → HTTP handlers, application services
  database-bootstrap           → Embedded DDL authority
  database-host                → sdkwork-database lifecycle bootstrap
  standalone-gateway             → Route composition + standalone binary
  router-catalog-{app,backend}-api → Route manifests + web framework
sdks/                          → Generated @sdkwork/models-* SDK families
apps/sdkwork-models-pc/        → Catalog browser + composed admin libraries
```

## 3. Runtime Topology

| Mode | Ingress | Notes |
| --- | --- | --- |
| Composed | Cloud Router backend/app routers | Primary production path |
| Standalone | `sdkwork-api-models-standalone-gateway` | Declared in `specs/topology.spec.json` |
| Cloud | `sdkwork-api-cloud-gateway` + app upstream | `configs/sdkwork-api-cloud-gateway.models.*.toml` |

## 4. Security

- Backend routes: `with_required_permission(...)` on every operation
- App catalog routes require a complete dual-token IAM session (`AuthToken` and `AccessToken` in the same security requirement). Anonymous catalog publication, if introduced later, must use a separately reviewed open-api authority rather than weakening app-api authentication.
- Backend voice and video profile routes require `intelligence.models.read`
- `/readyz`: fails closed; no internal error strings in response body
- Production gateway: restricted CORS, upstream readiness checks

## 5. Data

- L2 database module: `database/database.manifest.json`
- PostgreSQL is the authoritative server engine declared by the database module.
  SQLite is an explicit client-local/development adapter and is not a production
  cluster authority or a second server database contract.
- Catalog sync from JSON via admin `models.sync` (imports models, pricing, voices, voice bindings, and video generation profiles)
- `sdkwork-models-catalog-service::PriceService` is the reusable runtime price
  entry point. `ResourceDefinition` supplies vendor/provider/account, region,
  catalog/API/product/operation, meter, quantity, dimensions, and event time;
  `PriceResolution` returns explicit billability, rate identity, failure/audit
  evidence, and strategy-produced `BillingStructure` amounts.
- Token, API-call, generated-image quantity, duration, flat-fee, and general
  unit-quantity calculations are independent `BillingStrategy` components.
  Consumers may register additional strategies without adding formulas to
  routing, transport, settlement, or persistence modules.
- App catalog read uses in-memory `ModelCatalog` JSON snapshot in standalone mode
- TTS voice catalog: `voices.json` + `model-voices/` per vendor region; persisted to `ai_model_voice` / `ai_model_voice_binding`
- Video generation profiles: `model-video-profiles/{modelId}.json` per video model; persisted to `ai_model_video_profile`
- Front-end model configuration: `ai_model_frontend_channel`,
  `ai_model_frontend_channel_model`, `ai_model_frontend_field`, and
  `ai_model_frontend_revision` own which catalog models each product surface
  (LLM, video, image, audio, music, …) presents, its order, badge, field
  visibility, default model, enabled flag, and published revision. Catalog facts
  remain in `ai_model` / `ai_model_capability`; presentation never writes those
  rows. The field-vs-table decision is
  [ADR-20261115](../../decisions/ADR-20261115-frontend-model-config-tables.md).
- AI resource, resource-group, and group-member list paths push filtering, stable
  ordering, counting, and `LIMIT`/`OFFSET` into SQL. Resource member hydration is
  restricted to resource codes in the selected page, so page reads do not grow
  with the complete tenant catalog. API pagination defaults to `20` and is capped
  at `200`, following `PAGINATION_SPEC.md`.
- Manual resource groups support at most `512` persisted members. The admin UI
  assigns or removes one member through the generated Models backend SDK instead
  of aggregating the complete membership state. Bounded member
  arrays remain available on create/update for contract-compatible initialization.
- PostgreSQL locks the group row for member upsert/delete. Both SQL adapters commit
  membership state, `ops_audit_log`, and `ai_routing_config_change` atomically. A
  repeated delete is idempotent and does not emit audit or routing-change evidence
  for a mutation that did not occur. Routing version records retain tenant and
  global scope so cache invalidation can observe committed configuration changes.

## 6. Capacity And Failure Boundaries

- Request bodies, search text, field lengths, list page size, and resource-group
  membership are validated before persistence. P0 list and member management paths
  do not use unbounded `listAll`, full-table collect, or browser-side slice paging.
- PostgreSQL group-row locking serializes conflicting member changes for one group
  while allowing unrelated groups to proceed independently. Transaction failures
  roll back membership, audit, and routing events together.
- SQLite uses a database write lock and real transaction tests to preserve local
  semantic parity, but it is not used as evidence for multi-node availability.
- Commercial release still requires an isolated PostgreSQL test database to retain
  repeatable evidence for concurrent writers, retryable SQLSTATE classification,
  deadlock/serialization retry budgets, and pool saturation behavior.

## 7. Verification

```powershell
pnpm run verify
cargo check -p sdkwork-api-models-standalone-gateway
cargo test -p sdkwork-models-catalog-repository-sqlx
pnpm run topology:validate
node ../sdkwork-specs/tools/check-pagination.mjs --workspace .
```

See `docs/standards-alignment.md` for the current alignment matrix.

## 8. Architecture Decision Index

- [TECH-client-api-compatibility-schema.md](TECH-client-api-compatibility-schema.md)
- [TECH-client-api-plugins-standard.md](TECH-client-api-plugins-standard.md)
- [TECH-client-api-schema-simple.md](TECH-client-api-schema-simple.md)
- [TECH-client-api-standard-v2.md](TECH-client-api-standard-v2.md)
- [TECH-converter-naming-standard.md](TECH-converter-naming-standard.md)
- [TECH-root-layout.md](TECH-root-layout.md)
- [TECH-standards-alignment.md](TECH-standards-alignment.md)
- [TECH-vendor-model-architecture.md](TECH-vendor-model-architecture.md)
