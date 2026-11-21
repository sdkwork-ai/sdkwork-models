-- sdkwork:migration
-- id: 0001_organization_id_not_null
-- engine: postgres
-- module: sdkwork-models
-- purpose: Enforce organization_id NOT NULL DEFAULT on all tables in the
--   consolidated baseline. NULL rows (pre-standard data anomalies) are
--   backfilled with the platform sentinel before NOT NULL is set, and
--   NOT NULL columns without an explicit default receive the sentinel
--   default, keeping existing deployments consistent with fresh baseline
--   installs.
-- reversible: false
-- rollback: forward-fix (sentinel backfill is the canonical fix; NULL
--   organization rows are data anomalies)
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE ai_model_vendor ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_vendor SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_vendor ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_vendor ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_modality ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_modality SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_modality ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_modality ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_api_endpoint ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_api_endpoint SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_api_endpoint ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_api_endpoint ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_vendor_modality ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_vendor_modality SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_vendor_modality ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_vendor_modality ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_vendor_api_endpoint ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_vendor_api_endpoint SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_vendor_api_endpoint ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_vendor_api_endpoint ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_modality_api_endpoint ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_modality_api_endpoint SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_modality_api_endpoint ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_modality_api_endpoint ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_modality ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_modality SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_modality ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_modality ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_api_endpoint ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_api_endpoint SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_api_endpoint ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_api_endpoint ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_resource ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_resource SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_resource ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_resource ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_resource_group ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_resource_group SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_resource_group ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_resource_group ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_resource_group_item ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_resource_group_item SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_resource_group_item ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_resource_group_item ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_family ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_family SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_family ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_family ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_capability ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_capability SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_capability ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_capability ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_catalog_source ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_catalog_source SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_catalog_source ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_catalog_source ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_catalog_sync_run ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_catalog_sync_run SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_catalog_sync_run ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_catalog_sync_run ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_billing_meter ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_billing_meter SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_billing_meter ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_billing_meter ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_pricing ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_pricing SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_pricing ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_pricing ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_rank_snapshot ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_rank_snapshot SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_rank_snapshot ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_rank_snapshot ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_voice ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_voice SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_voice ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_voice ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_voice_binding ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_voice_binding SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_voice_binding ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_voice_binding ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE ai_model_video_profile ADD COLUMN IF NOT EXISTS organization_id BIGINT NOT NULL DEFAULT 0;
UPDATE ai_model_video_profile SET organization_id = 0 WHERE organization_id IS NULL;
ALTER TABLE ai_model_video_profile ALTER COLUMN organization_id SET DEFAULT 0;
ALTER TABLE ai_model_video_profile ALTER COLUMN organization_id SET NOT NULL;

COMMIT;
