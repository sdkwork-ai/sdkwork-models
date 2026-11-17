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
2. **Migrations** — `database/migrations/{engine}/` carries post-baseline incremental changes
   (`0001_organization_id_not_null`, `0002_ai_resource_route_kind`,
   `0003_ai_model_frontend_config`); a fresh install applies the baseline and then
   every ordered migration.
3. **Seeds** — `database/seeds/common/002_ai_model_frontend_defaults.sql` persists the
   front-end channel initialization document for the admin console tenant
   (`100001` / `0`): one channel per modality (`llm`, `video`, `image`, `audio`,
   `music`, `sound_effect`, `embedding`), each with its default model,
   initialization model list, and field visibility. It is idempotent
   (`ON CONFLICT DO NOTHING`) and never rewrites an operator-authored revision.
   The `db:seed` command and the Cloud Router installer
   (`cloudrouterctl install|upgrade|ensure`) apply it through the seed
   lifecycle; the runtime models assembly only migrates. The backend keeps an
   equivalent in-memory fallback for tenants that have no rows yet.
4. **Drift** — run `pnpm db:drift:check` before release.

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
