use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, patch, post};
use axum::Router;
use sdkwork_cloudrouter_http::TrustedRequestSubject;
use sdkwork_utils_rust::slugify;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::page_info::{offset_page_info, ApiPageInfo};
use crate::api::request_id::{generate_server_request_id, RequestIdError};
use sdkwork_utils_rust::SdkWorkResultCode;

use crate::api::response::{finish_success, finish_success_with_status, problem_for};
use crate::application::EntityUuidGenerator;
use crate::domain::DomainError;
use crate::ports::{
    AdminAiModelItem, AdminAiModelRegionPriceCommand, AdminModelCatalogSyncItem,
    AdminModelMappingRuleBindingDraft, AdminModelMappingRuleBindingItem,
    AdminModelMappingRuleDraft, AdminModelMappingRuleItem, AdminModelMappingRuleItemDraft,
    AdminModelMappingRuleMappingItem, AdminModelMappingRulePatch, AdminModelStore,
    AdminModelSubject, AdminModelVendorItem, CreateAdminAiModelCommand,
    CreateAdminModelMappingCommand, CreateAdminModelVendorCommand, DeleteAdminAiModelCommand,
    DeleteAdminModelMappingCommand, ListAdminAiModelsQuery, ListAdminModelMappingsQuery,
    ListAdminModelVendorsQuery, ResolveAdminModelMappingQuery, ResolveAdminModelMappingResult,
    SyncAdminModelCatalogCommand, UpdateAdminAiModelCommand, UpdateAdminModelMappingCommand,
};
use sdkwork_models_contract_service::DEFAULT_CATALOG_REFRESH_SOURCE;
use sdkwork_web_core::WebRequestContext;

const MAX_VENDOR_CODE_LEN: usize = 64;
const MAX_NAME_LEN: usize = 128;
const MAX_COLOR_LEN: usize = 64;
const MAX_DESCRIPTION_LEN: usize = 512;
const MAX_MAPPING_TEXT_LEN: usize = 256;
const MAX_MAPPING_BINDING_TYPE_LEN: usize = 32;
const MAX_MAPPING_QUERY_LEN: usize = 128;
const MAX_MODEL_MAPPING_CHILDREN: usize = 100;
const MAX_PUBLIC_DESCRIPTION_LEN: usize = 2048;
const MAX_CAPABILITY_INTRO_LEN: usize = 4096;
const MAX_TRAINING_DATA_CUTOFF_LEN: usize = 128;
const MAX_API_FORMAT_LEN: usize = 128;
const MAX_MODEL_METADATA_TEXT_LEN: usize = 512;
const MAX_MODEL_METADATA_ITEMS: usize = 128;
const MAX_SOURCE_LEN: usize = 64;
const MAX_SYNC_MODE_LEN: usize = 32;
const MAX_SYNC_VENDOR_CODES: usize = 32;
const MAX_CATALOG_ROOT_LEN: usize = 512;
const MAX_CATALOG_VERSION_LEN: usize = 128;
const MAX_MODEL_ID_LEN: usize = 256;
const MAX_REGION_CODE_LEN: usize = 64;
const DEFAULT_MODEL_REGION_CODE: &str = "global";
const MAX_CONTEXT_TOKENS: i64 = 100_000_000;
const MAX_OUTPUT_TOKENS: i64 = 100_000_000;
const INTEGRATION_PROVIDER_ONLY_CODES: &[&str] = &[
    "azure",
    "azure_ai",
    "azure_openai",
    "aws_bedrock",
    "bedrock",
    "gcp_vertex",
    "google_vertex",
    "ollama",
    "openrouter",
    "vertex",
    "vertex_ai",
];
const INTEGRATION_PROVIDER_ONLY_NAME_MARKERS: &[&str] = &[
    "aws bedrock",
    "azure openai",
    "google vertex",
    "openrouter",
    "ollama",
    "vertex ai",
];
const MAPPING_BINDING_TYPES: &[&str] = &[
    "global",
    "vendor",
    "upstream_account_group",
    "upstream_account",
    "upstream_account",
    "upstream_supplier",
    "supplier_endpoint",
];
const MAPPING_MODES: &[&str] = &["alias"];
const MAPPING_MATCH_TYPES: &[&str] = &["exact"];
const DEFAULT_LIST_PAGE_SIZE: i64 = 20;
const MAX_LIST_PAGE_SIZE: i64 = 200;

