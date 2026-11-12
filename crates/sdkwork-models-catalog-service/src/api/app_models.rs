use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::routing::{get, put};
use axum::Router;
use sdkwork_cloudrouter_http::TrustedRequestSubject;
use sdkwork_utils_rust::SdkWorkResultCode;
use sdkwork_web_core::WebRequestContext;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::api::page_info::{offset_page_info, ApiPageInfo};
use crate::api::request_id::{generate_server_request_id, RequestIdError};
use crate::api::response::{finish_success, problem_for};
use crate::application::{
    EntityUuidGenerator, ListModelCatalogQuery, ModelCatalogGroup, ModelCatalogItem,
    ModelCatalogPage, ModelCatalogQueryService, PriceAvailability,
};
use crate::domain::BillingMeter;
use crate::ports::{
    AdminAiModelItem, AdminAiModelListPage, AdminAiResourceHierarchyNodeCommand,
    AdminAiResourceItem, AdminAiResourceMemberCommand, AdminAiResourceStore,
    AdminAiResourceSubject, AdminModelSubject, AdminModelVendorItem, ListAdminAiModelsQuery,
    ListAdminAiResourcesQuery, ListAdminModelVendorsQuery, ModelCatalogAdminStore, PricingCatalog,
    ReplaceAdminAiResourceHierarchyCommand,
};

const DEFAULT_PAGE_SIZE: usize = 20;
const MAX_PAGE_SIZE: usize = 200;
const STORE_PAGE_SIZE: i64 = 200;
const MAX_SEARCH_LEN: usize = 256;
const MAX_ACCESS_CHANNEL_CODE_LEN: usize = 96;
const MAX_ACCESS_CHANNEL_NAME_LEN: usize = 128;
const MAX_ACCESS_CHANNEL_DESCRIPTION_LEN: usize = 512;
const MAX_ACCESS_CHANNEL_OFFERINGS: usize = 32;
const MAX_ACCESS_CHANNEL_MODELS: usize = 512;
const MODEL_ACCESS_CHANNEL_RESOURCE_TYPE: &str = "model_access_channel";
const OFFICIAL_PROVIDER_OVERLAY_JSON: &str =
    include_str!("../../../../overlays/cloudrouter/providers.json");
const CANONICAL_AGENT_PROVIDER_IDS: &[&str] = &[
    "codex",
    "claude-code",
    "gemini",
    "opencode",
    "openclaw",
    "hermes",
];

struct AppModelCatalogState<C> {
    fallback_catalog: Arc<C>,
    model_store: Option<Arc<dyn ModelCatalogAdminStore + Send + Sync>>,
    resource_store: Option<Arc<dyn AdminAiResourceStore + Send + Sync>>,
    entity_uuid_generator: Option<Arc<dyn EntityUuidGenerator + Send + Sync>>,
    /// Per-model catalog capability counts (video profiles, voice bindings).
    profile_counts: Arc<ProfileCounts>,
}

/// Per-model capability counts the app catalog item publishes.
///
/// Video profiles and voice bindings live in dedicated catalog tables/routes;
/// the model list only needs their cardinality so a client can decide whether
/// to load the detail surface without an extra request per model.
#[derive(Debug, Default)]
struct ProfileCounts {
    video_profiles: HashMap<String, usize>,
    voice_bindings: HashMap<String, usize>,
}

impl ProfileCounts {
    fn from_catalog(catalog: Option<&sdkwork_models::ModelCatalog>) -> Self {
        let Some(catalog) = catalog else {
            return Self::default();
        };
        let mut counts = Self::default();
        for vendor in &catalog.vendors {
            for file in &vendor.model_video_profiles {
                *counts
                    .video_profiles
                    .entry(file.catalog_key.trim().to_ascii_lowercase())
                    .or_default() += file.profiles.len();
            }
            for file in &vendor.model_voice_bindings {
                *counts
                    .voice_bindings
                    .entry(file.catalog_key.trim().to_ascii_lowercase())
                    .or_default() += file.bindings.len();
            }
        }
        counts
    }

    fn for_catalog_key(&self, catalog_key: &str) -> (usize, usize) {
        let key = catalog_key.trim().to_ascii_lowercase();
        (
            self.video_profiles.get(&key).copied().unwrap_or(0),
            self.voice_bindings.get(&key).copied().unwrap_or(0),
        )
    }
}

