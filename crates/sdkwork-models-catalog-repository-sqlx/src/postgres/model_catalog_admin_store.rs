use std::collections::BTreeMap;

use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::admin_models_list::{
    capability_codes_from_model_types, normalized_modalities, normalized_release_stages,
    normalized_search_pattern, normalized_vendor_codes, optional_non_empty,
    LIST_MODELS_BASE_WHERE_POSTGRES, LIST_MODELS_COUNT_WHERE_POSTGRES,
};
use crate::model_catalog_import::{
    catalog_preview_admin_items, catalog_scope_counts, catalog_scope_source_hash,
    catalog_scope_vendor_codes, catalog_with_selected_vendors, is_dry_run_mode,
    load_catalog_root_with_pin, model_catalog_key as build_model_base_catalog_key,
    pricing_catalog_key as build_model_pricing_catalog_key, stable_uuid, CatalogScopeCounts,
};
use crate::model_modality;
use crate::runtime_id::next_cloud_runtime_id;
use crate::ENV_MODELS_CATALOG_ROOT;
use sdkwork_models_contract_service::{
    AdminAiModelItem, AdminAiModelListPage, AdminAiModelRegionPriceCommand,
    AdminModelCatalogSyncItem, AdminModelCommandFuture, AdminModelMappingListPage,
    AdminModelMappingRuleBindingDraft, AdminModelMappingRuleBindingItem, AdminModelMappingRuleItem,
    AdminModelMappingRuleItemDraft, AdminModelMappingRuleMappingItem, AdminModelSubject,
    AdminModelVendorItem, CreateAdminAiModelCommand, CreateAdminModelMappingCommand,
    CreateAdminModelVendorCommand, DeleteAdminAiModelCommand, DeleteAdminModelMappingCommand,
    ListAdminAiModelsQuery, ListAdminModelMappingsQuery, ListAdminModelVendorsQuery,
    ModelCatalogAdminStore, ResolveAdminModelMappingQuery, ResolveAdminModelMappingResult,
    SyncAdminModelCatalogCommand, UpdateAdminAiModelCommand, UpdateAdminModelMappingCommand,
};
use sdkwork_models_contract_service::{DomainError, DomainResult};

const MODEL_VENDOR_TARGET_TYPE: i32 = 41;
const AI_MODEL_TARGET_TYPE: i32 = 42;
const MODEL_CATALOG_SYNC_TARGET_TYPE: i32 = 43;
const MODEL_MAPPING_TARGET_TYPE: i32 = 44;
const OFFICIAL_REFERENCE_PRICE_SIDE: i32 = 1;
const INPUT_BILLING_METER_FILTER_SQL: &str = "('llm_input_token', 'embedding_input_token', 'image_input_token', 'image_megapixel', 'audio_input_second', 'audio_input_minute', 'stt_audio_minute', 'tts_input_character', 'api_request')";
const OUTPUT_BILLING_METER_FILTER_SQL: &str = "('llm_output_token', 'image_output_token', 'image_result', 'image_megapixel', 'audio_output_second', 'music_output_second', 'sfx_result', 'video_output_second', 'video_result', 'api_result')";
const CACHE_READ_BILLING_METER_FILTER_SQL: &str = "('llm_cache_read_token')";
const CACHE_WRITE_BILLING_METER_FILTER_SQL: &str = "('llm_cache_write_token')";

#[derive(Debug, Clone)]
pub struct PostgresModelCatalogAdminStore {
    pool: PgPool,
    models_catalog_root: Option<String>,
}

#[derive(Debug, Clone)]
struct VendorIdentity {
    id: i64,
    code: String,
    name: String,
}

#[derive(Debug, Clone)]
struct EffectiveModelUpdate {
    model: String,
    display_name: String,
    model_type: String,
    status: String,
    description: Option<String>,
    modalities: Vec<String>,
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
    api_format: Option<String>,
    capability_intro: Option<String>,
    limitations: Vec<String>,
    supported_languages: Vec<String>,
    use_cases: Vec<String>,
    training_data_cutoff: Option<String>,
    context_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    supports_streaming: bool,
    supports_tools: bool,
    supports_json_schema: bool,
    usage_scopes: Vec<String>,
    coding_visible: bool,
    release_stage: i32,
    shelf_state: i32,
    routing_state: i32,
    replacement_model: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedModelMappingMatch {
    rule: AdminModelMappingRuleItem,
    item: AdminModelMappingRuleMappingItem,
    binding_type: String,
}

impl PostgresModelCatalogAdminStore {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            models_catalog_root: None,
        }
    }

    pub fn with_models_catalog_root(pool: PgPool, models_catalog_root: Option<String>) -> Self {
        Self {
            pool,
            models_catalog_root,
        }
    }
}

impl ModelCatalogAdminStore for PostgresModelCatalogAdminStore {
    fn list_vendors<'a>(
        &'a self,
        query: ListAdminModelVendorsQuery,
    ) -> AdminModelCommandFuture<'a, Vec<AdminModelVendorItem>> {
        Box::pin(async move { list_vendors(&self.pool, query).await })
    }

    fn list_models<'a>(
        &'a self,
        query: ListAdminAiModelsQuery,
    ) -> AdminModelCommandFuture<'a, AdminAiModelListPage> {
        Box::pin(async move { list_models(&self.pool, query).await })
    }

    fn list_model_mappings<'a>(
        &'a self,
        query: ListAdminModelMappingsQuery,
    ) -> AdminModelCommandFuture<'a, AdminModelMappingListPage> {
        Box::pin(async move { list_model_mappings(&self.pool, query).await })
    }

    fn create_vendor<'a>(
        &'a self,
        command: CreateAdminModelVendorCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelVendorItem> {
        Box::pin(async move {
            let mut tx =
                self.pool.begin().await.map_err(|error| {
                    store_error("failed to begin model vendor transaction", error)
                })?;
            let vendor_id = insert_vendor(&mut tx, &command).await?;
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "create_model_vendor",
                MODEL_VENDOR_TARGET_TYPE,
                vendor_id,
                serde_json::json!({
                    "action": "create_model_vendor",
                    "vendorId": vendor_id,
                    "vendorCode": &command.vendor_code,
                    "name": &command.name,
                    "status": &command.status
                }),
            )
            .await?;
            let item = load_vendor_by_id(
                &mut tx,
                vendor_id,
                command.subject.tenant_id,
                command.subject.organization_id,
            )
            .await?
            .ok_or_else(|| DomainError::new("created model vendor could not be reloaded"))?;
            tx.commit()
                .await
                .map_err(|error| store_error("failed to commit model vendor transaction", error))?;
            Ok(item)
        })
    }

    fn create_model<'a>(
        &'a self,
        command: CreateAdminAiModelCommand,
    ) -> AdminModelCommandFuture<'a, AdminAiModelItem> {
        Box::pin(async move {
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|error| store_error("failed to begin ai model transaction", error))?;
            let vendor = find_vendor(&mut tx, &command).await?;
            let model_id = insert_model(&mut tx, &command, &vendor).await?;
            insert_model_capability(&mut tx, model_id, &command, &vendor).await?;
            insert_model_region_pricing(&mut tx, model_id, &command, &vendor).await?;
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "create_ai_model",
                AI_MODEL_TARGET_TYPE,
                model_id,
                serde_json::json!({
                    "action": "create_ai_model",
                    "modelId": model_id,
                    "model": &command.model,
                    "displayName": &command.display_name,
                    "vendorId": vendor.id,
                    "vendorCode": &vendor.code,
                    "type": &command.model_type,
                    "regionPriceCount": command.region_prices.len(),
                    "regionCodes": command.region_prices.iter().map(|price| price.region_code.as_str()).collect::<Vec<_>>(),
                    "contextTokens": command.context_tokens
                }),
            )
            .await?;
            let item = load_model_by_id(
                &mut tx,
                model_id,
                command.subject.tenant_id,
                command.subject.organization_id,
            )
            .await?
            .ok_or_else(|| DomainError::new("created ai model could not be reloaded"))?;
            tx.commit()
                .await
                .map_err(|error| store_error("failed to commit ai model transaction", error))?;
            Ok(item)
        })
    }

    fn create_model_mapping<'a>(
        &'a self,
        command: CreateAdminModelMappingCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelMappingRuleItem> {
        Box::pin(async move {
            let mut tx =
                self.pool.begin().await.map_err(|error| {
                    store_error("failed to begin model mapping transaction", error)
                })?;
            let mapping_id = insert_model_mapping(&mut tx, &command).await?;
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "create_model_mapping",
                MODEL_MAPPING_TARGET_TYPE,
                mapping_id,
                serde_json::json!({
                    "action": "create_model_mapping",
                    "mappingId": mapping_id,
                    "sourceVendorCode": &command.draft.source_vendor_code,
                    "targetVendorCode": &command.draft.target_vendor_code,
                    "bindingCount": command.draft.bindings.len(),
                    "mappingItemCount": command.draft.mapping_items.len()
                }),
            )
            .await?;
            let item = load_model_mapping_by_id(
                &mut tx,
                mapping_id,
                command.subject.tenant_id,
                command.subject.organization_id,
            )
            .await?
            .ok_or_else(|| DomainError::new("created model mapping could not be reloaded"))?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit model mapping transaction", error)
            })?;
            Ok(item)
        })
    }

    fn update_model<'a>(
        &'a self,
        command: UpdateAdminAiModelCommand,
    ) -> AdminModelCommandFuture<'a, AdminAiModelItem> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(|error| {
                store_error("failed to begin ai model update transaction", error)
            })?;
            let current = find_model_for_update(&mut tx, &command).await?;
            if is_status_only_model_update(&command) {
                let status = command.status.as_deref().unwrap_or(current.status.as_str());
                update_model_status_only(&mut tx, current.id, &command, status).await?;
                insert_audit_log(
                    &mut tx,
                    &command.audit_log_uuid,
                    &command.request_id,
                    command.subject.tenant_id,
                    command.subject.organization_id,
                    command.subject.operator_id,
                    command.subject.operator_type,
                    "update_ai_model",
                    AI_MODEL_TARGET_TYPE,
                    current.id,
                    serde_json::json!({
                        "action": "update_ai_model",
                        "modelId": current.id,
                        "model": &current.model,
                        "status": status
                    }),
                )
                .await?;
                let item = load_model_by_id(
                    &mut tx,
                    current.id,
                    command.subject.tenant_id,
                    command.subject.organization_id,
                )
                .await?
                .ok_or_else(|| DomainError::new("updated ai model could not be reloaded"))?;
                tx.commit().await.map_err(|error| {
                    store_error("failed to commit ai model update transaction", error)
                })?;
                return Ok(item);
            }
            let vendor = match command.vendor_id.as_deref() {
                Some(vendor_id) => {
                    find_vendor_by_value(&mut tx, command.subject, vendor_id).await?
                }
                None => {
                    let vendor_id = current
                        .vendor_id
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| DomainError::not_found("model vendor was not found"))?;
                    find_vendor_by_id_value(&mut tx, command.subject, vendor_id).await?
                }
            };
            let update = effective_model_update(&current, &command);
            update_model_core(&mut tx, current.id, &command, &vendor, &update).await?;
            upsert_model_capability(&mut tx, current.id, &command, &vendor, &update).await?;
            if let Some(region_prices) = command.region_prices.as_ref() {
                replace_model_region_pricing(
                    &mut tx,
                    current.id,
                    &command,
                    &vendor,
                    &update,
                    region_prices,
                )
                .await?;
            }
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "update_ai_model",
                AI_MODEL_TARGET_TYPE,
                current.id,
                serde_json::json!({
                    "action": "update_ai_model",
                    "modelId": current.id,
                    "model": &update.model,
                    "displayName": &update.display_name,
                    "vendorId": vendor.id,
                    "vendorCode": &vendor.code,
                    "type": &update.model_type,
                    "regionPricesChanged": command.region_prices.is_some(),
                    "regionPriceCount": command.region_prices.as_ref().map(|prices| prices.len()).unwrap_or(0),
                    "regionCodes": command.region_prices.as_ref().map(|prices| prices.iter().map(|price| price.region_code.as_str()).collect::<Vec<_>>()),
                    "contextTokens": update.context_tokens
                }),
            )
            .await?;
            let item = load_model_by_id(
                &mut tx,
                current.id,
                command.subject.tenant_id,
                command.subject.organization_id,
            )
            .await?
            .ok_or_else(|| DomainError::new("updated ai model could not be reloaded"))?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit ai model update transaction", error)
            })?;
            Ok(item)
        })
    }

    fn update_model_mapping<'a>(
        &'a self,
        command: UpdateAdminModelMappingCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelMappingRuleItem> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(|error| {
                store_error("failed to begin model mapping update transaction", error)
            })?;
            let current = find_model_mapping_for_update(&mut tx, &command).await?;
            update_model_mapping_row(&mut tx, &current, &command).await?;
            if let Some(bindings) = command.patch.bindings.as_ref() {
                reconcile_model_mapping_bindings(&mut tx, &current, &command, bindings).await?;
            }
            if let Some(items) = command.patch.mapping_items.as_ref() {
                reconcile_model_mapping_items(&mut tx, &current, &command, items).await?;
            }
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "update_model_mapping",
                MODEL_MAPPING_TARGET_TYPE,
                current.id,
                serde_json::json!({
                    "action": "update_model_mapping",
                    "mappingId": current.id,
                    "sourceVendorCode": command.patch.source_vendor_code.as_deref().unwrap_or_else(|| current.source_vendor_code.as_deref().unwrap_or("")),
                    "targetVendorCode": command.patch.target_vendor_code.as_deref().unwrap_or_else(|| current.target_vendor_code.as_deref().unwrap_or("")),
                    "bindingsChanged": command.patch.bindings.is_some(),
                    "mappingItemsChanged": command.patch.mapping_items.is_some(),
                    "enabled": command.patch.enabled.unwrap_or(current.enabled)
                }),
            )
            .await?;
            let item = load_model_mapping_by_id(
                &mut tx,
                current.id,
                command.subject.tenant_id,
                command.subject.organization_id,
            )
            .await?
            .ok_or_else(|| DomainError::new("updated model mapping could not be reloaded"))?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit model mapping update transaction", error)
            })?;
            Ok(item)
        })
    }

    fn sync_catalog<'a>(
        &'a self,
        command: SyncAdminModelCatalogCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelCatalogSyncItem> {
        Box::pin(async move {
            let catalog = load_sync_model_catalog(&command, self.models_catalog_root.as_deref())?;
            let catalog_version = catalog.manifest.catalog_version.clone();
            let dry_run = is_dry_run_mode(&command.mode);
            let source_code = normalize_catalog_source_code(&command.source);
            let source_hash = catalog_scope_source_hash(&source_code, &catalog);
            let mut tx = self.pool.begin().await.map_err(|error| {
                store_error("failed to begin model catalog sync transaction", error)
            })?;
            if !dry_run {
                apply_sdkwork_models_catalog_refresh(&mut tx, &catalog).await?;
            }
            let (mut vendors, mut models) = if dry_run {
                catalog_preview_admin_items(&catalog, command.subject)
            } else {
                let vendors = list_vendors_tx(
                    &mut tx,
                    command.subject.tenant_id,
                    command.subject.organization_id,
                )
                .await?;
                let models = list_models_tx(
                    &mut tx,
                    command.subject.tenant_id,
                    command.subject.organization_id,
                )
                .await?;
                (vendors, models)
            };
            let scoped_vendor_codes = catalog_scope_vendor_codes(&catalog);
            filter_sync_catalog_items(&mut vendors, &mut models, &scoped_vendor_codes);
            let counts = catalog_scope_counts(&catalog);
            // Catalog sync evidence lives in ai_model_catalog_sync_run.
            // The retired ai_pricing_import_snapshot table must not be written.
            let sync_run_id = upsert_model_catalog_sync_run(
                &mut tx,
                &command,
                counts,
                &catalog_version,
                &source_hash,
                dry_run,
            )
            .await?;
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "sync_model_catalog",
                MODEL_CATALOG_SYNC_TARGET_TYPE,
                sync_run_id,
                serde_json::json!({
                    "action": "sync_model_catalog",
                    "snapshotId": &command.snapshot_uuid,
                    "syncRunId": sync_run_id,
                    "source": &command.source,
                    "mode": &command.mode,
                    "vendorCodes": &command.vendor_codes,
                    "force": command.force,
                    "catalogVersion": &catalog_version,
                    "catalogRoot": &command.catalog_root,
                    "sourceHash": &source_hash,
                    "dryRun": dry_run,
                    "vendorCount": vendors.len(),
                    "modelCount": models.len()
                }),
            )
            .await?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit model catalog sync transaction", error)
            })?;
            Ok(AdminModelCatalogSyncItem {
                synced: !dry_run,
                source: command.source,
                mode: command.mode,
                dry_run,
                catalog_version,
                requested_catalog_version: command.catalog_version,
                catalog_root: command.catalog_root,
                vendor_codes: scoped_vendor_codes,
                source_hash,
                meter_count: counts.meter_count,
                vendor_count: counts.vendor_count,
                family_count: counts.family_count,
                model_count: counts.model_count,
                capability_count: counts.capability_count,
                price_count: counts.price_count,
                ranking_count: counts.ranking_count,
                voice_count: counts.voice_count,
                voice_binding_count: counts.voice_binding_count,
                video_profile_count: counts.video_profile_count,
                accepted_count: counts.accepted_count(),
                snapshot_id: Some(command.snapshot_uuid.clone()),
                sync_run_id: Some(sync_run_id.to_string()),
                vendors,
                models,
            })
        })
    }

    fn delete_model<'a>(
        &'a self,
        command: DeleteAdminAiModelCommand,
    ) -> AdminModelCommandFuture<'a, bool> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(|error| {
                store_error("failed to begin ai model delete transaction", error)
            })?;
            let model = find_model_for_delete(&mut tx, &command).await?;
            soft_delete_model_graph(&mut tx, model.id, &command).await?;
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "delete_ai_model",
                AI_MODEL_TARGET_TYPE,
                model.id,
                serde_json::json!({
                    "action": "delete_ai_model",
                    "modelId": model.id,
                    "model": model.model,
                    "displayName": model.display_name,
                    "vendorId": model.vendor_id,
                    "vendorCode": model.vendor_code
                }),
            )
            .await?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit ai model delete transaction", error)
            })?;
            Ok(true)
        })
    }

    fn delete_model_mapping<'a>(
        &'a self,
        command: DeleteAdminModelMappingCommand,
    ) -> AdminModelCommandFuture<'a, bool> {
        Box::pin(async move {
            let mut tx = self.pool.begin().await.map_err(|error| {
                store_error("failed to begin model mapping delete transaction", error)
            })?;
            let mapping = find_model_mapping_for_delete(&mut tx, &command).await?;
            soft_delete_model_mapping(&mut tx, mapping.id, &command).await?;
            insert_audit_log(
                &mut tx,
                &command.audit_log_uuid,
                &command.request_id,
                command.subject.tenant_id,
                command.subject.organization_id,
                command.subject.operator_id,
                command.subject.operator_type,
                "delete_model_mapping",
                MODEL_MAPPING_TARGET_TYPE,
                mapping.id,
                serde_json::json!({
                    "action": "delete_model_mapping",
                    "mappingId": mapping.id,
                    "bindingType": mapping.binding_type,
                    "sourceVendorCode": mapping.source_vendor_code,
                    "targetVendorCode": mapping.target_vendor_code,
                    "mappingItemCount": mapping.mapping_items.len()
                }),
            )
            .await?;
            tx.commit().await.map_err(|error| {
                store_error("failed to commit model mapping delete transaction", error)
            })?;
            Ok(true)
        })
    }

    fn resolve_model_mapping<'a>(
        &'a self,
        query: ResolveAdminModelMappingQuery,
    ) -> AdminModelCommandFuture<'a, ResolveAdminModelMappingResult> {
        Box::pin(async move { resolve_model_mapping(&self.pool, query).await })
    }
}