#[derive(Clone)]
struct AdminModelCommandState {
    store: Arc<dyn AdminModelStore + Send + Sync>,
    entity_uuid_generator: Arc<dyn EntityUuidGenerator + Send + Sync>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelVendorCreateRequest {
    vendor_code: Option<String>,
    name: Option<String>,
    status: Option<String>,
    color: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminAiModelCreateRequest {
    vendor_id: Option<Value>,
    model: Option<String>,
    display_name: Option<String>,
    #[serde(rename = "type")]
    model_type: Option<String>,
    region_prices: Option<Vec<AdminAiModelRegionPriceRequest>>,
    context_tokens: Option<Value>,
    description: Option<String>,
    modalities: Option<Vec<String>>,
    input_modalities: Option<Vec<String>>,
    output_modalities: Option<Vec<String>>,
    api_format: Option<String>,
    capability_intro: Option<String>,
    limitations: Option<Vec<String>>,
    supported_languages: Option<Vec<String>>,
    use_cases: Option<Vec<String>>,
    training_data_cutoff: Option<String>,
    max_output_tokens: Option<Value>,
    supports_streaming: Option<bool>,
    supports_tools: Option<bool>,
    supports_json_schema: Option<bool>,
    usage_scopes: Option<Vec<String>>,
    coding_visible: Option<bool>,
    release_stage: Option<Value>,
    shelf_state: Option<Value>,
    routing_state: Option<Value>,
    replacement_model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminAiModelUpdateRequest {
    vendor_id: Option<Value>,
    model: Option<String>,
    display_name: Option<String>,
    #[serde(rename = "type")]
    model_type: Option<String>,
    region_prices: Option<Vec<AdminAiModelRegionPriceRequest>>,
    status: Option<String>,
    context_tokens: Option<Value>,
    description: Option<String>,
    modalities: Option<Vec<String>>,
    input_modalities: Option<Vec<String>>,
    output_modalities: Option<Vec<String>>,
    api_format: Option<String>,
    capability_intro: Option<String>,
    limitations: Option<Vec<String>>,
    supported_languages: Option<Vec<String>>,
    use_cases: Option<Vec<String>>,
    training_data_cutoff: Option<String>,
    max_output_tokens: Option<Value>,
    supports_streaming: Option<bool>,
    supports_tools: Option<bool>,
    supports_json_schema: Option<bool>,
    usage_scopes: Option<Vec<String>>,
    coding_visible: Option<bool>,
    release_stage: Option<Value>,
    shelf_state: Option<Value>,
    routing_state: Option<Value>,
    replacement_model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminAiModelRegionPriceRequest {
    region_code: Option<String>,
    currency: Option<String>,
    price_in: Option<Value>,
    price_out: Option<Value>,
    cache_read_price: Option<Value>,
    cache_write_price: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelCatalogSyncRequest {
    source: Option<String>,
    mode: Option<String>,
    vendor_codes: Option<Vec<String>>,
    force: Option<bool>,
    catalog_root: Option<String>,
    catalog_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdminModelMappingsQuery {
    binding_type: Option<String>,
    vendor_code: Option<String>,
    account_id: Option<Value>,
    account_code: Option<String>,
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdminModelsListQuery {
    q: Option<String>,
    vendor_codes: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingCreateRequest {
    source_vendor_id: Option<Value>,
    source_vendor_code: Option<String>,
    target_vendor_id: Option<Value>,
    target_vendor_code: Option<String>,
    mapping_mode: Option<String>,
    match_type: Option<String>,
    enabled: Option<bool>,
    bindings: Option<Vec<AdminModelMappingBindingRequest>>,
    mapping_items: Option<Vec<AdminModelMappingItemRequest>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingUpdateRequest {
    source_vendor_id: Option<Value>,
    source_vendor_code: Option<String>,
    target_vendor_id: Option<Value>,
    target_vendor_code: Option<String>,
    mapping_mode: Option<String>,
    match_type: Option<String>,
    enabled: Option<bool>,
    bindings: Option<Vec<AdminModelMappingBindingRequest>>,
    mapping_items: Option<Vec<AdminModelMappingItemRequest>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingBindingRequest {
    id: Option<Value>,
    binding_type: Option<String>,
    binding_id: Option<Value>,
    binding_code: Option<String>,
    binding_name: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingItemRequest {
    id: Option<Value>,
    source_model: Option<String>,
    source_catalog_key: Option<String>,
    target_model: Option<String>,
    target_catalog_key: Option<String>,
    target_provider_model: Option<String>,
    target_provider_native_model: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingResolveRequest {
    source_model: Option<String>,
    vendor_code: Option<String>,
    supplier_id: Option<Value>,
    supplier_code: Option<String>,
    endpoint_id: Option<Value>,
    endpoint_code: Option<String>,
    account_id: Option<Value>,
    account_code: Option<String>,
    account_group_id: Option<Value>,
    account_group_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedVendorCreateRequest {
    vendor_code: String,
    name: String,
    status: String,
    color: String,
    description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedModelCreateRequest {
    vendor_id: String,
    model: String,
    display_name: String,
    model_type: String,
    region_prices: Vec<AdminAiModelRegionPriceCommand>,
    description: Option<String>,
    modalities: Vec<String>,
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
    api_format: String,
    capability_intro: Option<String>,
    limitations: Vec<String>,
    supported_languages: Vec<String>,
    use_cases: Vec<String>,
    training_data_cutoff: Option<String>,
    context_tokens: i64,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedModelUpdateRequest {
    vendor_id: Option<String>,
    model: Option<String>,
    display_name: Option<Option<String>>,
    model_type: Option<String>,
    region_prices: Option<Vec<AdminAiModelRegionPriceCommand>>,
    status: Option<String>,
    description: Option<Option<String>>,
    modalities: Option<Vec<String>>,
    input_modalities: Option<Vec<String>>,
    output_modalities: Option<Vec<String>>,
    api_format: Option<String>,
    capability_intro: Option<Option<String>>,
    limitations: Option<Vec<String>>,
    supported_languages: Option<Vec<String>>,
    use_cases: Option<Vec<String>>,
    training_data_cutoff: Option<Option<String>>,
    context_tokens: Option<i64>,
    max_output_tokens: Option<Option<i64>>,
    supports_streaming: Option<bool>,
    supports_tools: Option<bool>,
    supports_json_schema: Option<bool>,
    usage_scopes: Option<Vec<String>>,
    coding_visible: Option<bool>,
    release_stage: Option<i32>,
    shelf_state: Option<i32>,
    routing_state: Option<i32>,
    replacement_model: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelListResponse<T> {
    items: Vec<T>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelPageResponse<T> {
    items: Vec<T>,
    page_info: ApiPageInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelItemEnvelope<T> {
    item: T,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelCatalogSyncResponse {
    synced: bool,
    source: String,
    mode: String,
    dry_run: bool,
    catalog_version: String,
    requested_catalog_version: Option<String>,
    catalog_root: Option<String>,
    vendor_codes: Vec<String>,
    source_hash: String,
    meter_count: usize,
    vendor_count: usize,
    family_count: usize,
    model_count: usize,
    capability_count: usize,
    price_count: usize,
    ranking_count: usize,
    voice_count: usize,
    voice_binding_count: usize,
    video_profile_count: usize,
    accepted_count: i64,
    snapshot_id: Option<String>,
    sync_run_id: Option<String>,
    vendors: Vec<AdminModelVendorItemResponse>,
    models: Vec<AdminAiModelItemResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelVendorItemResponse {
    id: String,
    vendor_code: String,
    name: String,
    status: String,
    color: String,
    description: String,
    supported_protocols: Value,
    client_api_compatibility: Value,
    model_count: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminAiModelItemResponse {
    id: String,
    vendor_id: String,
    vendor_code: String,
    model: String,
    display_name: String,
    name: String,
    #[serde(rename = "type")]
    model_type: String,
    region_prices: Vec<AdminAiModelRegionPriceResponse>,
    status: String,
    calls: String,
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
    release_stage: Option<i32>,
    shelf_state: Option<i32>,
    routing_state: Option<i32>,
    replacement_model: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminAiModelRegionPriceResponse {
    region_code: String,
    currency: String,
    price_in: String,
    price_out: String,
    cache_read_price: String,
    cache_write_price: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingRuleResponse {
    id: String,
    binding_type: String,
    source_vendor_id: Option<String>,
    source_vendor_code: String,
    target_vendor_id: Option<String>,
    target_vendor_code: String,
    mapping_mode: String,
    match_type: String,
    enabled: bool,
    bindings: Vec<AdminModelMappingRuleBindingResponse>,
    mapping_items: Vec<AdminModelMappingRuleItemResponse>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingRuleBindingResponse {
    id: String,
    binding_type: String,
    binding_id: Option<String>,
    binding_code: Option<String>,
    binding_name: Option<String>,
    sort_order: i32,
    enabled: bool,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingRuleItemResponse {
    id: String,
    source_model: String,
    source_catalog_key: Option<String>,
    target_model: String,
    target_catalog_key: Option<String>,
    target_provider_model: Option<String>,
    target_provider_native_model: Option<String>,
    sort_order: i32,
    enabled: bool,
    created_at: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminModelMappingResolveResponse {
    source_model: String,
    target_model: String,
    target_catalog_key: Option<String>,
    target_vendor_code: Option<String>,
    target_provider_model: Option<String>,
    target_provider_native_model: Option<String>,
    matched: bool,
    matched_binding_type: Option<String>,
    rule: Option<AdminModelMappingRuleResponse>,
}

#[derive(Debug)]
enum AdminModelCommandBuildError {
    BadRequest(String),
    System(DomainError),
}

pub fn admin_model_management_router_with_store(
    store: Arc<dyn AdminModelStore + Send + Sync>,
    entity_uuid_generator: Arc<dyn EntityUuidGenerator + Send + Sync>,
) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/ai/model_vendors",
            get(fetch_vendors).post(create_vendor),
        )
        .route(
            "/backend/v3/api/ai/models",
            get(fetch_models).post(create_model),
        )
        .route("/backend/v3/api/ai/models/sync", post(sync_catalog))
        .route(
            "/backend/v3/api/ai/model_mappings",
            get(fetch_model_mappings).post(create_model_mapping),
        )
        .route(
            "/backend/v3/api/ai/model_mappings/resolve",
            post(resolve_model_mapping),
        )
        .route(
            "/backend/v3/api/ai/model_mappings/{mapping_id}",
            patch(update_model_mapping).delete(delete_model_mapping),
        )
        .route(
            "/backend/v3/api/ai/models/{model_id}",
            patch(update_model).delete(delete_model),
        )
        .with_state(AdminModelCommandState {
            store,
            entity_uuid_generator,
        })
}

async fn fetch_vendors(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    trusted: TrustedRequestSubject,
    _headers: HeaderMap,
) -> Response {
    let subject = map_subject(trusted);
    match state
        .store
        .list_vendors(ListAdminModelVendorsQuery { subject })
        .await
    {
        Ok(items) => finish_success(
            &ctx,
            AdminModelListResponse {
                items: items.into_iter().map(to_vendor_response).collect(),
            },
        ),
        Err(error) => {
            admin_model_system_response(&ctx, "model vendor read model is unavailable", error)
        }
    }
}

async fn fetch_models(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    query: Result<Query<AdminModelsListQuery>, QueryRejection>,
    trusted: TrustedRequestSubject,
    _headers: HeaderMap,
) -> Response {
    let Query(query) = match query {
        Ok(query) => query,
        Err(rejection) => return invalid_models_query_response(&ctx, rejection),
    };
    let subject = map_subject(trusted);
    let list_query = match build_list_models_query(subject, query) {
        Ok(query) => query,
        Err(message) => return bad_request(&ctx, message),
    };
    tracing::info!(
        operation = "models.list",
        tenant_id = list_query.subject.tenant_id,
        organization_id = list_query.subject.organization_id,
        vendor_codes = ?list_query.vendor_codes,
        limit = list_query.normalized_limit(),
        offset = list_query.normalized_offset(),
        "listing admin ai models"
    );
    let page_size = list_query.normalized_limit();
    let page_no = (list_query.normalized_offset() / page_size) + 1;
    match state.store.list_models(list_query).await {
        Ok(page) => {
            tracing::debug!(
                operation = "models.list",
                item_count = page.items.len(),
                total_count = page.total_count,
                "listed admin ai models"
            );
            finish_success(
                &ctx,
                AdminModelPageResponse {
                    items: page.items.into_iter().map(to_model_response).collect(),
                    page_info: offset_page_info(page_no, page_size, page.total_count),
                },
            )
        }
        Err(error) => {
            admin_model_system_response(&ctx, "ai model read model is unavailable", error)
        }
    }
}

async fn fetch_model_mappings(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    Query(query): Query<AdminModelMappingsQuery>,
    trusted: TrustedRequestSubject,
    _headers: HeaderMap,
) -> Response {
    let subject = map_subject(trusted);
    let query = match build_list_model_mappings_query(subject, query) {
        Ok(query) => query,
        Err(error) => return command_build_error_response(&ctx, error),
    };
    let page_size = query.normalized_limit();
    let page_no = (query.normalized_offset() / page_size) + 1;
    match state.store.list_model_mappings(query).await {
        Ok(page) => finish_success(
            &ctx,
            AdminModelPageResponse {
                items: page.items.into_iter().map(to_mapping_response).collect(),
                page_info: offset_page_info(page_no, page_size, page.total_count),
            },
        ),
        Err(error) => {
            admin_model_system_response(&ctx, "model mapping read model is unavailable", error)
        }
    }
}

async fn create_vendor(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = map_subject(trusted);
    let request = match parse_json_body::<AdminModelVendorCreateRequest>(&body, "model vendor") {
        Ok(request) => request,
        Err(message) => return bad_request(&ctx, message),
    };
    let command = match build_create_vendor_command(state.clone(), &headers, subject, request) {
        Ok(command) => command,
        Err(error) => return command_build_error_response(&ctx, error),
    };
    match state.store.create_vendor(command).await {
        Ok(item) => finish_success_with_status(
            &ctx,
            StatusCode::CREATED,
            AdminModelItemEnvelope {
                item: to_vendor_response(item),
            },
        ),
        Err(error) if error.is_conflict() => conflict_response(&ctx, error),
        Err(error) => {
            admin_model_system_response(&ctx, "model vendor command store is unavailable", error)
        }
    }
}

async fn create_model(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = map_subject(trusted);
    let request = match parse_json_body::<AdminAiModelCreateRequest>(&body, "ai model") {
        Ok(request) => request,
        Err(message) => return bad_request(&ctx, message),
    };
    let command = match build_create_model_command(state.clone(), &headers, subject, request) {
        Ok(command) => command,
        Err(error) => return command_build_error_response(&ctx, error),
    };
    match state.store.create_model(command).await {
        Ok(item) => finish_success_with_status(
            &ctx,
            StatusCode::CREATED,
            AdminModelItemEnvelope {
                item: to_model_response(item),
            },
        ),
        Err(error) if error.is_not_found() => not_found_response(&ctx, error.to_string()),
        Err(error) if error.is_conflict() => conflict_response(&ctx, error),
        Err(error) => {
            admin_model_system_response(&ctx, "ai model command store is unavailable", error)
        }
    }
}

async fn create_model_mapping(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = map_subject(trusted);
    let request = match parse_json_body::<AdminModelMappingCreateRequest>(&body, "model mapping") {
        Ok(request) => request,
        Err(message) => return bad_request(&ctx, message),
    };
    let command =
        match build_create_model_mapping_command(state.clone(), &headers, subject, request) {
            Ok(command) => command,
            Err(error) => return command_build_error_response(&ctx, error),
        };
    match state.store.create_model_mapping(command).await {
        Ok(item) => finish_success_with_status(
            &ctx,
            StatusCode::CREATED,
            AdminModelItemEnvelope {
                item: to_mapping_response(item),
            },
        ),
        Err(error) if error.is_conflict() => conflict_response(&ctx, error),
        Err(error) => {
            admin_model_system_response(&ctx, "model mapping command store is unavailable", error)
        }
    }
}

async fn update_model(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    Path(model_id): Path<String>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = map_subject(trusted);
    let request = match parse_json_body::<AdminAiModelUpdateRequest>(&body, "ai model update") {
        Ok(request) => request,
        Err(message) => return bad_request(&ctx, message),
    };
    let command =
        match build_update_model_command(state.clone(), &headers, subject, model_id, request) {
            Ok(command) => command,
            Err(error) => return command_build_error_response(&ctx, error),
        };
    match state.store.update_model(command).await {
        Ok(item) => finish_success(
            &ctx,
            AdminModelItemEnvelope {
                item: to_model_response(item),
            },
        ),
        Err(error) if error.is_not_found() => not_found_response(&ctx, error.to_string()),
        Err(error) if error.is_conflict() => conflict_response(&ctx, error),
        Err(error) => {
            admin_model_system_response(&ctx, "ai model update store is unavailable", error)
        }
    }
}

async fn update_model_mapping(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    Path(mapping_id): Path<String>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = map_subject(trusted);
    let request =
        match parse_json_body::<AdminModelMappingUpdateRequest>(&body, "model mapping update") {
            Ok(request) => request,
            Err(message) => return bad_request(&ctx, message),
        };
    let command = match build_update_model_mapping_command(
        state.clone(),
        &headers,
        subject,
        mapping_id,
        request,
    ) {
        Ok(command) => command,
        Err(error) => return command_build_error_response(&ctx, error),
    };
    match state.store.update_model_mapping(command).await {
        Ok(item) => finish_success(
            &ctx,
            AdminModelItemEnvelope {
                item: to_mapping_response(item),
            },
        ),
        Err(error) if error.is_not_found() => not_found_response(&ctx, error.to_string()),
        Err(error) if error.is_conflict() => conflict_response(&ctx, error),
        Err(error) => {
            admin_model_system_response(&ctx, "model mapping update store is unavailable", error)
        }
    }
}

async fn delete_model(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    Path(model_id): Path<String>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
) -> Response {
    let subject = map_subject(trusted);
    let command = match build_delete_model_command(state.clone(), &headers, subject, model_id) {
        Ok(command) => command,
        Err(error) => return command_build_error_response(&ctx, error),
    };
    match state.store.delete_model(command).await {
        // Fail-closed delete confirmation: the response affirms the deletion so
        // SDK consumers can reject silent success when no row was removed.
        Ok(deleted) => finish_success(&ctx, serde_json::json!({ "deleted": deleted })),
        Err(error) if error.is_not_found() => not_found_response(&ctx, error.to_string()),
        Err(error) => {
            admin_model_system_response(&ctx, "ai model delete store is unavailable", error)
        }
    }
}

async fn delete_model_mapping(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    Path(mapping_id): Path<String>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
) -> Response {
    let subject = map_subject(trusted);
    let command =
        match build_delete_model_mapping_command(state.clone(), &headers, subject, mapping_id) {
            Ok(command) => command,
            Err(error) => return command_build_error_response(&ctx, error),
        };
    match state.store.delete_model_mapping(command).await {
        Ok(deleted) => finish_success(&ctx, serde_json::json!({ "deleted": deleted })),
        Err(error) if error.is_not_found() => not_found_response(&ctx, error.to_string()),
        Err(error) => {
            admin_model_system_response(&ctx, "model mapping delete store is unavailable", error)
        }
    }
}

async fn resolve_model_mapping(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    trusted: TrustedRequestSubject,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = map_subject(trusted);
    let request =
        match parse_json_body::<AdminModelMappingResolveRequest>(&body, "model mapping resolve") {
            Ok(request) => request,
            Err(message) => return bad_request(&ctx, message),
        };
    let query = match build_resolve_model_mapping_query(subject, request) {
        Ok(query) => query,
        Err(error) => return command_build_error_response(&ctx, error),
    };
    match state.store.resolve_model_mapping(query).await {
        Ok(result) => finish_success(&ctx, to_mapping_resolve_response(result)),
        Err(error) => {
            admin_model_system_response(&ctx, "model mapping resolve store is unavailable", error)
        }
    }
}

async fn sync_catalog(
    ctx: WebRequestContext,
    State(state): State<AdminModelCommandState>,
    trusted: TrustedRequestSubject,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let subject = map_subject(trusted);
    let request =
        match parse_optional_json_body::<AdminModelCatalogSyncRequest>(&body, "model catalog sync")
        {
            Ok(request) => request,
            Err(message) => return bad_request(&ctx, message),
        };
    let command = match build_sync_catalog_command(state.clone(), &headers, subject, request) {
        Ok(command) => command,
        Err(error) => return command_build_error_response(&ctx, error),
    };
    match state.store.sync_catalog(command).await {
        Ok(item) => finish_success(&ctx, to_sync_response(item)),
        Err(error) => {
            admin_model_system_response(&ctx, "model catalog sync store is unavailable", error)
        }
    }
}

fn map_subject(trusted: TrustedRequestSubject) -> AdminModelSubject {
    AdminModelSubject {
        tenant_id: trusted.tenant_id,
        organization_id: trusted.organization_id,
        operator_id: trusted.operator_id,
        operator_type: trusted.operator_type,
    }
}

fn parse_json_body<T>(body: &[u8], entity_name: &str) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    if body.iter().all(u8::is_ascii_whitespace) {
        return Err(format!("{entity_name} request body is required"));
    }
    serde_json::from_slice(body)
        .map_err(|error| format!("invalid {entity_name} request body: {error}"))
}

fn parse_optional_json_body<T>(body: &[u8], entity_name: &str) -> Result<T, String>
where
    T: Default + for<'de> Deserialize<'de>,
{
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(body)
        .map_err(|error| format!("invalid {entity_name} request body: {error}"))
}

impl Default for AdminModelCatalogSyncRequest {
    fn default() -> Self {
        Self {
            source: None,
            mode: None,
            vendor_codes: None,
            force: None,
            catalog_root: None,
            catalog_version: None,
        }
    }
}

fn build_create_vendor_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    request: AdminModelVendorCreateRequest,
) -> Result<CreateAdminModelVendorCommand, AdminModelCommandBuildError> {
    let vendor_uuid = generate_entity_uuid(&state)?;
    let request = normalize_vendor_create_request(request, &vendor_uuid)?;
    Ok(CreateAdminModelVendorCommand {
        subject,
        vendor_uuid,
        audit_log_uuid: generate_entity_uuid(&state)?,
        vendor_code: request.vendor_code,
        name: request.name,
        status: request.status,
        color: request.color,
        description: request.description,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_create_model_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    request: AdminAiModelCreateRequest,
) -> Result<CreateAdminAiModelCommand, AdminModelCommandBuildError> {
    let request = normalize_model_create_request(request)?;
    Ok(CreateAdminAiModelCommand {
        subject,
        model_uuid: generate_entity_uuid(&state)?,
        capability_uuid: generate_entity_uuid(&state)?,
        audit_log_uuid: generate_entity_uuid(&state)?,
        vendor_id: request.vendor_id,
        model: request.model,
        display_name: request.display_name,
        model_type: request.model_type,
        region_prices: request.region_prices,
        description: request.description,
        modalities: request.modalities,
        input_modalities: request.input_modalities,
        output_modalities: request.output_modalities,
        api_format: request.api_format,
        capability_intro: request.capability_intro,
        limitations: request.limitations,
        supported_languages: request.supported_languages,
        use_cases: request.use_cases,
        training_data_cutoff: request.training_data_cutoff,
        context_tokens: request.context_tokens,
        max_output_tokens: request.max_output_tokens,
        supports_streaming: request.supports_streaming,
        supports_tools: request.supports_tools,
        supports_json_schema: request.supports_json_schema,
        usage_scopes: request.usage_scopes,
        coding_visible: request.coding_visible,
        release_stage: request.release_stage,
        shelf_state: request.shelf_state,
        routing_state: request.routing_state,
        replacement_model: request.replacement_model,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_update_model_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    model_id: String,
    request: AdminAiModelUpdateRequest,
) -> Result<UpdateAdminAiModelCommand, AdminModelCommandBuildError> {
    let request = normalize_model_update_request(request)?;
    Ok(UpdateAdminAiModelCommand {
        subject,
        capability_uuid: generate_entity_uuid(&state)?,
        audit_log_uuid: generate_entity_uuid(&state)?,
        model_id: normalize_model_id(&model_id)?,
        vendor_id: request.vendor_id,
        model: request.model,
        display_name: request.display_name,
        model_type: request.model_type,
        region_prices: request.region_prices,
        status: request.status,
        description: request.description,
        modalities: request.modalities,
        input_modalities: request.input_modalities,
        output_modalities: request.output_modalities,
        api_format: request.api_format,
        capability_intro: request.capability_intro,
        limitations: request.limitations,
        supported_languages: request.supported_languages,
        use_cases: request.use_cases,
        training_data_cutoff: request.training_data_cutoff,
        context_tokens: request.context_tokens,
        max_output_tokens: request.max_output_tokens,
        supports_streaming: request.supports_streaming,
        supports_tools: request.supports_tools,
        supports_json_schema: request.supports_json_schema,
        usage_scopes: request.usage_scopes,
        coding_visible: request.coding_visible,
        release_stage: request.release_stage,
        shelf_state: request.shelf_state,
        routing_state: request.routing_state,
        replacement_model: request.replacement_model,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_sync_catalog_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    request: AdminModelCatalogSyncRequest,
) -> Result<SyncAdminModelCatalogCommand, AdminModelCommandBuildError> {
    Ok(SyncAdminModelCatalogCommand {
        subject,
        snapshot_uuid: generate_entity_uuid(&state)?,
        audit_log_uuid: generate_entity_uuid(&state)?,
        source: normalize_source(request.source.as_deref())?,
        mode: normalize_sync_mode(request.mode.as_deref())?,
        vendor_codes: normalize_sync_vendor_codes(request.vendor_codes)?,
        force: request.force.unwrap_or(false),
        catalog_root: normalize_optional_catalog_root(request.catalog_root.as_deref())?,
        catalog_version: normalize_optional_catalog_version(request.catalog_version.as_deref())?,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_list_models_query(
    subject: AdminModelSubject,
    query: AdminModelsListQuery,
) -> Result<ListAdminAiModelsQuery, String> {
    let page = query.page.unwrap_or(1);
    if page < 1 {
        return Err("page must be greater than or equal to 1".to_owned());
    }
    let page_size = query.page_size.unwrap_or(DEFAULT_LIST_PAGE_SIZE);
    if !(1..=MAX_LIST_PAGE_SIZE).contains(&page_size) {
        return Err(format!(
            "page_size must be between 1 and {MAX_LIST_PAGE_SIZE}"
        ));
    }
    Ok(ListAdminAiModelsQuery {
        subject,
        vendor_id: None,
        vendor_codes: normalize_list_vendor_codes(query.vendor_codes.as_deref())?,
        q: query.q,
        model_types: None,
        status: None,
        release_stages: Vec::new(),
        shelf_state: None,
        routing_state: None,
        modalities: Vec::new(),
        page_size: Some(page_size),
        offset: Some((page - 1) * page_size),
    })
}

fn normalize_list_vendor_codes(value: Option<&str>) -> Result<Vec<String>, String> {
    let mut vendor_codes = Vec::new();
    for value in value.unwrap_or_default().split(',') {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if value.len() > MAX_VENDOR_CODE_LEN {
            return Err(format!(
                "vendor_codes items must be at most {MAX_VENDOR_CODE_LEN} bytes"
            ));
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(
                "vendor_codes items must contain only letters, numbers, hyphen, or underscore"
                    .to_owned(),
            );
        }
        let code = value.to_ascii_lowercase();
        if !vendor_codes.iter().any(|existing| existing == &code) {
            vendor_codes.push(code);
        }
        if vendor_codes.len() > MAX_SYNC_VENDOR_CODES {
            return Err(format!(
                "vendor_codes must contain {MAX_SYNC_VENDOR_CODES} items or fewer"
            ));
        }
    }
    Ok(vendor_codes)
}

fn invalid_models_query_response(ctx: &WebRequestContext, rejection: QueryRejection) -> Response {
    tracing::warn!(
        operation = "models.list",
        rejection = %rejection.body_text(),
        "rejected invalid admin model list query"
    );
    problem_for(
        ctx,
        SdkWorkResultCode::InvalidParameter,
        "Query parameters do not match the operation schema.",
    )
}

fn build_list_model_mappings_query(
    subject: AdminModelSubject,
    query: AdminModelMappingsQuery,
) -> Result<ListAdminModelMappingsQuery, AdminModelCommandBuildError> {
    let page = query.page.unwrap_or(1);
    if page < 1 {
        return Err(AdminModelCommandBuildError::BadRequest(
            "page must be greater than or equal to 1".to_owned(),
        ));
    }
    let page_size = query.page_size.unwrap_or(DEFAULT_LIST_PAGE_SIZE);
    if !(1..=MAX_LIST_PAGE_SIZE).contains(&page_size) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "page_size must be between 1 and {MAX_LIST_PAGE_SIZE}"
        )));
    }
    Ok(ListAdminModelMappingsQuery {
        subject,
        binding_type: query
            .binding_type
            .as_deref()
            .map(normalize_mapping_binding_type)
            .transpose()?,
        vendor_code: normalize_nullable_code(
            query.vendor_code.as_deref(),
            "vendorCode",
            MAX_VENDOR_CODE_LEN,
        )?,
        account_id: normalize_optional_id_value(query.account_id.as_ref(), "accountId")?,
        account_code: normalize_nullable_code(
            query.account_code.as_deref(),
            "accountCode",
            MAX_VENDOR_CODE_LEN,
        )?,
        q: normalize_mapping_optional_text(query.q.as_deref(), "q", MAX_MAPPING_QUERY_LEN)?,
        page_size: Some(page_size),
        offset: Some((page - 1) * page_size),
    })
}

fn build_create_model_mapping_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    request: AdminModelMappingCreateRequest,
) -> Result<CreateAdminModelMappingCommand, AdminModelCommandBuildError> {
    let draft = normalize_model_mapping_create_request(request)?;
    Ok(CreateAdminModelMappingCommand {
        subject,
        mapping_uuid: generate_entity_uuid(&state)?,
        binding_uuids: generate_entity_uuids(&state, draft.bindings.len())?,
        item_uuids: generate_entity_uuids(&state, draft.mapping_items.len())?,
        audit_log_uuid: generate_entity_uuid(&state)?,
        draft,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_update_model_mapping_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    mapping_id: String,
    request: AdminModelMappingUpdateRequest,
) -> Result<UpdateAdminModelMappingCommand, AdminModelCommandBuildError> {
    let patch = normalize_model_mapping_update_request(request)?;
    Ok(UpdateAdminModelMappingCommand {
        subject,
        audit_log_uuid: generate_entity_uuid(&state)?,
        mapping_id: normalize_model_id(&mapping_id)?,
        binding_uuids: generate_entity_uuids(
            &state,
            patch
                .bindings
                .as_ref()
                .map(|items| items.iter().filter(|item| item.id.is_none()).count())
                .unwrap_or(0),
        )?,
        item_uuids: generate_entity_uuids(
            &state,
            patch
                .mapping_items
                .as_ref()
                .map(|items| items.iter().filter(|item| item.id.is_none()).count())
                .unwrap_or(0),
        )?,
        patch,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_delete_model_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    model_id: String,
) -> Result<DeleteAdminAiModelCommand, AdminModelCommandBuildError> {
    Ok(DeleteAdminAiModelCommand {
        subject,
        audit_log_uuid: generate_entity_uuid(&state)?,
        model_id: normalize_model_id(&model_id)?,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_delete_model_mapping_command(
    state: AdminModelCommandState,
    _headers: &HeaderMap,
    subject: AdminModelSubject,
    mapping_id: String,
) -> Result<DeleteAdminModelMappingCommand, AdminModelCommandBuildError> {
    Ok(DeleteAdminModelMappingCommand {
        subject,
        audit_log_uuid: generate_entity_uuid(&state)?,
        mapping_id: normalize_model_id(&mapping_id)?,
        request_id: generate_server_request_id().map_err(request_id_error)?,
        requested_at: current_timestamp_string(),
    })
}

fn build_resolve_model_mapping_query(
    subject: AdminModelSubject,
    request: AdminModelMappingResolveRequest,
) -> Result<ResolveAdminModelMappingQuery, AdminModelCommandBuildError> {
    Ok(ResolveAdminModelMappingQuery {
        subject,
        source_model: normalize_mapping_required_text(
            request.source_model.as_deref(),
            "sourceModel",
            MAX_MAPPING_TEXT_LEN,
        )?,
        vendor_code: normalize_nullable_code(
            request.vendor_code.as_deref(),
            "vendorCode",
            MAX_VENDOR_CODE_LEN,
        )?,
        supplier_id: normalize_optional_id_value(request.supplier_id.as_ref(), "supplierId")?,
        supplier_code: normalize_nullable_binding_code(
            request.supplier_code.as_deref(),
            "supplierCode",
        )?,
        endpoint_id: normalize_optional_id_value(request.endpoint_id.as_ref(), "endpointId")?,
        endpoint_code: normalize_nullable_binding_code(
            request.endpoint_code.as_deref(),
            "endpointCode",
        )?,
        account_id: normalize_optional_id_value(request.account_id.as_ref(), "accountId")?,
        account_code: normalize_nullable_binding_code(
            request.account_code.as_deref(),
            "accountCode",
        )?,
        account_group_id: normalize_optional_id_value(
            request.account_group_id.as_ref(),
            "accountGroupId",
        )?,
        account_group_code: normalize_nullable_binding_code(
            request.account_group_code.as_deref(),
            "accountGroupCode",
        )?,
    })
}

fn normalize_vendor_create_request(
    request: AdminModelVendorCreateRequest,
    vendor_uuid: &str,
) -> Result<NormalizedVendorCreateRequest, AdminModelCommandBuildError> {
    let name = normalize_required_text(request.name.as_deref(), "vendor name", MAX_NAME_LEN)?;
    let vendor_code = request
        .vendor_code
        .as_deref()
        .map(|value| normalize_code(value, "vendorCode", MAX_VENDOR_CODE_LEN))
        .transpose()?
        .unwrap_or_else(|| vendor_code_from_name(&name, vendor_uuid));
    reject_integration_provider_as_model_vendor(&vendor_code, &name)?;
    Ok(NormalizedVendorCreateRequest {
        vendor_code,
        name,
        status: normalize_status(request.status.as_deref())?,
        color: normalize_color(request.color.as_deref())?,
        description: normalize_optional_text(
            request.description.as_deref(),
            "description",
            MAX_DESCRIPTION_LEN,
        )?,
    })
}

fn normalize_model_create_request(
    request: AdminAiModelCreateRequest,
) -> Result<NormalizedModelCreateRequest, AdminModelCommandBuildError> {
    let model_type = normalize_model_type(request.model_type.as_deref())?;
    let defaults = model_defaults(&model_type);
    let context_tokens = normalize_positive_i64(
        request.context_tokens.as_ref(),
        "contextTokens",
        MAX_CONTEXT_TOKENS,
    )?;
    let modalities = normalize_text_array(
        request.modalities,
        "modalities",
        MAX_MODEL_METADATA_ITEMS,
        MAX_MODEL_METADATA_TEXT_LEN,
    )?
    .unwrap_or_else(|| defaults.modalities.clone());
    let input_modalities = normalize_text_array(
        request.input_modalities,
        "inputModalities",
        MAX_MODEL_METADATA_ITEMS,
        MAX_MODEL_METADATA_TEXT_LEN,
    )?
    .unwrap_or_else(|| defaults.input_modalities.clone());
    let output_modalities = normalize_text_array(
        request.output_modalities,
        "outputModalities",
        MAX_MODEL_METADATA_ITEMS,
        MAX_MODEL_METADATA_TEXT_LEN,
    )?
    .unwrap_or_else(|| defaults.output_modalities.clone());
    let region_prices = normalize_create_region_prices(request.region_prices)?;
    let model = normalize_model_name(request.model.as_deref())?;
    let display_name = normalize_model_display_name(request.display_name.as_deref(), &model)?;
    Ok(NormalizedModelCreateRequest {
        vendor_id: normalize_value_text(request.vendor_id.as_ref(), "vendorId", MAX_NAME_LEN)?,
        model,
        display_name,
        model_type,
        region_prices,
        description: normalize_nullable_text(
            request.description.as_deref(),
            "description",
            MAX_PUBLIC_DESCRIPTION_LEN,
        )?,
        modalities,
        input_modalities,
        output_modalities,
        api_format: normalize_model_code(
            request.api_format.as_deref(),
            "apiFormat",
            MAX_API_FORMAT_LEN,
        )?
        .unwrap_or_else(|| defaults.api_format.to_owned()),
        capability_intro: normalize_nullable_text(
            request.capability_intro.as_deref(),
            "capabilityIntro",
            MAX_CAPABILITY_INTRO_LEN,
        )?,
        limitations: normalize_text_array(
            request.limitations,
            "limitations",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?
        .unwrap_or_default(),
        supported_languages: normalize_text_array(
            request.supported_languages,
            "supportedLanguages",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?
        .unwrap_or_default(),
        use_cases: normalize_text_array(
            request.use_cases,
            "useCases",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?
        .unwrap_or_default(),
        training_data_cutoff: normalize_nullable_text(
            request.training_data_cutoff.as_deref(),
            "trainingDataCutoff",
            MAX_TRAINING_DATA_CUTOFF_LEN,
        )?,
        context_tokens,
        max_output_tokens: normalize_optional_positive_i64(
            request.max_output_tokens.as_ref(),
            "maxOutputTokens",
            MAX_OUTPUT_TOKENS,
        )?,
        supports_streaming: request
            .supports_streaming
            .unwrap_or(defaults.supports_streaming),
        supports_tools: request.supports_tools.unwrap_or(defaults.supports_tools),
        supports_json_schema: request
            .supports_json_schema
            .unwrap_or(defaults.supports_json_schema),
        usage_scopes: normalize_text_array(
            request.usage_scopes,
            "usageScopes",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?
        .unwrap_or_default(),
        coding_visible: request.coding_visible.unwrap_or(defaults.coding_visible),
        release_stage: normalize_enum_i32(request.release_stage.as_ref(), "releaseStage", 1, 3)?
            .unwrap_or(1),
        shelf_state: normalize_enum_i32(request.shelf_state.as_ref(), "shelfState", 1, 3)?
            .unwrap_or(1),
        routing_state: normalize_enum_i32(request.routing_state.as_ref(), "routingState", 0, 2)?
            .unwrap_or(1),
        replacement_model: normalize_nullable_model_name(
            request.replacement_model.as_deref(),
            "replacementModel",
        )?,
    })
}

fn normalize_model_update_request(
    request: AdminAiModelUpdateRequest,
) -> Result<NormalizedModelUpdateRequest, AdminModelCommandBuildError> {
    let model_type = request
        .model_type
        .as_deref()
        .map(|value| normalize_model_type(Some(value)))
        .transpose()?;
    let defaults = model_type.as_deref().map(model_defaults);
    let modalities = normalize_text_array(
        request.modalities,
        "modalities",
        MAX_MODEL_METADATA_ITEMS,
        MAX_MODEL_METADATA_TEXT_LEN,
    )?
    .or_else(|| defaults.as_ref().map(|value| value.modalities.clone()));
    let input_modalities = normalize_text_array(
        request.input_modalities,
        "inputModalities",
        MAX_MODEL_METADATA_ITEMS,
        MAX_MODEL_METADATA_TEXT_LEN,
    )?
    .or_else(|| {
        defaults
            .as_ref()
            .map(|value| value.input_modalities.clone())
    });
    let output_modalities = normalize_text_array(
        request.output_modalities,
        "outputModalities",
        MAX_MODEL_METADATA_ITEMS,
        MAX_MODEL_METADATA_TEXT_LEN,
    )?
    .or_else(|| {
        defaults
            .as_ref()
            .map(|value| value.output_modalities.clone())
    });
    let api_format = normalize_model_code(
        request.api_format.as_deref(),
        "apiFormat",
        MAX_API_FORMAT_LEN,
    )?
    .or_else(|| defaults.as_ref().map(|value| value.api_format.to_owned()));
    let region_prices = normalize_update_region_prices(request.region_prices)?;
    Ok(NormalizedModelUpdateRequest {
        vendor_id: request
            .vendor_id
            .as_ref()
            .map(|value| normalize_value_text(Some(value), "vendorId", MAX_NAME_LEN))
            .transpose()?,
        model: request
            .model
            .as_deref()
            .map(|value| normalize_model_name(Some(value)))
            .transpose()?,
        display_name: match request.display_name {
            Some(value) => Some(normalize_optional_model_display_name(Some(&value))?),
            None => None,
        },
        model_type,
        region_prices,
        status: request
            .status
            .as_deref()
            .map(|value| normalize_status(Some(value)))
            .transpose()?,
        description: request
            .description
            .as_deref()
            .map(|value| {
                normalize_nullable_text(Some(value), "description", MAX_PUBLIC_DESCRIPTION_LEN)
            })
            .transpose()?,
        modalities,
        input_modalities,
        output_modalities,
        api_format,
        capability_intro: request
            .capability_intro
            .as_deref()
            .map(|value| {
                normalize_nullable_text(Some(value), "capabilityIntro", MAX_CAPABILITY_INTRO_LEN)
            })
            .transpose()?,
        limitations: normalize_text_array(
            request.limitations,
            "limitations",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?,
        supported_languages: normalize_text_array(
            request.supported_languages,
            "supportedLanguages",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?,
        use_cases: normalize_text_array(
            request.use_cases,
            "useCases",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?,
        training_data_cutoff: request
            .training_data_cutoff
            .as_deref()
            .map(|value| {
                normalize_nullable_text(
                    Some(value),
                    "trainingDataCutoff",
                    MAX_TRAINING_DATA_CUTOFF_LEN,
                )
            })
            .transpose()?,
        context_tokens: request
            .context_tokens
            .as_ref()
            .map(|value| normalize_positive_i64(Some(value), "contextTokens", MAX_CONTEXT_TOKENS))
            .transpose()?,
        max_output_tokens: request
            .max_output_tokens
            .as_ref()
            .map(|value| {
                normalize_optional_positive_i64(Some(value), "maxOutputTokens", MAX_OUTPUT_TOKENS)
            })
            .transpose()?,
        supports_streaming: request
            .supports_streaming
            .or_else(|| defaults.as_ref().map(|value| value.supports_streaming)),
        supports_tools: request
            .supports_tools
            .or_else(|| defaults.as_ref().map(|value| value.supports_tools)),
        supports_json_schema: request
            .supports_json_schema
            .or_else(|| defaults.as_ref().map(|value| value.supports_json_schema)),
        usage_scopes: normalize_text_array(
            request.usage_scopes,
            "usageScopes",
            MAX_MODEL_METADATA_ITEMS,
            MAX_MODEL_METADATA_TEXT_LEN,
        )?,
        coding_visible: request
            .coding_visible
            .or_else(|| defaults.as_ref().map(|value| value.coding_visible)),
        release_stage: normalize_enum_i32(request.release_stage.as_ref(), "releaseStage", 1, 3)?,
        shelf_state: normalize_enum_i32(request.shelf_state.as_ref(), "shelfState", 1, 3)?,
        routing_state: normalize_enum_i32(request.routing_state.as_ref(), "routingState", 0, 2)?,
        replacement_model: request
            .replacement_model
            .as_deref()
            .map(|value| normalize_nullable_model_name(Some(value), "replacementModel"))
            .transpose()?,
    })
}

fn normalize_model_mapping_create_request(
    request: AdminModelMappingCreateRequest,
) -> Result<AdminModelMappingRuleDraft, AdminModelCommandBuildError> {
    let bindings = normalize_mapping_bindings(request.bindings, true)?.ok_or_else(|| {
        AdminModelCommandBuildError::BadRequest("bindings are required".to_owned())
    })?;
    let mapping_items = normalize_mapping_items(request.mapping_items, true)?.ok_or_else(|| {
        AdminModelCommandBuildError::BadRequest("mappingItems are required".to_owned())
    })?;
    validate_unique_mapping_item_sources(&mapping_items)?;
    Ok(AdminModelMappingRuleDraft {
        source_vendor_id: normalize_optional_id_value(
            request.source_vendor_id.as_ref(),
            "sourceVendorId",
        )?,
        source_vendor_code: normalize_required_code(
            request.source_vendor_code.as_deref(),
            "sourceVendorCode",
            MAX_VENDOR_CODE_LEN,
        )?,
        target_vendor_id: normalize_optional_id_value(
            request.target_vendor_id.as_ref(),
            "targetVendorId",
        )?,
        target_vendor_code: normalize_required_code(
            request.target_vendor_code.as_deref(),
            "targetVendorCode",
            MAX_VENDOR_CODE_LEN,
        )?,
        mapping_mode: normalize_mapping_mode(request.mapping_mode.as_deref().unwrap_or("alias"))?,
        match_type: normalize_mapping_match_type(request.match_type.as_deref().unwrap_or("exact"))?,
        enabled: request.enabled.unwrap_or(true),
        bindings,
        mapping_items,
    })
}

fn normalize_model_mapping_update_request(
    request: AdminModelMappingUpdateRequest,
) -> Result<AdminModelMappingRulePatch, AdminModelCommandBuildError> {
    let bindings = normalize_mapping_bindings(request.bindings, false)?;
    let mapping_items = normalize_mapping_items(request.mapping_items, false)?;
    if let Some(items) = mapping_items.as_ref() {
        validate_unique_mapping_item_sources(items)?;
    }
    let patch = AdminModelMappingRulePatch {
        source_vendor_id: request
            .source_vendor_id
            .as_ref()
            .map(|value| normalize_optional_id_value(Some(value), "sourceVendorId"))
            .transpose()?,
        source_vendor_code: request
            .source_vendor_code
            .as_deref()
            .map(|value| {
                normalize_required_code(Some(value), "sourceVendorCode", MAX_VENDOR_CODE_LEN)
            })
            .transpose()?,
        target_vendor_id: request
            .target_vendor_id
            .as_ref()
            .map(|value| normalize_optional_id_value(Some(value), "targetVendorId"))
            .transpose()?,
        target_vendor_code: request
            .target_vendor_code
            .as_deref()
            .map(|value| {
                normalize_required_code(Some(value), "targetVendorCode", MAX_VENDOR_CODE_LEN)
            })
            .transpose()?,
        mapping_mode: request
            .mapping_mode
            .as_deref()
            .map(normalize_mapping_mode)
            .transpose()?,
        match_type: request
            .match_type
            .as_deref()
            .map(normalize_mapping_match_type)
            .transpose()?,
        enabled: request.enabled,
        bindings,
        mapping_items,
    };
    Ok(patch)
}

fn normalize_required_text(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or("").trim();
    if value.is_empty() {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} is required"
        )));
    }
    if value.chars().count() > max_len {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be at most {max_len} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must not contain control characters"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_optional_text(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or("").trim();
    if value.chars().count() > max_len {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be at most {max_len} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must not contain control characters"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_nullable_text(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let value = normalize_optional_text(value, field_name, max_len)?;
    Ok((!value.is_empty()).then_some(value))
}

fn normalize_value_text(
    value: Option<&Value>,
    field_name: &str,
    max_len: usize,
) -> Result<String, AdminModelCommandBuildError> {
    let raw = match value {
        Some(Value::String(value)) => value.trim().to_owned(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    };
    normalize_required_text(Some(&raw), field_name, max_len)
}

fn normalize_model_name(value: Option<&str>) -> Result<String, AdminModelCommandBuildError> {
    let value = normalize_required_text(value, "model name", MAX_NAME_LEN)?;
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
    }) {
        return Err(AdminModelCommandBuildError::BadRequest(
            "model name must use ASCII letters, numbers, dot, underscore, colon, slash, or hyphen"
                .to_owned(),
        ));
    }
    Ok(value)
}

fn normalize_model_display_name(
    value: Option<&str>,
    fallback_model: &str,
) -> Result<String, AdminModelCommandBuildError> {
    let value = normalize_optional_model_display_name(value)?;
    Ok(value.unwrap_or_else(|| fallback_model.to_owned()))
}

fn normalize_optional_model_display_name(
    value: Option<&str>,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    normalize_nullable_text(value, "displayName", MAX_NAME_LEN)
}

fn normalize_nullable_model_name(
    value: Option<&str>,
    field_name: &str,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let value = normalize_nullable_text(value, field_name, MAX_NAME_LEN)?;
    match value {
        Some(value) => normalize_model_name(Some(&value)).map(Some),
        None => Ok(None),
    }
}

fn normalize_model_code(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let value = normalize_nullable_text(value, field_name, max_len)?;
    let Some(value) = value else {
        return Ok(None);
    };
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must use ASCII letters, numbers, hyphen, or underscore"
        )));
    }
    Ok(Some(value.replace('-', "_")))
}

fn normalize_region_code(
    value: Option<&str>,
    field_name: &str,
) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or(DEFAULT_MODEL_REGION_CODE).trim();
    if value.is_empty() {
        return Ok(DEFAULT_MODEL_REGION_CODE.to_owned());
    }
    if value.len() > MAX_REGION_CODE_LEN
        || !value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'-' | b'_'))
        })
    {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be a lowercase region code"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_currency_code(
    value: Option<&str>,
    region_code: &str,
    field_name: &str,
) -> Result<String, AdminModelCommandBuildError> {
    let fallback = default_currency_for_region(region_code);
    let value = value.unwrap_or(fallback).trim();
    if value.is_empty() {
        return Ok(fallback.to_owned());
    }
    let normalized = value.to_ascii_uppercase();
    if normalized.len() != 3 || !normalized.bytes().all(|byte| byte.is_ascii_uppercase()) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be a 3-letter ISO currency code"
        )));
    }
    Ok(normalized)
}

fn default_currency_for_region(region_code: &str) -> &'static str {
    match region_code {
        "cn" => "CNY",
        _ => "USD",
    }
}

fn normalize_create_region_prices(
    region_prices: Option<Vec<AdminAiModelRegionPriceRequest>>,
) -> Result<Vec<AdminAiModelRegionPriceCommand>, AdminModelCommandBuildError> {
    match region_prices {
        Some(region_prices) if !region_prices.is_empty() => {
            normalize_region_price_requests(region_prices, true)
        }
        _ => Err(AdminModelCommandBuildError::BadRequest(
            "regionPrices must not be empty".to_owned(),
        )),
    }
}

fn normalize_update_region_prices(
    region_prices: Option<Vec<AdminAiModelRegionPriceRequest>>,
) -> Result<Option<Vec<AdminAiModelRegionPriceCommand>>, AdminModelCommandBuildError> {
    match region_prices {
        Some(region_prices) => {
            if region_prices.is_empty() {
                return Err(AdminModelCommandBuildError::BadRequest(
                    "regionPrices must not be empty".to_owned(),
                ));
            }
            normalize_region_price_requests(region_prices, true).map(Some)
        }
        None => Ok(None),
    }
}

fn normalize_region_price_requests(
    region_prices: Vec<AdminAiModelRegionPriceRequest>,
    require_prices: bool,
) -> Result<Vec<AdminAiModelRegionPriceCommand>, AdminModelCommandBuildError> {
    let mut normalized = Vec::with_capacity(region_prices.len());
    for (index, price) in region_prices.into_iter().enumerate() {
        let field_prefix = format!("regionPrices[{index}]");
        let region_code = normalize_region_code(
            price.region_code.as_deref(),
            &format!("{field_prefix}.regionCode"),
        )?;
        if normalized
            .iter()
            .any(|item: &AdminAiModelRegionPriceCommand| item.region_code == region_code)
        {
            return Err(AdminModelCommandBuildError::BadRequest(format!(
                "{field_prefix}.regionCode duplicates region {region_code}"
            )));
        }
        let currency = normalize_currency_code(
            price.currency.as_deref(),
            &region_code,
            &format!("{field_prefix}.currency"),
        )?;
        let price_in = if require_prices {
            normalize_decimal_amount(price.price_in.as_ref(), &format!("{field_prefix}.priceIn"))?
        } else {
            normalize_optional_decimal_amount(
                price.price_in.as_ref(),
                &format!("{field_prefix}.priceIn"),
            )?
            .unwrap_or_default()
        };
        let price_out = if require_prices {
            normalize_decimal_amount(
                price.price_out.as_ref(),
                &format!("{field_prefix}.priceOut"),
            )?
        } else {
            normalize_optional_decimal_amount(
                price.price_out.as_ref(),
                &format!("{field_prefix}.priceOut"),
            )?
            .unwrap_or_default()
        };
        normalized.push(AdminAiModelRegionPriceCommand {
            region_code,
            currency,
            price_in,
            price_out,
            cache_read_price: normalize_optional_decimal_amount(
                price.cache_read_price.as_ref(),
                &format!("{field_prefix}.cacheReadPrice"),
            )?,
            cache_write_price: normalize_optional_decimal_amount(
                price.cache_write_price.as_ref(),
                &format!("{field_prefix}.cacheWritePrice"),
            )?,
        });
    }
    Ok(normalized)
}

fn normalize_model_type(value: Option<&str>) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or("Chat").trim().to_ascii_lowercase();
    match value.as_str() {
        "chat" | "llm" | "text" => Ok("Chat".to_owned()),
        "image" => Ok("Image".to_owned()),
        "audio" | "speech" => Ok("Audio".to_owned()),
        "embedding" | "embeddings" => Ok("Embedding".to_owned()),
        "music" => Ok("Music".to_owned()),
        "sfx" | "soundeffect" | "sound_effect" | "sound_effects" | "sound effect"
        | "sound effects" => Ok("SoundEffect".to_owned()),
        "video" => Ok("Video".to_owned()),
        _ => Err(AdminModelCommandBuildError::BadRequest(
            "type must be Chat, Image, Audio, Embedding, Music, SoundEffect, or Video".to_owned(),
        )),
    }
}

fn normalize_status(value: Option<&str>) -> Result<String, AdminModelCommandBuildError> {
    match value
        .unwrap_or("active")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "active" | "enabled" | "normal" => Ok("active".to_owned()),
        "inactive" | "disabled" => Ok("inactive".to_owned()),
        _ => Err(AdminModelCommandBuildError::BadRequest(
            "status must be active or inactive".to_owned(),
        )),
    }
}

fn normalize_color(value: Option<&str>) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or("bg-slate-700").trim();
    if value.is_empty() {
        return Ok("bg-slate-700".to_owned());
    }
    if value.chars().count() > MAX_COLOR_LEN
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'/' | b'#')
        })
    {
        return Err(AdminModelCommandBuildError::BadRequest(
            "color must be a safe style token".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

fn normalize_code(
    value: &str,
    field_name: &str,
    max_len: usize,
) -> Result<String, AdminModelCommandBuildError> {
    let code = slugify(value);
    if code.is_empty() {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} is invalid"
        )));
    }
    if code.len() > max_len {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be at most {max_len} bytes after normalization"
        )));
    }
    Ok(code)
}

fn normalize_nullable_code(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.trim().is_empty() {
        return Ok(None);
    }
    normalize_code(value, field_name, max_len).map(Some)
}

fn normalize_required_code(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or("").trim();
    if value.is_empty() {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} is required"
        )));
    }
    normalize_code(value, field_name, max_len)
}

fn normalize_nullable_binding_code(
    value: Option<&str>,
    field_name: &str,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':'))
    {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be a safe binding code"
        )));
    }
    Ok(Some(value.to_owned()))
}

fn normalize_mapping_binding_type(value: &str) -> Result<String, AdminModelCommandBuildError> {
    let value =
        normalize_mapping_required_text(Some(value), "bindingType", MAX_MAPPING_BINDING_TYPE_LEN)?
            .to_ascii_lowercase();
    if MAPPING_BINDING_TYPES.contains(&value.as_str()) {
        return Ok(value);
    }
    Err(AdminModelCommandBuildError::BadRequest(format!(
        "bindingType must be one of {}",
        MAPPING_BINDING_TYPES.join(", ")
    )))
}

fn normalize_mapping_mode(value: &str) -> Result<String, AdminModelCommandBuildError> {
    let value =
        normalize_mapping_required_text(Some(value), "mappingMode", MAX_MAPPING_BINDING_TYPE_LEN)?
            .to_ascii_lowercase();
    if MAPPING_MODES.contains(&value.as_str()) {
        return Ok(value);
    }
    Err(AdminModelCommandBuildError::BadRequest(
        "mappingMode must be alias".to_owned(),
    ))
}

fn normalize_mapping_match_type(value: &str) -> Result<String, AdminModelCommandBuildError> {
    let value =
        normalize_mapping_required_text(Some(value), "matchType", MAX_MAPPING_BINDING_TYPE_LEN)?
            .to_ascii_lowercase();
    if MAPPING_MATCH_TYPES.contains(&value.as_str()) {
        return Ok(value);
    }
    Err(AdminModelCommandBuildError::BadRequest(
        "matchType must be exact".to_owned(),
    ))
}

fn normalize_mapping_required_text(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<String, AdminModelCommandBuildError> {
    let value = normalize_required_text(value, field_name, max_len)?;
    if value.chars().any(char::is_control) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must not contain control characters"
        )));
    }
    Ok(value)
}

fn normalize_mapping_optional_text(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let value = normalize_optional_text(value, field_name, max_len)?;
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().any(char::is_control) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must not contain control characters"
        )));
    }
    Ok(Some(value))
}