impl<C> Clone for AppModelCatalogState<C> {
    fn clone(&self) -> Self {
        Self {
            fallback_catalog: Arc::clone(&self.fallback_catalog),
            model_store: self.model_store.as_ref().map(Arc::clone),
            resource_store: self.resource_store.as_ref().map(Arc::clone),
            entity_uuid_generator: self.entity_uuid_generator.as_ref().map(Arc::clone),
            profile_counts: Arc::clone(&self.profile_counts),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppModelCatalogQuery {
    billing_meter: Option<String>,
    vendor_code: Option<String>,
    vendor_codes: Option<String>,
    /// Matches any declared modality, including *input* modalities: the stored
    /// `modalities` column unions input and output, so `modalities=video` also
    /// returns chat models that read video input. Consumers selecting models by
    /// what the model *produces* must also filter on `capabilities` (the
    /// primary capability) or on `outputModalities` in the response.
    modalities: Option<String>,
    capabilities: Option<String>,
    categories: Option<String>,
    groups: Option<String>,
    q: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AppModelAccessChannelQuery {
    page: Option<usize>,
    page_size: Option<usize>,
    q: Option<String>,
    kind: Option<String>,
    vendor_code: Option<String>,
    agent_provider_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppModelAccessChannelUpsertRequest {
    name: String,
    kind: String,
    base_url: String,
    description: Option<String>,
    offerings: Vec<AppModelAccessChannelOfferingUpsertRequest>,
    default_vendor_code: String,
    default_model_id: String,
    supported_agent_provider_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppModelAccessChannelOfferingUpsertRequest {
    vendor_code: String,
    vendor_name: String,
    #[serde(default)]
    model_ids: Vec<String>,
    #[serde(default)]
    models: Vec<AppModelAccessChannelModelUpsertRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppModelAccessChannelModelUpsertRequest {
    model_id: String,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialProviderOverlay {
    providers: Vec<OfficialProviderDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialProviderDefinition {
    provider_code: String,
    display_name: String,
    vendor_code: String,
    protocol: String,
    base_url: String,
}

static OFFICIAL_PROVIDER_DEFINITIONS: OnceLock<Result<Vec<OfficialProviderDefinition>, String>> =
    OnceLock::new();

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelCatalogResponse {
    items: Vec<AppModelCatalogItemResponse>,
    groups: Vec<AppModelCatalogGroupResponse>,
    page_info: ApiPageInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelVendorCatalogResponse {
    items: Vec<AppModelVendorOptionResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelVendorOptionResponse {
    label: String,
    code: String,
    model_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelCatalogGroupResponse {
    key: String,
    label: String,
    model_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelCatalogItemResponse {
    catalog_key: String,
    model: String,
    display_name: String,
    vendor_code: String,
    vendor: String,
    capabilities: Vec<String>,
    groups: Vec<String>,
    categories: Vec<String>,
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
    #[serde(rename = "providerCodes")]
    supplier_codes: Vec<String>,
    supported_agent_provider_ids: Vec<String>,
    official_reference_prices: Vec<AppModelCatalogReferencePriceResponse>,
    price_availability: AppModelCatalogPriceAvailabilityResponse,
    /// Catalog model family code (`seedance`, `kling`, ...).
    family_code: Option<String>,
    /// Brand color token the catalog publishes for the model.
    color_token: Option<String>,
    /// Vendor documentation URL the catalog records.
    docs_url: Option<String>,
    /// Number of catalog video generation profiles the model declares.
    video_profile_count: usize,
    /// Number of voice bindings the model declares.
    voice_binding_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelAccessChannelsResponse {
    items: Vec<AppModelAccessChannelItemResponse>,
    page_info: ApiPageInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelAccessChannelPresetsResponse {
    items: Vec<AppModelAccessChannelPresetResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelAccessChannelPresetResponse {
    provider_code: String,
    provider_display_name: String,
    protocol: String,
    vendor_code: String,
    vendor_name: String,
    channel_name: String,
    base_url: String,
    models: Vec<AppModelAccessChannelModelResponse>,
    default_model_id: Option<String>,
    sort_order: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelAccessChannelItemResponse {
    id: String,
    code: String,
    name: String,
    kind: String,
    base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    default_vendor_code: String,
    default_model_id: String,
    supported_agent_provider_ids: Vec<String>,
    offerings: Vec<AppModelAccessChannelVendorOfferingResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort_order: Option<String>,
    vendor_count: usize,
    model_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelAccessChannelItemEnvelope {
    item: AppModelAccessChannelItemResponse,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelAccessChannelVendorOfferingResponse {
    vendor_code: String,
    vendor_name: String,
    models: Vec<AppModelAccessChannelModelResponse>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AppModelAccessChannelModelResponse {
    catalog_key: String,
    model: String,
    display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_rounds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supports_multimodal: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelCatalogReferencePriceResponse {
    region_code: String,
    billing_meter: String,
    unit_price: String,
    currency: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppModelCatalogPriceAvailabilityResponse {
    status: &'static str,
    reason: Option<String>,
}

const PUBLIC_REFERENCE_PRICE_REASON: &str =
    "Public reference price only. Customer-specific pricing requires an API key context.";
const PUBLIC_PRICE_UNAVAILABLE_REASON: &str =
    "Public reference price is not configured for this model.";

pub fn app_model_catalog_router<C>(catalog: Arc<C>) -> Router
where
    C: PricingCatalog + Send + Sync + 'static,
{
    app_model_catalog_router_with_optional_stores(catalog, None, None, None, None)
}

pub fn app_model_catalog_router_with_stores<C>(
    fallback_catalog: Arc<C>,
    model_store: Arc<dyn ModelCatalogAdminStore + Send + Sync>,
    resource_store: Arc<dyn AdminAiResourceStore + Send + Sync>,
    entity_uuid_generator: Arc<dyn EntityUuidGenerator + Send + Sync>,
) -> Router
where
    C: PricingCatalog + Send + Sync + 'static,
{
    app_model_catalog_router_with_optional_stores(
        fallback_catalog,
        Some(model_store),
        Some(resource_store),
        Some(entity_uuid_generator),
        None,
    )
}

/// Router with the catalog profile source, so list items can publish each
/// model's video-profile and voice-binding counts (capability attributes).
pub fn app_model_catalog_router_with_stores_and_profiles<C>(
    fallback_catalog: Arc<C>,
    model_store: Arc<dyn ModelCatalogAdminStore + Send + Sync>,
    resource_store: Arc<dyn AdminAiResourceStore + Send + Sync>,
    entity_uuid_generator: Arc<dyn EntityUuidGenerator + Send + Sync>,
    profile_catalog: Option<Arc<sdkwork_models::ModelCatalog>>,
) -> Router
where
    C: PricingCatalog + Send + Sync + 'static,
{
    app_model_catalog_router_with_optional_stores(
        fallback_catalog,
        Some(model_store),
        Some(resource_store),
        Some(entity_uuid_generator),
        profile_catalog,
    )
}

fn app_model_catalog_router_with_optional_stores<C>(
    fallback_catalog: Arc<C>,
    model_store: Option<Arc<dyn ModelCatalogAdminStore + Send + Sync>>,
    resource_store: Option<Arc<dyn AdminAiResourceStore + Send + Sync>>,
    entity_uuid_generator: Option<Arc<dyn EntityUuidGenerator + Send + Sync>>,
    profile_catalog: Option<Arc<sdkwork_models::ModelCatalog>>,
) -> Router
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let profile_counts = Arc::new(ProfileCounts::from_catalog(profile_catalog.as_deref()));
    Router::new()
        .route(
            "/app/v3/api/ai/model_vendors",
            get(fetch_model_vendors::<C>),
        )
        .route("/app/v3/api/ai/models", get(fetch_models::<C>))
        .route(
            "/app/v3/api/ai/model_access_channels",
            get(fetch_model_access_channels::<C>),
        )
        .route(
            "/app/v3/api/ai/model_access_channel_presets",
            get(fetch_model_access_channel_presets::<C>),
        )
        .route(
            "/app/v3/api/ai/model_access_channels/{channel_code}",
            put(upsert_model_access_channel::<C>),
        )
        .with_state(AppModelCatalogState {
            fallback_catalog,
            model_store,
            resource_store,
            entity_uuid_generator,
            profile_counts,
        })
}

async fn fetch_model_vendors<C>(
    ctx: WebRequestContext,
    State(state): State<AppModelCatalogState<C>>,
    subject: Option<TrustedRequestSubject>,
) -> Response
where
    C: PricingCatalog + Send + Sync + 'static,
{
    if let Some(snapshot) = load_database_catalog_snapshot(&state, subject.as_ref()).await {
        return finish_success(&ctx, to_database_vendor_response(&snapshot));
    }
    finish_success(&ctx, to_vendor_response(state.fallback_catalog.as_ref()))
}

async fn fetch_models<C>(
    ctx: WebRequestContext,
    State(state): State<AppModelCatalogState<C>>,
    subject: Option<TrustedRequestSubject>,
    Query(query): Query<AppModelCatalogQuery>,
) -> Response
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let billing_meter = query
        .billing_meter
        .as_deref()
        .map(BillingMeter::from_code)
        .unwrap_or(BillingMeter::LlmInputToken);
    let (page, page_size) = match validate_page_query(query.page, query.page_size) {
        Ok(value) => value,
        Err(message) => return problem_for(&ctx, SdkWorkResultCode::ValidationError, message),
    };
    if let Some(model_store) = state.model_store.as_ref() {
        let database_subject = app_model_subject(subject.as_ref());
        match database_model_catalog_is_authoritative(model_store.as_ref(), database_subject).await
        {
            Ok(true) => match load_database_model_page(
                model_store.as_ref(),
                database_subject,
                &query,
                page,
                page_size,
            )
            .await
            {
                Ok(database_page) => {
                    return finish_success(
                        &ctx,
                        to_database_model_response(
                            database_page,
                            &billing_meter,
                            page,
                            page_size,
                            &state.profile_counts,
                        ),
                    )
                }
                Err(error) => tracing::warn!(
                    error = %error,
                    "database model catalog query failed; using fallback catalog"
                ),
            },
            Ok(false) => {}
            Err(error) => tracing::warn!(
                error = %error,
                "database model catalog authority check failed; using fallback catalog"
            ),
        }
    }

    let offset = (page - 1) * page_size;
    let service = ModelCatalogQueryService::new(state.fallback_catalog.as_ref());

    match service.list_models(ListModelCatalogQuery {
        api_key_id: None,
        billing_meter,
        vendor_code: query.vendor_code,
        vendor_codes: comma_separated_query_values(query.vendor_codes.as_deref()),
        modalities: comma_separated_query_values(query.modalities.as_deref()),
        capabilities: comma_separated_query_values(query.capabilities.as_deref()),
        categories: comma_separated_query_values(query.categories.as_deref()),
        groups: comma_separated_query_values(query.groups.as_deref()),
        search_query: query.q,
        page_size: Some(page_size),
        offset: Some(offset),
    }) {
        Ok(page) => finish_success(&ctx, to_response(page, &state.profile_counts)),
        Err(error) => problem_for(&ctx, SdkWorkResultCode::ValidationError, error.to_string()),
    }
}

async fn fetch_model_access_channel_presets<C>(
    ctx: WebRequestContext,
    State(state): State<AppModelCatalogState<C>>,
    subject: Option<TrustedRequestSubject>,
) -> Response
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let providers = match official_provider_definitions() {
        Ok(providers) => providers,
        Err(message) => {
            return problem_for(&ctx, SdkWorkResultCode::ServiceUnavailable, message);
        }
    };
    let database_catalog = load_database_catalog_snapshot(&state, subject.as_ref()).await;
    finish_success(
        &ctx,
        AppModelAccessChannelPresetsResponse {
            items: build_official_access_channel_presets(
                state.fallback_catalog.as_ref(),
                database_catalog.as_ref(),
                providers,
            ),
        },
    )
}

async fn fetch_model_access_channels<C>(
    ctx: WebRequestContext,
    State(state): State<AppModelCatalogState<C>>,
    subject: Option<TrustedRequestSubject>,
    Query(query): Query<AppModelAccessChannelQuery>,
) -> Response
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let (page, page_size) = match validate_page_query(query.page, query.page_size) {
        Ok(value) => value,
        Err(message) => return problem_for(&ctx, SdkWorkResultCode::ValidationError, message),
    };
    let query = match validate_access_channel_query(query) {
        Ok(query) => query,
        Err(message) => return problem_for(&ctx, SdkWorkResultCode::ValidationError, message),
    };
    let Some(resource_store) = state.resource_store.as_ref() else {
        return finish_success(
            &ctx,
            AppModelAccessChannelsResponse {
                items: Vec::new(),
                page_info: offset_page_info(page as i64, page_size as i64, 0),
            },
        );
    };
    let resource_subject = app_resource_subject(subject.as_ref());
    let channel_page = match resource_store
        .list_ai_resources(build_access_channel_list_query(
            resource_subject,
            &query,
            page,
            page_size,
        ))
        .await
    {
        Ok(page) => page,
        Err(error) => {
            return problem_for(
                &ctx,
                SdkWorkResultCode::ServiceUnavailable,
                format!("model access channel read model is unavailable: {error}"),
            )
        }
    };
    let total_items = channel_page.total_count;
    let database_catalog = load_database_catalog_snapshot(&state, subject.as_ref()).await;
    let items = match build_model_access_channels(
        resource_store.as_ref(),
        resource_subject,
        channel_page.items,
        database_catalog.as_ref(),
        state.fallback_catalog.as_ref(),
    )
    .await
    {
        Ok(items) => items,
        Err(error) => {
            return problem_for(
                &ctx,
                SdkWorkResultCode::ServiceUnavailable,
                format!("model access channel offerings are unavailable: {error}"),
            )
        }
    };
    finish_success(
        &ctx,
        AppModelAccessChannelsResponse {
            items,
            page_info: offset_page_info(page as i64, page_size as i64, total_items),
        },
    )
}

#[derive(Debug)]
struct NormalizedAccessChannelOffering {
    vendor_code: String,
    vendor_name: String,
    models: Vec<NormalizedAccessChannelModel>,
}

#[derive(Debug, Clone)]
struct NormalizedAccessChannelModel {
    model_id: String,
    display_name: String,
    context_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    tool_call_rounds: Option<i64>,
    supports_multimodal: Option<bool>,
}

#[derive(Debug)]
struct NormalizedAccessChannelUpsert {
    channel_code: String,
    name: String,
    kind: String,
    base_url: String,
    description: Option<String>,
    offerings: Vec<NormalizedAccessChannelOffering>,
    default_vendor_code: String,
    default_model_id: String,
    supported_agent_provider_ids: Vec<String>,
}

#[derive(Debug)]
struct AppAiResourceDraft {
    resource_code: String,
    resource_type: String,
    display_name: String,
    vendor_code: Option<String>,
    catalog_key: Option<String>,
    model: Option<String>,
    provider_native_model: Option<String>,
    access_channel_kind: Option<String>,
    base_url: Option<String>,
    default_vendor_code: Option<String>,
    default_model_id: Option<String>,
    supported_agent_provider_ids: Vec<String>,
    context_tokens: Option<i64>,
    max_output_tokens: Option<i64>,
    tool_call_rounds: Option<i64>,
    supports_multimodal: Option<bool>,
    description: Option<String>,
    composition_mode: String,
    members: Vec<AdminAiResourceMemberCommand>,
}

async fn upsert_model_access_channel<C>(
    ctx: WebRequestContext,
    State(state): State<AppModelCatalogState<C>>,
    Path(channel_code): Path<String>,
    trusted: TrustedRequestSubject,
    body: Bytes,
) -> Response
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let Some(store) = state.resource_store.as_ref() else {
        return problem_for(
            &ctx,
            SdkWorkResultCode::ServiceUnavailable,
            "model access channel command store is unavailable",
        );
    };
    let Some(entity_uuid_generator) = state.entity_uuid_generator.as_ref() else {
        return problem_for(
            &ctx,
            SdkWorkResultCode::ServiceUnavailable,
            "model access channel entity id generator is unavailable",
        );
    };
    let request = match parse_access_channel_upsert_request(&body) {
        Ok(request) => request,
        Err(message) => return problem_for(&ctx, SdkWorkResultCode::ValidationError, message),
    };
    let mut command = match normalize_access_channel_upsert(channel_code, request) {
        Ok(command) => command,
        Err(message) => return problem_for(&ctx, SdkWorkResultCode::ValidationError, message),
    };
    let database_catalog = load_database_catalog_snapshot(&state, Some(&trusted)).await;
    if let Err(message) = validate_and_enrich_official_access_channel(
        state.fallback_catalog.as_ref(),
        database_catalog.as_ref(),
        &mut command,
    ) {
        return problem_for(&ctx, SdkWorkResultCode::ValidationError, message);
    }
    let subject = app_resource_subject(Some(&trusted));
    let channel = match persist_access_channel(
        store.as_ref(),
        entity_uuid_generator.as_ref(),
        subject,
        &command,
    )
    .await
    {
        Ok(channel) => channel,
        Err(error) if error.is_conflict() => {
            return problem_for(&ctx, SdkWorkResultCode::Conflict, error.to_string())
        }
        Err(error) => {
            return problem_for(
                &ctx,
                SdkWorkResultCode::ServiceUnavailable,
                format!("model access channel command store is unavailable: {error}"),
            )
        }
    };
    let mut items = match build_model_access_channels(
        store.as_ref(),
        subject,
        vec![channel],
        database_catalog.as_ref(),
        state.fallback_catalog.as_ref(),
    )
    .await
    {
        Ok(items) => items,
        Err(error) => {
            return problem_for(
                &ctx,
                SdkWorkResultCode::ServiceUnavailable,
                format!("model access channel projection is unavailable: {error}"),
            )
        }
    };
    let Some(item) = items.pop() else {
        return problem_for(
            &ctx,
            SdkWorkResultCode::ValidationError,
            "the saved model access channel is not publicly selectable",
        );
    };
    finish_success(&ctx, AppModelAccessChannelItemEnvelope { item })
}

fn parse_access_channel_upsert_request(
    body: &[u8],
) -> Result<AppModelAccessChannelUpsertRequest, String> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Err("model access channel request body is required".to_owned());
    }
    serde_json::from_slice(body)
        .map_err(|error| format!("invalid model access channel request body: {error}"))
}

fn normalize_access_channel_upsert(
    channel_code: String,
    request: AppModelAccessChannelUpsertRequest,
) -> Result<NormalizedAccessChannelUpsert, String> {
    let channel_code = normalized_public_resource_code(
        &channel_code,
        "channel_code",
        MAX_ACCESS_CHANNEL_CODE_LEN,
    )?;
    let name = bounded_required_text(&request.name, "name", MAX_ACCESS_CHANNEL_NAME_LEN)?;
    let kind = normalized_access_channel_kind(Some(&request.kind))
        .ok_or_else(|| "kind must be one of official, relay, custom".to_owned())?;
    let base_url = valid_public_base_url(Some(&request.base_url)).ok_or_else(|| {
        "baseUrl must be an http or https URL without embedded credentials".to_owned()
    })?;
    let description = bounded_optional_text(
        request.description.as_deref(),
        "description",
        MAX_ACCESS_CHANNEL_DESCRIPTION_LEN,
    )?;
    if request.offerings.is_empty() || request.offerings.len() > MAX_ACCESS_CHANNEL_OFFERINGS {
        return Err(format!(
            "offerings must contain between 1 and {MAX_ACCESS_CHANNEL_OFFERINGS} items"
        ));
    }
    if kind == "official" && request.offerings.len() != 1 {
        return Err("official model access channels must contain exactly one vendor".to_owned());
    }
    let mut seen_vendors = HashSet::new();
    let mut total_models = 0_usize;
    let mut offerings = Vec::with_capacity(request.offerings.len());
    for offering in request.offerings {
        let vendor_code =
            normalized_public_resource_code(&offering.vendor_code, "offerings.vendorCode", 64)?;
        if !seen_vendors.insert(vendor_code.clone()) {
            return Err(format!("duplicate vendor offering: {vendor_code}"));
        }
        let vendor_name = bounded_required_text(
            &offering.vendor_name,
            "offerings.vendorName",
            MAX_ACCESS_CHANNEL_NAME_LEN,
        )?;
        let models = normalize_access_channel_models(offering.models, offering.model_ids)?;
        if models.is_empty() {
            return Err(format!(
                "vendor {vendor_code} must contain at least one model"
            ));
        }
        total_models = total_models.saturating_add(models.len());
        offerings.push(NormalizedAccessChannelOffering {
            vendor_code,
            vendor_name,
            models,
        });
    }
    if total_models > MAX_ACCESS_CHANNEL_MODELS {
        return Err(format!(
            "offerings may contain at most {MAX_ACCESS_CHANNEL_MODELS} models"
        ));
    }
    let default_vendor_code =
        normalized_public_resource_code(&request.default_vendor_code, "defaultVendorCode", 64)?;
    let default_model_id = bounded_required_text(&request.default_model_id, "defaultModelId", 128)?;
    let default_offering = offerings
        .iter()
        .find(|offering| offering.vendor_code == default_vendor_code)
        .ok_or_else(|| "defaultVendorCode must identify an offering".to_owned())?;
    if !default_offering
        .models
        .iter()
        .any(|model| model.model_id.eq_ignore_ascii_case(&default_model_id))
    {
        return Err("defaultModelId must belong to the default vendor offering".to_owned());
    }
    let supported_agent_provider_ids =
        normalize_access_channel_provider_ids(request.supported_agent_provider_ids)?;
    Ok(NormalizedAccessChannelUpsert {
        channel_code,
        name,
        kind,
        base_url,
        description,
        offerings,
        default_vendor_code,
        default_model_id,
        supported_agent_provider_ids,
    })
}

fn normalize_access_channel_models(
    models: Vec<AppModelAccessChannelModelUpsertRequest>,
    legacy_model_ids: Vec<String>,
) -> Result<Vec<NormalizedAccessChannelModel>, String> {
    let mut normalized = Vec::with_capacity(models.len() + legacy_model_ids.len());
    let mut seen = HashSet::new();
    for model in models {
        let model_id = bounded_required_text(&model.model_id, "offerings.models.modelId", 128)?;
        if !seen.insert(model_id.to_ascii_lowercase()) {
            return Err(format!("duplicate model offering: {model_id}"));
        }
        let display_name = bounded_optional_text(
            model.display_name.as_deref(),
            "offerings.models.displayName",
            MAX_ACCESS_CHANNEL_NAME_LEN,
        )?
        .unwrap_or_else(|| model_id.clone());
        normalized.push(NormalizedAccessChannelModel {
            model_id,
            display_name,
            context_tokens: None,
            max_output_tokens: None,
            tool_call_rounds: None,
            supports_multimodal: None,
        });
    }
    for model_id in legacy_model_ids {
        let model_id = bounded_required_text(&model_id, "offerings.modelIds", 128)?;
        if seen.insert(model_id.to_ascii_lowercase()) {
            normalized.push(NormalizedAccessChannelModel {
                display_name: model_id.clone(),
                model_id,
                context_tokens: None,
                max_output_tokens: None,
                tool_call_rounds: None,
                supports_multimodal: None,
            });
        }
    }
    Ok(normalized)
}

fn normalized_public_resource_code(
    value: &str,
    field_name: &str,
    max_len: usize,
) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty()
        || value.len() > max_len
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(format!(
            "{field_name} may only contain letters, numbers, ., -, and _ and must not exceed {max_len} characters"
        ));
    }
    Ok(value)
}

fn bounded_required_text(value: &str, field_name: &str, max_len: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_len || value.chars().any(char::is_control) {
        return Err(format!(
            "{field_name} is required and must not exceed {max_len} visible characters"
        ));
    }
    Ok(value.to_owned())
}

fn bounded_optional_text(
    value: Option<&str>,
    field_name: &str,
    max_len: usize,
) -> Result<Option<String>, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| bounded_required_text(value, field_name, max_len))
        .transpose()
}

fn normalize_access_channel_provider_ids(values: Vec<String>) -> Result<Vec<String>, String> {
    if values.is_empty() {
        return Ok(canonical_agent_provider_ids());
    }
    let mut normalized = BTreeSet::new();
    for value in values {
        let value = value.trim().to_ascii_lowercase();
        if !CANONICAL_AGENT_PROVIDER_IDS.contains(&value.as_str()) {
            return Err(format!("unsupported Agent provider id: {value}"));
        }
        normalized.insert(value);
    }
    Ok(normalized.into_iter().collect())
}

fn official_provider_definitions() -> Result<&'static [OfficialProviderDefinition], String> {
    OFFICIAL_PROVIDER_DEFINITIONS
        .get_or_init(|| {
            let overlay =
                serde_json::from_str::<OfficialProviderOverlay>(OFFICIAL_PROVIDER_OVERLAY_JSON)
                    .map_err(|error| format!("official provider overlay is invalid: {error}"))?;
            let mut providers = Vec::with_capacity(overlay.providers.len());
            let mut seen_provider_codes = HashSet::new();
            let mut seen_vendor_codes = HashSet::new();
            for provider in overlay.providers {
                let provider_code = normalized_public_resource_code(
                    &provider.provider_code,
                    "official provider code",
                    128,
                )?;
                let vendor_code = normalized_public_resource_code(
                    &provider.vendor_code,
                    "official provider vendor code",
                    64,
                )?;
                let display_name = bounded_required_text(
                    &provider.display_name,
                    "official provider display name",
                    MAX_ACCESS_CHANNEL_NAME_LEN,
                )?;
                let protocol =
                    bounded_required_text(&provider.protocol, "official provider protocol", 64)?;
                let base_url =
                    valid_public_base_url(Some(&provider.base_url)).ok_or_else(|| {
                        format!(
                        "official provider {provider_code} baseUrl must be a public HTTP(S) URL"
                    )
                    })?;
                if !seen_provider_codes.insert(provider_code.clone()) {
                    return Err(format!("duplicate official provider code: {provider_code}"));
                }
                if !seen_vendor_codes.insert(vendor_code.clone()) {
                    return Err(format!(
                        "duplicate official provider vendor code: {vendor_code}"
                    ));
                }
                providers.push(OfficialProviderDefinition {
                    provider_code,
                    display_name,
                    vendor_code,
                    protocol,
                    base_url,
                });
            }
            Ok(providers)
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(Clone::clone)
}

fn build_official_access_channel_presets<C>(
    fallback_catalog: &C,
    database_catalog: Option<&DatabaseCatalogSnapshot>,
    providers: &[OfficialProviderDefinition],
) -> Vec<AppModelAccessChannelPresetResponse>
where
    C: PricingCatalog + Send + Sync + 'static,
{
    providers
        .iter()
        .enumerate()
        .map(|(sort_order, provider)| {
            let models = authoritative_models_for_vendor(
                fallback_catalog,
                database_catalog,
                &provider.vendor_code,
            );
            let vendor_name = authoritative_vendor_name(
                fallback_catalog,
                database_catalog,
                &provider.vendor_code,
            )
            .unwrap_or_else(|| provider.vendor_code.clone());
            AppModelAccessChannelPresetResponse {
                provider_code: provider.provider_code.clone(),
                provider_display_name: provider.display_name.clone(),
                protocol: provider.protocol.clone(),
                vendor_code: provider.vendor_code.clone(),
                vendor_name,
                channel_name: provider.display_name.clone(),
                base_url: provider.base_url.clone(),
                default_model_id: models.first().map(|model| model.model.clone()),
                models,
                sort_order,
            }
        })
        .collect()
}

fn validate_and_enrich_official_access_channel<C>(
    fallback_catalog: &C,
    database_catalog: Option<&DatabaseCatalogSnapshot>,
    command: &mut NormalizedAccessChannelUpsert,
) -> Result<(), String>
where
    C: PricingCatalog + Send + Sync + 'static,
{
    if command.kind != "official" {
        return Ok(());
    }
    let providers = official_provider_definitions()?;
    let offering = command.offerings.first_mut().ok_or_else(|| {
        "official model access channels must contain one vendor offering".to_owned()
    })?;
    let provider = providers
        .iter()
        .find(|provider| provider.vendor_code == offering.vendor_code)
        .ok_or_else(|| {
            format!(
                "official vendor is not configured by sdkwork-models: {}",
                offering.vendor_code
            )
        })?;
    if command.base_url != provider.base_url {
        return Err(format!(
            "baseUrl must exactly match the official {} endpoint: {}",
            provider.vendor_code, provider.base_url
        ));
    }
    let models =
        authoritative_models_for_vendor(fallback_catalog, database_catalog, &provider.vendor_code);
    if models.is_empty() {
        return Err(format!(
            "official vendor {} has no publicly selectable models",
            provider.vendor_code
        ));
    }
    offering.vendor_name =
        authoritative_vendor_name(fallback_catalog, database_catalog, &provider.vendor_code)
            .unwrap_or_else(|| provider.display_name.clone());
    for requested in &mut offering.models {
        let model = models
            .iter()
            .find(|model| model.model == requested.model_id)
            .ok_or_else(|| {
                format!(
                    "official model {} is not in the public {} model catalog",
                    requested.model_id, provider.vendor_code
                )
            })?;
        requested.display_name = model.display_name.clone();
        requested.context_tokens = model.context_tokens;
        requested.max_output_tokens = model.max_output_tokens;
        requested.tool_call_rounds = model.tool_call_rounds;
        requested.supports_multimodal = model.supports_multimodal;
    }
    Ok(())
}

fn authoritative_vendor_name<C>(
    fallback_catalog: &C,
    database_catalog: Option<&DatabaseCatalogSnapshot>,
    vendor_code: &str,
) -> Option<String>
where
    C: PricingCatalog + Send + Sync + 'static,
{
    database_catalog
        .and_then(|catalog| catalog.vendor_names.get(vendor_code).cloned())
        .or_else(|| {
            fallback_catalog
                .find_vendor(vendor_code)
                .map(|vendor| vendor.display_name)
        })
}

fn authoritative_models_for_vendor<C>(
    fallback_catalog: &C,
    database_catalog: Option<&DatabaseCatalogSnapshot>,
    vendor_code: &str,
) -> Vec<AppModelAccessChannelModelResponse>
where
    C: PricingCatalog + Send + Sync + 'static,
{
    if let Some(catalog) = database_catalog {
        return catalog
            .models
            .iter()
            .filter(|model| model.vendor_code.eq_ignore_ascii_case(vendor_code))
            .map(database_model_access_channel_response)
            .collect();
    }
    fallback_access_channel_models(fallback_catalog, vendor_code)
}

fn fallback_access_channel_models<C>(
    catalog: &C,
    vendor_code: &str,
) -> Vec<AppModelAccessChannelModelResponse>
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let mut models = Vec::new();
    catalog.visit_models(None, &mut |model| {
        if model.is_publicly_active() && model.vendor_code.eq_ignore_ascii_case(vendor_code) {
            models.push(fallback_model_access_channel_response(model));
        }
        true
    });
    models
}

fn database_model_access_channel_response(
    model: &AdminAiModelItem,
) -> AppModelAccessChannelModelResponse {
    AppModelAccessChannelModelResponse {
        catalog_key: model.catalog_key.clone(),
        model: model.model.clone(),
        display_name: model.display_name.clone(),
        context_tokens: model.context_tokens,
        max_output_tokens: model.max_output_tokens,
        tool_call_rounds: None,
        supports_multimodal: Some(model_supports_multimodal(
            &model.modalities,
            &model.input_modalities,
            &model.output_modalities,
        )),
    }
}

fn fallback_model_access_channel_response(
    model: &crate::domain::AiModel,
) -> AppModelAccessChannelModelResponse {
    AppModelAccessChannelModelResponse {
        catalog_key: model.catalog_key.clone(),
        model: model.model.clone(),
        display_name: model.display_name.clone(),
        context_tokens: model.context_tokens,
        max_output_tokens: model.max_output_tokens,
        tool_call_rounds: None,
        supports_multimodal: Some(model_supports_multimodal(
            &model.modalities,
            &model.input_modalities,
            &model.output_modalities,
        )),
    }
}

fn model_supports_multimodal(
    modalities: &[String],
    input_modalities: &[String],
    output_modalities: &[String],
) -> bool {
    modalities
        .iter()
        .chain(input_modalities)
        .chain(output_modalities)
        .any(|modality| {
            matches!(
                modality.trim().to_ascii_lowercase().as_str(),
                "image" | "audio" | "video"
            )
        })
}

async fn persist_access_channel(
    store: &(dyn AdminAiResourceStore + Send + Sync),
    entity_uuid_generator: &(dyn EntityUuidGenerator + Send + Sync),
    subject: AdminAiResourceSubject,
    command: &NormalizedAccessChannelUpsert,
) -> crate::domain::DomainResult<AdminAiResourceItem> {
    let mut drafts = Vec::new();
    let mut channel_members = Vec::with_capacity(command.offerings.len());
    for (vendor_index, offering) in command.offerings.iter().enumerate() {
        let vendor_resource_code = format!("{}.vendor.{}", command.channel_code, vendor_index + 1);
        let mut vendor_members = Vec::with_capacity(offering.models.len());
        for (model_index, model) in offering.models.iter().enumerate() {
            let model_resource_code = format!(
                "{}.model.{}.{}",
                command.channel_code,
                vendor_index + 1,
                model_index + 1
            );
            drafts.push(AppAiResourceDraft {
                resource_code: model_resource_code.clone(),
                resource_type: "model".to_owned(),
                display_name: model.display_name.clone(),
                vendor_code: Some(offering.vendor_code.clone()),
                catalog_key: Some(format!("{}/{}", offering.vendor_code, model.model_id)),
                model: Some(model.model_id.clone()),
                provider_native_model: Some(model.model_id.clone()),
                access_channel_kind: None,
                base_url: None,
                default_vendor_code: None,
                default_model_id: None,
                supported_agent_provider_ids: Vec::new(),
                context_tokens: model.context_tokens,
                max_output_tokens: model.max_output_tokens,
                tool_call_rounds: model.tool_call_rounds,
                supports_multimodal: model.supports_multimodal,
                description: None,
                composition_mode: "single".to_owned(),
                members: Vec::new(),
            });
            vendor_members.push(AdminAiResourceMemberCommand {
                member_resource_code: model_resource_code,
                member_role: "model".to_owned(),
                required: true,
                sort_order: Some(model_index as i64),
            });
        }
        drafts.push(AppAiResourceDraft {
            resource_code: vendor_resource_code.clone(),
            resource_type: "vendor".to_owned(),
            display_name: offering.vendor_name.clone(),
            vendor_code: Some(offering.vendor_code.clone()),
            catalog_key: None,
            model: None,
            provider_native_model: None,
            access_channel_kind: None,
            base_url: None,
            default_vendor_code: None,
            default_model_id: None,
            supported_agent_provider_ids: Vec::new(),
            context_tokens: None,
            max_output_tokens: None,
            tool_call_rounds: None,
            supports_multimodal: None,
            description: None,
            composition_mode: "all".to_owned(),
            members: vendor_members,
        });
        channel_members.push(AdminAiResourceMemberCommand {
            member_resource_code: vendor_resource_code,
            member_role: "vendor".to_owned(),
            required: true,
            sort_order: Some(vendor_index as i64),
        });
    }
    drafts.push(AppAiResourceDraft {
        resource_code: command.channel_code.clone(),
        resource_type: MODEL_ACCESS_CHANNEL_RESOURCE_TYPE.to_owned(),
        display_name: command.name.clone(),
        vendor_code: (command.kind == "official").then(|| command.default_vendor_code.clone()),
        catalog_key: None,
        model: None,
        provider_native_model: None,
        access_channel_kind: Some(command.kind.clone()),
        base_url: Some(command.base_url.clone()),
        default_vendor_code: Some(command.default_vendor_code.clone()),
        default_model_id: Some(command.default_model_id.clone()),
        supported_agent_provider_ids: command.supported_agent_provider_ids.clone(),
        context_tokens: None,
        max_output_tokens: None,
        tool_call_rounds: None,
        supports_multimodal: None,
        description: command.description.clone(),
        composition_mode: "all".to_owned(),
        members: channel_members,
    });
    let nodes = drafts
        .into_iter()
        .map(|draft| hierarchy_node_from_draft(entity_uuid_generator, draft))
        .collect::<crate::domain::DomainResult<Vec<_>>>()?;
    let requested_at = chrono::Utc::now().to_rfc3339();
    let request_id = generate_server_request_id().map_err(|error| match error {
        RequestIdError::Invalid(message) | RequestIdError::System(message) => {
            crate::domain::DomainError::new(message)
        }
    })?;
    store
        .replace_ai_resource_hierarchy(ReplaceAdminAiResourceHierarchyCommand {
            subject,
            root_resource_code: command.channel_code.clone(),
            owned_resource_code_prefixes: vec![
                format!("{}.vendor.", command.channel_code),
                format!("{}.model.", command.channel_code),
            ],
            nodes,
            audit_log_uuid: entity_uuid_generator.generate_entity_uuid()?,
            request_id,
            requested_at,
        })
        .await
}

fn hierarchy_node_from_draft(
    entity_uuid_generator: &(dyn EntityUuidGenerator + Send + Sync),
    draft: AppAiResourceDraft,
) -> crate::domain::DomainResult<AdminAiResourceHierarchyNodeCommand> {
    let member_uuids = (0..draft.members.len())
        .map(|_| entity_uuid_generator.generate_entity_uuid())
        .collect::<crate::domain::DomainResult<Vec<_>>>()?;
    Ok(AdminAiResourceHierarchyNodeCommand {
        resource_uuid: entity_uuid_generator.generate_entity_uuid()?,
        member_uuids,
        resource_code: draft.resource_code,
        resource_type: draft.resource_type,
        route_kind: None,
        display_name: draft.display_name,
        vendor_code: draft.vendor_code,
        modality_code: None,
        api_endpoint_code: None,
        catalog_key: draft.catalog_key,
        model: draft.model,
        provider_native_model: draft.provider_native_model,
        access_channel_kind: draft.access_channel_kind,
        base_url: draft.base_url,
        default_vendor_code: draft.default_vendor_code,
        default_model_id: draft.default_model_id,
        supported_agent_provider_ids: draft.supported_agent_provider_ids,
        context_tokens: draft.context_tokens,
        max_output_tokens: draft.max_output_tokens,
        tool_call_rounds: draft.tool_call_rounds,
        supports_multimodal: draft.supports_multimodal,
        description: draft.description,
        composition_mode: draft.composition_mode,
        status: "active".to_owned(),
        sort_order: None,
        members: draft.members,
    })
}

#[derive(Debug)]
struct DatabaseCatalogSnapshot {
    models: Vec<AdminAiModelItem>,
    vendor_names: BTreeMap<String, String>,
}

async fn load_database_catalog_snapshot<C>(
    state: &AppModelCatalogState<C>,
    subject: Option<&TrustedRequestSubject>,
) -> Option<DatabaseCatalogSnapshot>
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let store = state.model_store.as_ref()?;
    let subject = app_model_subject(subject);
    match database_model_catalog_is_authoritative(store.as_ref(), subject).await {
        Ok(true) => {}
        Ok(false) => return None,
        Err(error) => {
            tracing::warn!(error = %error, "database model catalog authority check failed; using fallback catalog");
            return None;
        }
    }
    let models = match load_all_database_models(store.as_ref(), subject).await {
        Ok(models) => models,
        Err(error) => {
            tracing::warn!(error = %error, "database model catalog is unavailable; using fallback catalog");
            return None;
        }
    };
    let vendors = match store
        .list_vendors(ListAdminModelVendorsQuery { subject })
        .await
    {
        Ok(vendors) => vendors,
        Err(error) => {
            tracing::warn!(error = %error, "database model vendors are unavailable; using fallback catalog");
            return None;
        }
    };
    Some(DatabaseCatalogSnapshot {
        models,
        vendor_names: database_vendor_names(vendors),
    })
}

async fn load_all_database_models(
    store: &(dyn ModelCatalogAdminStore + Send + Sync),
    subject: AdminModelSubject,
) -> crate::domain::DomainResult<Vec<AdminAiModelItem>> {
    let mut offset = 0_i64;
    let mut models = Vec::new();
    loop {
        let page = store
            .list_models(ListAdminAiModelsQuery {
                subject,
                vendor_id: None,
                vendor_codes: Vec::new(),
                q: None,
                model_types: None,
                status: Some("active".to_owned()),
                release_stages: vec![1, 2],
                shelf_state: Some(1),
                routing_state: Some(1),
                modalities: Vec::new(),
                page_size: Some(STORE_PAGE_SIZE),
                offset: Some(offset),
            })
            .await?;
        let total_count = page.total_count;
        let page_len = page.items.len() as i64;
        models.extend(page.items);
        offset += page_len;
        if page_len == 0 || offset >= total_count {
            break;
        }
    }
    Ok(models)
}

async fn database_model_catalog_is_authoritative(
    store: &(dyn ModelCatalogAdminStore + Send + Sync),
    subject: AdminModelSubject,
) -> crate::domain::DomainResult<bool> {
    store
        .list_models(ListAdminAiModelsQuery {
            subject,
            vendor_id: None,
            vendor_codes: Vec::new(),
            q: None,
            model_types: None,
            status: None,
            release_stages: Vec::new(),
            shelf_state: None,
            routing_state: None,
            modalities: Vec::new(),
            page_size: Some(1),
            offset: Some(0),
        })
        .await
        .map(|page| page.total_count > 0)
}

async fn load_database_model_page(
    store: &(dyn ModelCatalogAdminStore + Send + Sync),
    subject: AdminModelSubject,
    query: &AppModelCatalogQuery,
    page: usize,
    page_size: usize,
) -> crate::domain::DomainResult<AdminAiModelListPage> {
    store
        .list_models(build_database_model_list_query(
            subject, query, page, page_size,
        ))
        .await
}

fn build_database_model_list_query(
    subject: AdminModelSubject,
    query: &AppModelCatalogQuery,
    page: usize,
    page_size: usize,
) -> ListAdminAiModelsQuery {
    let mut vendor_codes = comma_separated_query_values(query.vendor_codes.as_deref());
    if let Some(vendor_code) = query.vendor_code.as_ref() {
        vendor_codes.push(vendor_code.clone());
    }
    let mut model_types = comma_separated_query_values(query.categories.as_deref());
    model_types.extend(comma_separated_query_values(query.capabilities.as_deref()));
    model_types.extend(comma_separated_query_values(query.groups.as_deref()));
    model_types.sort();
    model_types.dedup();
    ListAdminAiModelsQuery {
        subject,
        vendor_id: None,
        vendor_codes,
        q: query.q.clone(),
        model_types: (!model_types.is_empty()).then(|| model_types.join(",")),
        status: Some("active".to_owned()),
        release_stages: vec![1, 2],
        shelf_state: Some(1),
        routing_state: Some(1),
        modalities: comma_separated_query_values(query.modalities.as_deref()),
        page_size: Some(page_size as i64),
        offset: Some(((page - 1) * page_size) as i64),
    }
}

fn build_access_channel_list_query(
    subject: AdminAiResourceSubject,
    query: &AppModelAccessChannelQuery,
    page: usize,
    page_size: usize,
) -> ListAdminAiResourcesQuery {
    ListAdminAiResourcesQuery {
        subject,
        q: query.q.clone(),
        resource_type: Some(MODEL_ACCESS_CHANNEL_RESOURCE_TYPE.to_owned()),
        status: Some("active".to_owned()),
        access_channel_kind: query.kind.clone(),
        vendor_code: query.vendor_code.clone(),
        agent_provider_id: query.agent_provider_id.clone(),
        require_valid_access_channel_metadata: true,
        limit: Some(page_size as i64),
        offset: Some(((page - 1) * page_size) as i64),
    }
}

fn app_model_subject(subject: Option<&TrustedRequestSubject>) -> AdminModelSubject {
    AdminModelSubject {
        tenant_id: subject.map(|value| value.tenant_id).unwrap_or(0),
        organization_id: subject.map(|value| value.organization_id).unwrap_or(0),
        operator_id: subject.map(|value| value.user_id).unwrap_or(0),
        operator_type: subject.map(|value| value.operator_type).unwrap_or(0),
    }
}

fn app_resource_subject(subject: Option<&TrustedRequestSubject>) -> AdminAiResourceSubject {
    AdminAiResourceSubject {
        tenant_id: subject.map(|value| value.tenant_id).unwrap_or(0),
        organization_id: subject.map(|value| value.organization_id).unwrap_or(0),
        operator_id: subject.map(|value| value.user_id).unwrap_or(0),
        operator_type: subject.map(|value| value.operator_type).unwrap_or(0),
    }
}

fn database_vendor_names(vendors: Vec<AdminModelVendorItem>) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for vendor in vendors {
        if vendor.status != "active" {
            continue;
        }
        let code = vendor.vendor_code.trim().to_ascii_lowercase();
        if code.is_empty() {
            continue;
        }
        names.entry(code).or_insert(vendor.name);
    }
    names
}

fn to_database_vendor_response(
    snapshot: &DatabaseCatalogSnapshot,
) -> AppModelVendorCatalogResponse {
    let mut counts = BTreeMap::<String, usize>::new();
    for model in snapshot
        .models
        .iter()
        .filter(|model| model.status == "active")
    {
        *counts
            .entry(model.vendor_code.trim().to_ascii_lowercase())
            .or_default() += 1;
    }
    let mut items = counts
        .into_iter()
        .filter(|(code, _)| !code.is_empty())
        .map(|(code, model_count)| AppModelVendorOptionResponse {
            label: snapshot
                .vendor_names
                .get(&code)
                .cloned()
                .unwrap_or_else(|| code.clone()),
            code,
            model_count,
        })
        .collect::<Vec<_>>();
    items.sort_by(|first, second| {
        first
            .label
            .to_ascii_lowercase()
            .cmp(&second.label.to_ascii_lowercase())
            .then_with(|| first.code.cmp(&second.code))
    });
    AppModelVendorCatalogResponse { items }
}

fn to_database_model_response(
    database_page: AdminAiModelListPage,
    billing_meter: &BillingMeter,
    page: usize,
    page_size: usize,
    profile_counts: &ProfileCounts,
) -> AppModelCatalogResponse {
    let groups = database_model_groups(&database_page.items);
    let total_items = database_page.total_count;
    let items = database_page
        .items
        .into_iter()
        .map(|item| to_database_model_item(item, billing_meter, profile_counts))
        .collect();
    AppModelCatalogResponse {
        items,
        groups,
        page_info: offset_page_info(page as i64, page_size as i64, total_items),
    }
}

fn database_model_groups(models: &[AdminAiModelItem]) -> Vec<AppModelCatalogGroupResponse> {
    let mut counts = BTreeMap::<String, usize>::new();
    for model in models {
        let key = model.model_type.trim().to_ascii_lowercase();
        if !key.is_empty() {
            *counts.entry(key).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .map(|(key, model_count)| AppModelCatalogGroupResponse {
            label: key.clone(),
            key,
            model_count,
        })
        .collect()
}

fn database_model_capabilities(model: &AdminAiModelItem) -> Vec<String> {
    let mut capabilities = BTreeSet::new();
    if !model.model_type.trim().is_empty() {
        capabilities.insert(model.model_type.trim().to_ascii_lowercase());
    }
    capabilities.into_iter().collect()
}

fn to_database_model_item(
    item: AdminAiModelItem,
    billing_meter: &BillingMeter,
    profile_counts: &ProfileCounts,
) -> AppModelCatalogItemResponse {
    let official_reference_prices = database_reference_prices(&item, billing_meter);
    let capabilities = database_model_capabilities(&item);
    let (video_profile_count, voice_binding_count) =
        profile_counts.for_catalog_key(&item.catalog_key);
    let price_availability = AppModelCatalogPriceAvailabilityResponse {
        status: if official_reference_prices.is_empty() {
            "unavailable"
        } else {
            "reference"
        },
        reason: Some(
            if official_reference_prices.is_empty() {
                PUBLIC_PRICE_UNAVAILABLE_REASON
            } else {
                PUBLIC_REFERENCE_PRICE_REASON
            }
            .to_owned(),
        ),
    };
    let groups = vec![item.model_type.clone()];
    let categories = vec![item.model_type.clone()];
    AppModelCatalogItemResponse {
        catalog_key: item.catalog_key,
        model: item.model,
        display_name: item.display_name,
        vendor_code: item.vendor_code.clone(),
        vendor: item.vendor_name,
        capabilities,
        groups,
        categories,
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
        supplier_codes: Vec::new(),
        supported_agent_provider_ids: canonical_agent_provider_ids(),
        official_reference_prices,
        price_availability,
        family_code: item.family_code,
        color_token: item.color_token,
        docs_url: item.docs_url,
        video_profile_count,
        voice_binding_count,
    }
}

fn database_reference_prices(
    item: &AdminAiModelItem,
    billing_meter: &BillingMeter,
) -> Vec<AppModelCatalogReferencePriceResponse> {
    item.region_prices
        .iter()
        .filter_map(|price| {
            let unit_price = match billing_meter {
                BillingMeter::LlmOutputToken => Some(price.price_out.as_str()),
                BillingMeter::LlmCacheReadToken => price.cache_read_price.as_deref(),
                BillingMeter::LlmCacheWriteToken => price.cache_write_price.as_deref(),
                _ => Some(price.price_in.as_str()),
            }?;
            (!unit_price.trim().is_empty()).then(|| AppModelCatalogReferencePriceResponse {
                region_code: price.region_code.clone(),
                billing_meter: billing_meter.code().to_owned(),
                unit_price: unit_price.to_owned(),
                currency: price.currency.clone(),
            })
        })
        .collect()
}

#[derive(Default)]
struct AccessChannelSelection {
    vendor_codes: BTreeSet<String>,
    legacy_vendor_codes: BTreeSet<String>,
    models: BTreeMap<(String, String), AppModelAccessChannelModelResponse>,
}

async fn build_model_access_channels<C>(
    store: &(dyn AdminAiResourceStore + Send + Sync),
    subject: AdminAiResourceSubject,
    channels: Vec<AdminAiResourceItem>,
    database_catalog: Option<&DatabaseCatalogSnapshot>,
    fallback_catalog: &C,
) -> crate::domain::DomainResult<Vec<AppModelAccessChannelItemResponse>>
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let mut vendor_names = database_catalog
        .map(|catalog| catalog.vendor_names.clone())
        .unwrap_or_default();
    let mut items = Vec::with_capacity(channels.len());
    for channel in channels {
        if channel.status != "active" || channel.resource_type != MODEL_ACCESS_CHANNEL_RESOURCE_TYPE
        {
            continue;
        }
        let Some(kind) = normalized_access_channel_kind(channel.access_channel_kind.as_deref())
        else {
            continue;
        };
        let Some(base_url) = valid_public_base_url(channel.base_url.as_deref()) else {
            continue;
        };
        let lookup = load_channel_member_lookup(store, subject, &channel).await?;
        for resource in lookup.values() {
            if resource.resource_type == "vendor" {
                if let Some(code) = normalized_resource_vendor_code(resource) {
                    vendor_names
                        .entry(code)
                        .or_insert_with(|| resource.display_name.clone());
                }
            }
        }
        let mut selection = AccessChannelSelection::default();
        if let Some(vendor_code) = channel.vendor_code.as_deref() {
            selection
                .vendor_codes
                .insert(vendor_code.trim().to_ascii_lowercase());
        }
        let mut visited = HashSet::new();
        for member in &channel.members {
            collect_channel_member(
                &member.member_resource_code,
                &lookup,
                &mut visited,
                &mut selection,
            );
        }
        enrich_or_expand_vendor_models(&mut selection, &lookup, database_catalog, fallback_catalog);
        let offerings = to_access_channel_offerings(selection, &vendor_names);
        if offerings.is_empty() || (kind == "official" && offerings.len() != 1) {
            continue;
        }
        let default_vendor_code = channel
            .default_vendor_code
            .as_deref()
            .map(str::trim)
            .filter(|vendor_code| {
                offerings
                    .iter()
                    .any(|offering| offering.vendor_code.eq_ignore_ascii_case(vendor_code))
            })
            .map(str::to_owned)
            .unwrap_or_else(|| offerings[0].vendor_code.clone());
        let Some(default_offering) = offerings.iter().find(|offering| {
            offering
                .vendor_code
                .eq_ignore_ascii_case(&default_vendor_code)
        }) else {
            continue;
        };
        let default_model_id = channel
            .default_model_id
            .as_deref()
            .map(str::trim)
            .filter(|model_id| {
                default_offering
                    .models
                    .iter()
                    .any(|model| model.model.eq_ignore_ascii_case(model_id))
            })
            .map(str::to_owned)
            .or_else(|| {
                default_offering
                    .models
                    .first()
                    .map(|model| model.model.clone())
            });
        let Some(default_model_id) = default_model_id else {
            continue;
        };
        let vendor_count = offerings.len();
        let model_count = offerings.iter().map(|offering| offering.models.len()).sum();
        items.push(AppModelAccessChannelItemResponse {
            id: channel.id.to_string(),
            code: channel.resource_code,
            name: channel.display_name,
            kind,
            base_url,
            description: channel.description,
            default_vendor_code,
            default_model_id,
            supported_agent_provider_ids: if channel.supported_agent_provider_ids.is_empty() {
                canonical_agent_provider_ids()
            } else {
                channel.supported_agent_provider_ids
            },
            offerings,
            sort_order: channel.sort_order.map(|value| value.to_string()),
            vendor_count,
            model_count,
        });
    }
    Ok(items)
}

async fn load_channel_member_lookup(
    store: &(dyn AdminAiResourceStore + Send + Sync),
    subject: AdminAiResourceSubject,
    channel: &AdminAiResourceItem,
) -> crate::domain::DomainResult<HashMap<String, AdminAiResourceItem>> {
    let mut pending = channel
        .members
        .iter()
        .map(|member| member.member_resource_code.clone())
        .collect::<Vec<_>>();
    let mut visited = HashSet::new();
    let mut lookup = HashMap::new();
    while let Some(resource_code) = pending.pop() {
        if !visited.insert(resource_code.clone()) {
            continue;
        }
        let page = store
            .list_ai_resources(ListAdminAiResourcesQuery {
                subject,
                q: Some(resource_code.clone()),
                resource_type: None,
                status: Some("active".to_owned()),
                access_channel_kind: None,
                vendor_code: None,
                agent_provider_id: None,
                require_valid_access_channel_metadata: false,
                limit: Some(STORE_PAGE_SIZE),
                offset: Some(0),
            })
            .await?;
        let Some(resource) = page
            .items
            .into_iter()
            .find(|resource| resource.resource_code == resource_code)
        else {
            continue;
        };
        pending.extend(
            resource
                .members
                .iter()
                .map(|member| member.member_resource_code.clone()),
        );
        lookup.insert(resource_code, resource);
    }
    Ok(lookup)
}

fn collect_channel_member(
    resource_code: &str,
    lookup: &HashMap<String, AdminAiResourceItem>,
    visited: &mut HashSet<String>,
    selection: &mut AccessChannelSelection,
) {
    if !visited.insert(resource_code.to_owned()) {
        return;
    }
    let Some(resource) = lookup.get(resource_code) else {
        return;
    };
    match resource.resource_type.as_str() {
        "vendor" => {
            if let Some(vendor_code) = normalized_resource_vendor_code(resource) {
                selection.vendor_codes.insert(vendor_code.clone());
                if resource.members.is_empty() {
                    selection.legacy_vendor_codes.insert(vendor_code);
                }
            }
            for member in &resource.members {
                collect_channel_member(&member.member_resource_code, lookup, visited, selection);
            }
        }
        "model" | "model_api" => insert_resource_model(selection, resource),
        _ => {
            for member in &resource.members {
                collect_channel_member(&member.member_resource_code, lookup, visited, selection);
            }
        }
    }
}

fn normalized_resource_vendor_code(resource: &AdminAiResourceItem) -> Option<String> {
    resource
        .vendor_code
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn insert_resource_model(selection: &mut AccessChannelSelection, resource: &AdminAiResourceItem) {
    let Some(vendor_code) = normalized_resource_vendor_code(resource) else {
        return;
    };
    let Some(model) = resource
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let catalog_key = resource
        .catalog_key
        .clone()
        .unwrap_or_else(|| format!("{vendor_code}/{model}"));
    selection.vendor_codes.insert(vendor_code.clone());
    selection.models.insert(
        (vendor_code, catalog_key.clone()),
        AppModelAccessChannelModelResponse {
            catalog_key,
            model: model.to_owned(),
            display_name: resource.display_name.clone(),
            context_tokens: resource.context_tokens,
            max_output_tokens: resource.max_output_tokens,
            tool_call_rounds: resource.tool_call_rounds,
            supports_multimodal: resource.supports_multimodal,
        },
    );
}

fn enrich_or_expand_vendor_models<C>(
    selection: &mut AccessChannelSelection,
    lookup: &HashMap<String, AdminAiResourceItem>,
    database_catalog: Option<&DatabaseCatalogSnapshot>,
    fallback_catalog: &C,
) where
    C: PricingCatalog + Send + Sync + 'static,
{
    if let Some(catalog) = database_catalog {
        enrich_selected_models_from_database(selection, catalog);
        for model in catalog.models.iter().filter(|model| {
            selection
                .legacy_vendor_codes
                .contains(&model.vendor_code.trim().to_ascii_lowercase())
        }) {
            let vendor_code = model.vendor_code.trim().to_ascii_lowercase();
            selection.models.insert(
                (vendor_code, model.catalog_key.clone()),
                database_model_access_channel_response(model),
            );
        }
        return;
    }
    enrich_selected_models_from_fallback(selection, fallback_catalog);
    let legacy_vendor_codes = selection.legacy_vendor_codes.clone();
    for resource in lookup.values().filter(|resource| {
        matches!(resource.resource_type.as_str(), "model" | "model_api")
            && normalized_resource_vendor_code(resource)
                .is_some_and(|vendor_code| legacy_vendor_codes.contains(&vendor_code))
    }) {
        insert_resource_model(selection, resource);
    }
    for vendor_code in legacy_vendor_codes {
        for model in fallback_access_channel_models(fallback_catalog, &vendor_code) {
            selection
                .models
                .insert((vendor_code.clone(), model.catalog_key.clone()), model);
        }
    }
}

fn enrich_selected_models_from_database(
    selection: &mut AccessChannelSelection,
    catalog: &DatabaseCatalogSnapshot,
) {
    for ((vendor_code, catalog_key), selected) in &mut selection.models {
        let Some(model) = catalog.models.iter().find(|model| {
            model.catalog_key.eq_ignore_ascii_case(catalog_key)
                || (model.vendor_code.eq_ignore_ascii_case(vendor_code)
                    && model.model == selected.model)
        }) else {
            continue;
        };
        let stored_tool_call_rounds = selected.tool_call_rounds;
        *selected = database_model_access_channel_response(model);
        selected.tool_call_rounds = stored_tool_call_rounds;
    }
}

fn enrich_selected_models_from_fallback<C>(selection: &mut AccessChannelSelection, catalog: &C)
where
    C: PricingCatalog + Send + Sync + 'static,
{
    let vendor_codes = selection.vendor_codes.clone();
    for vendor_code in vendor_codes {
        for model in fallback_access_channel_models(catalog, &vendor_code) {
            let key = (vendor_code.clone(), model.catalog_key.clone());
            let matching_key = selection
                .models
                .iter()
                .find(|((selected_vendor, selected_catalog_key), selected)| {
                    selected_vendor == &vendor_code
                        && (selected_catalog_key.eq_ignore_ascii_case(&model.catalog_key)
                            || selected.model == model.model)
                })
                .map(|(key, _)| key.clone());
            let Some(matching_key) = matching_key else {
                continue;
            };
            let stored_tool_call_rounds = selection
                .models
                .get(&matching_key)
                .and_then(|selected| selected.tool_call_rounds);
            let mut enriched = model;
            enriched.tool_call_rounds = stored_tool_call_rounds;
            selection.models.remove(&matching_key);
            selection.models.insert(key, enriched);
        }
    }
}

fn to_access_channel_offerings(
    selection: AccessChannelSelection,
    vendor_names: &BTreeMap<String, String>,
) -> Vec<AppModelAccessChannelVendorOfferingResponse> {
    let mut grouped = BTreeMap::<String, Vec<AppModelAccessChannelModelResponse>>::new();
    for ((vendor_code, _), model) in selection.models {
        grouped.entry(vendor_code).or_default().push(model);
    }
    for vendor_code in selection.vendor_codes {
        grouped.entry(vendor_code).or_default();
    }
    let mut offerings = grouped
        .into_iter()
        .map(|(vendor_code, mut models)| {
            models.sort_by(|first, second| {
                first
                    .display_name
                    .to_ascii_lowercase()
                    .cmp(&second.display_name.to_ascii_lowercase())
                    .then_with(|| first.model.cmp(&second.model))
            });
            AppModelAccessChannelVendorOfferingResponse {
                vendor_name: vendor_names
                    .get(&vendor_code)
                    .cloned()
                    .unwrap_or_else(|| vendor_code.clone()),
                vendor_code,
                models,
            }
        })
        .collect::<Vec<_>>();
    offerings.sort_by(|first, second| {
        first
            .vendor_name
            .to_ascii_lowercase()
            .cmp(&second.vendor_name.to_ascii_lowercase())
            .then_with(|| first.vendor_code.cmp(&second.vendor_code))
    });
    offerings
}

fn normalized_access_channel_kind(kind: Option<&str>) -> Option<String> {
    match kind.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("official") => Some("official".to_owned()),
        Some("relay") => Some("relay".to_owned()),
        Some("custom") => Some("custom".to_owned()),
        _ => None,
    }
}

fn valid_public_base_url(value: Option<&str>) -> Option<String> {
    let parsed = Url::parse(value?.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return None;
    }
    Some(parsed.to_string())
}

fn validate_access_channel_query(
    mut query: AppModelAccessChannelQuery,
) -> Result<AppModelAccessChannelQuery, String> {
    query.q = normalize_bounded_query(query.q, "q")?;
    query.kind =
        normalize_bounded_query(query.kind, "kind")?.map(|value| value.to_ascii_lowercase());
    if query
        .kind
        .as_deref()
        .is_some_and(|kind| !matches!(kind, "official" | "relay"))
    {
        return Err("kind must be one of official, relay".to_owned());
    }
    query.vendor_code = normalize_bounded_query(query.vendor_code, "vendor_code")?
        .map(|value| value.to_ascii_lowercase());
    query.agent_provider_id =
        normalize_bounded_query(query.agent_provider_id, "agent_provider_id")?
            .map(|value| value.to_ascii_lowercase());
    if let Some(provider_id) = query.agent_provider_id.as_deref() {
        if !CANONICAL_AGENT_PROVIDER_IDS.contains(&provider_id) {
            return Err(format!("agent_provider_id is unsupported: {provider_id}"));
        }
    }
    Ok(query)
}

fn normalize_bounded_query(
    value: Option<String>,
    field_name: &str,
) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > MAX_SEARCH_LEN {
        return Err(format!(
            "{field_name} must not exceed {MAX_SEARCH_LEN} characters"
        ));
    }
    Ok(Some(value.to_owned()))
}

fn canonical_agent_provider_ids() -> Vec<String> {
    CANONICAL_AGENT_PROVIDER_IDS
        .iter()
        .map(|value| (*value).to_owned())
        .collect()
}

fn to_response(
    page: ModelCatalogPage,
    profile_counts: &ProfileCounts,
) -> AppModelCatalogResponse {
    AppModelCatalogResponse {
        items: page
            .items
            .into_iter()
            .map(|item| to_item_response(item, profile_counts))
            .collect(),
        groups: page.groups.into_iter().map(to_group_response).collect(),
        page_info: offset_page_info(
            ((page.offset / page.page_size) + 1) as i64,
            page.page_size as i64,
            page.total_items as i64,
        ),
    }
}

fn validate_page_query(
    page: Option<usize>,
    page_size: Option<usize>,
) -> Result<(usize, usize), String> {
    let page = page.unwrap_or(1);
    if page == 0 {
        return Err("page must be greater than or equal to 1".to_owned());
    }
    let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
        return Err(format!("page_size must be between 1 and {MAX_PAGE_SIZE}"));
    }
    page.checked_sub(1)
        .and_then(|value| value.checked_mul(page_size))
        .ok_or_else(|| "page is too large".to_owned())?;
    Ok((page, page_size))
}

fn to_group_response(group: ModelCatalogGroup) -> AppModelCatalogGroupResponse {
    AppModelCatalogGroupResponse {
        key: group.key,
        label: group.label,
        model_count: group.model_count,
    }
}

fn to_vendor_response(
    catalog: &(impl PricingCatalog + Send + Sync),
) -> AppModelVendorCatalogResponse {
    let mut vendors_by_code: BTreeMap<String, AppModelVendorOptionResponse> = BTreeMap::new();
    catalog.visit_models(None, &mut |model| {
        if !model.is_publicly_active() {
            return true;
        }
        let code = model.vendor_code.trim();
        if code.is_empty() {
            return true;
        }
        let entry = vendors_by_code.entry(code.to_owned()).or_insert_with(|| {
            AppModelVendorOptionResponse {
                label: catalog
                    .find_vendor(code)
                    .map(|vendor| vendor.display_name)
                    .unwrap_or_else(|| code.to_owned()),
                code: code.to_owned(),
                model_count: 0,
            }
        });
        entry.model_count += 1;
        true
    });

    let mut items: Vec<_> = vendors_by_code.into_values().collect();
    items.sort_by(|first, second| {
        first
            .label
            .to_lowercase()
            .cmp(&second.label.to_lowercase())
            .then_with(|| first.code.cmp(&second.code))
    });
    AppModelVendorCatalogResponse { items }
}

fn to_item_response(
    item: ModelCatalogItem,
    profile_counts: &ProfileCounts,
) -> AppModelCatalogItemResponse {
    let price_availability =
        to_price_availability_response(&item.official_reference_prices, item.price_availability);
    let (video_profile_count, voice_binding_count) =
        profile_counts.for_catalog_key(&item.catalog_key);

    AppModelCatalogItemResponse {
        catalog_key: item.catalog_key,
        model: item.model,
        display_name: item.display_name,
        vendor_code: item.vendor_code,
        vendor: item.vendor.code().to_owned(),
        capabilities: item.capabilities,
        groups: item.groups,
        categories: item.categories,
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
        supplier_codes: item.supplier_codes,
        supported_agent_provider_ids: canonical_agent_provider_ids(),
        official_reference_prices: item
            .official_reference_prices
            .into_iter()
            .map(|price| AppModelCatalogReferencePriceResponse {
                region_code: price.region_code,
                billing_meter: price.billing_meter,
                unit_price: price.unit_price,
                currency: price.currency,
            })
            .collect(),
        price_availability,
        family_code: None,
        color_token: None,
        docs_url: None,
        video_profile_count,
        voice_binding_count,
    }
}

fn comma_separated_query_values(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn to_price_availability_response(
    official_reference_prices: &[crate::application::ModelCatalogReferencePriceView],
    availability: PriceAvailability,
) -> AppModelCatalogPriceAvailabilityResponse {
    if public_reference_price_is_configured(official_reference_prices) {
        return AppModelCatalogPriceAvailabilityResponse {
            status: "reference",
            reason: Some(PUBLIC_REFERENCE_PRICE_REASON.to_owned()),
        };
    }

    match availability {
        PriceAvailability::Available(_) | PriceAvailability::Unavailable { .. } => {
            AppModelCatalogPriceAvailabilityResponse {
                status: "unavailable",
                reason: Some(PUBLIC_PRICE_UNAVAILABLE_REASON.to_owned()),
            }
        }
    }
}

fn public_reference_price_is_configured(
    official_reference_prices: &[crate::application::ModelCatalogReferencePriceView],
) -> bool {
    official_reference_prices
        .iter()
        .any(|price| !price.unit_price.trim().is_empty() && !price.currency.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_channel_kind_normalizes_all_three_kinds() {
        assert_eq!(
            normalized_access_channel_kind(Some("official")).as_deref(),
            Some("official")
        );
        assert_eq!(
            normalized_access_channel_kind(Some("relay")).as_deref(),
            Some("relay")
        );
        assert_eq!(
            normalized_access_channel_kind(Some("custom")).as_deref(),
            Some("custom")
        );
        assert_eq!(normalized_access_channel_kind(Some("unknown")), None);
        assert_eq!(normalized_access_channel_kind(None), None);
    }

    #[test]
    fn database_model_query_keeps_filtering_and_pagination_in_the_store() {
        let query = AppModelCatalogQuery {
            vendor_code: Some("openai".to_owned()),
            vendor_codes: Some("anthropic,google".to_owned()),
            modalities: Some("text,image".to_owned()),
            capabilities: Some("chat".to_owned()),
            categories: Some("embedding".to_owned()),
            groups: Some("image".to_owned()),
            q: Some("latest".to_owned()),
            ..Default::default()
        };
        let store_query = build_database_model_list_query(
            AdminModelSubject {
                tenant_id: 7,
                organization_id: 9,
                operator_id: 11,
                operator_type: 1,
            },
            &query,
            3,
            25,
        );
        assert_eq!(store_query.status.as_deref(), Some("active"));
        assert_eq!(store_query.release_stages, vec![1, 2]);
        assert_eq!(store_query.shelf_state, Some(1));
        assert_eq!(store_query.routing_state, Some(1));
        assert_eq!(store_query.page_size, Some(25));
        assert_eq!(store_query.offset, Some(50));
        assert_eq!(store_query.q.as_deref(), Some("latest"));
        assert!(store_query.vendor_codes.contains(&"openai".to_owned()));
        assert_eq!(store_query.modalities, vec!["text", "image"]);
        assert_eq!(
            store_query.model_types.as_deref(),
            Some("chat,embedding,image")
        );
    }

    #[test]
    fn database_model_response_preserves_store_rank_order_and_vendor_name() {
        let response = to_database_model_response(
            AdminAiModelListPage {
                items: vec![
                    database_model_item("ranked-first", "openai", "OpenAI"),
                    database_model_item("ranked-second", "anthropic", "Anthropic"),
                ],
                total_count: 2,
            },
            &BillingMeter::LlmInputToken,
            1,
            20,
            &ProfileCounts::default(),
        );

        assert_eq!(response.items[0].model, "ranked-first");
        assert_eq!(response.items[0].vendor_code, "openai");
        assert_eq!(response.items[0].vendor, "OpenAI");
        assert_eq!(response.items[1].model, "ranked-second");
        assert_eq!(response.items[1].vendor, "Anthropic");
    }

    #[test]
    fn database_model_item_publishes_catalog_attributes_and_profile_counts() {
        let mut item = database_model_item(
            "doubao-seedance-2-0-mini-260615",
            "bytedance",
            "ByteDance",
        );
        item.family_code = Some("seedance".to_owned());
        item.color_token = Some("#7c3aed".to_owned());
        item.docs_url = Some("https://www.volcengine.com/docs/82379/1330310".to_owned());
        let mut counts = ProfileCounts::default();
        counts.video_profiles.insert(
            "bytedance/doubao-seedance-2-0-mini-260615".to_owned(),
            6,
        );
        counts
            .voice_bindings
            .insert("bytedance/doubao-seedance-2-0-mini-260615".to_owned(), 0);

        let response = to_database_model_item(item, &BillingMeter::LlmInputToken, &counts);
        assert_eq!(response.family_code.as_deref(), Some("seedance"));
        assert_eq!(response.color_token.as_deref(), Some("#7c3aed"));
        assert!(response.docs_url.is_some());
        assert_eq!(response.video_profile_count, 6);
        assert_eq!(response.voice_binding_count, 0);
    }

    #[test]
    fn profile_counts_read_video_profiles_and_voice_bindings_from_the_catalog() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalog = sdkwork_models::load_catalog(root).expect("bundled catalog loads");
        let counts = ProfileCounts::from_catalog(Some(&catalog));

        let (video_profiles, voice_bindings) =
            counts.for_catalog_key("bytedance/doubao-seedance-2-0-mini-260615");
        assert_eq!(video_profiles, 6, "seedance mini declares six profiles");
        assert_eq!(voice_bindings, 0);

        let (_, voice_bindings) = counts.for_catalog_key("elevenlabs/eleven_multilingual_v2");
        assert!(voice_bindings >= 1, "TTS models publish a voice binding");

        assert_eq!(counts.for_catalog_key("unknown/model"), (0, 0));
        assert_eq!(
            ProfileCounts::from_catalog(None).for_catalog_key("bytedance/x"),
            (0, 0)
        );
    }

    fn database_model_item(model: &str, vendor_code: &str, vendor_name: &str) -> AdminAiModelItem {
        AdminAiModelItem {
            id: 1,
            uuid: format!("uuid-{model}"),
            tenant_id: 0,
            organization_id: 0,
            vendor_id: vendor_code.to_owned(),
            vendor_code: vendor_code.to_owned(),
            vendor_name: vendor_name.to_owned(),
            catalog_key: format!("{vendor_code}/{model}"),
            model: model.to_owned(),
            display_name: model.to_owned(),
            name: model.to_owned(),
            model_type: "Chat".to_owned(),
            family_code: None,
            color_token: None,
            docs_url: None,
            region_prices: Vec::new(),
            status: "active".to_owned(),
            calls: "0".to_owned(),
            description: None,
            modalities: vec!["text".to_owned()],
            input_modalities: vec!["text".to_owned()],
            output_modalities: vec!["text".to_owned()],
            api_format: Some("openai".to_owned()),
            capability_intro: None,
            limitations: Vec::new(),
            supported_languages: Vec::new(),
            use_cases: Vec::new(),
            training_data_cutoff: None,
            context_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            supports_streaming: true,
            supports_tools: true,
            supports_json_schema: true,
            usage_scopes: vec!["coding".to_owned(), "chat".to_owned(), "agent".to_owned()],
            coding_visible: true,
            release_stage: Some(1),
            shelf_state: Some(1),
            routing_state: Some(1),
            replacement_model: None,
            deleted_at: None,
        }
    }

    #[test]
    fn access_channel_query_pushes_all_primary_filters_to_the_store() {
        let query = AppModelAccessChannelQuery {
            page: None,
            page_size: None,
            q: Some("gpt".to_owned()),
            kind: Some("relay".to_owned()),
            vendor_code: Some("openai".to_owned()),
            agent_provider_id: Some("codex".to_owned()),
        };
        let store_query = build_access_channel_list_query(
            AdminAiResourceSubject {
                tenant_id: 7,
                organization_id: 9,
                operator_id: 11,
                operator_type: 1,
            },
            &query,
            2,
            20,
        );
        assert_eq!(
            store_query.resource_type.as_deref(),
            Some("model_access_channel")
        );
        assert_eq!(store_query.status.as_deref(), Some("active"));
        assert_eq!(store_query.access_channel_kind.as_deref(), Some("relay"));
        assert_eq!(store_query.vendor_code.as_deref(), Some("openai"));
        assert_eq!(store_query.agent_provider_id.as_deref(), Some("codex"));
        assert!(store_query.require_valid_access_channel_metadata);
        assert_eq!(store_query.limit, Some(20));
        assert_eq!(store_query.offset, Some(20));
    }

    #[test]
    fn access_channel_public_metadata_and_vendor_names_are_total() {
        assert_eq!(
            valid_public_base_url(Some("https://relay.example.test/v1")),
            Some("https://relay.example.test/v1".to_owned())
        );
        assert!(valid_public_base_url(Some("https://user:secret@example.test/v1")).is_none());
        assert!(normalized_access_channel_kind(Some("legacy")).is_none());

        let mut selection = AccessChannelSelection::default();
        selection.vendor_codes.insert("openai".to_owned());
        let offerings = to_access_channel_offerings(selection, &BTreeMap::new());
        assert_eq!(offerings.len(), 1);
        assert_eq!(offerings[0].vendor_name, "openai");
    }

    #[test]
    fn explicit_model_offering_is_enriched_without_expanding_the_vendor() {
        let mut selection = AccessChannelSelection::default();
        selection.vendor_codes.insert("openai".to_owned());
        selection.models.insert(
            ("openai".to_owned(), "openai/gpt-5".to_owned()),
            AppModelAccessChannelModelResponse {
                catalog_key: "openai/gpt-5".to_owned(),
                model: "gpt-5".to_owned(),
                display_name: "Configured name".to_owned(),
                context_tokens: None,
                max_output_tokens: None,
                tool_call_rounds: Some(8),
                supports_multimodal: None,
            },
        );
        let catalog = DatabaseCatalogSnapshot {
            models: vec![
                database_model_item("gpt-5", "openai", "OpenAI"),
                database_model_item("gpt-4.1", "openai", "OpenAI"),
            ],
            vendor_names: BTreeMap::from([(String::from("openai"), String::from("OpenAI"))]),
        };

        enrich_selected_models_from_database(&mut selection, &catalog);

        assert_eq!(selection.models.len(), 1);
        let selected = selection.models.values().next().expect("selected model");
        assert_eq!(selected.model, "gpt-5");
        assert_eq!(selected.context_tokens, Some(128_000));
        assert_eq!(selected.tool_call_rounds, Some(8));
    }

    #[test]
    fn official_provider_overlay_exposes_strict_vendor_presets() {
        let providers = official_provider_definitions().expect("valid provider overlay");
        assert_eq!(providers.len(), 3);
        assert_eq!(providers[0].vendor_code, "openai");
        assert_eq!(providers[0].base_url, "https://api.birdcoder.com/v1");
        assert_eq!(providers[1].vendor_code, "anthropic");
        assert_eq!(providers[2].vendor_code, "google");
    }

    #[test]
    fn access_channel_upsert_rejects_credentials_and_invalid_official_composition() {
        let credential_body = br#"{
            "name":"OpenAI",
            "kind":"official",
            "baseUrl":"https://api.birdcoder.com/v1",
            "offerings":[{"vendorCode":"openai","vendorName":"OpenAI","modelIds":["gpt-5"]}],
            "defaultVendorCode":"openai",
            "defaultModelId":"gpt-5",
            "supportedAgentProviderIds":["codex"],
            "apiKey":"must-not-enter-models"
        }"#;
        assert!(parse_access_channel_upsert_request(credential_body).is_err());

        let request = AppModelAccessChannelUpsertRequest {
            name: "Invalid official".to_owned(),
            kind: "official".to_owned(),
            base_url: "https://relay.example.test/v1".to_owned(),
            description: None,
            offerings: vec![
                AppModelAccessChannelOfferingUpsertRequest {
                    vendor_code: "openai".to_owned(),
                    vendor_name: "OpenAI".to_owned(),
                    model_ids: vec!["gpt-5".to_owned()],
                    models: Vec::new(),
                },
                AppModelAccessChannelOfferingUpsertRequest {
                    vendor_code: "anthropic".to_owned(),
                    vendor_name: "Anthropic".to_owned(),
                    model_ids: vec!["claude-opus".to_owned()],
                    models: Vec::new(),
                },
            ],
            default_vendor_code: "openai".to_owned(),
            default_model_id: "gpt-5".to_owned(),
            supported_agent_provider_ids: Vec::new(),
        };
        assert!(normalize_access_channel_upsert("official.invalid".to_owned(), request).is_err());
    }

    #[test]
    fn relay_upsert_supports_multiple_vendors_and_defaults_to_all_agent_providers() {
        let request = AppModelAccessChannelUpsertRequest {
            name: "Team Relay".to_owned(),
            kind: "relay".to_owned(),
            base_url: "https://relay.example.test/v1".to_owned(),
            description: Some("Shared route".to_owned()),
            offerings: vec![
                AppModelAccessChannelOfferingUpsertRequest {
                    vendor_code: "openai".to_owned(),
                    vendor_name: "OpenAI".to_owned(),
                    model_ids: vec!["gpt-5".to_owned()],
                    models: Vec::new(),
                },
                AppModelAccessChannelOfferingUpsertRequest {
                    vendor_code: "anthropic".to_owned(),
                    vendor_name: "Anthropic".to_owned(),
                    model_ids: vec!["claude-opus".to_owned()],
                    models: Vec::new(),
                },
            ],
            default_vendor_code: "anthropic".to_owned(),
            default_model_id: "claude-opus".to_owned(),
            supported_agent_provider_ids: Vec::new(),
        };
        let normalized = normalize_access_channel_upsert("relay.team".to_owned(), request)
            .expect("valid relay configuration");
        assert_eq!(normalized.offerings.len(), 2);
        assert_eq!(normalized.default_vendor_code, "anthropic");
        assert_eq!(
            normalized.supported_agent_provider_ids,
            canonical_agent_provider_ids()
        );
    }
}
