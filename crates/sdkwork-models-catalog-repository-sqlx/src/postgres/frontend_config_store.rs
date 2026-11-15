use std::collections::HashMap;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};

use sdkwork_models_contract_service::{
    DomainError, DomainResult, FrontendChannel, FrontendChannelCommand, FrontendChannelField,
    FrontendChannelFieldCommand, FrontendChannelModel, FrontendChannelModelCommand, FrontendConfig,
    FrontendConfigFuture, FrontendConfigStore, FrontendConfigSubject, LoadFrontendConfigQuery,
    ReplaceFrontendConfigCommand,
};

use crate::runtime_id::next_cloud_runtime_id;

/// PostgreSQL store for the front-end model configuration tables.
///
/// The read joins three tables keyed by `channel_id`; the replace runs in one
/// transaction so a served revision never mixes two configurations.
#[derive(Debug, Clone)]
pub struct PostgresFrontendConfigStore {
    pool: PgPool,
}

impl PostgresFrontendConfigStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl FrontendConfigStore for PostgresFrontendConfigStore {
    fn load_frontend_config<'a>(
        &'a self,
        query: LoadFrontendConfigQuery,
    ) -> FrontendConfigFuture<'a, FrontendConfig> {
        Box::pin(async move { load_frontend_config(&self.pool, query.subject).await })
    }

    fn replace_frontend_config<'a>(
        &'a self,
        command: ReplaceFrontendConfigCommand,
    ) -> FrontendConfigFuture<'a, FrontendConfig> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(|error| {
                store_error("failed to begin frontend config transaction", error)
            })?;
            let revision = replace_frontend_config(&mut tx, &command).await?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit frontend config transaction", error)
            })?;
            Ok(FrontendConfig {
                revision,
                channels: command
                    .channels
                    .iter()
                    .map(|channel| channel_from_command(channel, revision))
                    .collect(),
            })
        })
    }
}