fn normalize_mapping_bindings(
    items: Option<Vec<AdminModelMappingBindingRequest>>,
    required: bool,
) -> Result<Option<Vec<AdminModelMappingRuleBindingDraft>>, AdminModelCommandBuildError> {
    let Some(items) = items else {
        return if required {
            Err(AdminModelCommandBuildError::BadRequest(
                "bindings are required".to_owned(),
            ))
        } else {
            Ok(None)
        };
    };
    if items.is_empty() {
        return Err(AdminModelCommandBuildError::BadRequest(
            "bindings must contain at least one item".to_owned(),
        ));
    }
    if items.len() > MAX_MODEL_MAPPING_CHILDREN {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "bindings cannot exceed {MAX_MODEL_MAPPING_CHILDREN} items"
        )));
    }
    let mut normalized = Vec::with_capacity(items.len());
    for (index, item) in items.into_iter().enumerate() {
        let binding_type =
            normalize_mapping_binding_type(item.binding_type.as_deref().unwrap_or("global"))?;
        let binding_id = normalize_optional_id_value(item.binding_id.as_ref(), "bindingId")?;
        let binding_code =
            normalize_nullable_binding_code(item.binding_code.as_deref(), "bindingCode")?;
        if binding_type != "global" && binding_id.is_none() && binding_code.is_none() {
            return Err(AdminModelCommandBuildError::BadRequest(format!(
                "binding content is required for binding row {}",
                index + 1
            )));
        }
        normalized.push(AdminModelMappingRuleBindingDraft {
            id: normalize_optional_id_value(item.id.as_ref(), "binding id")?,
            binding_type,
            binding_id,
            binding_code,
            binding_name: normalize_mapping_optional_text(
                item.binding_name.as_deref(),
                "bindingName",
                MAX_MAPPING_TEXT_LEN,
            )?,
            enabled: item.enabled.unwrap_or(true),
        });
    }
    Ok(Some(normalized))
}