fn is_status_only_model_update(command: &UpdateAdminAiModelCommand) -> bool {
    command.status.is_some()
        && command.vendor_id.is_none()
        && command.model.is_none()
        && command.display_name.is_none()
        && command.model_type.is_none()
        && command.region_prices.is_none()
        && command.description.is_none()
        && command.modalities.is_none()
        && command.input_modalities.is_none()
        && command.output_modalities.is_none()
        && command.api_format.is_none()
        && command.capability_intro.is_none()
        && command.limitations.is_none()
        && command.supported_languages.is_none()
        && command.use_cases.is_none()
        && command.training_data_cutoff.is_none()
        && command.context_tokens.is_none()
        && command.max_output_tokens.is_none()
        && command.supports_streaming.is_none()
        && command.supports_tools.is_none()
        && command.supports_json_schema.is_none()
        && command.release_stage.is_none()
        && command.shelf_state.is_none()
        && command.routing_state.is_none()
        && command.replacement_model.is_none()
}

async fn apply_sdkwork_models_catalog_refresh(
    tx: &mut Transaction<'_, Postgres>,
    catalog: &sdkwork_models::ModelCatalog,
) -> DomainResult<()> {
    crate::postgres::model_catalog_import::import_postgres_model_catalog_tx(tx, catalog)
        .await
        .map_err(|error| store_error("failed to refresh sdkwork models catalog", error))?;
    Ok(())
}

fn load_sync_model_catalog(
    command: &SyncAdminModelCatalogCommand,
    configured_catalog_root: Option<&str>,
) -> DomainResult<sdkwork_models::ModelCatalog> {
    let env_root = std::env::var(ENV_MODELS_CATALOG_ROOT).ok();
    let root = command
        .catalog_root
        .as_deref()
        .or(configured_catalog_root)
        .or_else(|| env_root.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let catalog =
        load_catalog_root_with_pin(root, command.catalog_version.as_deref()).map_err(|error| {
            DomainError::new(format!("failed to load sdkwork models catalog: {error}"))
        })?;
    catalog_with_selected_vendors(&catalog, &command.vendor_codes).map_err(|error| {
        DomainError::new(format!(
            "failed to select sdkwork models catalog vendors: {error}"
        ))
    })
}

async fn list_vendors(
    pool: &PgPool,
    query: ListAdminModelVendorsQuery,
) -> DomainResult<Vec<AdminModelVendorItem>> {
    let rows = sqlx::query(vendor_select_sql(
        r#"
        WHERE (tenant_id = 0 OR tenant_id = $1)
          AND (organization_id = 0 OR organization_id = $2)
          AND deleted_at IS NULL
        ORDER BY
          CASE WHEN tenant_id = $3 AND organization_id = $4 THEN 0 ELSE 1 END,
          COALESCE(sort_order, 1000000) ASC,
          display_name ASC NULLS LAST,
          id ASC
        "#,
    ))
    .bind(query.subject.tenant_id)
    .bind(query.subject.organization_id)
    .bind(query.subject.tenant_id)
    .bind(query.subject.organization_id)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to list model vendors", error))?;
    rows.into_iter().map(vendor_from_row).collect()
}

async fn list_models(
    pool: &PgPool,
    query: ListAdminAiModelsQuery,
) -> DomainResult<AdminAiModelListPage> {
    let vendor_id = optional_non_empty(&query.vendor_id).map(str::to_owned);
    let vendor_codes = normalized_vendor_codes(&query);
    let search_pattern = normalized_search_pattern(&query);
    let capability_codes = capability_codes_from_model_types(query.model_types.as_deref());
    let modalities = normalized_modalities(&query);
    let release_stages = normalized_release_stages(&query);
    let status = query.status.as_deref().map(status_code);
    let limit = query.normalized_limit();
    let offset = query.normalized_offset();

    let count_row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*)::bigint AS total_count FROM ai_model m {LIST_MODELS_COUNT_WHERE_POSTGRES}"
    )))
    .bind(query.subject.tenant_id)
    .bind(query.subject.organization_id)
    .bind(query.subject.tenant_id)
    .bind(query.subject.organization_id)
    .bind(vendor_id.as_deref())
    .bind(&vendor_codes)
    .bind(search_pattern.as_deref())
    .bind(&capability_codes)
    .bind(status)
    .bind(&modalities)
    .bind(&release_stages)
    .bind(query.shelf_state)
    .bind(query.routing_state)
    .fetch_one(pool)
    .await
    .map_err(|error| store_error("failed to count ai models", error))?;
    let total_count: i64 = count_row.try_get("total_count").map_err(row_error)?;

    let list_sql = model_select_sql(
        LIST_MODELS_BASE_WHERE_POSTGRES,
        query.subject.tenant_id,
        query.subject.organization_id,
    );
    let list_query = sqlx::query(list_sql)
        .bind(query.subject.tenant_id)
        .bind(query.subject.organization_id)
        .bind(query.subject.tenant_id)
        .bind(query.subject.organization_id)
        .bind(query.subject.tenant_id)
        .bind(query.subject.organization_id)
        .bind(vendor_id)
        .bind(&vendor_codes)
        .bind(search_pattern)
        .bind(&capability_codes)
        .bind(status)
        .bind(&modalities)
        .bind(&release_stages)
        .bind(query.shelf_state)
        .bind(query.routing_state);

    let rows = list_query
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|error| store_error("failed to list ai models", error))?;
    let mut models = rows
        .into_iter()
        .map(model_from_row)
        .collect::<DomainResult<Vec<_>>>()?;
    attach_model_region_prices(pool, &mut models).await?;
    Ok(AdminAiModelListPage {
        items: models,
        total_count,
    })
}

async fn list_model_mappings(
    pool: &PgPool,
    query: ListAdminModelMappingsQuery,
) -> DomainResult<AdminModelMappingListPage> {
    let account_id = query.account_id;
    let account_code = query.account_code.as_deref();
    let binding_type = query.binding_type.as_deref();
    let vendor_code = query.vendor_code.as_deref();
    let q = query.q.as_deref();
    let limit = query.normalized_limit();
    let offset = query.normalized_offset();
    let count_row = sqlx::query(
        r#"
        SELECT COUNT(*)::bigint AS total_count
        FROM ai_model_mapping_rule r
        WHERE r.tenant_id = $1
          AND r.organization_id = $2
          AND r.deleted_at IS NULL
          AND r.status = 1
          AND (
              $3::text IS NULL
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = $3
              )
          )
          AND (
              $4::text IS NULL
              OR r.source_vendor_code = $4
              OR r.target_vendor_code = $4
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = 'vendor'
                    AND b.binding_code = $4
              )
          )
          AND (
              $5::bigint IS NULL
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = 'upstream_account'
                    AND b.binding_id = $5
              )
          )
          AND (
              $6::text IS NULL
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = 'upstream_account'
                    AND b.binding_code = $6
              )
          )
          AND (
              $7::text IS NULL
              OR r.source_vendor_code ILIKE '%' || $7 || '%'
              OR r.target_vendor_code ILIKE '%' || $7 || '%'
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_item i
                  WHERE i.rule_id = r.id
                    AND i.tenant_id = r.tenant_id
                    AND i.organization_id = r.organization_id
                    AND i.deleted_at IS NULL
                    AND i.status = 1
                    AND (i.source_model ILIKE '%' || $7 || '%' OR i.target_model ILIKE '%' || $7 || '%')
              )
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND (COALESCE(b.binding_code, '') ILIKE '%' || $7 || '%' OR COALESCE(b.binding_name_snapshot, '') ILIKE '%' || $7 || '%')
              )
          )
        "#,
    )
    .bind(query.subject.tenant_id)
    .bind(query.subject.organization_id)
    .bind(binding_type)
    .bind(vendor_code)
    .bind(account_id)
    .bind(account_code)
    .bind(q)
    .fetch_one(pool)
    .await
    .map_err(|error| store_error("failed to count model mappings", error))?;
    let total_count: i64 = count_row.try_get("total_count").map_err(row_error)?;
    let rows = sqlx::query(mapping_select_sql(
        r#"
        WHERE r.tenant_id = $1
          AND r.organization_id = $2
          AND r.deleted_at IS NULL
          AND r.status = 1
          AND (
              $3::text IS NULL
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = $3
              )
          )
          AND (
              $4::text IS NULL
              OR r.source_vendor_code = $4
              OR r.target_vendor_code = $4
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = 'vendor'
                    AND b.binding_code = $4
              )
          )
          AND (
              $5::bigint IS NULL
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = 'upstream_account'
                    AND b.binding_id = $5
              )
          )
          AND (
              $6::text IS NULL
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND b.binding_type = 'upstream_account'
                    AND b.binding_code = $6
              )
          )
          AND (
              $7::text IS NULL
              OR r.source_vendor_code ILIKE '%' || $7 || '%'
              OR r.target_vendor_code ILIKE '%' || $7 || '%'
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_item i
                  WHERE i.rule_id = r.id
                    AND i.tenant_id = r.tenant_id
                    AND i.organization_id = r.organization_id
                    AND i.deleted_at IS NULL
                    AND i.status = 1
                    AND (i.source_model ILIKE '%' || $7 || '%' OR i.target_model ILIKE '%' || $7 || '%')
              )
              OR EXISTS (
                  SELECT 1 FROM ai_model_mapping_rule_binding b
                  WHERE b.rule_id = r.id
                    AND b.tenant_id = r.tenant_id
                    AND b.organization_id = r.organization_id
                    AND b.deleted_at IS NULL
                    AND b.status = 1
                    AND (COALESCE(b.binding_code, '') ILIKE '%' || $7 || '%' OR COALESCE(b.binding_name_snapshot, '') ILIKE '%' || $7 || '%')
              )
          )
        ORDER BY
          CASE COALESCE((
              SELECT b.binding_type
              FROM ai_model_mapping_rule_binding b
              WHERE b.rule_id = r.id
                AND b.tenant_id = r.tenant_id
                AND b.organization_id = r.organization_id
                AND b.deleted_at IS NULL
                AND b.status = 1
                AND b.enabled = TRUE
              ORDER BY CASE b.binding_type
                  WHEN 'upstream_account' THEN 0
                  WHEN 'upstream_account_group' THEN 1
                  WHEN 'supplier_endpoint' THEN 2
                  WHEN 'upstream_supplier' THEN 3
                  WHEN 'vendor' THEN 4
                  WHEN 'global' THEN 5
                  ELSE 6
              END, b.sort_order ASC, b.id ASC
              LIMIT 1
          ), 'global')
              WHEN 'upstream_account' THEN 0
              WHEN 'upstream_account_group' THEN 1
              WHEN 'supplier_endpoint' THEN 2
              WHEN 'upstream_supplier' THEN 3
              WHEN 'vendor' THEN 4
              WHEN 'global' THEN 5
              ELSE 6
          END,
          r.updated_at DESC,
          r.id DESC
        LIMIT $8
        OFFSET $9
        "#,
    ))
    .bind(query.subject.tenant_id)
    .bind(query.subject.organization_id)
    .bind(binding_type)
    .bind(vendor_code)
    .bind(account_id)
    .bind(account_code)
    .bind(q)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|error| store_error("failed to list model mappings", error))?;
    let mut items = rows
        .into_iter()
        .map(mapping_from_row)
        .collect::<DomainResult<Vec<_>>>()?;
    attach_model_mapping_children(pool, &mut items).await?;
    Ok(AdminModelMappingListPage { items, total_count })
}

async fn list_vendors_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    organization_id: i64,
) -> DomainResult<Vec<AdminModelVendorItem>> {
    let rows = sqlx::query(vendor_select_sql(
        r#"
        WHERE (tenant_id = 0 OR tenant_id = $1)
          AND (organization_id = 0 OR organization_id = $2)
          AND deleted_at IS NULL
        ORDER BY
          CASE WHEN tenant_id = $3 AND organization_id = $4 THEN 0 ELSE 1 END,
          COALESCE(sort_order, 1000000) ASC,
          display_name ASC NULLS LAST,
          id ASC
        "#,
    ))
    .bind(tenant_id)
    .bind(organization_id)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error("failed to list model vendors", error))?;
    rows.into_iter().map(vendor_from_row).collect()
}

async fn list_models_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    organization_id: i64,
) -> DomainResult<Vec<AdminAiModelItem>> {
    let rows = sqlx::query(model_select_sql(
        r#"
        WHERE (m.tenant_id = 0 OR m.tenant_id = $1)
          AND (m.organization_id = 0 OR m.organization_id = $2)
          AND m.deleted_at IS NULL
          AND NOT EXISTS (
              SELECT 1
              FROM ai_model tenant_model
              WHERE tenant_model.tenant_id = $3
                AND tenant_model.organization_id = $4
                AND tenant_model.model = m.model
                AND tenant_model.id <> m.id
                AND tenant_model.deleted_at IS NULL
          )
        ORDER BY
          COALESCE(m.rank_score, 0) DESC,
          CASE WHEN m.tenant_id = $5 AND m.organization_id = $6 THEN 0 ELSE 1 END,
          m.display_name ASC NULLS LAST,
          m.id ASC
        "#,
        tenant_id,
        organization_id,
    ))
    .bind(tenant_id)
    .bind(organization_id)
    .bind(tenant_id)
    .bind(organization_id)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error("failed to list ai models", error))?;
    let mut models = rows
        .into_iter()
        .map(model_from_row)
        .collect::<DomainResult<Vec<_>>>()?;
    attach_model_region_prices_tx(tx, &mut models).await?;
    Ok(models)
}