async fn load_frontend_config(
    pool: &PgPool,
    subject: FrontendConfigSubject,
) -> DomainResult<FrontendConfig> {
    let channel_rows = sqlx::query(
        r#"
        SELECT id,
               channel_code,
               display_name,
               COALESCE(display_name_i18n::text, '{}') AS display_name_i18n_json,
               description,
               enabled,
               sort_order,
               default_catalog_key,
               COALESCE(capability_codes::text, '[]') AS capability_codes_json,
               COALESCE(config::text, '{}') AS config_json,
               schema_version,
               revision
        FROM ai_model_frontend_channel
        WHERE tenant_id = $1
          AND organization_id = $2
          AND status = 1
          AND deleted_at IS NULL
        ORDER BY sort_order, id
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load frontend channels", error))?;

    let model_rows = sqlx::query(
        r#"
        SELECT channel_id,
               catalog_key,
               model_id,
               visible,
               featured,
               sort_order,
               badge,
               COALESCE(config::text, '{}') AS config_json
        FROM ai_model_frontend_channel_model
        WHERE tenant_id = $1
          AND organization_id = $2
          AND status = 1
          AND deleted_at IS NULL
        ORDER BY sort_order, id
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load frontend channel models", error))?;

    let field_rows = sqlx::query(
        r#"
        SELECT channel_id,
               field_key,
               visible,
               sort_order,
               COALESCE(config::text, '{}') AS config_json
        FROM ai_model_frontend_field
        WHERE tenant_id = $1
          AND organization_id = $2
          AND status = 1
          AND deleted_at IS NULL
        ORDER BY sort_order, id
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to load frontend channel fields", error))?;

    let revision: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(revision), 0)
        FROM ai_model_frontend_revision
        WHERE tenant_id = $1
          AND organization_id = $2
          AND status = 1
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_one(pool)
    .await
    .map_err(|error| store_error("failed to load frontend config revision", error))?;

    let mut models_by_channel: HashMap<i64, Vec<FrontendChannelModel>> = HashMap::new();
    for row in model_rows {
        let channel_id: i64 = row.get("channel_id");
        let config_json: String = row.get("config_json");
        models_by_channel
            .entry(channel_id)
            .or_default()
            .push(FrontendChannelModel {
                catalog_key: row.get("catalog_key"),
                model_id: row.get("model_id"),
                visible: row.get("visible"),
                featured: row.get("featured"),
                sort_order: row.get("sort_order"),
                badge: row.get("badge"),
                config: parse_json(&config_json, "frontend channel model config")?,
            });
    }

    let mut fields_by_channel: HashMap<i64, Vec<FrontendChannelField>> = HashMap::new();
    for row in field_rows {
        let channel_id: i64 = row.get("channel_id");
        let config_json: String = row.get("config_json");
        fields_by_channel
            .entry(channel_id)
            .or_default()
            .push(FrontendChannelField {
                field_key: row.get("field_key"),
                visible: row.get("visible"),
                sort_order: row.get("sort_order"),
                config: parse_json(&config_json, "frontend field config")?,
            });
    }

    let mut channels = Vec::with_capacity(channel_rows.len());
    for row in channel_rows {
        let id: i64 = row.get("id");
        let display_name_i18n_json: String = row.get("display_name_i18n_json");
        let capability_codes_json: String = row.get("capability_codes_json");
        let config_json: String = row.get("config_json");
        channels.push(FrontendChannel {
            channel_code: row.get("channel_code"),
            display_name: row.get("display_name"),
            display_name_i18n: parse_json(
                &display_name_i18n_json,
                "frontend channel display name i18n",
            )?,
            description: row.get("description"),
            enabled: row.get("enabled"),
            sort_order: row.get("sort_order"),
            default_catalog_key: row.get("default_catalog_key"),
            capability_codes: parse_string_array(
                &capability_codes_json,
                "frontend channel capability codes",
            )?,
            config: parse_json(&config_json, "frontend channel config")?,
            schema_version: row.get("schema_version"),
            revision: row.get("revision"),
            models: models_by_channel.remove(&id).unwrap_or_default(),
            fields: fields_by_channel.remove(&id).unwrap_or_default(),
        });
    }

    Ok(FrontendConfig { revision, channels })
}

async fn replace_frontend_config(
    tx: &mut Transaction<'_, Postgres>,
    command: &ReplaceFrontendConfigCommand,
) -> DomainResult<i64> {
    let subject = command.subject;
    retire_live_rows(tx, subject, command).await?;
    let revision: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(revision), 0) + 1
        FROM ai_model_frontend_revision
        WHERE tenant_id = $1
          AND organization_id = $2
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("failed to allocate frontend config revision", error))?;

    for channel in &command.channels {
        let channel_id = insert_channel(tx, command, channel, revision).await?;
        for model in &channel.models {
            insert_channel_model(tx, command, channel_id, &channel.channel_code, model).await?;
        }
        for field in &channel.fields {
            insert_channel_field(tx, command, channel_id, &channel.channel_code, field).await?;
        }
    }

    insert_revision(tx, command, revision).await?;
    Ok(revision)
}

async fn retire_live_rows(
    tx: &mut Transaction<'_, Postgres>,
    subject: FrontendConfigSubject,
    command: &ReplaceFrontendConfigCommand,
) -> DomainResult<()> {
    for table in [
        "ai_model_frontend_channel_model",
        "ai_model_frontend_field",
        "ai_model_frontend_channel",
    ] {
        let statement = format!(
            "UPDATE {table}
             SET status = 0,
                 deleted_at = CAST($3 AS TIMESTAMPTZ),
                 version = version + 1
             WHERE tenant_id = $1
               AND organization_id = $2
               AND deleted_at IS NULL"
        );
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .bind(subject.tenant_id)
            .bind(subject.organization_id)
            .bind(&command.requested_at)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error("failed to retire frontend config rows", error))?;
    }
    Ok(())
}

