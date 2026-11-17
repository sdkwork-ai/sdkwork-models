# Catalog seeds

Catalog dictionary seed bundles are imported through `sdkwork-models-database-host` and Cloud Router installer flows. Locale-specific seed payloads will live under `seeds/locales/{locale}/` when boot-time seeding is enabled.

`seeds/common/001_bootstrap.sql` is the minimal bootstrap seed.
`seeds/common/002_ai_model_frontend_defaults.sql` persists the front-end model
configuration initialization document (one channel per modality, each with its
default model, initialization models, and visible fields) behind
`/admin/model/frontend`. It is idempotent and never overwrites an
operator-authored surface.