fn normalize_mapping_items(
    items: Option<Vec<AdminModelMappingItemRequest>>,
    required: bool,
) -> Result<Option<Vec<AdminModelMappingRuleItemDraft>>, AdminModelCommandBuildError> {
    let Some(items) = items else {
        return if required {
            Err(AdminModelCommandBuildError::BadRequest(
                "mappingItems are required".to_owned(),
            ))
        } else {
            Ok(None)
        };
    };
    if items.is_empty() {
        return Err(AdminModelCommandBuildError::BadRequest(
            "mappingItems must contain at least one item".to_owned(),
        ));
    }
    if items.len() > MAX_MODEL_MAPPING_CHILDREN {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "mappingItems cannot exceed {MAX_MODEL_MAPPING_CHILDREN} items"
        )));
    }
    let mut normalized = Vec::with_capacity(items.len());
    for item in items {
        normalized.push(AdminModelMappingRuleItemDraft {
            id: normalize_optional_id_value(item.id.as_ref(), "mapping item id")?,
            source_model: normalize_mapping_required_text(
                item.source_model.as_deref(),
                "sourceModel",
                MAX_MAPPING_TEXT_LEN,
            )?,
            source_catalog_key: normalize_mapping_optional_text(
                item.source_catalog_key.as_deref(),
                "sourceCatalogKey",
                MAX_MAPPING_TEXT_LEN,
            )?,
            target_model: normalize_mapping_required_text(
                item.target_model.as_deref(),
                "targetModel",
                MAX_MAPPING_TEXT_LEN,
            )?,
            target_catalog_key: normalize_mapping_optional_text(
                item.target_catalog_key.as_deref(),
                "targetCatalogKey",
                MAX_MAPPING_TEXT_LEN,
            )?,
            target_provider_model: normalize_mapping_optional_text(
                item.target_provider_model.as_deref(),
                "targetProviderModel",
                MAX_MAPPING_TEXT_LEN,
            )?,
            target_provider_native_model: normalize_mapping_optional_text(
                item.target_provider_native_model.as_deref(),
                "targetProviderNativeModel",
                MAX_MAPPING_TEXT_LEN,
            )?,
            enabled: item.enabled,
        });
    }
    Ok(Some(normalized))
}