async fn insert_channel(
    tx: &mut Transaction<'_, Postgres>,
    command: &ReplaceFrontendConfigCommand,
    channel: &FrontendChannelCommand,
    revision: i64,
) -> DomainResult<i64> {
    let subject = command.subject;
    let display_name_i18n =
        serialize_json(&channel.display_name_i18n, "channel display name i18n")?;
    let capability_codes = serialize_json(&channel.capability_codes, "channel capability codes")?;
    let config = serialize_json(&channel.config, "channel config")?;
    sqlx::query_scalar(
        r#"
        INSERT INTO ai_model_frontend_channel
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata,
             channel_code, display_name, display_name_i18n, description, enabled, sort_order,
             default_catalog_key, capability_codes, config, schema_version, revision, last_published_at, id)
        VALUES
            ($1, $2, $3, 1, 1, CAST($4 AS TIMESTAMPTZ), CAST($4 AS TIMESTAMPTZ), 0, '{}'::jsonb,
             $5, $6, $7::jsonb, $8, $9, $10,
             $11, $12::jsonb, $13::jsonb, $14, $15, CAST($4 AS TIMESTAMPTZ), $16)
        RETURNING id
        "#,
    )
    .bind(&channel.uuid)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(&command.requested_at)
    .bind(&channel.channel_code)
    .bind(&channel.display_name)
    .bind(display_name_i18n)
    .bind(channel.description.as_deref())
    .bind(channel.enabled)
    .bind(channel.sort_order)
    .bind(channel.default_catalog_key.as_deref())
    .bind(capability_codes)
    .bind(config)
    .bind(&channel.schema_version)
    .bind(revision)
    .bind(next_cloud_runtime_id("ai_model_frontend_channel")?)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("failed to insert frontend channel", error))
}

async fn insert_channel_model(
    tx: &mut Transaction<'_, Postgres>,
    command: &ReplaceFrontendConfigCommand,
    channel_id: i64,
    channel_code: &str,
    model: &FrontendChannelModelCommand,
) -> DomainResult<()> {
    let subject = command.subject;
    let config = serialize_json(&model.config, "channel model config")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_frontend_channel_model
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata,
             channel_id, channel_code, catalog_key, model_id, visible, featured, sort_order, badge, config, id)
        VALUES
            ($1, $2, $3, 1, 1, CAST($4 AS TIMESTAMPTZ), CAST($4 AS TIMESTAMPTZ), 0, '{}'::jsonb,
             $5, $6, $7, $8, $9, $10, $11, $12, $13::jsonb, $14)
        "#,
    )
    .bind(&model.uuid)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(&command.requested_at)
    .bind(channel_id)
    .bind(channel_code)
    .bind(&model.catalog_key)
    .bind(model.model_id)
    .bind(model.visible)
    .bind(model.featured)
    .bind(model.sort_order)
    .bind(model.badge.as_deref())
    .bind(config)
    .bind(next_cloud_runtime_id("ai_model_frontend_channel_model")?)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to insert frontend channel model", error))?;
    Ok(())
}

async fn insert_channel_field(
    tx: &mut Transaction<'_, Postgres>,
    command: &ReplaceFrontendConfigCommand,
    channel_id: i64,
    channel_code: &str,
    field: &FrontendChannelFieldCommand,
) -> DomainResult<()> {
    let subject = command.subject;
    let config = serialize_json(&field.config, "channel field config")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_frontend_field
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata,
             channel_id, channel_code, field_key, visible, sort_order, config, id)
        VALUES
            ($1, $2, $3, 1, 1, CAST($4 AS TIMESTAMPTZ), CAST($4 AS TIMESTAMPTZ), 0, '{}'::jsonb,
             $5, $6, $7, $8, $9, $10::jsonb, $11)
        "#,
    )
    .bind(&field.uuid)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(&command.requested_at)
    .bind(channel_id)
    .bind(channel_code)
    .bind(&field.field_key)
    .bind(field.visible)
    .bind(field.sort_order)
    .bind(config)
    .bind(next_cloud_runtime_id("ai_model_frontend_field")?)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to insert frontend channel field", error))?;
    Ok(())
}

