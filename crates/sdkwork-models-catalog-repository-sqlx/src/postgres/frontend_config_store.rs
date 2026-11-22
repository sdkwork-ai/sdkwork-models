use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

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

/// How many tenant documents one process keeps in the revision cache.
const DEFAULT_FRONTEND_CONFIG_CACHE_CAPACITY: usize = 64;

/// Cache key: tenant, organization, projection kind, and catalog freshness.
///
/// The app projection (catalog-backed) can change when the catalog changes
/// without a front-end revision bump, so its key carries the catalog freshness
/// stamp. The admin projection always uses `0`.
type FrontendCacheKey = (i64, i64, bool, i64);

/// One tenant's cached document, valid only for the revision it was read at.
#[derive(Debug, Clone)]
struct CachedFrontendConfig {
    revision: i64,
    config: Arc<FrontendConfig>,
}

fn cache_key(
    subject: FrontendConfigSubject,
    catalog_backed_only: bool,
    catalog_freshness: i64,
) -> FrontendCacheKey {
    (
        subject.tenant_id,
        subject.organization_id,
        catalog_backed_only,
        if catalog_backed_only {
            catalog_freshness
        } else {
            0
        },
    )
}

/// Process-local, revision-checked cache for front-end documents.
///
/// The front-end document is read on every app start and on every console
/// refresh, while it changes only when an operator publishes a revision. The
/// cache therefore keeps the assembled document and validates it against the
/// tenant's current revision before serving it: a stale entry can never be
/// returned because the revision is part of the lookup, so a write in another
/// process still invalidates by observation rather than by cross-process push.
///
/// The cache is bounded and evicts the least-recently-used tenant so a
/// multi-tenant deployment cannot grow it without limit.
#[derive(Debug)]
struct FrontendConfigRevisionCache {
    capacity: usize,
    entries: HashMap<FrontendCacheKey, CachedFrontendConfig>,
    lru: VecDeque<FrontendCacheKey>,
}

impl FrontendConfigRevisionCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    fn get(
        &mut self,
        subject: FrontendConfigSubject,
        catalog_backed_only: bool,
        catalog_freshness: i64,
        revision: i64,
    ) -> Option<Arc<FrontendConfig>> {
        let key = cache_key(subject, catalog_backed_only, catalog_freshness);
        let entry = self.entries.get(&key)?;
        if entry.revision != revision {
            return None;
        }
        let config = Arc::clone(&entry.config);
        self.touch(key);
        Some(config)
    }

    fn put(
        &mut self,
        subject: FrontendConfigSubject,
        catalog_backed_only: bool,
        catalog_freshness: i64,
        revision: i64,
        config: &FrontendConfig,
    ) {
        let key = cache_key(subject, catalog_backed_only, catalog_freshness);
        let cached = CachedFrontendConfig {
            revision,
            config: Arc::new(config.clone()),
        };
        self.entries.insert(key, cached);
        self.touch(key);
        self.evict();
    }

    #[cfg(test)]
    fn invalidate(&mut self, subject: FrontendConfigSubject, catalog_backed_only: bool) {
        let key = cache_key(subject, catalog_backed_only, 0);
        self.entries
            .retain(|candidate, _| candidate.0 != key.0 || candidate.1 != key.1);
        self.lru
            .retain(|candidate| candidate.0 != key.0 || candidate.1 != key.1);
    }

    #[cfg(test)]
    fn clear(&mut self) {
        self.entries.clear();
        self.lru.clear();
    }

    fn touch(&mut self, key: FrontendCacheKey) {
        self.lru.retain(|candidate| *candidate != key);
        self.lru.push_back(key);
    }

    fn evict(&mut self) {
        while self.entries.len() > self.capacity {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }
}

/// PostgreSQL store for the front-end model configuration tables.
///
/// The read joins three tables keyed by `channel_id`; the replace runs in one
/// transaction so a served revision never mixes two configurations. Reads are
/// served from a bounded revision cache after one indexed revision lookup; the
/// full join only runs when the revision moved or the tenant is not cached.
#[derive(Debug, Clone)]
pub struct PostgresFrontendConfigStore {
    pool: PgPool,
    cache: Arc<Mutex<FrontendConfigRevisionCache>>,
}