async fn insert_vendor(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateAdminModelVendorCommand,
) -> DomainResult<i64> {
    let id = next_cloud_runtime_id("ai_model_vendor")?;
    sqlx::query_scalar(
        r#"
        INSERT INTO ai_model_vendor
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata, vendor_code, display_name, description, color_token, sort_order, id)
        VALUES
            ($1, $2, $3, 1, $4, $5::timestamptz, $6::timestamptz, 0, '{}'::jsonb, $7, $8, $9, $10, COALESCE((SELECT MAX(sort_order) + 1 FROM ai_model_vendor), 1), $11)
        RETURNING id
        "#,
    )
    .bind(&command.vendor_uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(status_code(&command.status))
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(&command.vendor_code)
    .bind(&command.name)
    .bind(&command.description)
    .bind(&command.color)
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create model vendor", error))
}

async fn insert_model_mapping(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateAdminModelMappingCommand,
) -> DomainResult<i64> {
    let id = next_cloud_runtime_id("ai_model_mapping_rule")?;
    let mapping_id = sqlx::query_scalar(
        r#"
        INSERT INTO ai_model_mapping_rule
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata,
             source_vendor_id, source_vendor_code, target_vendor_id, target_vendor_code,
             mapping_mode, match_type, enabled, id)
        VALUES
            ($1, $2, $3, 0, 1, $4::timestamptz, $4::timestamptz, 0, '{}'::jsonb,
             $5, $6, $7, $8,
             $9, $10, $11, $12)
        RETURNING id
        "#,
    )
    .bind(&command.mapping_uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(&command.requested_at)
    .bind(command.draft.source_vendor_id)
    .bind(&command.draft.source_vendor_code)
    .bind(command.draft.target_vendor_id)
    .bind(&command.draft.target_vendor_code)
    .bind(&command.draft.mapping_mode)
    .bind(&command.draft.match_type)
    .bind(command.draft.enabled)
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create model mapping", error))?;
    insert_model_mapping_bindings(
        tx,
        command.subject,
        mapping_id,
        &command.mapping_uuid,
        &command.binding_uuids,
        &command.draft.bindings,
        &command.requested_at,
    )
    .await?;
    insert_model_mapping_items(
        tx,
        command.subject,
        mapping_id,
        &command.mapping_uuid,
        &command.item_uuids,
        &command.draft.mapping_items,
        &command.requested_at,
    )
    .await?;
    Ok(mapping_id)
}

async fn insert_model_mapping_bindings(
    tx: &mut Transaction<'_, Postgres>,
    subject: AdminModelSubject,
    rule_id: i64,
    rule_uuid: &str,
    binding_uuids: &[String],
    bindings: &[AdminModelMappingRuleBindingDraft],
    requested_at: &str,
) -> DomainResult<()> {
    for (index, binding) in bindings.iter().enumerate() {
        let uuid = binding_uuids
            .get(index)
            .ok_or_else(|| DomainError::new("missing generated model mapping binding uuid"))?;
        insert_model_mapping_binding_row(
            tx,
            subject,
            rule_id,
            rule_uuid,
            uuid,
            binding,
            child_sort_order(index),
            requested_at,
        )
        .await?;
    }
    Ok(())
}

async fn insert_model_mapping_binding_row(
    tx: &mut Transaction<'_, Postgres>,
    subject: AdminModelSubject,
    rule_id: i64,
    rule_uuid: &str,
    binding_uuid: &str,
    binding: &AdminModelMappingRuleBindingDraft,
    sort_order: i32,
    requested_at: &str,
) -> DomainResult<()> {
    let id = next_cloud_runtime_id("ai_model_mapping_rule_binding")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_mapping_rule_binding
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata,
             rule_id, rule_uuid, binding_type, binding_id, binding_code, binding_name_snapshot, sort_order, enabled, id)
        VALUES
            ($1, $2, $3, 0, 1, $4::timestamptz, $4::timestamptz, 0, '{}'::jsonb,
             $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(binding_uuid)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(requested_at)
    .bind(rule_id)
    .bind(rule_uuid)
    .bind(&binding.binding_type)
    .bind(binding.binding_id)
    .bind(binding.binding_code.as_deref())
    .bind(binding.binding_name.as_deref())
    .bind(sort_order)
    .bind(binding.enabled)
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create model mapping binding", error))?;
    Ok(())
}

async fn insert_model_mapping_items(
    tx: &mut Transaction<'_, Postgres>,
    subject: AdminModelSubject,
    rule_id: i64,
    rule_uuid: &str,
    item_uuids: &[String],
    items: &[AdminModelMappingRuleItemDraft],
    requested_at: &str,
) -> DomainResult<()> {
    for (index, item) in items.iter().enumerate() {
        let uuid = item_uuids
            .get(index)
            .ok_or_else(|| DomainError::new("missing generated model mapping item uuid"))?;
        insert_model_mapping_item_row(
            tx,
            subject,
            rule_id,
            rule_uuid,
            uuid,
            item,
            child_sort_order(index),
            requested_at,
        )
        .await?;
    }
    Ok(())
}

async fn insert_model_mapping_item_row(
    tx: &mut Transaction<'_, Postgres>,
    subject: AdminModelSubject,
    rule_id: i64,
    rule_uuid: &str,
    item_uuid: &str,
    item: &AdminModelMappingRuleItemDraft,
    sort_order: i32,
    requested_at: &str,
) -> DomainResult<()> {
    let id = next_cloud_runtime_id("ai_model_mapping_rule_item")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_mapping_rule_item
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata,
             rule_id, rule_uuid, source_model, source_catalog_key, target_model, target_catalog_key,
             target_provider_model, target_provider_native_model, sort_order, enabled, id)
        VALUES
            ($1, $2, $3, 0, 1, $4::timestamptz, $4::timestamptz, 0, '{}'::jsonb,
             $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        "#,
    )
    .bind(item_uuid)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(requested_at)
    .bind(rule_id)
    .bind(rule_uuid)
    .bind(&item.source_model)
    .bind(item.source_catalog_key.as_deref())
    .bind(&item.target_model)
    .bind(item.target_catalog_key.as_deref())
    .bind(item.target_provider_model.as_deref())
    .bind(item.target_provider_native_model.as_deref())
    .bind(sort_order)
    .bind(item.enabled.unwrap_or(true))
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create model mapping item", error))?;
    Ok(())
}

async fn reconcile_model_mapping_bindings(
    tx: &mut Transaction<'_, Postgres>,
    current: &AdminModelMappingRuleItem,
    command: &UpdateAdminModelMappingCommand,
    bindings: &[AdminModelMappingRuleBindingDraft],
) -> DomainResult<()> {
    let mut retained_ids = Vec::new();
    let mut new_uuid_index = 0usize;
    for (index, binding) in bindings.iter().enumerate() {
        let sort_order = child_sort_order(index);
        if let Some(binding_id) = binding.id {
            update_model_mapping_binding_row(tx, current, command, binding_id, binding, sort_order)
                .await?;
            retained_ids.push(binding_id);
        } else {
            let uuid = command
                .binding_uuids
                .get(new_uuid_index)
                .ok_or_else(|| DomainError::new("missing generated model mapping binding uuid"))?;
            new_uuid_index += 1;
            insert_model_mapping_binding_row(
                tx,
                command.subject,
                current.id,
                &current.uuid,
                uuid,
                binding,
                sort_order,
                &command.requested_at,
            )
            .await?;
        }
    }
    soft_delete_omitted_model_mapping_bindings(tx, current, command, &retained_ids).await
}

async fn update_model_mapping_binding_row(
    tx: &mut Transaction<'_, Postgres>,
    current: &AdminModelMappingRuleItem,
    command: &UpdateAdminModelMappingCommand,
    binding_id: i64,
    binding: &AdminModelMappingRuleBindingDraft,
    sort_order: i32,
) -> DomainResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE ai_model_mapping_rule_binding
        SET binding_type = $1,
            binding_id = $2,
            binding_code = $3,
            binding_name_snapshot = $4,
            sort_order = $5,
            enabled = $6,
            status = 1,
            deleted_at = NULL,
            deleted_by = NULL,
            updated_at = $7::timestamptz,
            version = COALESCE(version, 0) + 1
        WHERE id = $8
          AND rule_id = $9
          AND tenant_id = $10
          AND organization_id = $11
        "#,
    )
    .bind(&binding.binding_type)
    .bind(binding.binding_id)
    .bind(binding.binding_code.as_deref())
    .bind(binding.binding_name.as_deref())
    .bind(sort_order)
    .bind(binding.enabled)
    .bind(&command.requested_at)
    .bind(binding_id)
    .bind(current.id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to update model mapping binding", error))?;
    if result.rows_affected() == 0 {
        return Err(DomainError::not_found(
            "model mapping binding was not found",
        ));
    }
    Ok(())
}

async fn soft_delete_omitted_model_mapping_bindings(
    tx: &mut Transaction<'_, Postgres>,
    current: &AdminModelMappingRuleItem,
    command: &UpdateAdminModelMappingCommand,
    retained_ids: &[i64],
) -> DomainResult<()> {
    for binding in &current.bindings {
        if retained_ids.contains(&binding.id) {
            continue;
        }
        sqlx::query(
            r#"
            UPDATE ai_model_mapping_rule_binding
            SET status = 0,
                deleted_at = $1::timestamptz,
                deleted_by = $2,
                updated_at = $1::timestamptz,
                version = COALESCE(version, 0) + 1
            WHERE id = $3
              AND rule_id = $4
              AND tenant_id = $5
              AND organization_id = $6
              AND deleted_at IS NULL
            "#,
        )
        .bind(&command.requested_at)
        .bind(command.subject.operator_id)
        .bind(binding.id)
        .bind(current.id)
        .bind(command.subject.tenant_id)
        .bind(command.subject.organization_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error("failed to delete model mapping binding", error))?;
    }
    Ok(())
}

async fn reconcile_model_mapping_items(
    tx: &mut Transaction<'_, Postgres>,
    current: &AdminModelMappingRuleItem,
    command: &UpdateAdminModelMappingCommand,
    items: &[AdminModelMappingRuleItemDraft],
) -> DomainResult<()> {
    let mut retained_ids = Vec::new();
    let mut new_uuid_index = 0usize;
    for (index, item) in items.iter().enumerate() {
        let sort_order = child_sort_order(index);
        if let Some(item_id) = item.id {
            update_model_mapping_item_row(tx, current, command, item_id, item, sort_order).await?;
            retained_ids.push(item_id);
        } else {
            let uuid = command
                .item_uuids
                .get(new_uuid_index)
                .ok_or_else(|| DomainError::new("missing generated model mapping item uuid"))?;
            new_uuid_index += 1;
            insert_model_mapping_item_row(
                tx,
                command.subject,
                current.id,
                &current.uuid,
                uuid,
                item,
                sort_order,
                &command.requested_at,
            )
            .await?;
        }
    }
    soft_delete_omitted_model_mapping_items(tx, current, command, &retained_ids).await
}

async fn update_model_mapping_item_row(
    tx: &mut Transaction<'_, Postgres>,
    current: &AdminModelMappingRuleItem,
    command: &UpdateAdminModelMappingCommand,
    item_id: i64,
    item: &AdminModelMappingRuleItemDraft,
    sort_order: i32,
) -> DomainResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE ai_model_mapping_rule_item
        SET source_model = $1,
            source_catalog_key = $2,
            target_model = $3,
            target_catalog_key = $4,
            target_provider_model = $5,
            target_provider_native_model = $6,
            sort_order = $7,
            enabled = $8,
            status = 1,
            deleted_at = NULL,
            deleted_by = NULL,
            updated_at = $9::timestamptz,
            version = COALESCE(version, 0) + 1
        WHERE id = $10
          AND rule_id = $11
          AND tenant_id = $12
          AND organization_id = $13
        "#,
    )
    .bind(&item.source_model)
    .bind(item.source_catalog_key.as_deref())
    .bind(&item.target_model)
    .bind(item.target_catalog_key.as_deref())
    .bind(item.target_provider_model.as_deref())
    .bind(item.target_provider_native_model.as_deref())
    .bind(sort_order)
    .bind(item.enabled.unwrap_or(true))
    .bind(&command.requested_at)
    .bind(item_id)
    .bind(current.id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to update model mapping item", error))?;
    if result.rows_affected() == 0 {
        return Err(DomainError::not_found("model mapping item was not found"));
    }
    Ok(())
}

async fn soft_delete_omitted_model_mapping_items(
    tx: &mut Transaction<'_, Postgres>,
    current: &AdminModelMappingRuleItem,
    command: &UpdateAdminModelMappingCommand,
    retained_ids: &[i64],
) -> DomainResult<()> {
    for item in &current.mapping_items {
        if retained_ids.contains(&item.id) {
            continue;
        }
        sqlx::query(
            r#"
            UPDATE ai_model_mapping_rule_item
            SET status = 0,
                deleted_at = $1::timestamptz,
                deleted_by = $2,
                updated_at = $1::timestamptz,
                version = COALESCE(version, 0) + 1
            WHERE id = $3
              AND rule_id = $4
              AND tenant_id = $5
              AND organization_id = $6
              AND deleted_at IS NULL
            "#,
        )
        .bind(&command.requested_at)
        .bind(command.subject.operator_id)
        .bind(item.id)
        .bind(current.id)
        .bind(command.subject.tenant_id)
        .bind(command.subject.organization_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error("failed to delete model mapping item", error))?;
    }
    Ok(())
}

fn child_sort_order(index: usize) -> i32 {
    ((index as i32) + 1) * 100
}

async fn find_vendor(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateAdminAiModelCommand,
) -> DomainResult<VendorIdentity> {
    find_vendor_by_value(tx, command.subject, &command.vendor_id).await
}

async fn find_vendor_by_value(
    tx: &mut Transaction<'_, Postgres>,
    subject: AdminModelSubject,
    vendor_id: &str,
) -> DomainResult<VendorIdentity> {
    let vendor_code = normalize_vendor_lookup(vendor_id);
    let numeric_id = vendor_id.trim().parse::<i64>().ok();
    let row = sqlx::query(
        r#"
        SELECT id, COALESCE(vendor_code, '') AS vendor_code, COALESCE(display_name, vendor_code, '') AS display_name
        FROM ai_model_vendor
        WHERE (tenant_id = 0 OR tenant_id = $1)
          AND (organization_id = 0 OR organization_id = $2)
          AND deleted_at IS NULL
          AND (($3::bigint IS NOT NULL AND id = $4) OR vendor_code = $5 OR display_name = $6)
        ORDER BY
          CASE WHEN tenant_id = $7 AND organization_id = $8 THEN 0 ELSE 1 END,
          CASE WHEN id = $9 THEN 0 ELSE 1 END,
          id ASC
        LIMIT 1
        "#,
    )
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(numeric_id)
    .bind(numeric_id.unwrap_or(0))
    .bind(&vendor_code)
    .bind(vendor_id)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .bind(numeric_id.unwrap_or(0))
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to find model vendor", error))?;
    let Some(row) = row else {
        return Err(DomainError::not_found("model vendor was not found"));
    };
    Ok(VendorIdentity {
        id: row.try_get("id").map_err(row_error)?,
        code: row.try_get("vendor_code").map_err(row_error)?,
        name: row.try_get("display_name").map_err(row_error)?,
    })
}

