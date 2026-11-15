-- sdkwork:migration
-- id: 0003_ai_model_frontend_config
-- engine: postgres
-- module: sdkwork-models
-- purpose: Reverse 0003: drop the frontend model configuration tables. Only
--   intended for rollback of the expand migration; fresh baselines keep them.
-- reversible: true
-- rollback: up-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

DROP INDEX IF EXISTS idx_ai_model_frontend_revision_published;
DROP INDEX IF EXISTS uk_ai_model_frontend_revision_number;
DROP INDEX IF EXISTS uk_ai_model_frontend_revision_uuid;
DROP TABLE IF EXISTS ai_model_frontend_revision;

DROP INDEX IF EXISTS idx_ai_model_frontend_field_list;
DROP INDEX IF EXISTS uk_ai_model_frontend_field_key;
DROP INDEX IF EXISTS uk_ai_model_frontend_field_uuid;
DROP TABLE IF EXISTS ai_model_frontend_field;

DROP INDEX IF EXISTS idx_ai_model_frontend_channel_model_model;
DROP INDEX IF EXISTS idx_ai_model_frontend_channel_model_list;
DROP INDEX IF EXISTS uk_ai_model_frontend_channel_model_key;
DROP INDEX IF EXISTS uk_ai_model_frontend_channel_model_uuid;
DROP TABLE IF EXISTS ai_model_frontend_channel_model;

DROP INDEX IF EXISTS idx_ai_model_frontend_channel_updated;
DROP INDEX IF EXISTS idx_ai_model_frontend_channel_list;
DROP INDEX IF EXISTS uk_ai_model_frontend_channel_code;
DROP INDEX IF EXISTS uk_ai_model_frontend_channel_uuid;
DROP TABLE IF EXISTS ai_model_frontend_channel;

COMMIT;