fn validate_unique_mapping_item_sources(
    items: &[AdminModelMappingRuleItemDraft],
) -> Result<(), AdminModelCommandBuildError> {
    let mut seen = std::collections::BTreeSet::new();
    for item in items {
        let key = item.source_model.to_ascii_lowercase();
        if !seen.insert(key) {
            return Err(AdminModelCommandBuildError::BadRequest(
                "duplicate source model mapping is not allowed".to_owned(),
            ));
        }
    }
    Ok(())
}

fn normalize_optional_id_value(
    value: Option<&Value>,
    field_name: &str,
) -> Result<Option<i64>, AdminModelCommandBuildError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let raw = match value {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        _ => {
            return Err(AdminModelCommandBuildError::BadRequest(format!(
                "{field_name} must be a positive integer"
            )))
        }
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let parsed = raw.parse::<i64>().map_err(|_| {
        AdminModelCommandBuildError::BadRequest(format!("{field_name} must be a positive integer"))
    })?;
    if parsed <= 0 {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be a positive integer"
        )));
    }
    Ok(Some(parsed))
}

fn vendor_code_from_name(name: &str, vendor_uuid: &str) -> String {
    let code = slugify(name);
    if !code.is_empty() && code.len() <= MAX_VENDOR_CODE_LEN {
        return code;
    }
    let short = vendor_uuid.chars().take(16).collect::<String>();
    format!("custom_{short}")
        .chars()
        .take(MAX_VENDOR_CODE_LEN)
        .collect()
}