async fn find_vendor_by_id_value(
    tx: &mut Transaction<'_, Postgres>,
    subject: AdminModelSubject,
    vendor_id: i64,
) -> DomainResult<VendorIdentity> {
    let row = sqlx::query(
        r#"
        SELECT id, COALESCE(vendor_code, '') AS vendor_code, COALESCE(display_name, vendor_code, '') AS display_name
        FROM ai_model_vendor
        WHERE id = $1
          AND (tenant_id = 0 OR tenant_id = $2)
          AND (organization_id = 0 OR organization_id = $3)
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(vendor_id)
    .bind(subject.tenant_id)
    .bind(subject.organization_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to find model vendor", error))?;
    let Some(row) = row else {
        return Err(DomainError::not_found("model vendor was not found"));
    };
    Ok(VendorIdentity {
        id: row.try_get("id").map_err(row_error)?,
        code: row.try_get("vendor_code").map_err(row_error)?,
        name: row.try_get("display_name").map_err(row_error)?,
    })
}

async fn insert_model(
    tx: &mut Transaction<'_, Postgres>,
    command: &CreateAdminAiModelCommand,
    vendor: &VendorIdentity,
) -> DomainResult<i64> {
    let modalities = json_array_text(&command.modalities)?;
    let input_modalities = json_array_text(&command.input_modalities)?;
    let output_modalities = json_array_text(&command.output_modalities)?;
    let limitations = json_array_text(&command.limitations)?;
    let supported_languages = json_array_text(&command.supported_languages)?;
    let use_cases = json_array_text(&command.use_cases)?;
    let catalog_key = build_model_base_catalog_key(&vendor.code, &command.model);
    let id = next_cloud_runtime_id("ai_model")?;
    sqlx::query_scalar(
        r#"
        INSERT INTO ai_model
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, deleted_at, metadata, catalog_key, model, display_name, vendor_id, vendor_code, vendor_name_snapshot, capability, modalities, input_modalities, output_modalities, description, capability_intro, limitations, supported_languages, use_cases, training_data_cutoff, context_tokens, max_output_tokens, supports_streaming, supports_tools, supports_json_schema, usage_scopes, coding_visible, api_format, release_stage, shelf_state, routing_state, replacement_model, rank_score, id)
        VALUES
            ($1, $2, $3, 1, 1, $4::timestamptz, $5::timestamptz, 0, NULL, '{}'::jsonb, $6, $7, $8, $9, $10, $11, $12, $13::jsonb, $14::jsonb, $15::jsonb, $16, $17, $18::jsonb, $19::jsonb, $20::jsonb, $21, $22, $23, $24, $25, $26, $27::jsonb, $28, $29, $30, $31, $32, $33, NULL, $34)
        RETURNING id
        "#,
    )
    .bind(&command.model_uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(catalog_key)
    .bind(&command.model)
    .bind(&command.display_name)
    .bind(vendor.id)
    .bind(&vendor.code)
    .bind(&vendor.name)
    .bind(capability_code(&command.model_type))
    .bind(modalities)
    .bind(input_modalities)
    .bind(output_modalities)
    .bind(command.description.as_deref())
    .bind(command.capability_intro.as_deref())
    .bind(limitations)
    .bind(supported_languages)
    .bind(use_cases)
    .bind(command.training_data_cutoff.as_deref())
    .bind(command.context_tokens)
    .bind(command.max_output_tokens)
    .bind(command.supports_streaming)
    .bind(command.supports_tools)
    .bind(command.supports_json_schema)
    .bind(json_array_text(&command.usage_scopes)?)
    .bind(command.coding_visible)
    .bind(&command.api_format)
    .bind(command.release_stage)
    .bind(command.shelf_state)
    .bind(command.routing_state)
    .bind(command.replacement_model.as_deref())
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create ai model", error))
}

async fn insert_model_capability(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &CreateAdminAiModelCommand,
    vendor: &VendorIdentity,
) -> DomainResult<()> {
    let capability_code_text = model_capability_code(&command.model_type);
    let catalog_key = build_model_base_catalog_key(&vendor.code, &command.model);
    let id = next_cloud_runtime_id("ai_model_capability")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_capability
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata, model_id, catalog_key, model, vendor_code, capability, capability_code, modality, input_modalities, output_modalities, supported, schema_version, sort_order, id)
        VALUES
            ($1, $2, $3, 1, 1, $4::timestamptz, $5::timestamptz, 0, '{}'::jsonb, $6, $7, $8, $9, $10, $11, $12, $13::jsonb, $14::jsonb, true, 'v1', 1, $15)
        "#,
    )
    .bind(&command.capability_uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(model_id)
    .bind(catalog_key)
    .bind(&command.model)
    .bind(&vendor.code)
    .bind(capability_code(&command.model_type))
    .bind(capability_code_text)
    .bind(modality_code(&command.model_type))
    .bind(json_array_text(&command.input_modalities)?)
    .bind(json_array_text(&command.output_modalities)?)
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create model capability", error))?;
    Ok(())
}

async fn insert_model_region_pricing(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &CreateAdminAiModelCommand,
    vendor: &VendorIdentity,
) -> DomainResult<()> {
    for region_price in &command.region_prices {
        insert_region_model_pricing(
            tx,
            model_id,
            command,
            vendor,
            &region_price.region_code,
            &region_price.currency,
            input_billing_meter(&command.model_type),
            &region_price.price_in,
            1,
            "input",
        )
        .await?;
        insert_region_model_pricing(
            tx,
            model_id,
            command,
            vendor,
            &region_price.region_code,
            &region_price.currency,
            output_billing_meter(&command.model_type),
            &region_price.price_out,
            2,
            "output",
        )
        .await?;
        if let Some(price) = region_price
            .cache_read_price
            .as_deref()
            .filter(|price| !price.trim().is_empty())
        {
            insert_region_model_pricing(
                tx,
                model_id,
                command,
                vendor,
                &region_price.region_code,
                &region_price.currency,
                "llm_cache_read_token",
                price,
                3,
                "cache_read",
            )
            .await?;
        }
        if let Some(price) = region_price
            .cache_write_price
            .as_deref()
            .filter(|price| !price.trim().is_empty())
        {
            insert_region_model_pricing(
                tx,
                model_id,
                command,
                vendor,
                &region_price.region_code,
                &region_price.currency,
                "llm_cache_write_token",
                price,
                4,
                "cache_write",
            )
            .await?;
        }
    }
    Ok(())
}

async fn insert_region_model_pricing(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &CreateAdminAiModelCommand,
    vendor: &VendorIdentity,
    region_code: &str,
    currency: &str,
    meter: &str,
    unit_price: &str,
    priority: i32,
    price_kind: &str,
) -> DomainResult<()> {
    let catalog_key = model_pricing_catalog_key(&vendor.code, &command.model);
    let uuid = stable_uuid(
        "admin-price",
        &[&command.model_uuid, region_code, meter, price_kind],
    );
    let id = next_cloud_runtime_id("ai_model_pricing")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_pricing
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata, model_id, catalog_key, model, vendor_code, region_code, price_side, pricing_scope, billing_type, billing_mode, billing_meter_code, price_item_type, unit, unit_size, metering_mode, quantity_source, minimum_quantity, quantity_step, included_quantity, unit_price, currency, rounding_mode, min_charge_amount, pricing_formula_mode, price_origin, priority, effective_from, id)
        VALUES
            ($1, $2, $3, 1, 1, $4::timestamptz, $5::timestamptz, 0, '{}'::jsonb, $6, $7, $8, $9, $10, $11, 1, 1, 1, $12, 1, 1, 1, 1, 1, 0, 1, 0, $13::numeric, $14, 1, 0, 1, 1, $15, $16::timestamptz, $17)
        "#,
    )
    .bind(uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(model_id)
    .bind(catalog_key)
    .bind(&command.model)
    .bind(&vendor.code)
    .bind(region_code)
    .bind(OFFICIAL_REFERENCE_PRICE_SIDE)
    .bind(meter)
    .bind(unit_price)
    .bind(currency)
    .bind(priority)
    .bind(&command.requested_at)
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create regional model pricing", error))?;
    Ok(())
}

fn filter_sync_catalog_items(
    vendors: &mut Vec<AdminModelVendorItem>,
    models: &mut Vec<AdminAiModelItem>,
    vendor_codes: &[String],
) {
    if vendor_codes.is_empty() {
        return;
    }
    vendors.retain(|vendor| {
        vendor_codes
            .iter()
            .any(|vendor_code| vendor_code == &vendor.vendor_code)
    });
    models.retain(|model| {
        vendor_codes
            .iter()
            .any(|vendor_code| vendor_code == &model.vendor_code)
    });
}

async fn upsert_model_catalog_sync_run(
    tx: &mut Transaction<'_, Postgres>,
    command: &SyncAdminModelCatalogCommand,
    counts: CatalogScopeCounts,
    catalog_version: &str,
    source_hash: &str,
    dry_run: bool,
) -> DomainResult<i64> {
    let source_code = normalize_catalog_source_code(&command.source);
    let source_name = format!("{} model catalog", command.source);
    let source_url = format!("manual://{}", source_code);
    let source_uuid = catalog_source_uuid(
        command.subject.tenant_id,
        command.subject.organization_id,
        &source_code,
    );
    let sync_run_uuid = crate::model_catalog_import::catalog_sync_run_uuid(&command.snapshot_uuid);
    let last_success_at = if dry_run {
        None
    } else {
        Some(command.requested_at.as_str())
    };
    let metadata = serde_json::json!({
        "source": command.source,
        "traceId": command.request_id,
        "catalogVersion": catalog_version,
        "requestedCatalogVersion": &command.catalog_version,
        "catalogRoot": &command.catalog_root,
        "syncMode": command.mode,
        "vendorCodes": command.vendor_codes,
        "force": command.force,
        "sourceHash": source_hash,
        "dryRun": dry_run,
    })
    .to_string();
    let change_summary = serde_json::json!({
        "vendors": "snapshot",
        "models": counts.model_count,
        "accepted": counts.accepted_count(),
        "rejected": 0,
        "mode": command.mode,
        "vendorCodes": command.vendor_codes,
        "force": command.force,
        "catalogVersion": catalog_version,
        "sourceHash": source_hash,
        "dryRun": dry_run,
        "counts": {
            "meters": counts.meter_count,
            "vendors": counts.vendor_count,
            "families": counts.family_count,
            "models": counts.model_count,
            "capabilities": counts.capability_count,
            "prices": counts.price_count,
            "rankings": counts.ranking_count,
            "accepted": counts.accepted_count()
        }
    })
    .to_string();

    let source_insert_id = next_cloud_runtime_id("ai_model_catalog_source")?;
    let source_id: i64 = if dry_run {
        let dry_run_metadata = serde_json::json!({
            "source": command.source,
            "traceId": command.request_id,
            "syncMode": command.mode,
            "vendorCodes": command.vendor_codes,
            "force": command.force,
            "dryRun": true,
            "lastObservationOnly": true,
            "observedSourceHash": source_hash,
        })
        .to_string();
        sqlx::query_scalar(
            r#"
            INSERT INTO ai_model_catalog_source
                (uuid, tenant_id, organization_id, data_scope, status, metadata, source_code, vendor_code, supplier_code, source_name, source_url, source_kind, trust_level, parser_kind, refresh_interval_seconds, last_observed_at, last_success_at, catalog_version, source_hash, id)
            VALUES
                ($1, $2, $3, 1, 1, $4::jsonb, $5, 'mixed', NULL, $6, $7, 2, 1, 'manual_refresh', 21600, $8::timestamptz, $9::timestamptz, $10, $11, $12)
            ON CONFLICT(tenant_id, organization_id, source_code) DO UPDATE SET
                updated_at = CURRENT_TIMESTAMP,
                source_name = excluded.source_name,
                source_url = excluded.source_url,
                last_observed_at = excluded.last_observed_at,
                deleted_at = NULL,
                deleted_by = NULL,
                status = excluded.status
            RETURNING id
            "#,
        )
        .bind(&source_uuid)
        .bind(command.subject.tenant_id)
        .bind(command.subject.organization_id)
        .bind(&dry_run_metadata)
        .bind(&source_code)
        .bind(&source_name)
        .bind(&source_url)
        .bind(&command.requested_at)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(Option::<&str>::None)
        .bind(source_insert_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| store_error("failed to upsert model catalog source", error))?
    } else {
        sqlx::query_scalar(
            r#"
            INSERT INTO ai_model_catalog_source
                (uuid, tenant_id, organization_id, data_scope, status, metadata, source_code, vendor_code, supplier_code, source_name, source_url, source_kind, trust_level, parser_kind, refresh_interval_seconds, last_observed_at, last_success_at, catalog_version, source_hash, id)
            VALUES
                ($1, $2, $3, 1, 1, $4::jsonb, $5, 'mixed', NULL, $6, $7, 2, 1, 'manual_refresh', 21600, $8::timestamptz, $9::timestamptz, $10, $11, $12)
            ON CONFLICT(tenant_id, organization_id, source_code) DO UPDATE SET
                updated_at = CURRENT_TIMESTAMP,
                metadata = excluded.metadata,
                source_name = excluded.source_name,
                source_url = excluded.source_url,
                last_observed_at = excluded.last_observed_at,
                last_success_at = excluded.last_success_at,
                catalog_version = excluded.catalog_version,
                source_hash = excluded.source_hash,
                deleted_at = NULL,
                deleted_by = NULL,
                status = excluded.status
            RETURNING id
            "#,
        )
        .bind(&source_uuid)
        .bind(command.subject.tenant_id)
        .bind(command.subject.organization_id)
        .bind(&metadata)
        .bind(&source_code)
        .bind(&source_name)
        .bind(&source_url)
        .bind(&command.requested_at)
        .bind(last_success_at)
        .bind(catalog_version)
        .bind(source_hash)
        .bind(source_insert_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| store_error("failed to upsert model catalog source", error))?
    };

    let id = next_cloud_runtime_id("ai_model_catalog_sync_run")?;
    sqlx::query_scalar(
        r#"
        INSERT INTO ai_model_catalog_sync_run
            (uuid, tenant_id, organization_id, source_type, source_id, source_version, status, metadata, source_code, vendor_code, supplier_code, run_status, started_at, finished_at, observed_at, catalog_version, source_hash, observed_vendor_count, observed_model_count, observed_meter_count, observed_price_count, accepted_count, rejected_count, change_summary, id)
        VALUES
            ($1, $2, $3, 'manual_refresh', $4, 1, 1, $5::jsonb, $6, 'mixed', NULL, 1, $7::timestamptz, $8::timestamptz, $9::timestamptz, $10, $11, $12, $13, $14, $15, $16, 0, $17::jsonb, $18)
        RETURNING id
        "#,
    )
    .bind(&sync_run_uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(source_id)
    .bind(metadata)
    .bind(&source_code)
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(catalog_version)
    .bind(source_hash)
    .bind(counts.vendor_count as i64)
    .bind(counts.model_count as i64)
    .bind(counts.meter_count as i64)
    .bind(counts.price_count as i64)
    .bind(counts.accepted_count())
    .bind(change_summary)
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error("failed to insert model catalog sync run", error))
}

async fn load_vendor_by_id(
    tx: &mut Transaction<'_, Postgres>,
    vendor_id: i64,
    tenant_id: i64,
    organization_id: i64,
) -> DomainResult<Option<AdminModelVendorItem>> {
    let row = sqlx::query(vendor_select_sql(
        r#"
        WHERE id = $1
          AND (tenant_id = 0 OR tenant_id = $2)
          AND (organization_id = 0 OR organization_id = $3)
        LIMIT 1
        "#,
    ))
    .bind(vendor_id)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load model vendor", error))?;
    row.map(vendor_from_row).transpose()
}

async fn load_model_by_id(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    tenant_id: i64,
    organization_id: i64,
) -> DomainResult<Option<AdminAiModelItem>> {
    let row = sqlx::query(model_select_sql(
        r#"
        WHERE m.id = $1
          AND (m.tenant_id = 0 OR m.tenant_id = $2)
          AND (m.organization_id = 0 OR m.organization_id = $3)
        LIMIT 1
        "#,
        tenant_id,
        organization_id,
    ))
    .bind(model_id)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load ai model", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut model = model_from_row(row)?;
    attach_single_model_region_prices_tx(tx, &mut model).await?;
    Ok(Some(model))
}

async fn attach_model_region_prices(
    pool: &PgPool,
    models: &mut [AdminAiModelItem],
) -> DomainResult<()> {
    for model in models {
        let region_prices = load_model_region_prices(pool, model.id).await?;
        if !region_prices.is_empty() {
            model.region_prices = region_prices;
        }
    }
    Ok(())
}

async fn attach_model_region_prices_tx(
    tx: &mut Transaction<'_, Postgres>,
    models: &mut [AdminAiModelItem],
) -> DomainResult<()> {
    for model in models {
        attach_single_model_region_prices_tx(tx, model).await?;
    }
    Ok(())
}

async fn attach_single_model_region_prices_tx(
    tx: &mut Transaction<'_, Postgres>,
    model: &mut AdminAiModelItem,
) -> DomainResult<()> {
    let region_prices = load_model_region_prices_tx(tx, model.id).await?;
    if !region_prices.is_empty() {
        model.region_prices = region_prices;
    }
    Ok(())
}

async fn load_model_region_prices(
    pool: &PgPool,
    model_id: i64,
) -> DomainResult<Vec<AdminAiModelRegionPriceCommand>> {
    let rows = sqlx::query(region_pricing_select_sql())
        .bind(model_id)
        .fetch_all(pool)
        .await
        .map_err(|error| store_error("failed to load model regional pricing", error))?;
    region_prices_from_rows(rows)
}

async fn load_model_region_prices_tx(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
) -> DomainResult<Vec<AdminAiModelRegionPriceCommand>> {
    let rows = sqlx::query(region_pricing_select_sql())
        .bind(model_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| store_error("failed to load model regional pricing", error))?;
    region_prices_from_rows(rows)
}

