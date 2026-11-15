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
  the first successful replace persists them.

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
- Backend: `FrontendConfigStore` port, `PostgresFrontendConfigStore`, the
  `frontend_config` handlers, backend/app route descriptors and manifests, and
  mounts in the models assembly and the Cloud Router app/backend gateways.
- Admin: `/admin/model/frontend` (`模型前端配置`) in the Models menu, including
  "import from catalog" filtered by the channel's capability binding.
- The app read ships `ETag`/`If-None-Match` over the published revision and
  returns only enabled channels, visible models, and visible fields.
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
