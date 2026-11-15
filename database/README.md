# SDKWork Models Catalog Database Module

Canonical lifecycle assets for `sdkwork-models` per `DATABASE_FRAMEWORK_SPEC.md`.

- moduleId: `sdkwork-models`
- serviceCode: `SDKWORK_MODELS`
- tablePrefix: `ai_`

## Owned Tables

Catalog dictionary and public model facts imported from the JSON catalog:

- `ai_model_vendor`, `ai_model_family`, `ai_model`, `ai_model_capability`
- `ai_modality`, `ai_api_endpoint`, vendor/model modality and endpoint join tables
- `ai_resource`, `ai_resource_group`, `ai_resource_group_item`
- `ai_billing_meter`, `ai_model_pricing` (reference prices)
- `ai_model_catalog_source`, `ai_model_catalog_sync_run`, `ai_model_rank_snapshot`
- `ai_model_voice`, `ai_model_voice_binding` (TTS speaker catalog and model bindings)
- `ai_model_video_profile` (video generation profile catalog per video model)
- `ai_model_frontend_channel`, `ai_model_frontend_channel_model`,
  `ai_model_frontend_field`, `ai_model_frontend_revision` (front-end surface
  configuration: which models each product surface presents, in which order,
  with which fields, and the published revision; see
  [ADR-20261115](../docs/architecture/decisions/ADR-20261115-frontend-model-config-tables.md))

Cloud Router retains tenant routing overlays (`ai_model_mapping_*`), gateway channels, and tenant pricing plans.

## Initialization state

This module is in **initialization state** for greenfield deployments:

1. **Baseline** — `database/ddl/baseline/{engine}/0001_sdkwork-models_baseline.sql` contains the full DDL snapshot.
2. **Migrations** — `database/migrations/{engine}/` is reserved for post-GA incremental schema changes only. It is intentionally empty at initialization.
3. **Drift** — run `pnpm db:drift:check` before release.

## Commands

```bash
pnpm run db:validate
pnpm run db:materialize:contract
pnpm run db:plan
pnpm run db:init
pnpm run db:migrate
pnpm run db:seed
pnpm run db:status
pnpm run db:drift:check
```