fn region_pricing_select_sql() -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(format!(
        r#"
        SELECT
            COALESCE(NULLIF(region_code, ''), 'global') AS region_code,
            billing_meter_code,
            metadata::text AS metadata,
            unit_price::text AS unit_price,
            COALESCE(
              NULLIF(currency, ''),
              CASE COALESCE(NULLIF(region_code, ''), 'global')
                WHEN 'cn' THEN 'CNY'
                ELSE 'USD'
              END
            ) AS currency
        FROM ai_model_pricing
        WHERE model_id = $1
          AND price_side = {OFFICIAL_REFERENCE_PRICE_SIDE}
          AND status = 1
          AND deleted_at IS NULL
          AND (
              billing_meter_code IN {INPUT_BILLING_METER_FILTER_SQL}
              OR billing_meter_code IN {OUTPUT_BILLING_METER_FILTER_SQL}
              OR billing_meter_code IN {CACHE_READ_BILLING_METER_FILTER_SQL}
              OR billing_meter_code IN {CACHE_WRITE_BILLING_METER_FILTER_SQL}
          )
        ORDER BY
          CASE COALESCE(NULLIF(region_code, ''), 'global')
            WHEN 'cn' THEN 0
            WHEN 'global' THEN 1
            ELSE 2
          END,
          COALESCE(NULLIF(region_code, ''), 'global') ASC,
          priority ASC,
          id ASC
        "#
    ))
}

fn region_prices_from_rows(
    rows: Vec<sqlx::postgres::PgRow>,
) -> DomainResult<Vec<AdminAiModelRegionPriceCommand>> {
    let mut grouped: BTreeMap<String, AdminAiModelRegionPriceCommand> = BTreeMap::new();
    let mut region_order = Vec::<String>::new();
    for row in rows {
        let region_code = row
            .try_get::<String, _>("region_code")
            .map_err(row_error)?
            .trim()
            .to_owned();
        let region_code = if region_code.is_empty() {
            "global".to_owned()
        } else {
            region_code
        };
        let meter = row
            .try_get::<String, _>("billing_meter_code")
            .map_err(row_error)?;
        let metadata = row.try_get::<String, _>("metadata").unwrap_or_default();
        let unit_price = row.try_get::<String, _>("unit_price").map_err(row_error)?;
        let currency = row
            .try_get::<String, _>("currency")
            .map(|value| model_region_price_currency(&value, &region_code))
            .unwrap_or_else(|_| default_currency_for_region(&region_code).to_owned());
        let entry = grouped.entry(region_code.clone()).or_insert_with(|| {
            region_order.push(region_code.clone());
            AdminAiModelRegionPriceCommand {
                region_code,
                currency,
                price_in: String::new(),
                price_out: String::new(),
                cache_read_price: None,
                cache_write_price: None,
            }
        });
        let direction = model_price_direction(&meter, &metadata);
        if direction.allows_input() && entry.price_in.is_empty() {
            entry.price_in = unit_price;
        } else if direction.allows_output() && entry.price_out.is_empty() {
            entry.price_out = unit_price;
        } else if is_cache_read_billing_meter(&meter) && entry.cache_read_price.is_none() {
            entry.cache_read_price = Some(unit_price);
        } else if is_cache_write_billing_meter(&meter) && entry.cache_write_price.is_none() {
            entry.cache_write_price = Some(unit_price);
        }
    }
    Ok(region_order
        .into_iter()
        .filter_map(|region_code| grouped.remove(&region_code))
        .filter(|price| !price.price_in.is_empty() || !price.price_out.is_empty())
        .collect())
}

fn model_region_price_currency(value: &str, region_code: &str) -> String {
    let normalized = value.trim().to_ascii_uppercase();
    if normalized.len() == 3 && normalized.bytes().all(|byte| byte.is_ascii_uppercase()) {
        normalized
    } else {
        default_currency_for_region(region_code).to_owned()
    }
}

fn default_currency_for_region(region_code: &str) -> &'static str {
    match region_code {
        "cn" => "CNY",
        _ => "USD",
    }
}

