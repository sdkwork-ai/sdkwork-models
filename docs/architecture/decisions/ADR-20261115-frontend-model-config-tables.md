# ADR-20261115: Frontend model configuration tables

Status: accepted
Date: 2026-11-15
Owners: sdkwork-models-platform

## Context

The product surfaces that present models (LLM chat, video generation, image
generation, audio, music, and any later surface) need deployment-owned
configuration: which models a surface offers, in what order, which fields it
shows, which model is the default, and whether the surface is enabled at all.
Today that intent is spread across client constants and, in a previous draft
change, a free-form `modelCatalog` document in Cloud Router's
`ops_config_snapshot`. Both are wrong owners:

- Client constants cannot be changed by an operator and drift per release.
- `ops_config_snapshot` is Cloud Router's operational settings store; it has no
  model foreign keys, no per-channel rows, no uniqueness rules, and no catalog
  revision. It cannot answer "which models does the video surface expose?"

The catalog facts themselves already live in `ai_model`,
`ai_model_capability`, `ai_model_vendor`, `ai_model_pricing`, and the voice and
video-profile tables. Those rows are imported from the JSON catalog by
`models.sync` and must stay import-owned.

## Decision

Add four new tables in the `sdkwork-models` database module (prefix `ai_`), and
do **not** add presentation columns to `ai_model` or `ai_model_capability`:

1. `ai_model_frontend_channel` — one row per front-end surface
   (`llm`, `video`, `image`, `audio`, `music`, `sound_effect`, `embedding`, …):
   display name, i18n label, enabled flag, sort order, default model,
   `capability_codes` binding to catalog capabilities, free-form `config` JSONB,
   `schema_version`, and a per-channel `revision`.
2. `ai_model_frontend_channel_model` — the exposure join: channel, catalog key,
   optional model id, visible/featured flags, sort order, badge, per-entry
   `config` JSONB. A model may appear in several channels with different
   ordering, badges, and visibility.
3. `ai_model_frontend_field` — per-channel field visibility and order
   (`price`, `context`, `modalities`, `capabilities`, `latency`, …) with a
   per-field `config` JSONB, so a surface can add a field without a DDL change.
4. `ai_model_frontend_revision` — one row per published configuration revision
   (content hash, channel count, published time, source), so app clients can
   fetch an immutable snapshot with an ETag and delta-sync later.

All four follow the module's standard envelope: `id`, `uuid`, `tenant_id`,
`organization_id`, `data_scope`, `status`, timestamps, optimistic `version`,
soft delete, and `metadata` JSONB. Uniqueness is scoped to the live row:
`(tenant_id, organization_id, channel_code)` for channels,
`(tenant_id, organization_id, channel_id, catalog_key)` for exposure, and
`(tenant_id, organization_id, channel_id, field_key)` for fields.

## Why not add fields to `ai_model`

- **Ownership**: `ai_model` is a catalog import target. Presentation columns
  would have to be excluded from every import/update or they would be silently
  reset by a catalog sync.
- **Cardinality**: exposure is many-to-many. A multimodal model belongs to the
  LLM, image, and video surfaces with independent order, badge, and visibility;
  a single set of columns cannot express three independent answers.
- **Sparsity and coupling**: per-surface columns (`llm_enabled`,
  `video_enabled`, …) grow with every new surface and couple all surfaces into
  one wide row.
- **Field flexibility**: which fields a surface shows is open-ended and changes
  far more often than catalog facts. JSONB per channel/field absorbs that
  without DDL churn.

## Why not reuse `ai_model_capability` or `ai_resource`

- `ai_model_capability` stores *what a model can do* (capability, modality,
  parameter schema). A front-end channel is *where and how the product shows
  it*; mixing the two makes a capability row answer two different questions and
  breaks the `(model, capability_code, modality, parameter_name)` identity.
- `ai_resource` models callable AI resources/agents, not presentation
  configuration. Reusing it would overload its `route_kind` and membership
  semantics and make front-end settings appear as callable endpoints.

## API surface