fn reject_integration_provider_as_model_vendor(
    vendor_code: &str,
    name: &str,
) -> Result<(), AdminModelCommandBuildError> {
    let code = vendor_code.trim().to_ascii_lowercase();
    let normalized_name = name
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', '.', '/'], " ");
    if INTEGRATION_PROVIDER_ONLY_CODES.contains(&code.as_str())
        || INTEGRATION_PROVIDER_ONLY_NAME_MARKERS
            .iter()
            .any(|marker| normalized_name.contains(marker))
    {
        return Err(AdminModelCommandBuildError::BadRequest(
            "model vendor must be the model publisher; cloud, relay, local runtime, and aggregator access belongs in integration_provider".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_decimal_amount(
    value: Option<&Value>,
    field_name: &str,
) -> Result<String, AdminModelCommandBuildError> {
    let raw = match value {
        Some(Value::String(value)) => value.trim().to_owned(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    };
    if raw.is_empty() {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} is required"
        )));
    }
    let value = raw.trim().trim_start_matches('$').replace(',', "");
    if value.is_empty()
        || value.starts_with('-')
        || value.starts_with('+')
        || value.contains('e')
        || value.contains('E')
    {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be a positive decimal amount"
        )));
    }
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() > 2
        || parts[0].is_empty()
        || !parts[0].chars().all(|ch| ch.is_ascii_digit())
        || parts
            .get(1)
            .map(|part| !part.chars().all(|ch| ch.is_ascii_digit()) || part.len() > 12)
            .unwrap_or(false)
        || parts[0].len() > 24
    {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be a valid decimal amount with at most 12 decimal places"
        )));
    }
    let whole = parts[0].trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let mut fraction = parts
        .get(1)
        .copied()
        .unwrap_or("")
        .trim_end_matches('0')
        .to_owned();
    let has_non_zero = whole != "0" || fraction.chars().any(|ch| ch != '0');
    if !has_non_zero {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be greater than zero"
        )));
    }
    while fraction.len() < 6 {
        fraction.push('0');
    }
    Ok(format!("{whole}.{fraction}"))
}

