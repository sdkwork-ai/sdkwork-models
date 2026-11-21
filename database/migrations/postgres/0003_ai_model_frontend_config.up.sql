-- sdkwork:migration
-- id: 0003_ai_model_frontend_config
-- engine: postgres
-- module: sdkwork-models
-- purpose: Add the frontend model configuration tables (channel, channel model
--   exposure, field visibility, published revision). Greenfield installs get
--   them from the consolidated baseline; this migration converges existing
--   databases. The field-vs-table decision is
--   docs/architecture/decisions/ADR-20261115-frontend-model-config-tables.md.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: lightweight
-- lock_timeout: 2s
-- statement_timeout: 60s

BEGIN;

CREATE TABLE IF NOT EXISTS ai_model_frontend_channel (
    id BIGINT NOT NULL PRIMARY KEY,
    uuid VARCHAR(64) NOT NULL,
    tenant_id BIGINT NOT NULL DEFAULT 0,
    organization_id BIGINT NOT NULL DEFAULT 0,
    data_scope INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    version BIGINT NOT NULL DEFAULT 0,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    channel_code VARCHAR(64) NOT NULL,
    display_name VARCHAR(128) NOT NULL,
    display_name_i18n JSONB NOT NULL DEFAULT '{}'::jsonb,
    description VARCHAR(512),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    default_catalog_key VARCHAR(256),
    capability_codes JSONB NOT NULL DEFAULT '[]'::jsonb,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    schema_version VARCHAR(32) NOT NULL DEFAULT '1.0.0',
    revision BIGINT NOT NULL DEFAULT 0,
    last_published_at TIMESTAMPTZ
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_channel_uuid ON ai_model_frontend_channel (uuid);
CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_channel_code ON ai_model_frontend_channel (tenant_id, organization_id, channel_code) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_ai_model_frontend_channel_list ON ai_model_frontend_channel (tenant_id, organization_id, status, enabled, sort_order, id);
CREATE INDEX IF NOT EXISTS idx_ai_model_frontend_channel_updated ON ai_model_frontend_channel (tenant_id, organization_id, status, updated_at, id);

CREATE TABLE IF NOT EXISTS ai_model_frontend_channel_model (
    id BIGINT NOT NULL PRIMARY KEY,
    uuid VARCHAR(64) NOT NULL,
    tenant_id BIGINT NOT NULL DEFAULT 0,
    organization_id BIGINT NOT NULL DEFAULT 0,
    data_scope INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    version BIGINT NOT NULL DEFAULT 0,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    channel_id BIGINT NOT NULL,
    channel_code VARCHAR(64) NOT NULL,
    catalog_key VARCHAR(256) NOT NULL,
    model_id BIGINT,
    visible BOOLEAN NOT NULL DEFAULT TRUE,
    featured BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    badge VARCHAR(64),
    config JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_channel_model_uuid ON ai_model_frontend_channel_model (uuid);
CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_channel_model_key ON ai_model_frontend_channel_model (tenant_id, organization_id, channel_id, catalog_key) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_ai_model_frontend_channel_model_list ON ai_model_frontend_channel_model (tenant_id, organization_id, channel_code, status, visible, sort_order, id);
CREATE INDEX IF NOT EXISTS idx_ai_model_frontend_channel_model_model ON ai_model_frontend_channel_model (tenant_id, organization_id, model_id, status, id);

CREATE TABLE IF NOT EXISTS ai_model_frontend_field (
    id BIGINT NOT NULL PRIMARY KEY,
    uuid VARCHAR(64) NOT NULL,
    tenant_id BIGINT NOT NULL DEFAULT 0,
    organization_id BIGINT NOT NULL DEFAULT 0,
    data_scope INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    version BIGINT NOT NULL DEFAULT 0,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    channel_id BIGINT NOT NULL,
    channel_code VARCHAR(64) NOT NULL,
    field_key VARCHAR(64) NOT NULL,
    visible BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    config JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_field_uuid ON ai_model_frontend_field (uuid);
CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_field_key ON ai_model_frontend_field (tenant_id, organization_id, channel_id, field_key) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_ai_model_frontend_field_list ON ai_model_frontend_field (tenant_id, organization_id, channel_code, status, visible, sort_order, id);

CREATE TABLE IF NOT EXISTS ai_model_frontend_revision (
    id BIGINT NOT NULL PRIMARY KEY,
    uuid VARCHAR(64) NOT NULL,
    tenant_id BIGINT NOT NULL DEFAULT 0,
    organization_id BIGINT NOT NULL DEFAULT 0,
    data_scope INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    version BIGINT NOT NULL DEFAULT 0,
    deleted_at TIMESTAMPTZ,
    deleted_by BIGINT,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    revision BIGINT NOT NULL,
    content_sha256 VARCHAR(64) NOT NULL,
    channel_count INTEGER NOT NULL DEFAULT 0,
    published_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source VARCHAR(64) NOT NULL DEFAULT 'admin'
);

CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_revision_uuid ON ai_model_frontend_revision (uuid);
CREATE UNIQUE INDEX IF NOT EXISTS uk_ai_model_frontend_revision_number ON ai_model_frontend_revision (tenant_id, organization_id, revision);
CREATE INDEX IF NOT EXISTS idx_ai_model_frontend_revision_published ON ai_model_frontend_revision (tenant_id, organization_id, status, published_at DESC, id);

COMMIT;