async fn find_model_for_delete(
    tx: &mut Transaction<'_, Postgres>,
    command: &DeleteAdminAiModelCommand,
) -> DomainResult<AdminAiModelItem> {
    let numeric_id = command.model_id.trim().parse::<i64>().ok();
    let row = sqlx::query(model_select_sql(
        r#"
        WHERE (m.tenant_id = 0 OR m.tenant_id = $1)
          AND (m.organization_id = 0 OR m.organization_id = $2)
          AND m.deleted_at IS NULL
          AND (($3::bigint IS NOT NULL AND m.id = $4) OR m.uuid = $5)
        ORDER BY
          CASE WHEN m.tenant_id = $6 AND m.organization_id = $7 THEN 0 ELSE 1 END,
          CASE WHEN m.id = $8 THEN 0 ELSE 1 END,
          m.id ASC
        LIMIT 1
        "#,
        command.subject.tenant_id,
        command.subject.organization_id,
    ))
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(numeric_id)
    .bind(numeric_id.unwrap_or(0))
    .bind(&command.model_id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(numeric_id.unwrap_or(0))
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to find ai model for delete", error))?;
    row.map(model_from_row)
        .transpose()?
        .ok_or_else(|| DomainError::not_found("ai model was not found"))
}

async fn find_model_for_update(
    tx: &mut Transaction<'_, Postgres>,
    command: &UpdateAdminAiModelCommand,
) -> DomainResult<AdminAiModelItem> {
    let numeric_id = command.model_id.trim().parse::<i64>().ok();
    let row = sqlx::query(model_select_sql(
        r#"
        WHERE (m.tenant_id = 0 OR m.tenant_id = $1)
          AND (m.organization_id = 0 OR m.organization_id = $2)
          AND m.deleted_at IS NULL
          AND (($3::bigint IS NOT NULL AND m.id = $4) OR m.uuid = $5)
        ORDER BY
          CASE WHEN m.tenant_id = $6 AND m.organization_id = $7 THEN 0 ELSE 1 END,
          CASE WHEN m.id = $8 THEN 0 ELSE 1 END,
          m.id ASC
        LIMIT 1
        "#,
        command.subject.tenant_id,
        command.subject.organization_id,
    ))
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(numeric_id)
    .bind(numeric_id.unwrap_or(0))
    .bind(&command.model_id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(numeric_id.unwrap_or(0))
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to find ai model for update", error))?;
    row.map(model_from_row)
        .transpose()?
        .ok_or_else(|| DomainError::not_found("ai model was not found"))
}

async fn load_model_mapping_by_id(
    tx: &mut Transaction<'_, Postgres>,
    mapping_id: i64,
    tenant_id: i64,
    organization_id: i64,
) -> DomainResult<Option<AdminModelMappingRuleItem>> {
    let row = sqlx::query(mapping_select_sql(
        r#"
        WHERE r.id = $1
          AND r.tenant_id = $2
          AND r.organization_id = $3
        LIMIT 1
        "#,
    ))
    .bind(mapping_id)
    .bind(tenant_id)
    .bind(organization_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to load model mapping", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let mut item = mapping_from_row(row)?;
    attach_model_mapping_children_to_rule(tx, &mut item).await?;
    Ok(Some(item))
}

async fn find_model_mapping_for_update(
    tx: &mut Transaction<'_, Postgres>,
    command: &UpdateAdminModelMappingCommand,
) -> DomainResult<AdminModelMappingRuleItem> {
    let numeric_id = command.mapping_id.trim().parse::<i64>().ok();
    let row = sqlx::query(mapping_select_sql(
        r#"
        WHERE r.tenant_id = $1
          AND r.organization_id = $2
          AND r.deleted_at IS NULL
          AND ($3::bigint IS NOT NULL AND r.id = $4 OR r.uuid = $5)
        ORDER BY CASE WHEN r.id = $6 THEN 0 ELSE 1 END, r.id ASC
        LIMIT 1
        "#,
    ))
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(numeric_id)
    .bind(numeric_id.unwrap_or(0))
    .bind(&command.mapping_id)
    .bind(numeric_id.unwrap_or(0))
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to find model mapping for update", error))?;
    let Some(row) = row else {
        return Err(DomainError::not_found("model mapping was not found"));
    };
    let mut item = mapping_from_row(row)?;
    attach_model_mapping_children_to_rule(tx, &mut item).await?;
    Ok(item)
}

async fn find_model_mapping_for_delete(
    tx: &mut Transaction<'_, Postgres>,
    command: &DeleteAdminModelMappingCommand,
) -> DomainResult<AdminModelMappingRuleItem> {
    let numeric_id = command.mapping_id.trim().parse::<i64>().ok();
    let row = sqlx::query(mapping_select_sql(
        r#"
        WHERE r.tenant_id = $1
          AND r.organization_id = $2
          AND r.deleted_at IS NULL
          AND ($3::bigint IS NOT NULL AND r.id = $4 OR r.uuid = $5)
        ORDER BY CASE WHEN r.id = $6 THEN 0 ELSE 1 END, r.id ASC
        LIMIT 1
        "#,
    ))
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(numeric_id)
    .bind(numeric_id.unwrap_or(0))
    .bind(&command.mapping_id)
    .bind(numeric_id.unwrap_or(0))
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| store_error("failed to find model mapping for delete", error))?;
    let Some(row) = row else {
        return Err(DomainError::not_found("model mapping was not found"));
    };
    let mut item = mapping_from_row(row)?;
    attach_model_mapping_children_to_rule(tx, &mut item).await?;
    Ok(item)
}

async fn update_model_mapping_row(
    tx: &mut Transaction<'_, Postgres>,
    current: &AdminModelMappingRuleItem,
    command: &UpdateAdminModelMappingCommand,
) -> DomainResult<()> {
    let source_vendor_id = command
        .patch
        .source_vendor_id
        .unwrap_or(current.source_vendor_id);
    let source_vendor_code = command
        .patch
        .source_vendor_code
        .clone()
        .unwrap_or_else(|| current.source_vendor_code.clone().unwrap_or_default());
    let target_vendor_id = command
        .patch
        .target_vendor_id
        .unwrap_or(current.target_vendor_id);
    let target_vendor_code = command
        .patch
        .target_vendor_code
        .clone()
        .unwrap_or_else(|| current.target_vendor_code.clone().unwrap_or_default());
    let mapping_mode = command
        .patch
        .mapping_mode
        .clone()
        .unwrap_or_else(|| current.mapping_mode.clone());
    let match_type = command
        .patch
        .match_type
        .clone()
        .unwrap_or_else(|| current.match_type.clone());
    let enabled = command.patch.enabled.unwrap_or(current.enabled);
    sqlx::query(
        r#"
        UPDATE ai_model_mapping_rule
        SET source_vendor_id = $1,
            source_vendor_code = $2,
            target_vendor_id = $3,
            target_vendor_code = $4,
            mapping_mode = $5,
            match_type = $6,
            enabled = $7,
            updated_at = $8::timestamptz,
            version = COALESCE(version, 0) + 1
        WHERE id = $9
          AND tenant_id = $10
          AND organization_id = $11
          AND deleted_at IS NULL
        "#,
    )
    .bind(source_vendor_id)
    .bind(&source_vendor_code)
    .bind(target_vendor_id)
    .bind(&target_vendor_code)
    .bind(&mapping_mode)
    .bind(&match_type)
    .bind(enabled)
    .bind(&command.requested_at)
    .bind(current.id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to update model mapping", error))?;
    Ok(())
}

fn effective_model_update(
    current: &AdminAiModelItem,
    command: &UpdateAdminAiModelCommand,
) -> EffectiveModelUpdate {
    let model_type = command
        .model_type
        .clone()
        .unwrap_or_else(|| current.model_type.clone());
    let next_model = command
        .model
        .clone()
        .unwrap_or_else(|| current.model.clone());
    let display_name = match command.display_name.clone() {
        Some(Some(display_name)) => display_name,
        Some(None) => next_model.clone(),
        None if current.display_name.trim().is_empty() || current.display_name == current.model => {
            next_model.clone()
        }
        None => current.display_name.clone(),
    };
    EffectiveModelUpdate {
        model: next_model,
        display_name,
        model_type,
        status: command
            .status
            .clone()
            .unwrap_or_else(|| current.status.clone()),
        description: command
            .description
            .clone()
            .unwrap_or_else(|| current.description.clone()),
        modalities: command
            .modalities
            .clone()
            .unwrap_or_else(|| current.modalities.clone()),
        input_modalities: command
            .input_modalities
            .clone()
            .unwrap_or_else(|| current.input_modalities.clone()),
        output_modalities: command
            .output_modalities
            .clone()
            .unwrap_or_else(|| current.output_modalities.clone()),
        api_format: command
            .api_format
            .clone()
            .or_else(|| current.api_format.clone()),
        capability_intro: command
            .capability_intro
            .clone()
            .unwrap_or_else(|| current.capability_intro.clone()),
        limitations: command
            .limitations
            .clone()
            .unwrap_or_else(|| current.limitations.clone()),
        supported_languages: command
            .supported_languages
            .clone()
            .unwrap_or_else(|| current.supported_languages.clone()),
        use_cases: command
            .use_cases
            .clone()
            .unwrap_or_else(|| current.use_cases.clone()),
        training_data_cutoff: command
            .training_data_cutoff
            .clone()
            .unwrap_or_else(|| current.training_data_cutoff.clone()),
        context_tokens: command.context_tokens.or(current.context_tokens),
        max_output_tokens: command
            .max_output_tokens
            .unwrap_or(current.max_output_tokens),
        supports_streaming: command
            .supports_streaming
            .unwrap_or(current.supports_streaming),
        supports_tools: command.supports_tools.unwrap_or(current.supports_tools),
        supports_json_schema: command
            .supports_json_schema
            .unwrap_or(current.supports_json_schema),
        usage_scopes: command
            .usage_scopes
            .clone()
            .unwrap_or_else(|| current.usage_scopes.clone()),
        coding_visible: command.coding_visible.unwrap_or(current.coding_visible),
        release_stage: command.release_stage.or(current.release_stage).unwrap_or(1),
        shelf_state: command.shelf_state.or(current.shelf_state).unwrap_or(1),
        routing_state: command.routing_state.or(current.routing_state).unwrap_or(1),
        replacement_model: command
            .replacement_model
            .clone()
            .unwrap_or_else(|| current.replacement_model.clone()),
    }
}

async fn update_model_status_only(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &UpdateAdminAiModelCommand,
    status: &str,
) -> DomainResult<()> {
    sqlx::query(
        r#"
        UPDATE ai_model
        SET status = $1,
            updated_at = $2::timestamptz,
            version = COALESCE(version, 0) + 1
        WHERE id = $3
          AND deleted_at IS NULL
        "#,
    )
    .bind(status_code(status))
    .bind(&command.requested_at)
    .bind(model_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to update ai model status", error))?;
    Ok(())
}

async fn update_model_core(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &UpdateAdminAiModelCommand,
    vendor: &VendorIdentity,
    update: &EffectiveModelUpdate,
) -> DomainResult<()> {
    sqlx::query(
        r#"
        UPDATE ai_model
        SET status = $1,
            updated_at = $2::timestamptz,
            version = COALESCE(version, 0) + 1,
            model = $3,
            display_name = $4,
            vendor_id = $5,
            vendor_code = $6,
            catalog_key = $7,
            vendor_name_snapshot = $8,
            capability = $9,
            modalities = $10::jsonb,
            input_modalities = $11::jsonb,
            output_modalities = $12::jsonb,
            description = $13,
            capability_intro = $14,
            limitations = $15::jsonb,
            supported_languages = $16::jsonb,
            use_cases = $17::jsonb,
            training_data_cutoff = $18,
            context_tokens = $19,
            max_output_tokens = $20,
            supports_streaming = $21,
            supports_tools = $22,
            supports_json_schema = $23,
            usage_scopes = $24::jsonb,
            coding_visible = $25,
            api_format = $26,
            release_stage = $27,
            shelf_state = $28,
            routing_state = $29,
            replacement_model = $30
        WHERE id = $31
          AND deleted_at IS NULL
        "#,
    )
    .bind(status_code(&update.status))
    .bind(&command.requested_at)
    .bind(&update.model)
    .bind(&update.display_name)
    .bind(vendor.id)
    .bind(&vendor.code)
    .bind(build_model_base_catalog_key(&vendor.code, &update.model))
    .bind(&vendor.name)
    .bind(capability_code(&update.model_type))
    .bind(json_array_text(&update.modalities)?)
    .bind(json_array_text(&update.input_modalities)?)
    .bind(json_array_text(&update.output_modalities)?)
    .bind(update.description.as_deref())
    .bind(update.capability_intro.as_deref())
    .bind(json_array_text(&update.limitations)?)
    .bind(json_array_text(&update.supported_languages)?)
    .bind(json_array_text(&update.use_cases)?)
    .bind(update.training_data_cutoff.as_deref())
    .bind(update.context_tokens)
    .bind(update.max_output_tokens)
    .bind(update.supports_streaming)
    .bind(update.supports_tools)
    .bind(update.supports_json_schema)
    .bind(json_array_text(&update.usage_scopes)?)
    .bind(update.coding_visible)
    .bind(update.api_format.as_deref())
    .bind(update.release_stage)
    .bind(update.shelf_state)
    .bind(update.routing_state)
    .bind(update.replacement_model.as_deref())
    .bind(model_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to update ai model", error))?;
    Ok(())
}

async fn upsert_model_capability(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &UpdateAdminAiModelCommand,
    vendor: &VendorIdentity,
    update: &EffectiveModelUpdate,
) -> DomainResult<()> {
    let capability_code_text = model_capability_code(&update.model_type);
    let result = sqlx::query(
        r#"
        UPDATE ai_model_capability
        SET status = 1,
            deleted_at = NULL,
            updated_at = $1::timestamptz,
            model = $2,
            vendor_code = $3,
            catalog_key = $4,
            capability = $5,
            capability_code = $6,
            modality = $7,
            input_modalities = $8::jsonb,
            output_modalities = $9::jsonb,
            supported = true
        WHERE model_id = $10
          AND deleted_at IS NULL
        "#,
    )
    .bind(&command.requested_at)
    .bind(&update.model)
    .bind(&vendor.code)
    .bind(build_model_base_catalog_key(&vendor.code, &update.model))
    .bind(capability_code(&update.model_type))
    .bind(capability_code_text)
    .bind(modality_code(&update.model_type))
    .bind(json_array_text(&update.input_modalities)?)
    .bind(json_array_text(&update.output_modalities)?)
    .bind(model_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to update model capability", error))?;
    if result.rows_affected() > 0 {
        return Ok(());
    }
    let id = next_cloud_runtime_id("ai_model_capability")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_capability
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata, model_id, catalog_key, model, vendor_code, capability, capability_code, modality, input_modalities, output_modalities, supported, schema_version, sort_order, id)
        VALUES
            ($1, $2, $3, 1, 1, $4::timestamptz, $5::timestamptz, 0, '{}'::jsonb, $6, $7, $8, $9, $10, $11, $12, $13::jsonb, $14::jsonb, true, 'v1', 1, $15)
        "#,
    )
    .bind(&command.capability_uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(model_id)
    .bind(build_model_base_catalog_key(&vendor.code, &update.model))
    .bind(&update.model)
    .bind(&vendor.code)
    .bind(capability_code(&update.model_type))
    .bind(capability_code_text)
    .bind(modality_code(&update.model_type))
    .bind(json_array_text(&update.input_modalities)?)
    .bind(json_array_text(&update.output_modalities)?)
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to create model capability during update", error))?;
    Ok(())
}

async fn replace_model_region_pricing(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &UpdateAdminAiModelCommand,
    vendor: &VendorIdentity,
    update: &EffectiveModelUpdate,
    region_prices: &[sdkwork_models_contract_service::AdminAiModelRegionPriceCommand],
) -> DomainResult<()> {
    sqlx::query(
        r#"
        UPDATE ai_model_pricing
        SET status = 0,
            deleted_at = $1::timestamptz,
            updated_at = $2::timestamptz
        WHERE model_id = $3
          AND price_side = $4
          AND deleted_at IS NULL
        "#,
    )
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(model_id)
    .bind(OFFICIAL_REFERENCE_PRICE_SIDE)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to replace regional model pricing", error))?;

    for region_price in region_prices {
        insert_update_region_model_pricing(
            tx,
            model_id,
            command,
            vendor,
            update,
            &region_price.region_code,
            &region_price.currency,
            input_billing_meter(&update.model_type),
            &region_price.price_in,
            1,
            "input",
        )
        .await?;
        insert_update_region_model_pricing(
            tx,
            model_id,
            command,
            vendor,
            update,
            &region_price.region_code,
            &region_price.currency,
            output_billing_meter(&update.model_type),
            &region_price.price_out,
            2,
            "output",
        )
        .await?;
        if let Some(price) = region_price
            .cache_read_price
            .as_deref()
            .filter(|price| !price.trim().is_empty())
        {
            insert_update_region_model_pricing(
                tx,
                model_id,
                command,
                vendor,
                update,
                &region_price.region_code,
                &region_price.currency,
                "llm_cache_read_token",
                price,
                3,
                "cache_read",
            )
            .await?;
        }
        if let Some(price) = region_price
            .cache_write_price
            .as_deref()
            .filter(|price| !price.trim().is_empty())
        {
            insert_update_region_model_pricing(
                tx,
                model_id,
                command,
                vendor,
                update,
                &region_price.region_code,
                &region_price.currency,
                "llm_cache_write_token",
                price,
                4,
                "cache_write",
            )
            .await?;
        }
    }
    Ok(())
}

async fn insert_update_region_model_pricing(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &UpdateAdminAiModelCommand,
    vendor: &VendorIdentity,
    update: &EffectiveModelUpdate,
    region_code: &str,
    currency: &str,
    meter: &str,
    unit_price: &str,
    priority: i32,
    price_kind: &str,
) -> DomainResult<()> {
    let catalog_key = model_pricing_catalog_key(&vendor.code, &update.model);
    let uuid = stable_uuid(
        "admin-price",
        &[
            &command.model_id,
            &command.audit_log_uuid,
            &command.requested_at,
            region_code,
            meter,
            price_kind,
        ],
    );
    let id = next_cloud_runtime_id("ai_model_pricing")?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_pricing
            (uuid, tenant_id, organization_id, data_scope, status, created_at, updated_at, version, metadata, model_id, catalog_key, model, vendor_code, region_code, price_side, pricing_scope, billing_type, billing_mode, billing_meter_code, price_item_type, unit, unit_size, metering_mode, quantity_source, minimum_quantity, quantity_step, included_quantity, unit_price, currency, rounding_mode, min_charge_amount, pricing_formula_mode, price_origin, priority, effective_from, id)
        VALUES
            ($1, $2, $3, 1, 1, $4::timestamptz, $5::timestamptz, 0, '{}'::jsonb, $6, $7, $8, $9, $10, $11, 1, 1, 1, $12, 1, 1, 1, 1, 1, 0, 1, 0, $13::numeric, $14, 1, 0, 1, 1, $15, $16::timestamptz, $17)
        "#,
    )
    .bind(uuid)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .bind(&command.requested_at)
    .bind(&command.requested_at)
    .bind(model_id)
    .bind(catalog_key)
    .bind(&update.model)
    .bind(&vendor.code)
    .bind(region_code)
    .bind(OFFICIAL_REFERENCE_PRICE_SIDE)
    .bind(meter)
    .bind(unit_price)
    .bind(currency)
    .bind(priority)
    .bind(&command.requested_at)
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to replace regional model pricing row", error))?;
    Ok(())
}

async fn soft_delete_model_graph(
    tx: &mut Transaction<'_, Postgres>,
    model_id: i64,
    command: &DeleteAdminAiModelCommand,
) -> DomainResult<()> {
    for statement in [
        r#"
        UPDATE ai_model_pricing
        SET status = 0,
            deleted_at = $1::timestamptz,
            updated_at = $2::timestamptz,
            deleted_by = $3
        WHERE model_id = $4
          AND deleted_at IS NULL
        "#,
        r#"
        UPDATE ai_model_capability
        SET status = 0,
            deleted_at = $1::timestamptz,
            updated_at = $2::timestamptz,
            deleted_by = $3
        WHERE model_id = $4
          AND deleted_at IS NULL
        "#,
        r#"
        UPDATE ai_model
        SET status = 0,
            deleted_at = $1::timestamptz,
            updated_at = $2::timestamptz,
            deleted_by = $3
        WHERE id = $4
          AND deleted_at IS NULL
        "#,
    ] {
        sqlx::query(statement)
            .bind(&command.requested_at)
            .bind(&command.requested_at)
            .bind(command.subject.operator_id)
            .bind(model_id)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error("failed to delete ai model graph", error))?;
    }
    Ok(())
}

async fn soft_delete_model_mapping(
    tx: &mut Transaction<'_, Postgres>,
    mapping_id: i64,
    command: &DeleteAdminModelMappingCommand,
) -> DomainResult<()> {
    sqlx::query(
        r#"
        UPDATE ai_model_mapping_rule
        SET deleted_at = $1::timestamptz,
            updated_at = $1::timestamptz,
            deleted_by = $2,
            version = COALESCE(version, 0) + 1
        WHERE id = $3
          AND tenant_id = $4
          AND organization_id = $5
          AND deleted_at IS NULL
        "#,
    )
    .bind(&command.requested_at)
    .bind(command.subject.operator_id)
    .bind(mapping_id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to delete model mapping", error))?;
    sqlx::query(
        r#"
        UPDATE ai_model_mapping_rule_item
        SET status = 0,
            deleted_at = $1::timestamptz,
            updated_at = $1::timestamptz,
            deleted_by = $2,
            version = COALESCE(version, 0) + 1
        WHERE rule_id = $3
          AND tenant_id = $4
          AND organization_id = $5
          AND deleted_at IS NULL
        "#,
    )
    .bind(&command.requested_at)
    .bind(command.subject.operator_id)
    .bind(mapping_id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to delete model mapping items", error))?;
    sqlx::query(
        r#"
        UPDATE ai_model_mapping_rule_binding
        SET status = 0,
            deleted_at = $1::timestamptz,
            updated_at = $1::timestamptz,
            deleted_by = $2,
            version = COALESCE(version, 0) + 1
        WHERE rule_id = $3
          AND tenant_id = $4
          AND organization_id = $5
          AND deleted_at IS NULL
        "#,
    )
    .bind(&command.requested_at)
    .bind(command.subject.operator_id)
    .bind(mapping_id)
    .bind(command.subject.tenant_id)
    .bind(command.subject.organization_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to delete model mapping bindings", error))?;
    Ok(())
}

async fn resolve_model_mapping(
    pool: &PgPool,
    query: ResolveAdminModelMappingQuery,
) -> DomainResult<ResolveAdminModelMappingResult> {
    let matched = find_matching_model_mapping(pool, &query).await?;
    let Some(matched) = matched else {
        return Ok(ResolveAdminModelMappingResult {
            source_model: query.source_model.clone(),
            target_model: query.source_model,
            target_catalog_key: None,
            target_vendor_code: query.vendor_code,
            target_provider_model: None,
            target_provider_native_model: None,
            matched: false,
            matched_binding_type: None,
            rule: None,
        });
    };
    Ok(ResolveAdminModelMappingResult {
        source_model: query.source_model,
        target_model: matched.item.target_model.clone(),
        target_catalog_key: matched.item.target_catalog_key.clone(),
        target_vendor_code: matched.rule.target_vendor_code.clone(),
        target_provider_model: matched.item.target_provider_model.clone(),
        target_provider_native_model: matched.item.target_provider_native_model.clone(),
        matched: true,
        matched_binding_type: Some(matched.binding_type),
        rule: Some(matched.rule),
    })
}

async fn find_matching_model_mapping(
    pool: &PgPool,
    query: &ResolveAdminModelMappingQuery,
) -> DomainResult<Option<ResolvedModelMappingMatch>> {
    let row = sqlx::query(mapping_match_select_sql(
        r#"
        JOIN ai_model_mapping_rule_item i
          ON i.rule_id = r.id
         AND i.tenant_id = r.tenant_id
         AND i.organization_id = r.organization_id
         AND i.deleted_at IS NULL
         AND i.status = 1
         AND i.enabled = TRUE
         AND i.source_model = $1
        JOIN ai_model_mapping_rule_binding b
          ON b.rule_id = r.id
         AND b.tenant_id = r.tenant_id
         AND b.organization_id = r.organization_id
         AND b.deleted_at IS NULL
         AND b.status = 1
         AND b.enabled = TRUE
        WHERE r.tenant_id = $2
          AND r.organization_id = $3
          AND r.deleted_at IS NULL
          AND r.status = 1
          AND r.enabled = TRUE
          AND r.match_type = 'exact'
          AND (
              (b.binding_type = 'upstream_account' AND (($4::bigint IS NOT NULL AND b.binding_id = $4) OR ($5::text IS NOT NULL AND b.binding_code = $5)))
              OR (b.binding_type = 'upstream_account_group' AND (
                  ($6::bigint IS NOT NULL AND b.binding_id = $6)
                  OR ($7::text IS NOT NULL AND b.binding_code = $7)
                  OR EXISTS (
                  SELECT 1
                  FROM ai_upstream_account_group_member gm
                  JOIN ai_upstream_account a
                    ON a.id = gm.account_id
                   AND a.tenant_id = gm.tenant_id
                   AND a.organization_id = gm.organization_id
                   AND a.deleted_at IS NULL
                   AND a.status = 1
                  LEFT JOIN ai_upstream_account_group g
                    ON g.id = gm.account_group_id
                   AND g.tenant_id = gm.tenant_id
                   AND g.organization_id = gm.organization_id
                   AND g.deleted_at IS NULL
                   AND g.status = 1
                  WHERE gm.tenant_id = r.tenant_id
                    AND gm.organization_id = r.organization_id
                    AND gm.deleted_at IS NULL
                    AND gm.status = 1
                    AND (($4::bigint IS NOT NULL AND a.id = $4) OR ($5::text IS NOT NULL AND a.account_code = $5))
                    AND (b.binding_id = gm.account_group_id OR b.binding_code = g.group_code)
                  )
              ))
              OR (b.binding_type = 'supplier_endpoint' AND (($8::bigint IS NOT NULL AND b.binding_id = $8) OR ($9::text IS NOT NULL AND b.binding_code = $9)))
              OR (b.binding_type = 'upstream_supplier' AND (
                  ($10::bigint IS NOT NULL AND b.binding_id = $10)
                  OR ($11::text IS NOT NULL AND b.binding_code = $11)
                  OR EXISTS (
                      SELECT 1
                      FROM ai_upstream_account a
                      WHERE a.tenant_id = r.tenant_id
                        AND a.organization_id = r.organization_id
                        AND a.deleted_at IS NULL
                        AND a.status = 1
                        AND (($4::bigint IS NOT NULL AND a.id = $4) OR ($5::text IS NOT NULL AND a.account_code = $5))
                        AND (b.binding_id = a.supplier_id OR b.binding_code = a.supplier_code)
                  )
              ))
              OR (b.binding_type = 'vendor' AND $12::text IS NOT NULL AND b.binding_code = $12)
              OR b.binding_type = 'global'
          )
        ORDER BY CASE b.binding_type
              WHEN 'upstream_account' THEN 0
              WHEN 'upstream_account_group' THEN 1
              WHEN 'supplier_endpoint' THEN 2
              WHEN 'upstream_supplier' THEN 3
              WHEN 'vendor' THEN 4
              WHEN 'global' THEN 5
              ELSE 7
          END,
          b.sort_order ASC,
          i.sort_order ASC,
          r.updated_at DESC,
          r.id DESC
        LIMIT 1
        "#,
    ))
    .bind(&query.source_model)
    .bind(query.subject.tenant_id)
    .bind(query.subject.organization_id)
    .bind(query.account_id)
    .bind(query.account_code.as_deref())
    .bind(query.account_group_id)
    .bind(query.account_group_code.as_deref())
    .bind(query.endpoint_id)
    .bind(query.endpoint_code.as_deref())
    .bind(query.supplier_id)
    .bind(query.supplier_code.as_deref())
    .bind(query.vendor_code.as_deref())
    .fetch_optional(pool)
    .await
    .map_err(|error| store_error("failed to resolve model mapping", error))?;
    let Some(row) = row else {
        return Ok(None);
    };
    let item = mapping_rule_item_from_joined_row(&row)?;
    let binding_type = row.try_get("matched_binding_type").map_err(row_error)?;
    let mut rule = mapping_from_row(row)?;
    attach_model_mapping_children(pool, std::slice::from_mut(&mut rule)).await?;
    Ok(Some(ResolvedModelMappingMatch {
        rule,
        item,
        binding_type,
    }))
}

async fn insert_audit_log(
    tx: &mut Transaction<'_, Postgres>,
    audit_log_uuid: &str,
    request_id: &str,
    tenant_id: i64,
    organization_id: i64,
    operator_id: i64,
    operator_type: i32,
    action: &'static str,
    target_type: i32,
    target_id: i64,
    change_summary: serde_json::Value,
) -> DomainResult<()> {
    let id = next_cloud_runtime_id("ops_audit_log")?;
    sqlx::query(
        r#"
        INSERT INTO ops_audit_log
            (uuid, tenant_id, organization_id, action, target_type, target_id, request_id, operator_id, operator_type, change_summary, id)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb, $11)
        "#,
    )
    .bind(audit_log_uuid)
    .bind(tenant_id)
    .bind(organization_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(request_id)
    .bind(operator_id)
    .bind(operator_type)
    .bind(change_summary.to_string())
    .bind(id)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to write admin model audit log", error))?;
    Ok(())
}

fn vendor_select_sql(predicate: &'static str) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(format!(
        r#"
        SELECT
            id,
            COALESCE(uuid, '') AS uuid,
            COALESCE(tenant_id, 0) AS tenant_id,
            COALESCE(organization_id, 0) AS organization_id,
            COALESCE(vendor_code, '') AS vendor_code,
            COALESCE(display_name, vendor_code, '') AS name,
            COALESCE(description, '') AS description,
            COALESCE(color_token, 'bg-slate-700') AS color,
            COALESCE(supported_protocols, '[]'::jsonb)::text AS supported_protocols,
            COALESCE(client_api_compatibility, '{{}}'::jsonb)::text AS client_api_compatibility,
            status,
            deleted_at::text AS deleted_at
        FROM ai_model_vendor
        {predicate}
        "#
    ))
}

fn model_select_sql(
    predicate: &str,
    ranking_tenant_id: i64,
    ranking_organization_id: i64,
) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(format!(
        r#"
        WITH selected_rank_snapshot AS (
            SELECT
                s.tenant_id,
                s.organization_id,
                s.snapshot_date,
                s.snapshot_period,
                lower(COALESCE(s.rank_scope, 'commercial-default')) AS rank_scope
            FROM ai_model_rank_snapshot s
            WHERE s.status = 1
              AND (
                  ({ranking_tenant_id} > 0 AND s.tenant_id = {ranking_tenant_id} AND s.organization_id = {ranking_organization_id})
                  OR ({ranking_tenant_id} > 0 AND {ranking_organization_id} > 0 AND s.tenant_id = {ranking_tenant_id} AND s.organization_id = 0)
                  OR (s.tenant_id = 0 AND s.organization_id = 0)
              )
              AND lower(COALESCE(s.rank_scope, 'commercial-default')) = 'commercial-default'
            GROUP BY
                s.tenant_id,
                s.organization_id,
                s.snapshot_date,
                s.snapshot_period,
                lower(COALESCE(s.rank_scope, 'commercial-default'))
            ORDER BY
                CASE
                    WHEN {ranking_tenant_id} > 0 AND s.tenant_id = {ranking_tenant_id} AND s.organization_id = {ranking_organization_id} THEN 3
                    WHEN {ranking_tenant_id} > 0 AND {ranking_organization_id} > 0 AND s.tenant_id = {ranking_tenant_id} AND s.organization_id = 0 THEN 2
                    WHEN s.tenant_id = 0 AND s.organization_id = 0 THEN 1
                    ELSE 0
                END DESC,
                s.snapshot_date DESC NULLS LAST,
                s.snapshot_period DESC NULLS LAST
            LIMIT 1
        ),
        selected_rank_calls AS (
            SELECT
                r.model,
                MAX(COALESCE(r.request_count, r.base_volume, 0))::text AS calls
            FROM ai_model_rank_snapshot r
            JOIN selected_rank_snapshot s
              ON r.tenant_id = s.tenant_id
             AND r.organization_id = s.organization_id
             AND COALESCE(r.snapshot_date, DATE '0001-01-01') = COALESCE(s.snapshot_date, DATE '0001-01-01')
             AND COALESCE(r.snapshot_period, -1) = COALESCE(s.snapshot_period, -1)
             AND lower(COALESCE(r.rank_scope, 'commercial-default')) = s.rank_scope
            WHERE r.status = 1
            GROUP BY r.model
        )
        SELECT
            m.id,
            COALESCE(m.uuid, '') AS uuid,
            COALESCE(m.tenant_id, 0) AS tenant_id,
            COALESCE(m.organization_id, 0) AS organization_id,
            COALESCE(m.vendor_id, 0)::text AS vendor_id,
            COALESCE(m.vendor_code, '') AS vendor_code,
            COALESCE(
                NULLIF(v.display_name, ''),
                NULLIF(m.vendor_name_snapshot, ''),
                m.vendor_code,
                ''
            ) AS vendor_name,
            COALESCE(m.catalog_key, COALESCE(m.vendor_code, '') || '/' || COALESCE(m.model, '')) AS catalog_key,
            COALESCE(m.model, '') AS model,
            COALESCE(NULLIF(m.display_name, ''), m.model, '') AS display_name,
            COALESCE(NULLIF(m.display_name, ''), m.model, '') AS name,
            m.capability,
            COALESCE(m.modalities::text, '[]') AS modalities_json,
            COALESCE(m.input_modalities::text, '[]') AS input_modalities_json,
            COALESCE(m.output_modalities::text, '[]') AS output_modalities_json,
            NULLIF(COALESCE(m.description, ''), '') AS description,
            NULLIF(COALESCE(m.api_format, ''), '') AS api_format,
            NULLIF(COALESCE(m.capability_intro, ''), '') AS capability_intro,
            COALESCE(m.limitations::text, '[]') AS limitations_json,
            COALESCE(m.supported_languages::text, '[]') AS supported_languages_json,
            COALESCE(m.use_cases::text, '[]') AS use_cases_json,
            NULLIF(COALESCE(m.training_data_cutoff, ''), '') AS training_data_cutoff,
            COALESCE(rc.calls, '0') AS calls,
            m.status,
            m.context_tokens AS context_tokens,
            m.max_output_tokens AS max_output_tokens,
            COALESCE(m.supports_streaming, false) AS supports_streaming,
            COALESCE(m.supports_tools, false) AS supports_tools,
            COALESCE(m.supports_json_schema, false) AS supports_json_schema,
            COALESCE(m.usage_scopes::text, '[]') AS usage_scopes_json,
            COALESCE(m.coding_visible, TRUE) AS coding_visible,
            m.release_stage AS release_stage,
            m.shelf_state AS shelf_state,
            m.routing_state AS routing_state,
            NULLIF(COALESCE(m.replacement_model, ''), '') AS replacement_model,
            m.deleted_at::text AS deleted_at
        FROM ai_model m
        LEFT JOIN selected_rank_calls rc ON rc.model = m.model
        LEFT JOIN ai_model_vendor v ON v.id = m.vendor_id AND v.deleted_at IS NULL
        {predicate}
        "#
    ))
}

fn mapping_select_sql(predicate: &'static str) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(format!(
        r#"
        SELECT
            r.id,
            COALESCE(r.uuid, '') AS uuid,
            COALESCE(r.tenant_id, 0) AS tenant_id,
            COALESCE(r.organization_id, 0) AS organization_id,
            COALESCE((
                SELECT b.binding_type
                FROM ai_model_mapping_rule_binding b
                WHERE b.rule_id = r.id
                  AND b.tenant_id = r.tenant_id
                  AND b.organization_id = r.organization_id
                  AND b.deleted_at IS NULL
                  AND b.status = 1
                  AND b.enabled = TRUE
                ORDER BY CASE b.binding_type
                    WHEN 'upstream_account' THEN 0
                    WHEN 'upstream_account_group' THEN 1
                    WHEN 'supplier_endpoint' THEN 2
                    WHEN 'upstream_supplier' THEN 3
                    WHEN 'vendor' THEN 4
                    WHEN 'global' THEN 5
                    ELSE 6
                END, b.sort_order ASC, b.id ASC
                LIMIT 1
            ), 'global') AS binding_type,
            r.source_vendor_id AS source_vendor_id,
            NULLIF(COALESCE(r.source_vendor_code, ''), '') AS source_vendor_code,
            r.target_vendor_id AS target_vendor_id,
            NULLIF(COALESCE(r.target_vendor_code, ''), '') AS target_vendor_code,
            COALESCE(r.mapping_mode, 'alias') AS mapping_mode,
            COALESCE(r.match_type, 'exact') AS match_type,
            COALESCE(r.enabled, FALSE) AS enabled,
            CAST(r.created_at AS TEXT) AS created_at,
            CAST(r.updated_at AS TEXT) AS updated_at,
            CAST(r.deleted_at AS TEXT) AS deleted_at
        FROM ai_model_mapping_rule r
        {predicate}
        "#
    ))
}

fn mapping_match_select_sql(predicate: &'static str) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(format!(
        r#"
        SELECT
            r.id,
            COALESCE(r.uuid, '') AS uuid,
            COALESCE(r.tenant_id, 0) AS tenant_id,
            COALESCE(r.organization_id, 0) AS organization_id,
            b.binding_type AS binding_type,
            b.binding_type AS matched_binding_type,
            r.source_vendor_id AS source_vendor_id,
            NULLIF(COALESCE(r.source_vendor_code, ''), '') AS source_vendor_code,
            r.target_vendor_id AS target_vendor_id,
            NULLIF(COALESCE(r.target_vendor_code, ''), '') AS target_vendor_code,
            COALESCE(r.mapping_mode, 'alias') AS mapping_mode,
            COALESCE(r.match_type, 'exact') AS match_type,
            COALESCE(r.enabled, FALSE) AS enabled,
            CAST(r.created_at AS TEXT) AS created_at,
            CAST(r.updated_at AS TEXT) AS updated_at,
            CAST(r.deleted_at AS TEXT) AS deleted_at,
            i.id AS item_id,
            COALESCE(i.uuid, '') AS item_uuid,
            COALESCE(i.source_model, '') AS item_source_model,
            NULLIF(COALESCE(i.source_catalog_key, ''), '') AS item_source_catalog_key,
            COALESCE(i.target_model, '') AS item_target_model,
            NULLIF(COALESCE(i.target_catalog_key, ''), '') AS item_target_catalog_key,
            NULLIF(COALESCE(i.target_provider_model, ''), '') AS item_target_provider_model,
            NULLIF(COALESCE(i.target_provider_native_model, ''), '') AS item_target_provider_native_model,
            COALESCE(i.sort_order, 100) AS item_sort_order,
            COALESCE(i.enabled, FALSE) AS item_enabled,
            CAST(i.created_at AS TEXT) AS item_created_at,
            CAST(i.updated_at AS TEXT) AS item_updated_at,
            CAST(i.deleted_at AS TEXT) AS item_deleted_at
        FROM ai_model_mapping_rule r
        {predicate}
        "#
    ))
}