The models backend exposes one document-oriented read and one transactional
replace, which keeps the four tables internally consistent and gives clients a
single revision:

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/backend/v3/api/ai/frontend_config` | Every channel with its fields and exposed models, plus `revision` |
| PUT | `/backend/v3/api/ai/frontend_config` | Replace the complete configuration in one transaction; bumps `revision` and writes `ai_model_frontend_revision` |
| GET | `/app/v3/api/ai/frontend_config` | Enabled channels, visible models and fields, plus `revision`/`ETag` |

The replace payload is the same shape the read returns:

```json
{
  "revision": 12,
  "channels": [
    {
      "channelCode": "llm",
      "displayName": "LLM",
      "enabled": true,
      "sortOrder": 10,
      "defaultCatalogKey": "deepseek/deepseek-flash",
      "capabilityCodes": ["chat"],
      "config": { "pageSize": 100 },
      "schemaVersion": "1.0.0",
      "fields": [{ "fieldKey": "price", "visible": true, "sortOrder": 10, "config": {} }],
      "models": [{ "catalogKey": "deepseek/deepseek-flash", "visible": true, "featured": true, "sortOrder": 10, "badge": null, "config": {} }]
    }
  ]
}
```

Rules:

- `channelCode` is unique per tenant; `catalogKey` is unique per channel; the
  request is rejected when a model key is duplicated or a field key repeats.
- A replace validates the whole payload before opening the transaction, then
  soft-deletes the previous live rows, inserts the new set, and appends one
  `ai_model_frontend_revision` row. A failed request leaves the previous
  revision serving.
- `GET` returns `ETag: "fmc-<revision>-<content_sha256-prefix>"`; the app read
  supports `If-None-Match`.
- Channel defaults (`llm`, `video`, `image`, `audio`, `music`, `sound_effect`,
  `embedding`) are returned by the service when a tenant has no rows yet, and
  `database/seeds/common/002_ai_model_frontend_defaults.sql` persists the same
  document (one channel per modality with its default model, initialization
  models, and fields) through the seed lifecycle on install/upgrade. The first
  successful replace persists operator edits.
- `ai_model_frontend_channel_model.model_id` is an int64 and crosses the HTTP
  boundary as a decimal string (`modelId`) per `API_SPEC.md` section 13.6; the
  admin client never converts it to a JavaScript number.

The admin Models page adds one menu item, `模型前端配置` (`/admin/model/frontend`),
with a tab per channel. The page reads and writes this document through the
models backend SDK; it never talks to Cloud Router's site settings.

## Consequences

- The admin Models page gains a `模型前端配置` menu item backed by the models
  backend API; each channel is a tab (LLM / 视频 / 图片 / 音频 / 音乐 / …) with
  enable, order, default model, field visibility, and model exposure.
- The app catalog read can serve a versioned frontend snapshot from
  `ai_model_frontend_channel` + `ai_model_frontend_channel_model` +
  `ai_model_frontend_field`, with `ai_model_frontend_revision` supplying the
  ETag/revision.
- Catalog import remains the only writer of catalog facts; front-end
  configuration is the only writer of these four tables. The two are joined by
  `catalog_key`, never by copying model facts.
- Existing databases converge through migration `0003_ai_model_frontend_config`;
  greenfield databases receive the tables from the baseline DDL.

## Implementation status

- Database: baseline DDL, migration `0003_ai_model_frontend_config`, schema
  contract 1.2.0, table registry.
- Initialization: `database/seeds/common/002_ai_model_frontend_defaults.sql`
  persists the per-modality channels, default/initialization models, and fields
  for the console tenant (`100001` / `0`). It is idempotent, never overwrites an
  operator-authored surface, and is applied by `db:seed` and the Cloud Router
  installer (`install|upgrade|ensure`). The backend keeps an equivalent
  in-memory fallback for tenants that have no rows yet.
- Backend: `FrontendConfigStore` port, `PostgresFrontendConfigStore`, the
  `frontend_config` handlers, backend/app route descriptors and manifests, and
  mounts in the models assembly and the Cloud Router app/backend gateways. The
  admin read/write validates that a channel's default model is one of its
  visible exposed models, and the app read normalizes a legacy dangling default
  to the first visible model.
- Admin: `/admin/model/frontend` (`模型前端配置`) in the Models menu, including a
  catalog model picker for the add action and "import from catalog" (filtered by
  the channel's capability binding, paginated over the whole catalog).
- Admin header action `更新模型数据`: calls the models backend `models.sync`
  (the Rust-owned `sdkwork-models` catalog import), reloads the catalog, then
  aligns every channel with the refreshed catalog. Alignment is pure and
  client-side: a key the catalog no longer describes is disabled and loses its
  stale int64 id, a bound id is refreshed from the catalog, duplicate rows are
  collapsed, and a dangling default moves to the first visible model. Newly
  published models are reported (never exposed automatically) and can be added
  disabled in one action; bulk enable/disable only touches models the catalog
  still describes. The aligned document is persisted as its own revision with
  `source: "model_sync_alignment"`, so the app read and product clients observe
  the same surface the operator reviewed.
- Admin replace accepts an optional `expectedRevision`. The PostgreSQL store
  serializes concurrent replaces on the tenant's live channel rows and refuses a
  stale base revision with HTTP 409 (`40901`); a racing first publication is
  caught by the unique `(tenant_id, organization_id, revision)` index and also
  surfaces as 409. The admin client reloads on that conflict instead of
  overwriting another operator's revision.
- Both the admin and app reads of one revision are served from a bounded,
  process-local revision cache in `PostgresFrontendConfigStore`: one indexed
  revision lookup decides whether the cached three-table document is still
  current, and the replace publishes the new document into the cache. A cached
  entry is keyed by tenant and revision, so a write in another process is
  invalidated by observation, never by a stale pointer.
- Admin read emits `ETag: "fmc-<revision>"` and honors `If-None-Match`, matching
  the app read; both send `Cache-Control: no-cache`.
- The app read ships `ETag`/`If-None-Match` over the published revision and
  returns only enabled channels, visible models, and visible fields. It also
  filters exposed models to rows the catalog still serves (live, active,
  tenant-or-global), so a retired or disabled catalog model never reaches a
  product client even before an operator reopens the admin page; the admin read
  deliberately keeps stale rows visible for repair (`LoadFrontendConfigQuery.
  catalog_backed_only`).
- Both reads use a membership-aware entity tag (`fmc-<revision>-<hash>`): the
  revision alone cannot validate the app body, because the catalog-backed filter
  can change the served set without a front-end revision bump. The app
  projection's cache key additionally carries a catalog freshness stamp
  (`MAX(updated_at)` over the visible catalog rows), so a catalog import, model
  disable, or removal misses the cached app projection while the admin
  projection stays cached by revision alone.
- Generated OpenAPI/SDK exposure for the two operations is still pending; the
  admin page talks to the route through the generated client's HTTP seam in the
  meantime.

## Alternatives rejected

- **One `settings` JSON document** (including `ops_config_snapshot`): no
  relational integrity, no per-channel uniqueness, no efficient per-channel
  reads, and it mixes operational settings with catalog presentation.
- **Columns on `ai_model`**: see above.
- **Client-side configuration only**: cannot be changed by an operator and
  cannot be versioned per deployment.
- **Separate database module**: the configuration is meaningless without the
  catalog keys and would create a cross-module join and a second migration
  lifecycle for one concern; a new table family inside the owning module has
  higher cohesion and a lower deployment cost.
