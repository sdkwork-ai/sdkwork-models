-- sdkwork:migration
-- id: 0002_ai_resource_route_kind
-- engine: postgres
-- module: sdkwork-models
-- purpose: Backfill `ai_resource.route_kind` (model|api marker) on databases that
--   were bootstrapped before the consolidated baseline introduced the column.
--   Fresh installs already receive the column from the baseline; this migration
--   converges existing schemas so the Cloud Router catalog snapshot query
--   (`resource.route_kind`) can load without a missing-column error.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

ALTER TABLE ai_resource ADD COLUMN IF NOT EXISTS route_kind VARCHAR(16) NOT NULL DEFAULT 'api';

-- Safety backfill: if any pre-existing row carried an explicit NULL, normalize
-- it to the baseline default before the check constraint is enforced.
UPDATE ai_resource SET route_kind = 'api' WHERE route_kind IS NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'ck_ai_resource_route_kind'
          AND conrelid = 'ai_resource'::regclass
    ) THEN
        ALTER TABLE ai_resource
            ADD CONSTRAINT ck_ai_resource_route_kind
            CHECK (route_kind IN ('model', 'api'));
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS idx_ai_resource_route_kind_status
    ON ai_resource (tenant_id, organization_id, route_kind, status, id);

COMMIT;