fn normalize_positive_i64(
    value: Option<&Value>,
    field_name: &str,
    max_value: i64,
) -> Result<i64, AdminModelCommandBuildError> {
    let raw = match value {
        Some(Value::String(value)) => value.trim().to_owned(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    };
    if raw.is_empty() {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} is required"
        )));
    }
    let normalized = raw.replace(',', "").replace('_', "");
    let (number, multiplier) = match normalized.chars().last() {
        Some('k') | Some('K') => (&normalized[..normalized.len() - 1], 1_000_i64),
        Some('m') | Some('M') => (&normalized[..normalized.len() - 1], 1_000_000_i64),
        _ => (normalized.as_str(), 1_i64),
    };
    let value = number.parse::<i64>().map_err(|_| {
        AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be a positive integer, K, or M token count"
        ))
    })?;
    let value = value.checked_mul(multiplier).ok_or_else(|| {
        AdminModelCommandBuildError::BadRequest(format!("{field_name} is too large"))
    })?;
    if !(1..=max_value).contains(&value) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be between 1 and {max_value}"
        )));
    }
    Ok(value)
}

fn normalize_optional_decimal_amount(
    value: Option<&Value>,
    field_name: &str,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(value) => normalize_decimal_amount(Some(value), field_name).map(Some),
    }
}

fn normalize_optional_positive_i64(
    value: Option<&Value>,
    field_name: &str,
    max_value: i64,
) -> Result<Option<i64>, AdminModelCommandBuildError> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(None),
        Some(_) => normalize_positive_i64(value, field_name, max_value).map(Some),
        None => Ok(None),
    }
}

fn normalize_enum_i32(
    value: Option<&Value>,
    field_name: &str,
    min_value: i32,
    max_value: i32,
) -> Result<Option<i32>, AdminModelCommandBuildError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let raw = match value {
        Value::String(value) => value.trim().to_owned(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    };
    if raw.is_empty() {
        return Ok(None);
    }
    let value = raw.parse::<i32>().map_err(|_| {
        AdminModelCommandBuildError::BadRequest(format!("{field_name} must be an integer"))
    })?;
    if !(min_value..=max_value).contains(&value) {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must be between {min_value} and {max_value}"
        )));
    }
    Ok(Some(value))
}

fn normalize_text_array(
    value: Option<Vec<String>>,
    field_name: &str,
    max_items: usize,
    max_item_len: usize,
) -> Result<Option<Vec<String>>, AdminModelCommandBuildError> {
    let Some(values) = value else {
        return Ok(None);
    };
    if values.len() > max_items {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "{field_name} must contain at most {max_items} items"
        )));
    }
    let mut normalized = Vec::new();
    for value in values {
        let value = normalize_optional_text(Some(&value), field_name, max_item_len)?;
        if !value.is_empty() && !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    Ok(Some(normalized))
}

fn normalize_model_id(value: &str) -> Result<String, AdminModelCommandBuildError> {
    let value = normalize_required_text(Some(value), "modelId", MAX_MODEL_ID_LEN)?;
    if !value.bytes().all(is_model_identity_byte) {
        return Err(AdminModelCommandBuildError::BadRequest(
            "modelId must use ASCII letters, numbers, slash, dot, colon, hyphen, or underscore"
                .to_owned(),
        ));
    }
    Ok(value)
}

fn is_model_identity_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b':' | b'-' | b'_')
}

fn normalize_source(value: Option<&str>) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or(DEFAULT_CATALOG_REFRESH_SOURCE).trim();
    if value.is_empty() {
        return Ok(DEFAULT_CATALOG_REFRESH_SOURCE.to_owned());
    }
    if value.len() > MAX_SOURCE_LEN
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AdminModelCommandBuildError::BadRequest(
            "source must contain only letters, numbers, -, and _".to_owned(),
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn normalize_sync_mode(value: Option<&str>) -> Result<String, AdminModelCommandBuildError> {
    let value = value.unwrap_or("official_refresh").trim();
    if value.is_empty() {
        return Ok("official_refresh".to_owned());
    }
    if value.len() > MAX_SYNC_MODE_LEN
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AdminModelCommandBuildError::BadRequest(
            "mode must contain only letters, numbers, -, and _".to_owned(),
        ));
    }
    let value = value.to_ascii_lowercase();
    if !matches!(
        value.as_str(),
        "official_refresh" | "vendor_refresh" | "catalog_version_refresh" | "dry_run"
    ) {
        return Err(AdminModelCommandBuildError::BadRequest(
            "mode must be official_refresh, vendor_refresh, catalog_version_refresh, or dry_run"
                .to_owned(),
        ));
    }
    Ok(value)
}

fn normalize_sync_vendor_codes(
    value: Option<Vec<String>>,
) -> Result<Vec<String>, AdminModelCommandBuildError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.len() > MAX_SYNC_VENDOR_CODES {
        return Err(AdminModelCommandBuildError::BadRequest(format!(
            "vendorCodes must contain {MAX_SYNC_VENDOR_CODES} items or fewer"
        )));
    }
    let mut vendor_codes = Vec::new();
    for item in value {
        let item = normalize_code(&item, "vendorCodes", MAX_VENDOR_CODE_LEN)?;
        if !vendor_codes.iter().any(|existing| existing == &item) {
            vendor_codes.push(item);
        }
    }
    Ok(vendor_codes)
}

fn normalize_optional_catalog_root(
    value: Option<&str>,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let value = normalize_optional_text(value, "catalogRoot", MAX_CATALOG_ROOT_LEN)?;
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().any(char::is_control) {
        return Err(AdminModelCommandBuildError::BadRequest(
            "catalogRoot must not contain control characters".to_owned(),
        ));
    }
    Ok(Some(value))
}

fn normalize_optional_catalog_version(
    value: Option<&str>,
) -> Result<Option<String>, AdminModelCommandBuildError> {
    let value = normalize_optional_text(value, "catalogVersion", MAX_CATALOG_VERSION_LEN)?;
    if value.is_empty() {
        return Ok(None);
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(AdminModelCommandBuildError::BadRequest(
            "catalogVersion must contain only letters, numbers, ., -, and _".to_owned(),
        ));
    }
    Ok(Some(value))
}

fn generate_entity_uuid(
    state: &AdminModelCommandState,
) -> Result<String, AdminModelCommandBuildError> {
    state
        .entity_uuid_generator
        .generate_entity_uuid()
        .map_err(AdminModelCommandBuildError::System)
}

fn generate_entity_uuids(
    state: &AdminModelCommandState,
    count: usize,
) -> Result<Vec<String>, AdminModelCommandBuildError> {
    (0..count).map(|_| generate_entity_uuid(state)).collect()
}

fn request_id_error(error: RequestIdError) -> AdminModelCommandBuildError {
    match error {
        RequestIdError::Invalid(message) => AdminModelCommandBuildError::BadRequest(message),
        RequestIdError::System(message) => {
            AdminModelCommandBuildError::System(DomainError::new(message))
        }
    }
}

fn to_vendor_response(item: AdminModelVendorItem) -> AdminModelVendorItemResponse {
    AdminModelVendorItemResponse {
        id: item.id.to_string(),
        vendor_code: item.vendor_code,
        name: item.name,
        status: item.status,
        color: item.color,
        description: item.description,
        supported_protocols: parse_json_response_value(
            &item.supported_protocols,
            Value::Array(Vec::new()),
        ),
        client_api_compatibility: parse_json_response_value(
            &item.client_api_compatibility,
            Value::Object(Default::default()),
        ),
        model_count: item.model_count,
    }
}

fn parse_json_response_value(source: &str, fallback: Value) -> Value {
    serde_json::from_str(source).unwrap_or(fallback)
}

fn to_model_response(item: AdminAiModelItem) -> AdminAiModelItemResponse {
    AdminAiModelItemResponse {
        id: item.id.to_string(),
        vendor_id: item.vendor_id,
        vendor_code: item.vendor_code,
        model: item.model,
        display_name: item.display_name,
        name: item.name,
        model_type: item.model_type,
        region_prices: item
            .region_prices
            .into_iter()
            .map(to_model_region_price_response)
            .collect(),
        status: item.status,
        calls: item.calls,
        description: item.description,
        modalities: item.modalities,
        input_modalities: item.input_modalities,
        output_modalities: item.output_modalities,
        api_format: item.api_format,
        capability_intro: item.capability_intro,
        limitations: item.limitations,
        supported_languages: item.supported_languages,
        use_cases: item.use_cases,
        training_data_cutoff: item.training_data_cutoff,
        context_tokens: item.context_tokens,
        max_output_tokens: item.max_output_tokens,
        supports_streaming: item.supports_streaming,
        supports_tools: item.supports_tools,
        supports_json_schema: item.supports_json_schema,
        usage_scopes: item.usage_scopes,
        coding_visible: item.coding_visible,
        release_stage: item.release_stage,
        shelf_state: item.shelf_state,
        routing_state: item.routing_state,
        replacement_model: item.replacement_model,
    }
}