impl PostgresFrontendConfigStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            cache: Arc::new(Mutex::new(FrontendConfigRevisionCache::new(
                DEFAULT_FRONTEND_CONFIG_CACHE_CAPACITY,
            ))),
        }
    }

    fn lock_cache(&self) -> std::sync::MutexGuard<'_, FrontendConfigRevisionCache> {
        self.cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl FrontendConfigStore for PostgresFrontendConfigStore {
    fn load_frontend_config<'a>(
        &'a self,
        query: LoadFrontendConfigQuery,
    ) -> FrontendConfigFuture<'a, FrontendConfig> {
        Box::pin(async move {
            let subject = query.subject;
            let catalog_backed_only = query.catalog_backed_only;
            // The app projection can change when the catalog changes without a
            // front-end revision bump, so it carries a catalog freshness stamp
            // in its cache key. The admin projection never filters by catalog,
            // so it uses the constant zero.
            let catalog_freshness = if catalog_backed_only {
                load_catalog_freshness(&self.pool, subject).await?
            } else {
                0
            };
            // One indexed scalar lookup decides whether the cached document is
            // still the published revision; the three-table join is skipped on
            // a hit.
            let revision = load_current_revision(&self.pool, subject).await?;
            if let Some(cached) =
                self.lock_cache()
                    .get(subject, catalog_backed_only, catalog_freshness, revision)
            {
                return Ok((*cached).clone());
            }
            let config =
                load_frontend_config_rows(&self.pool, subject, catalog_backed_only).await?;
            // Only cache a document whose rows agree with the revision read
            // before the join; a concurrent replace in between must not poison
            // the cache with a mixed revision.
            if config.revision == revision {
                self.lock_cache().put(
                    subject,
                    catalog_backed_only,
                    catalog_freshness,
                    config.revision,
                    &config,
                );
            }
            Ok(config)
        })
    }

    fn replace_frontend_config<'a>(
        &'a self,
        command: ReplaceFrontendConfigCommand,
    ) -> FrontendConfigFuture<'a, FrontendConfig> {
        Box::pin(async move {
            let subject = command.subject;
            let mut tx = self.pool.begin().await.map_err(|error| {
                store_error("failed to begin frontend config transaction", error)
            })?;
            let revision = replace_frontend_config(&mut tx, &command).await?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit frontend config transaction", error)
            })?;
            let config = FrontendConfig {
                revision,
                channels: command
                    .channels
                    .iter()
                    .map(|channel| channel_from_command(channel, revision))
                    .collect(),
            };
            self.lock_cache().put(subject, false, 0, revision, &config);
            Ok(config)
        })
    }
}

