use std::future::Future;
use std::pin::Pin;

use crate::DomainResult;

pub type AdminModelCommandFuture<'a, T> =
    Pin<Box<dyn Future<Output = DomainResult<T>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdminModelSubject {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub operator_id: i64,
    pub operator_type: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminModelVendorItem {
    pub id: i64,
    pub uuid: String,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub vendor_code: String,
    pub name: String,
    pub status: String,
    pub color: String,
    pub description: String,
    pub supported_protocols: String,
    pub client_api_compatibility: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminAiModelItem {
    pub id: i64,
    pub uuid: String,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub vendor_id: String,
    pub vendor_code: String,
    pub vendor_name: String,
    pub catalog_key: String,
    pub model: String,
    pub display_name: String,
    pub name: String,
    pub model_type: String,
    /// Catalog model family code (`seedance`, `kling`, ...).
    pub family_code: Option<String>,
    /// Brand color token the catalog publishes for the model.
    pub color_token: Option<String>,
    /// Vendor documentation URL the catalog records.
    pub docs_url: Option<String>,
    pub region_prices: Vec<AdminAiModelRegionPriceCommand>,
    pub status: String,
    pub calls: String,
    pub description: Option<String>,
    pub modalities: Vec<String>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub api_format: Option<String>,
    pub capability_intro: Option<String>,
    pub limitations: Vec<String>,
    pub supported_languages: Vec<String>,
    pub use_cases: Vec<String>,
    pub training_data_cutoff: Option<String>,
    pub context_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_json_schema: bool,
    pub usage_scopes: Vec<String>,
    pub coding_visible: bool,
    pub release_stage: Option<i32>,
    pub shelf_state: Option<i32>,
    pub routing_state: Option<i32>,
    pub replacement_model: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminModelCatalogSyncItem {
    pub synced: bool,
    pub source: String,
    pub mode: String,
    pub dry_run: bool,
    pub catalog_version: String,
    pub requested_catalog_version: Option<String>,
    pub catalog_root: Option<String>,
    pub vendor_codes: Vec<String>,
    pub source_hash: String,
    pub meter_count: usize,
    pub vendor_count: usize,
    pub family_count: usize,
    pub model_count: usize,
    pub capability_count: usize,
    pub price_count: usize,
    pub ranking_count: usize,
    pub voice_count: usize,
    pub voice_binding_count: usize,
    pub video_profile_count: usize,
    pub accepted_count: i64,
    pub snapshot_id: Option<String>,
    pub sync_run_id: Option<String>,
    pub vendors: Vec<AdminModelVendorItem>,
    pub models: Vec<AdminAiModelItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminModelMappingRuleBindingItem {
    pub id: i64,
    pub uuid: String,
    pub binding_type: String,
    pub binding_id: Option<i64>,
    pub binding_code: Option<String>,
    pub binding_name: Option<String>,
    pub sort_order: i32,
    pub enabled: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminModelMappingRuleMappingItem {
    pub id: i64,
    pub uuid: String,
    pub source_model: String,
    pub source_catalog_key: Option<String>,
    pub target_model: String,
    pub target_catalog_key: Option<String>,
    pub target_provider_model: Option<String>,
    pub target_provider_native_model: Option<String>,
    pub sort_order: i32,
    pub enabled: bool,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminModelMappingRuleItem {
    pub id: i64,
    pub uuid: String,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub binding_type: String,
    pub source_vendor_id: Option<i64>,
    pub source_vendor_code: Option<String>,
    pub target_vendor_id: Option<i64>,
    pub target_vendor_code: Option<String>,
    pub mapping_mode: String,
    pub match_type: String,
    pub enabled: bool,
    pub bindings: Vec<AdminModelMappingRuleBindingItem>,
    pub mapping_items: Vec<AdminModelMappingRuleMappingItem>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminModelMappingRuleBindingDraft {
    pub id: Option<i64>,
    pub binding_type: String,
    pub binding_id: Option<i64>,
    pub binding_code: Option<String>,
    pub binding_name: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminModelMappingRuleItemDraft {
    pub id: Option<i64>,
    pub source_model: String,
    pub source_catalog_key: Option<String>,
    pub target_model: String,
    pub target_catalog_key: Option<String>,
    pub target_provider_model: Option<String>,
    pub target_provider_native_model: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminModelMappingRuleDraft {
    pub source_vendor_id: Option<i64>,
    pub source_vendor_code: String,
    pub target_vendor_id: Option<i64>,
    pub target_vendor_code: String,
    pub mapping_mode: String,
    pub match_type: String,
    pub enabled: bool,
    pub bindings: Vec<AdminModelMappingRuleBindingDraft>,
    pub mapping_items: Vec<AdminModelMappingRuleItemDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminModelMappingRulePatch {
    pub source_vendor_id: Option<Option<i64>>,
    pub source_vendor_code: Option<String>,
    pub target_vendor_id: Option<Option<i64>>,
    pub target_vendor_code: Option<String>,
    pub mapping_mode: Option<String>,
    pub match_type: Option<String>,
    pub enabled: Option<bool>,
    pub bindings: Option<Vec<AdminModelMappingRuleBindingDraft>>,
    pub mapping_items: Option<Vec<AdminModelMappingRuleItemDraft>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListAdminModelVendorsQuery {
    pub subject: AdminModelSubject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAdminAiModelsQuery {
    pub subject: AdminModelSubject,
    pub vendor_id: Option<String>,
    pub vendor_codes: Vec<String>,
    pub q: Option<String>,
    pub model_types: Option<String>,
    pub status: Option<String>,
    pub release_stages: Vec<i32>,
    pub shelf_state: Option<i32>,
    pub routing_state: Option<i32>,
    pub modalities: Vec<String>,
    pub page_size: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminAiModelListPage {
    pub items: Vec<AdminAiModelItem>,
    pub total_count: i64,
}

impl ListAdminAiModelsQuery {
    pub const DEFAULT_LIMIT: i64 = 50;
    pub const MAX_LIMIT: i64 = 200;

    pub fn normalized_limit(&self) -> i64 {
        self.page_size
            .unwrap_or(Self::DEFAULT_LIMIT)
            .clamp(1, Self::MAX_LIMIT)
    }

    pub fn normalized_offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListAdminModelMappingsQuery {
    pub subject: AdminModelSubject,
    pub binding_type: Option<String>,
    pub vendor_code: Option<String>,
    pub account_id: Option<i64>,
    pub account_code: Option<String>,
    pub q: Option<String>,
    pub page_size: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminModelMappingListPage {
    pub items: Vec<AdminModelMappingRuleItem>,
    pub total_count: i64,
}

impl ListAdminModelMappingsQuery {
    pub const DEFAULT_LIMIT: i64 = 20;
    pub const MAX_LIMIT: i64 = 200;

    pub fn normalized_limit(&self) -> i64 {
        self.page_size
            .unwrap_or(Self::DEFAULT_LIMIT)
            .clamp(1, Self::MAX_LIMIT)
    }

    pub fn normalized_offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateAdminModelVendorCommand {
    pub subject: AdminModelSubject,
    pub vendor_uuid: String,
    pub audit_log_uuid: String,
    pub vendor_code: String,
    pub name: String,
    pub status: String,
    pub color: String,
    pub description: String,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminAiModelRegionPriceCommand {
    pub region_code: String,
    pub currency: String,
    pub price_in: String,
    pub price_out: String,
    pub cache_read_price: Option<String>,
    pub cache_write_price: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateAdminAiModelCommand {
    pub subject: AdminModelSubject,
    pub model_uuid: String,
    pub capability_uuid: String,
    pub audit_log_uuid: String,
    pub vendor_id: String,
    pub model: String,
    pub display_name: String,
    pub model_type: String,
    pub region_prices: Vec<AdminAiModelRegionPriceCommand>,
    pub description: Option<String>,
    pub modalities: Vec<String>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub api_format: String,
    pub capability_intro: Option<String>,
    pub limitations: Vec<String>,
    pub supported_languages: Vec<String>,
    pub use_cases: Vec<String>,
    pub training_data_cutoff: Option<String>,
    pub context_tokens: i64,
    pub max_output_tokens: Option<i64>,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_json_schema: bool,
    pub usage_scopes: Vec<String>,
    pub coding_visible: bool,
    pub release_stage: i32,
    pub shelf_state: i32,
    pub routing_state: i32,
    pub replacement_model: Option<String>,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAdminAiModelCommand {
    pub subject: AdminModelSubject,
    pub capability_uuid: String,
    pub audit_log_uuid: String,
    pub model_id: String,
    pub vendor_id: Option<String>,
    pub model: Option<String>,
    pub display_name: Option<Option<String>>,
    pub model_type: Option<String>,
    pub region_prices: Option<Vec<AdminAiModelRegionPriceCommand>>,
    pub status: Option<String>,
    pub description: Option<Option<String>>,
    pub modalities: Option<Vec<String>>,
    pub input_modalities: Option<Vec<String>>,
    pub output_modalities: Option<Vec<String>>,
    pub api_format: Option<String>,
    pub capability_intro: Option<Option<String>>,
    pub limitations: Option<Vec<String>>,
    pub supported_languages: Option<Vec<String>>,
    pub use_cases: Option<Vec<String>>,
    pub training_data_cutoff: Option<Option<String>>,
    pub context_tokens: Option<i64>,
    pub max_output_tokens: Option<Option<i64>>,
    pub supports_streaming: Option<bool>,
    pub supports_tools: Option<bool>,
    pub supports_json_schema: Option<bool>,
    pub usage_scopes: Option<Vec<String>>,
    pub coding_visible: Option<bool>,
    pub release_stage: Option<i32>,
    pub shelf_state: Option<i32>,
    pub routing_state: Option<i32>,
    pub replacement_model: Option<Option<String>>,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncAdminModelCatalogCommand {
    pub subject: AdminModelSubject,
    pub snapshot_uuid: String,
    pub audit_log_uuid: String,
    pub source: String,
    pub mode: String,
    pub vendor_codes: Vec<String>,
    pub force: bool,
    pub catalog_root: Option<String>,
    pub catalog_version: Option<String>,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteAdminAiModelCommand {
    pub subject: AdminModelSubject,
    pub audit_log_uuid: String,
    pub model_id: String,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateAdminModelMappingCommand {
    pub subject: AdminModelSubject,
    pub mapping_uuid: String,
    pub binding_uuids: Vec<String>,
    pub item_uuids: Vec<String>,
    pub audit_log_uuid: String,
    pub draft: AdminModelMappingRuleDraft,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateAdminModelMappingCommand {
    pub subject: AdminModelSubject,
    pub audit_log_uuid: String,
    pub mapping_id: String,
    pub binding_uuids: Vec<String>,
    pub item_uuids: Vec<String>,
    pub patch: AdminModelMappingRulePatch,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteAdminModelMappingCommand {
    pub subject: AdminModelSubject,
    pub audit_log_uuid: String,
    pub mapping_id: String,
    pub request_id: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveAdminModelMappingQuery {
    pub subject: AdminModelSubject,
    pub source_model: String,
    pub vendor_code: Option<String>,
    pub supplier_id: Option<i64>,
    pub supplier_code: Option<String>,
    pub endpoint_id: Option<i64>,
    pub endpoint_code: Option<String>,
    pub account_id: Option<i64>,
    pub account_code: Option<String>,
    pub account_group_id: Option<i64>,
    pub account_group_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveAdminModelMappingResult {
    pub source_model: String,
    pub target_model: String,
    pub target_catalog_key: Option<String>,
    pub target_vendor_code: Option<String>,
    pub target_provider_model: Option<String>,
    pub target_provider_native_model: Option<String>,
    pub matched: bool,
    pub matched_binding_type: Option<String>,
    pub rule: Option<AdminModelMappingRuleItem>,
}

pub trait ModelCatalogAdminStore {
    fn list_vendors<'a>(
        &'a self,
        query: ListAdminModelVendorsQuery,
    ) -> AdminModelCommandFuture<'a, Vec<AdminModelVendorItem>>;

    fn list_models<'a>(
        &'a self,
        query: ListAdminAiModelsQuery,
    ) -> AdminModelCommandFuture<'a, AdminAiModelListPage>;

    fn list_model_mappings<'a>(
        &'a self,
        query: ListAdminModelMappingsQuery,
    ) -> AdminModelCommandFuture<'a, AdminModelMappingListPage>;

    fn create_vendor<'a>(
        &'a self,
        command: CreateAdminModelVendorCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelVendorItem>;

    fn create_model<'a>(
        &'a self,
        command: CreateAdminAiModelCommand,
    ) -> AdminModelCommandFuture<'a, AdminAiModelItem>;

    fn create_model_mapping<'a>(
        &'a self,
        command: CreateAdminModelMappingCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelMappingRuleItem>;

    fn update_model<'a>(
        &'a self,
        command: UpdateAdminAiModelCommand,
    ) -> AdminModelCommandFuture<'a, AdminAiModelItem>;

    fn update_model_mapping<'a>(
        &'a self,
        command: UpdateAdminModelMappingCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelMappingRuleItem>;

    fn sync_catalog<'a>(
        &'a self,
        command: SyncAdminModelCatalogCommand,
    ) -> AdminModelCommandFuture<'a, AdminModelCatalogSyncItem>;

    fn delete_model<'a>(
        &'a self,
        command: DeleteAdminAiModelCommand,
    ) -> AdminModelCommandFuture<'a, bool>;

    fn delete_model_mapping<'a>(
        &'a self,
        command: DeleteAdminModelMappingCommand,
    ) -> AdminModelCommandFuture<'a, bool>;

    fn resolve_model_mapping<'a>(
        &'a self,
        query: ResolveAdminModelMappingQuery,
    ) -> AdminModelCommandFuture<'a, ResolveAdminModelMappingResult>;
}