async fn attach_model_mapping_children(
    pool: &PgPool,
    rules: &mut [AdminModelMappingRuleItem],
) -> DomainResult<()> {
    for rule in rules {
        rule.bindings =
            load_model_mapping_bindings(pool, rule.tenant_id, rule.organization_id, rule.id)
                .await?;
        rule.mapping_items =
            load_model_mapping_items(pool, rule.tenant_id, rule.organization_id, rule.id).await?;
    }
    Ok(())
}

async fn attach_model_mapping_children_to_rule(
    tx: &mut Transaction<'_, Postgres>,
    rule: &mut AdminModelMappingRuleItem,
) -> DomainResult<()> {
    rule.bindings =
        load_model_mapping_bindings_tx(tx, rule.tenant_id, rule.organization_id, rule.id).await?;
    rule.mapping_items =
        load_model_mapping_items_tx(tx, rule.tenant_id, rule.organization_id, rule.id).await?;
    Ok(())
}

async fn load_model_mapping_bindings(
    pool: &PgPool,
    tenant_id: i64,
    organization_id: i64,
    rule_id: i64,
) -> DomainResult<Vec<AdminModelMappingRuleBindingItem>> {
    let rows = sqlx::query(mapping_binding_select_sql())
        .bind(tenant_id)
        .bind(organization_id)
        .bind(rule_id)
        .fetch_all(pool)
        .await
        .map_err(|error| store_error("failed to load model mapping bindings", error))?;
    rows.into_iter()
        .map(mapping_rule_binding_from_row)
        .collect()
}

async fn load_model_mapping_bindings_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    organization_id: i64,
    rule_id: i64,
) -> DomainResult<Vec<AdminModelMappingRuleBindingItem>> {
    let rows = sqlx::query(mapping_binding_select_sql())
        .bind(tenant_id)
        .bind(organization_id)
        .bind(rule_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| store_error("failed to load model mapping bindings", error))?;
    rows.into_iter()
        .map(mapping_rule_binding_from_row)
        .collect()
}

async fn load_model_mapping_items(
    pool: &PgPool,
    tenant_id: i64,
    organization_id: i64,
    rule_id: i64,
) -> DomainResult<Vec<AdminModelMappingRuleMappingItem>> {
    let rows = sqlx::query(mapping_item_select_sql())
        .bind(tenant_id)
        .bind(organization_id)
        .bind(rule_id)
        .fetch_all(pool)
        .await
        .map_err(|error| store_error("failed to load model mapping items", error))?;
    rows.into_iter().map(mapping_rule_item_from_row).collect()
}

async fn load_model_mapping_items_tx(
    tx: &mut Transaction<'_, Postgres>,
    tenant_id: i64,
    organization_id: i64,
    rule_id: i64,
) -> DomainResult<Vec<AdminModelMappingRuleMappingItem>> {
    let rows = sqlx::query(mapping_item_select_sql())
        .bind(tenant_id)
        .bind(organization_id)
        .bind(rule_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| store_error("failed to load model mapping items", error))?;
    rows.into_iter().map(mapping_rule_item_from_row).collect()
}

fn mapping_binding_select_sql() -> &'static str {
    r#"
    SELECT
        id,
        COALESCE(uuid, '') AS uuid,
        COALESCE(binding_type, 'global') AS binding_type,
        binding_id AS binding_id,
        NULLIF(COALESCE(binding_code, ''), '') AS binding_code,
        NULLIF(COALESCE(binding_name_snapshot, ''), '') AS binding_name,
        COALESCE(sort_order, 100) AS sort_order,
        COALESCE(enabled, FALSE) AS enabled,
        CAST(created_at AS TEXT) AS created_at,
        CAST(updated_at AS TEXT) AS updated_at,
        CAST(deleted_at AS TEXT) AS deleted_at
    FROM ai_model_mapping_rule_binding
    WHERE tenant_id = $1
      AND organization_id = $2
      AND rule_id = $3
      AND deleted_at IS NULL
      AND status = 1
    ORDER BY sort_order ASC, id ASC
    "#
}

fn mapping_item_select_sql() -> &'static str {
    r#"
    SELECT
        id,
        COALESCE(uuid, '') AS uuid,
        COALESCE(source_model, '') AS source_model,
        NULLIF(COALESCE(source_catalog_key, ''), '') AS source_catalog_key,
        COALESCE(target_model, '') AS target_model,
        NULLIF(COALESCE(target_catalog_key, ''), '') AS target_catalog_key,
        NULLIF(COALESCE(target_provider_model, ''), '') AS target_provider_model,
        NULLIF(COALESCE(target_provider_native_model, ''), '') AS target_provider_native_model,
        COALESCE(sort_order, 100) AS sort_order,
        COALESCE(enabled, FALSE) AS enabled,
        CAST(created_at AS TEXT) AS created_at,
        CAST(updated_at AS TEXT) AS updated_at,
        CAST(deleted_at AS TEXT) AS deleted_at
    FROM ai_model_mapping_rule_item
    WHERE tenant_id = $1
      AND organization_id = $2
      AND rule_id = $3
      AND deleted_at IS NULL
      AND status = 1
    ORDER BY sort_order ASC, id ASC
    "#
}

fn vendor_from_row(row: sqlx::postgres::PgRow) -> DomainResult<AdminModelVendorItem> {
    Ok(AdminModelVendorItem {
        id: row.try_get("id").map_err(row_error)?,
        uuid: row.try_get("uuid").map_err(row_error)?,
        tenant_id: optional_integer_cell(&row, "tenant_id").unwrap_or(0),
        organization_id: optional_integer_cell(&row, "organization_id").unwrap_or(0),
        vendor_code: row.try_get("vendor_code").map_err(row_error)?,
        name: row.try_get("name").map_err(row_error)?,
        status: status_label(required_integer_cell(&row, "status", "vendor status")?)?,
        color: row.try_get("color").map_err(row_error)?,
        description: row.try_get("description").map_err(row_error)?,
        supported_protocols: row
            .try_get("supported_protocols")
            .unwrap_or_else(|_| "[]".to_owned()),
        client_api_compatibility: row
            .try_get("client_api_compatibility")
            .unwrap_or_else(|_| "{}".to_owned()),
        deleted_at: row.try_get("deleted_at").ok().flatten(),
    })
}