fn to_model_region_price_response(
    item: AdminAiModelRegionPriceCommand,
) -> AdminAiModelRegionPriceResponse {
    AdminAiModelRegionPriceResponse {
        region_code: item.region_code,
        currency: item.currency,
        price_in: item.price_in,
        price_out: item.price_out,
        cache_read_price: item.cache_read_price.unwrap_or_default(),
        cache_write_price: item.cache_write_price.unwrap_or_default(),
    }
}

fn to_mapping_response(item: AdminModelMappingRuleItem) -> AdminModelMappingRuleResponse {
    AdminModelMappingRuleResponse {
        id: item.id.to_string(),
        binding_type: item.binding_type,
        source_vendor_id: item.source_vendor_id.map(|value| value.to_string()),
        source_vendor_code: item.source_vendor_code.unwrap_or_default(),
        target_vendor_id: item.target_vendor_id.map(|value| value.to_string()),
        target_vendor_code: item.target_vendor_code.unwrap_or_default(),
        mapping_mode: item.mapping_mode,
        match_type: item.match_type,
        enabled: item.enabled,
        bindings: item
            .bindings
            .into_iter()
            .map(to_mapping_binding_response)
            .collect(),
        mapping_items: item
            .mapping_items
            .into_iter()
            .map(to_mapping_item_response)
            .collect(),
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

fn to_mapping_binding_response(
    item: AdminModelMappingRuleBindingItem,
) -> AdminModelMappingRuleBindingResponse {
    AdminModelMappingRuleBindingResponse {
        id: item.id.to_string(),
        binding_type: item.binding_type,
        binding_id: item.binding_id.map(|value| value.to_string()),
        binding_code: item.binding_code,
        binding_name: item.binding_name,
        sort_order: item.sort_order,
        enabled: item.enabled,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

fn to_mapping_item_response(
    item: AdminModelMappingRuleMappingItem,
) -> AdminModelMappingRuleItemResponse {
    AdminModelMappingRuleItemResponse {
        id: item.id.to_string(),
        source_model: item.source_model,
        source_catalog_key: item.source_catalog_key,
        target_model: item.target_model,
        target_catalog_key: item.target_catalog_key,
        target_provider_model: item.target_provider_model,
        target_provider_native_model: item.target_provider_native_model,
        sort_order: item.sort_order,
        enabled: item.enabled,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

fn to_mapping_resolve_response(
    result: ResolveAdminModelMappingResult,
) -> AdminModelMappingResolveResponse {
    AdminModelMappingResolveResponse {
        source_model: result.source_model,
        target_model: result.target_model,
        target_catalog_key: result.target_catalog_key,
        target_vendor_code: result.target_vendor_code,
        target_provider_model: result.target_provider_model,
        target_provider_native_model: result.target_provider_native_model,
        matched: result.matched,
        matched_binding_type: result.matched_binding_type,
        rule: result.rule.map(to_mapping_response),
    }
}

#[derive(Debug, Clone)]
struct ModelDefaults {
    modalities: Vec<String>,
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
    api_format: &'static str,
    supports_streaming: bool,
    supports_tools: bool,
    supports_json_schema: bool,
    coding_visible: bool,
}

fn model_defaults(model_type: &str) -> ModelDefaults {
    match model_type {
        "Image" => ModelDefaults {
            modalities: vec!["image".to_owned()],
            input_modalities: vec!["text".to_owned(), "image".to_owned()],
            output_modalities: vec!["image".to_owned()],
            api_format: "openai_compatible",
            supports_streaming: false,
            supports_tools: false,
            supports_json_schema: false,
            coding_visible: false,
        },
        "Audio" => ModelDefaults {
            modalities: vec!["audio".to_owned()],
            input_modalities: vec!["audio".to_owned(), "text".to_owned()],
            output_modalities: vec!["audio".to_owned(), "text".to_owned()],
            api_format: "openai_compatible",
            supports_streaming: false,
            supports_tools: false,
            supports_json_schema: false,
            coding_visible: false,
        },
        "Embedding" => ModelDefaults {
            modalities: vec!["embedding".to_owned()],
            input_modalities: vec!["text".to_owned()],
            output_modalities: vec!["embedding".to_owned()],
            api_format: "openai_compatible",
            supports_streaming: false,
            supports_tools: false,
            supports_json_schema: false,
            coding_visible: false,
        },
        "Music" => ModelDefaults {
            modalities: vec!["music".to_owned()],
            input_modalities: vec!["text".to_owned(), "audio".to_owned()],
            output_modalities: vec!["audio".to_owned()],
            api_format: "openai_compatible",
            supports_streaming: false,
            supports_tools: false,
            supports_json_schema: false,
            coding_visible: false,
        },
        "SoundEffect" => ModelDefaults {
            modalities: vec!["sfx".to_owned()],
            input_modalities: vec!["text".to_owned(), "audio".to_owned()],
            output_modalities: vec!["audio".to_owned()],
            api_format: "openai_compatible",
            supports_streaming: false,
            supports_tools: false,
            supports_json_schema: false,
            coding_visible: false,
        },
        "Video" => ModelDefaults {
            modalities: vec!["video".to_owned()],
            input_modalities: vec!["text".to_owned(), "image".to_owned(), "video".to_owned()],
            output_modalities: vec!["video".to_owned()],
            api_format: "openai_compatible",
            supports_streaming: false,
            supports_tools: false,
            supports_json_schema: false,
            coding_visible: false,
        },
        _ => ModelDefaults {
            modalities: vec!["text".to_owned()],
            input_modalities: vec!["text".to_owned(), "image".to_owned()],
            output_modalities: vec!["text".to_owned()],
            api_format: "openai_responses",
            supports_streaming: true,
            supports_tools: true,
            supports_json_schema: true,
            coding_visible: true,
        },
    }
}

fn to_sync_response(item: AdminModelCatalogSyncItem) -> AdminModelCatalogSyncResponse {
    AdminModelCatalogSyncResponse {
        synced: item.synced,
        source: item.source,
        mode: item.mode,
        dry_run: item.dry_run,
        catalog_version: item.catalog_version,
        requested_catalog_version: item.requested_catalog_version,
        catalog_root: item.catalog_root,
        vendor_codes: item.vendor_codes,
        source_hash: item.source_hash,
        meter_count: item.meter_count,
        vendor_count: item.vendor_count,
        family_count: item.family_count,
        model_count: item.model_count,
        capability_count: item.capability_count,
        price_count: item.price_count,
        ranking_count: item.ranking_count,
        voice_count: item.voice_count,
        voice_binding_count: item.voice_binding_count,
        video_profile_count: item.video_profile_count,
        accepted_count: item.accepted_count,
        snapshot_id: item.snapshot_id,
        sync_run_id: item.sync_run_id,
        vendors: item.vendors.into_iter().map(to_vendor_response).collect(),
        models: item.models.into_iter().map(to_model_response).collect(),
    }
}

fn bad_request(ctx: &WebRequestContext, message: impl Into<String>) -> Response {
    problem_for(ctx, SdkWorkResultCode::ValidationError, message.into())
}

fn not_found_response(ctx: &WebRequestContext, message: String) -> Response {
    problem_for(ctx, SdkWorkResultCode::NotFound, message)
}

fn conflict_response(ctx: &WebRequestContext, error: DomainError) -> Response {
    problem_for(ctx, SdkWorkResultCode::Conflict, error.to_string())
}

fn command_build_error_response(
    ctx: &WebRequestContext,
    error: AdminModelCommandBuildError,
) -> Response {
    match error {
        AdminModelCommandBuildError::BadRequest(message) => bad_request(&ctx, message),
        AdminModelCommandBuildError::System(error) => {
            admin_model_system_response(&ctx, "admin model command is invalid", error)
        }
    }
}

fn admin_model_system_response(
    ctx: &WebRequestContext,
    context: &str,
    error: DomainError,
) -> Response {
    problem_for(
        ctx,
        SdkWorkResultCode::InternalError,
        format!("{context}: {error}"),
    )
}

fn current_timestamp_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    format_unix_timestamp(seconds)
}

fn format_unix_timestamp(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_subject() -> AdminModelSubject {
        AdminModelSubject {
            tenant_id: 100001,
            organization_id: 0,
            operator_id: 42,
            operator_type: 1,
        }
    }

    #[test]
    fn model_mappings_query_maps_page_and_page_size_to_limit_offset() {
        let query = build_list_model_mappings_query(
            test_subject(),
            AdminModelMappingsQuery {
                binding_type: Some("upstream_account".to_owned()),
                vendor_code: Some("openai".to_owned()),
                account_id: Some(Value::String("123".to_owned())),
                account_code: Some("primary".to_owned()),
                q: Some("gpt".to_owned()),
                page: Some(3),
                page_size: Some(25),
            },
        )
        .expect("model mappings query should be valid");

        assert_eq!(query.binding_type.as_deref(), Some("upstream_account"));
        assert_eq!(query.vendor_code.as_deref(), Some("openai"));
        assert_eq!(query.account_id, Some(123));
        assert_eq!(query.account_code.as_deref(), Some("primary"));
        assert_eq!(query.q.as_deref(), Some("gpt"));
        assert_eq!(query.normalized_limit(), 25);
        assert_eq!(query.normalized_offset(), 50);
    }

    #[test]
    fn model_mappings_query_rejects_page_size_over_standard_limit() {
        let error = build_list_model_mappings_query(
            test_subject(),
            AdminModelMappingsQuery {
                binding_type: None,
                vendor_code: None,
                account_id: None,
                account_code: None,
                q: None,
                page: Some(1),
                page_size: Some(MAX_LIST_PAGE_SIZE + 1),
            },
        )
        .expect_err("page_size over SDKWork max must fail");

        match error {
            AdminModelCommandBuildError::BadRequest(message) => {
                assert!(message.contains("page_size must be between 1 and 200"));
            }
            AdminModelCommandBuildError::System(_) => {
                panic!("page_size validation must be a bad request");
            }
        }
    }

    #[test]
    fn models_query_normalizes_comma_separated_vendor_codes() {
        let query = build_list_models_query(
            test_subject(),
            AdminModelsListQuery {
                q: Some("gpt".to_owned()),
                vendor_codes: Some(" OpenAI,anthropic,openai ".to_owned()),
                page: Some(2),
                page_size: Some(10),
            },
        )
        .expect("admin model list query should be valid");

        assert_eq!(
            query.vendor_codes,
            vec!["openai".to_owned(), "anthropic".to_owned()]
        );
        assert_eq!(query.q.as_deref(), Some("gpt"));
        assert_eq!(query.normalized_limit(), 10);
        assert_eq!(query.normalized_offset(), 10);
    }
}
