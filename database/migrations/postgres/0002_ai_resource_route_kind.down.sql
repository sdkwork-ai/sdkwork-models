-- sdkwork:migration
-- id: 0002_ai_resource_route_kind
-- engine: postgres
-- module: sdkwork-models
-- purpose: Reverse 0002: drop the route_kind index, check constraint, and column.
--   Only intended for rollback of the expand migration; fresh baselines keep the
--   column.
-- reversible: true
-- rollback: up-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 30s

BEGIN;

DROP INDEX IF EXISTS idx_ai_resource_route_kind_status;

ALTER TABLE ai_resource DROP CONSTRAINT IF EXISTS ck_ai_resource_route_kind;

ALTER TABLE ai_resource DROP COLUMN IF EXISTS route_kind;

COMMIT;