/// Catalog freshness stamp for the app projection's cache key.
///
/// The front-end revision only changes when an operator saves the surface
/// document; a catalog import, a model disable, or a model removal changes the
/// catalog-backed answer without touching it. The newest `updated_at` across
/// the catalog rows the tenant can see orders those changes; it is combined
/// with the front-end revision in the cache key so the app read cannot serve a
/// projection whose catalog membership no longer holds.
async fn load_catalog_freshness(
    pool: &PgPool,
    subject: FrontendConfigSubject,
) -> DomainResult<i64> {
    sqlx::query_scalar(
        r#"
        SELECT COALESCE(
            (EXTRACT(EPOCH FROM MAX(updated_at)) * 1000000)::bigint,
            0
        )
        FROM ai_model
        WHERE (tenant_id = 0 OR tenant_id = $1)
          AND (organization_id = 0 OR organization_id = $2)
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_one(pool)
    .await
    .map_err(|error| store_error("failed to load catalog freshness", error))
}

/// The tenant's published front-end configuration revision, or `0` before any
/// replace. Served by the `(tenant_id, organization_id)` index on the revision
/// table.
async fn load_current_revision(pool: &PgPool, subject: FrontendConfigSubject) -> DomainResult<i64> {
    sqlx::query_scalar(
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
    .map_err(|error| store_error("failed to load frontend config revision", error))
}

async fn load_frontend_config_rows(
    pool: &PgPool,
    subject: FrontendConfigSubject,
    catalog_backed_only: bool,
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
        SELECT fm.channel_id,
               fm.catalog_key,
               fm.model_id,
               fm.visible,
               fm.featured,
               fm.sort_order,
               fm.badge,
               COALESCE(fm.config::text, '{}') AS config_json
        FROM ai_model_frontend_channel_model fm
        WHERE fm.tenant_id = $1
          AND fm.organization_id = $2
          AND fm.status = 1
          AND fm.deleted_at IS NULL
          AND (
              $3 = FALSE
              OR EXISTS (
                  SELECT 1
                  FROM ai_model m
                  WHERE m.catalog_key = fm.catalog_key
                    AND m.deleted_at IS NULL
                    AND m.status = 1
                    AND (m.tenant_id = 0 OR m.tenant_id = $1)
                    AND (m.organization_id = 0 OR m.organization_id = $2)
              )
          )
        ORDER BY fm.sort_order, fm.id
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(catalog_backed_only)
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
                sort_order: i64::from(row.get::<i32, _>("sort_order")),
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
                sort_order: i64::from(row.get::<i32, _>("sort_order")),
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
            sort_order: i64::from(row.get::<i32, _>("sort_order")),
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
    // Serialize concurrent replaces for this tenant on its live channel rows:
    // the first writer retires them, so a second replace waits here and then
    // observes the revision the first committed.
    lock_live_channels(tx, subject).await?;
    let current_revision: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(revision), 0)
        FROM ai_model_frontend_revision
        WHERE tenant_id = $1
          AND organization_id = $2
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("failed to read frontend config revision", error))?;
    if let Some(expected) = command.expected_revision {
        if expected != current_revision {
            return Err(DomainError::conflict(format!(
                "frontend config revision conflict: expected {expected}, current {current_revision}"
            )));
        }
    }
    retire_live_rows(tx, subject, command).await?;
    let revision = current_revision + 1;

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

/// Locks the tenant's live channel rows for the duration of the replace
/// transaction. A tenant with no channels yet has no row to lock, so a racing
/// first publication is still caught by the unique
/// `(tenant_id, organization_id, revision)` index.
async fn lock_live_channels(
    tx: &mut Transaction<'_, Postgres>,
    subject: FrontendConfigSubject,
) -> DomainResult<()> {
    sqlx::query(
        r#"
        SELECT id
        FROM ai_model_frontend_channel
        WHERE tenant_id = $1
          AND organization_id = $2
          AND deleted_at IS NULL
        ORDER BY id
        FOR UPDATE
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error("failed to lock frontend channel rows", error))?;
    Ok(())
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
    .map_err(|error| {
        // The unique `(tenant_id, organization_id, revision)` index is the last
        // line of defence for two racing first publications; surface it as a
        // retryable conflict instead of a server error.
        if error
            .as_database_error()
            .map(|database_error| database_error.is_unique_violation())
            .unwrap_or(false)
        {
            DomainError::conflict(
                "frontend config revision was published concurrently; reload and retry",
            )
        } else {
            store_error("failed to insert frontend config revision", error)
        }
    })?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(tenant_id: i64) -> FrontendConfigSubject {
        FrontendConfigSubject {
            tenant_id,
            organization_id: 0,
            operator_id: 1,
            operator_type: 1,
        }
    }

    fn config(revision: i64, channel_code: &str) -> FrontendConfig {
        FrontendConfig {
            revision,
            channels: vec![FrontendChannel {
                channel_code: channel_code.to_owned(),
                display_name: channel_code.to_owned(),
                display_name_i18n: json!({}),
                description: None,
                enabled: true,
                sort_order: 0,
                default_catalog_key: None,
                capability_codes: vec!["chat".to_owned()],
                config: json!({}),
                schema_version: "1.0.0".to_owned(),
                revision,
                models: Vec::new(),
                fields: Vec::new(),
            }],
        }
    }

    #[test]
    fn cache_serves_only_the_revision_it_was_written_at() {
        let mut cache = FrontendConfigRevisionCache::new(4);
        cache.put(subject(7), false, 0, 3, &config(3, "llm"));

        let cached = cache
            .get(subject(7), false, 0, 3)
            .expect("revision 3 is cached");
        assert_eq!(cached.channels[0].channel_code, "llm");
        assert!(
            cache.get(subject(7), false, 0, 4).is_none(),
            "a newer revision must miss instead of serving the stale document"
        );
        assert!(
            cache.get(subject(8), false, 0, 3).is_none(),
            "tenants never share entries"
        );
    }

    #[test]
    fn cache_keeps_admin_and_catalog_backed_projections_separate() {
        let mut cache = FrontendConfigRevisionCache::new(4);
        cache.put(subject(7), false, 0, 3, &config(3, "llm"));
        cache.put(subject(7), true, 0, 3, &config(3, "catalog-backed"));

        assert_eq!(
            cache
                .get(subject(7), false, 0, 3)
                .expect("admin projection")
                .channels[0]
                .channel_code,
            "llm"
        );
        assert_eq!(
            cache
                .get(subject(7), true, 0, 3)
                .expect("app projection")
                .channels[0]
                .channel_code,
            "catalog-backed"
        );
    }

    #[test]
    fn cache_invalidates_the_app_projection_when_catalog_freshness_moves() {
        let mut cache = FrontendConfigRevisionCache::new(4);
        cache.put(subject(7), true, 100, 3, &config(3, "app"));
        cache.put(subject(7), false, 0, 3, &config(3, "admin"));

        assert!(
            cache.get(subject(7), true, 101, 3).is_none(),
            "a catalog change must miss the cached app projection"
        );
        assert!(
            cache.get(subject(7), false, 0, 3).is_some(),
            "the admin projection does not depend on catalog freshness"
        );
    }

    #[test]
    fn cache_replaces_a_tenant_document_on_the_next_revision() {
        let mut cache = FrontendConfigRevisionCache::new(4);
        cache.put(subject(7), false, 0, 3, &config(3, "llm"));
        cache.put(subject(7), false, 0, 4, &config(4, "video"));

        let cached = cache
            .get(subject(7), false, 0, 4)
            .expect("revision 4 is cached");
        assert_eq!(cached.revision, 4);
        assert_eq!(cached.channels[0].channel_code, "video");
    }

    #[test]
    fn cache_evicts_the_least_recently_used_tenant() {
        let mut cache = FrontendConfigRevisionCache::new(2);
        cache.put(subject(1), false, 0, 1, &config(1, "llm"));
        cache.put(subject(2), false, 0, 1, &config(1, "llm"));
        assert!(
            cache.get(subject(1), false, 0, 1).is_some(),
            "touching tenant 1 protects it"
        );
        cache.put(subject(3), false, 0, 1, &config(1, "llm"));

        assert!(cache.get(subject(1), false, 0, 1).is_some());
        assert!(
            cache.get(subject(2), false, 0, 1).is_none(),
            "tenant 2 is the LRU entry"
        );
        assert!(cache.get(subject(3), false, 0, 1).is_some());
    }

    #[test]
    fn invalidate_and_clear_drop_documents() {
        let mut cache = FrontendConfigRevisionCache::new(4);
        cache.put(subject(1), false, 0, 1, &config(1, "llm"));
        cache.put(subject(2), false, 0, 1, &config(1, "llm"));
        cache.invalidate(subject(1), false);
        assert!(cache.get(subject(1), false, 0, 1).is_none());
        assert!(cache.get(subject(2), false, 0, 1).is_some());
        cache.clear();
        assert!(cache.get(subject(2), false, 0, 1).is_none());
    }
}