fn mapping_from_row(row: sqlx::postgres::PgRow) -> DomainResult<AdminModelMappingRuleItem> {
    Ok(AdminModelMappingRuleItem {
        id: row.try_get("id").map_err(row_error)?,
        uuid: row.try_get("uuid").map_err(row_error)?,
        tenant_id: optional_integer_cell(&row, "tenant_id").unwrap_or(0),
        organization_id: optional_integer_cell(&row, "organization_id").unwrap_or(0),
        binding_type: row.try_get("binding_type").map_err(row_error)?,
        source_vendor_id: optional_integer_cell(&row, "source_vendor_id"),
        source_vendor_code: row.try_get("source_vendor_code").ok().flatten(),
        target_vendor_id: optional_integer_cell(&row, "target_vendor_id"),
        target_vendor_code: row.try_get("target_vendor_code").ok().flatten(),
        mapping_mode: row.try_get("mapping_mode").map_err(row_error)?,
        match_type: row.try_get("match_type").map_err(row_error)?,
        enabled: bool_cell(&row, "enabled"),
        bindings: Vec::new(),
        mapping_items: Vec::new(),
        created_at: row.try_get("created_at").ok().flatten(),
        updated_at: row.try_get("updated_at").ok().flatten(),
        deleted_at: row.try_get("deleted_at").ok().flatten(),
    })
}

fn mapping_rule_binding_from_row(
    row: sqlx::postgres::PgRow,
) -> DomainResult<AdminModelMappingRuleBindingItem> {
    Ok(AdminModelMappingRuleBindingItem {
        id: row.try_get("id").map_err(row_error)?,
        uuid: row.try_get("uuid").map_err(row_error)?,
        binding_type: row.try_get("binding_type").map_err(row_error)?,
        binding_id: optional_integer_cell(&row, "binding_id"),
        binding_code: row.try_get("binding_code").ok().flatten(),
        binding_name: row.try_get("binding_name").ok().flatten(),
        sort_order: optional_i32_cell(&row, "sort_order").unwrap_or(100),
        enabled: bool_cell(&row, "enabled"),
        created_at: row.try_get("created_at").ok().flatten(),
        updated_at: row.try_get("updated_at").ok().flatten(),
        deleted_at: row.try_get("deleted_at").ok().flatten(),
    })
}

fn mapping_rule_item_from_row(
    row: sqlx::postgres::PgRow,
) -> DomainResult<AdminModelMappingRuleMappingItem> {
    Ok(AdminModelMappingRuleMappingItem {
        id: row.try_get("id").map_err(row_error)?,
        uuid: row.try_get("uuid").map_err(row_error)?,
        source_model: row.try_get("source_model").map_err(row_error)?,
        source_catalog_key: row.try_get("source_catalog_key").ok().flatten(),
        target_model: row.try_get("target_model").map_err(row_error)?,
        target_catalog_key: row.try_get("target_catalog_key").ok().flatten(),
        target_provider_model: row.try_get("target_provider_model").ok().flatten(),
        target_provider_native_model: row.try_get("target_provider_native_model").ok().flatten(),
        sort_order: optional_i32_cell(&row, "sort_order").unwrap_or(100),
        enabled: bool_cell(&row, "enabled"),
        created_at: row.try_get("created_at").ok().flatten(),
        updated_at: row.try_get("updated_at").ok().flatten(),
        deleted_at: row.try_get("deleted_at").ok().flatten(),
    })
}

fn mapping_rule_item_from_joined_row(
    row: &sqlx::postgres::PgRow,
) -> DomainResult<AdminModelMappingRuleMappingItem> {
    Ok(AdminModelMappingRuleMappingItem {
        id: row.try_get("item_id").map_err(row_error)?,
        uuid: row.try_get("item_uuid").map_err(row_error)?,
        source_model: row.try_get("item_source_model").map_err(row_error)?,
        source_catalog_key: row.try_get("item_source_catalog_key").ok().flatten(),
        target_model: row.try_get("item_target_model").map_err(row_error)?,
        target_catalog_key: row.try_get("item_target_catalog_key").ok().flatten(),
        target_provider_model: row.try_get("item_target_provider_model").ok().flatten(),
        target_provider_native_model: row
            .try_get("item_target_provider_native_model")
            .ok()
            .flatten(),
        sort_order: optional_i32_cell(row, "item_sort_order").unwrap_or(100),
        enabled: bool_cell(row, "item_enabled"),
        created_at: row.try_get("item_created_at").ok().flatten(),
        updated_at: row.try_get("item_updated_at").ok().flatten(),
        deleted_at: row.try_get("item_deleted_at").ok().flatten(),
    })
}

fn model_from_row(row: sqlx::postgres::PgRow) -> DomainResult<AdminAiModelItem> {
    let capability = optional_integer_cell(&row, "capability");
    let modalities_json = row
        .try_get::<String, _>("modalities_json")
        .map_err(row_error)?;
    let modalities = parse_string_array(&modalities_json, "modalities")?;
    Ok(AdminAiModelItem {
        id: row.try_get("id").map_err(row_error)?,
        uuid: row.try_get("uuid").map_err(row_error)?,
        tenant_id: optional_integer_cell(&row, "tenant_id").unwrap_or(0),
        organization_id: optional_integer_cell(&row, "organization_id").unwrap_or(0),
        vendor_id: row.try_get("vendor_id").map_err(row_error)?,
        vendor_code: row.try_get("vendor_code").map_err(row_error)?,
        vendor_name: row.try_get("vendor_name").map_err(row_error)?,
        catalog_key: row.try_get("catalog_key").unwrap_or_default(),
        model: row.try_get("model").map_err(row_error)?,
        display_name: row.try_get("display_name").map_err(row_error)?,
        name: row.try_get("name").map_err(row_error)?,
        model_type: model_type_label(capability, &modalities)?,
        region_prices: Vec::new(),
        status: status_label(required_integer_cell(&row, "status", "model status")?)?,
        calls: row.try_get("calls").unwrap_or_else(|_| "0".to_owned()),
        description: row.try_get("description").ok().flatten(),
        modalities,
        input_modalities: parse_string_array(
            &row.try_get::<String, _>("input_modalities_json")
                .map_err(row_error)?,
            "input_modalities",
        )?,
        output_modalities: parse_string_array(
            &row.try_get::<String, _>("output_modalities_json")
                .map_err(row_error)?,
            "output_modalities",
        )?,
        api_format: row.try_get("api_format").ok().flatten(),
        capability_intro: row.try_get("capability_intro").ok().flatten(),
        limitations: parse_string_array(
            &row.try_get::<String, _>("limitations_json")
                .map_err(row_error)?,
            "limitations",
        )?,
        supported_languages: parse_string_array(
            &row.try_get::<String, _>("supported_languages_json")
                .map_err(row_error)?,
            "supported_languages",
        )?,
        use_cases: parse_string_array(
            &row.try_get::<String, _>("use_cases_json")
                .map_err(row_error)?,
            "use_cases",
        )?,
        training_data_cutoff: row.try_get("training_data_cutoff").ok().flatten(),
        context_tokens: optional_integer_cell(&row, "context_tokens"),
        max_output_tokens: optional_integer_cell(&row, "max_output_tokens"),
        supports_streaming: bool_cell(&row, "supports_streaming"),
        supports_tools: bool_cell(&row, "supports_tools"),
        supports_json_schema: bool_cell(&row, "supports_json_schema"),
        usage_scopes: parse_string_array(
            &row.try_get::<String, _>("usage_scopes_json")
                .map_err(row_error)?,
            "usage_scopes",
        )?,
        coding_visible: bool_cell(&row, "coding_visible"),
        release_stage: optional_i32_cell(&row, "release_stage"),
        shelf_state: optional_i32_cell(&row, "shelf_state"),
        routing_state: optional_i32_cell(&row, "routing_state"),
        replacement_model: row.try_get("replacement_model").ok().flatten(),
        deleted_at: row.try_get("deleted_at").ok().flatten(),
    })
}

fn normalize_vendor_lookup(value: &str) -> String {
    let value = value.trim();
    let value = value.strip_prefix("v_").unwrap_or(value);
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() {
                (byte as char).to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned()
}

fn status_code(value: &str) -> i32 {
    if value == "inactive" {
        0
    } else {
        1
    }
}

fn status_label(value: i64) -> DomainResult<String> {
    match value {
        0 => Ok("inactive"),
        1 => Ok("active"),
        value => Err(DomainError::new(format!(
            "invalid admin model status from database row: {value}"
        ))),
    }
    .map(str::to_owned)
}

fn capability_code(model_type: &str) -> i32 {
    model_modality::model_type_capability_code(model_type)
}

fn modality_code(model_type: &str) -> i32 {
    model_modality::model_type_capability_code(model_type)
}

fn model_capability_code(model_type: &str) -> &'static str {
    match model_type {
        "Image" => "image",
        "Audio" => "audio",
        "Embedding" => "embedding",
        "Music" => "music",
        "SoundEffect" => "sfx",
        "Video" => "video",
        _ => "chat",
    }
}

fn model_type_label(capability: Option<i64>, modalities: &[String]) -> DomainResult<String> {
    if modalities
        .iter()
        .any(|modality| modality == "embedding" || modality == "embeddings")
    {
        return Ok("Embedding".to_owned());
    }
    if modalities.iter().any(|modality| {
        modality == "sfx" || modality == "sound_effect" || modality == "sound_effects"
    }) {
        return Ok("SoundEffect".to_owned());
    }
    Ok(match capability {
        Some(2) => "Image",
        Some(3) => "Audio",
        Some(4) => "Music",
        Some(5) => "Video",
        _ => "Chat",
    }
    .to_owned())
}

fn parse_string_array(value: &str, field_name: &str) -> DomainResult<Vec<String>> {
    let parsed: Vec<String> = serde_json::from_str(value).map_err(|error| {
        DomainError::new(format!(
            "invalid model {field_name} json from database row: {error}"
        ))
    })?;
    Ok(parsed
        .into_iter()
        .map(|modality| modality.trim().to_ascii_lowercase())
        .filter(|modality| !modality.is_empty())
        .collect())
}

fn json_array_text(values: &[String]) -> DomainResult<String> {
    serde_json::to_string(values)
        .map_err(|error| DomainError::new(format!("failed to encode ai model json array: {error}")))
}

fn model_pricing_catalog_key(vendor_code: &str, model: &str) -> String {
    build_model_pricing_catalog_key(vendor_code, model)
}

fn input_billing_meter(model_type: &str) -> &'static str {
    match model_type {
        "Image" => "image_input_token",
        "Audio" => "audio_input_second",
        "Embedding" => "embedding_input_token",
        "Music" => "api_request",
        "SoundEffect" => "api_request",
        "Video" => "api_request",
        _ => "llm_input_token",
    }
}

fn output_billing_meter(model_type: &str) -> &'static str {
    match model_type {
        "Image" => "image_output_token",
        "Audio" => "audio_output_second",
        "Music" => "music_output_second",
        "SoundEffect" => "sfx_result",
        "Embedding" => "api_result",
        "Video" => "video_output_second",
        _ => "llm_output_token",
    }
}

fn is_input_billing_meter(value: &str) -> bool {
    matches!(
        value,
        "llm_input_token"
            | "embedding_input_token"
            | "image_input_token"
            | "image_megapixel"
            | "audio_input_second"
            | "audio_input_minute"
            | "stt_audio_minute"
            | "tts_input_character"
            | "api_request"
    )
}

fn is_output_billing_meter(value: &str) -> bool {
    matches!(
        value,
        "llm_output_token"
            | "image_output_token"
            | "image_result"
            | "image_megapixel"
            | "audio_output_second"
            | "music_output_second"
            | "sfx_result"
            | "video_output_second"
            | "video_result"
            | "api_result"
    )
}

fn is_cache_read_billing_meter(value: &str) -> bool {
    value == "llm_cache_read_token"
}

fn is_cache_write_billing_meter(value: &str) -> bool {
    value == "llm_cache_write_token"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelPriceDirection {
    Input,
    Output,
    Both,
    Unknown,
}

impl ModelPriceDirection {
    fn allows_input(self) -> bool {
        matches!(self, Self::Input | Self::Both)
    }

    fn allows_output(self) -> bool {
        matches!(self, Self::Output | Self::Both)
    }
}

fn model_price_direction(meter: &str, metadata: &str) -> ModelPriceDirection {
    match price_direction_from_metadata(metadata) {
        ModelPriceDirection::Unknown => {}
        direction => return direction,
    }
    match (
        is_input_billing_meter(meter),
        is_output_billing_meter(meter),
    ) {
        (true, true) => ModelPriceDirection::Both,
        (true, false) => ModelPriceDirection::Input,
        (false, true) => ModelPriceDirection::Output,
        (false, false) => ModelPriceDirection::Unknown,
    }
}

fn price_direction_from_metadata(metadata: &str) -> ModelPriceDirection {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return ModelPriceDirection::Unknown;
    };
    let extra = value.get("extra").unwrap_or(&value);
    if let Some(price_side) = extra
        .get("priceSide")
        .and_then(|value| value.as_str())
        .map(normalize_price_direction_token)
    {
        match price_side.as_str() {
            "input" => return ModelPriceDirection::Input,
            "output" | "result" => return ModelPriceDirection::Output,
            _ => {}
        }
    }
    let Some(price_id) = extra
        .get("priceId")
        .and_then(|value| value.as_str())
        .map(normalize_price_direction_token)
    else {
        return ModelPriceDirection::Unknown;
    };
    if price_id.contains("cache_read") || price_id.contains("cache_write") {
        return ModelPriceDirection::Unknown;
    }
    if price_id.contains("input") {
        ModelPriceDirection::Input
    } else if price_id.contains("output")
        || price_id.contains("result")
        || price_id.contains("second")
    {
        ModelPriceDirection::Output
    } else if price_id.contains("audio") {
        ModelPriceDirection::Input
    } else {
        ModelPriceDirection::Unknown
    }
}

fn normalize_price_direction_token(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn normalize_catalog_source_code(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_separator = false;
    for character in value.chars() {
        let next = if character.is_ascii_alphanumeric() {
            last_was_separator = false;
            Some(character.to_ascii_lowercase())
        } else if !last_was_separator {
            last_was_separator = true;
            Some('_')
        } else {
            None
        };
        if let Some(character) = next {
            normalized.push(character);
        }
    }
    let normalized = normalized.trim_matches('_');
    if normalized.is_empty() {
        "manual_model_catalog".to_owned()
    } else {
        normalized.chars().take(96).collect()
    }
}

fn catalog_source_uuid(tenant_id: i64, organization_id: i64, source_code: &str) -> String {
    crate::model_catalog_import::stable_uuid(
        "catalog-source",
        &[
            &tenant_id.to_string(),
            &organization_id.to_string(),
            source_code,
        ],
    )
}

fn optional_integer_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<i64> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
}

fn optional_i32_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<i32> {
    optional_integer_cell(row, column).and_then(|value| i32::try_from(value).ok())
}

fn bool_cell(row: &sqlx::postgres::PgRow, column: &str) -> bool {
    row.try_get::<Option<bool>, _>(column)
        .ok()
        .flatten()
        .or_else(|| row.try_get::<bool, _>(column).ok())
        .unwrap_or(false)
}

fn required_integer_cell(
    row: &sqlx::postgres::PgRow,
    column: &str,
    field: &str,
) -> DomainResult<i64> {
    optional_integer_cell(row, column).ok_or_else(|| missing_integer_cell_error(field))
}

fn missing_integer_cell_error(field: &str) -> DomainError {
    match field {
        "vendor status" => DomainError::new("missing admin model vendor status from database row"),
        "model status" => DomainError::new("missing admin model model status from database row"),
        _ => DomainError::new(format!("missing admin model {field} from database row")),
    }
}

fn row_error(error: sqlx::Error) -> DomainError {
    DomainError::new(error.to_string())
}

fn store_error(context: &str, error: sqlx::Error) -> DomainError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error
            .code()
            .map(|code| code == "23505")
            .unwrap_or(false)
        {
            return DomainError::conflict(format!("{context}: model catalog entry already exists"));
        }
    }
    DomainError::new(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_type_label_uses_parsed_modalities() {
        let modalities =
            parse_string_array(r#"["embedding"]"#, "modalities").expect("valid modalities json");

        assert_eq!(
            "Embedding",
            model_type_label(Some(1), &modalities).expect("valid model type label")
        );
    }

    #[test]
    fn parse_string_array_rejects_invalid_modalities_json() {
        let invalid =
            parse_string_array("not-json", "modalities").expect_err("invalid modalities json");
        assert!(invalid
            .to_string()
            .contains("invalid model modalities json from database row"));
    }
}