async fn insert_revision(
    tx: &mut Transaction<'_, Postgres>,
    command: &ReplaceFrontendConfigCommand,
    revision: i64,
) -> DomainResult<()> {
    let subject = command.subject;
    let content_sha256 = content_sha256(command);
    sqlx::query(
        r#"
        INSERT INTO ai_model_frontend_revision
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata,
             revision, content_sha256, channel_count, published_at, source, id)
        VALUES
            ($1, $2, $3, 1, 1, CAST($4 AS TIMESTAMPTZ), CAST($4 AS TIMESTAMPTZ), 0, '{}'::jsonb,
             $5, $6, $7, CAST($4 AS TIMESTAMPTZ), $8, $9)
        "#,
    )
    .bind(&command.snapshot_uuid)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(&command.requested_at)
    .bind(revision)
    .bind(content_sha256)
    .bind(command.channels.len() as i32)
    .bind(&command.source)
    .bind(next_cloud_runtime_id("ai_model_frontend_revision")?)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to insert frontend config revision", error))?;
    Ok(())
}

fn content_sha256(command: &ReplaceFrontendConfigCommand) -> String {
    let document = json!({
        "channels": command
            .channels
            .iter()
            .map(|channel| json!({
                "channelCode": channel.channel_code,
                "displayName": channel.display_name,
                "displayNameI18n": channel.display_name_i18n,
                "description": channel.description,
                "enabled": channel.enabled,
                "sortOrder": channel.sort_order,
                "defaultCatalogKey": channel.default_catalog_key,
                "capabilityCodes": channel.capability_codes,
                "config": channel.config,
                "schemaVersion": channel.schema_version,
                "models": channel.models.iter().map(|model| json!({
                    "catalogKey": model.catalog_key,
                    "modelId": model.model_id,
                    "visible": model.visible,
                    "featured": model.featured,
                    "sortOrder": model.sort_order,
                    "badge": model.badge,
                    "config": model.config,
                })).collect::<Vec<_>>(),
                "fields": channel.fields.iter().map(|field| json!({
                    "fieldKey": field.field_key,
                    "visible": field.visible,
                    "sortOrder": field.sort_order,
                    "config": field.config,
                })).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    });
    let bytes = serde_json::to_vec(&document).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    hex::encode(digest.as_slice())
}

fn channel_from_command(channel: &FrontendChannelCommand, revision: i64) -> FrontendChannel {
    FrontendChannel {
        channel_code: channel.channel_code.clone(),
        display_name: channel.display_name.clone(),
        display_name_i18n: channel.display_name_i18n.clone(),
        description: channel.description.clone(),
        enabled: channel.enabled,
        sort_order: channel.sort_order,
        default_catalog_key: channel.default_catalog_key.clone(),
        capability_codes: channel.capability_codes.clone(),
        config: channel.config.clone(),
        schema_version: channel.schema_version.clone(),
        revision,
        models: channel
            .models
            .iter()
            .map(|model| FrontendChannelModel {
                catalog_key: model.catalog_key.clone(),
                model_id: model.model_id,
                visible: model.visible,
                featured: model.featured,
                sort_order: model.sort_order,
                badge: model.badge.clone(),
                config: model.config.clone(),
            })
            .collect(),
        fields: channel
            .fields
            .iter()
            .map(|field| FrontendChannelField {
                field_key: field.field_key.clone(),
                visible: field.visible,
                sort_order: field.sort_order,
                config: field.config.clone(),
            })
            .collect(),
    }
}

fn parse_json(raw: &str, context: &str) -> DomainResult<Value> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::new(format!("{context} is not valid JSON: {error}")))
}

fn parse_string_array(raw: &str, context: &str) -> DomainResult<Vec<String>> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::new(format!("{context} is not a JSON string array: {error}")))
}

fn serialize_json<T: serde::Serialize>(value: &T, context: &str) -> DomainResult<String> {
    serde_json::to_string(value)
        .map_err(|error| DomainError::new(format!("{context} could not be serialized: {error}")))
}

fn store_error(context: &str, error: sqlx::Error) -> DomainError {
    DomainError::new(format!("{context}: {error}"))
}
