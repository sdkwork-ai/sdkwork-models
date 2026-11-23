use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogManifest {
    pub name: String,
    pub schema_version: String,
    pub catalog_version: String,
    pub generated_at: String,
    pub models_root: String,
    pub schemas_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BillingMeter {
    pub meter_code: String,
    pub display_name: String,
    pub description: Option<String>,
    pub modality: String,
    pub usage_type: Option<String>,
    pub billing_mode: Option<String>,
    pub default_unit: Option<String>,
    pub default_unit_size: String,
    pub quantity_precision: Option<i32>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolStandard {
    pub protocol_code: String,
    pub vendor_origin: String,
    pub display_name: String,
    pub family: String,
    pub docs_url: String,
    pub maturity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ClientApiCompatibility {
    pub client_api_code: String,
    pub display_name: String,
    pub support_status: String,
    #[serde(default)]
    pub protocol_codes: Vec<String>,
    #[serde(default)]
    pub api_codes: Vec<String>,
    #[serde(default)]
    pub resource_codes: Vec<String>,
    pub notes: String,
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VendorApiEndpoint {
    pub host: String,
    pub path_prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelVendor {
    pub vendor_code: String,
    pub region_code: String,
    pub display_name: String,
    pub legal_name: Option<String>,
    pub description: Option<String>,
    pub website_url: Option<String>,
    pub docs_url: Option<String>,
    pub country_region: Option<String>,
    pub vendor_type: String,
    pub market_scope: String,
    pub billing_currency: String,
    pub billing_jurisdiction: String,
    #[serde(default)]
    pub operating_regions: Vec<String>,
    #[serde(default)]
    pub model_families: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub supported_protocols: Vec<String>,
    #[serde(default)]
    pub api_endpoints: BTreeMap<String, VendorApiEndpoint>,
    pub client_api_compatibility: BTreeMap<String, ClientApiCompatibility>,
    pub open_source: Option<bool>,
    pub sort_order: Option<i32>,
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FamilyFile {
    pub vendor_code: String,
    pub region_code: String,
    #[serde(default)]
    pub families: Vec<ModelFamily>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelFamily {
    pub family_code: String,
    pub display_name: String,
    pub description: Option<String>,
    pub family_type: String,
    pub primary_modality: String,
    pub default_model: Option<String>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub catalog_key: String,
    pub model_id: String,
    pub display_name: String,
    pub vendor_code: String,
    pub region_code: String,
    pub vendor_name: Option<String>,
    pub family_code: String,
    pub primary_capability: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
    pub api_format: String,
    pub context_tokens: Option<i64>,
    pub max_input_tokens: Option<i64>,
    pub max_output_tokens: Option<i64>,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_tools: bool,
    #[serde(default)]
    pub supports_json_schema: bool,
    #[serde(default)]
    pub usage_scopes: Vec<String>,
    #[serde(default = "default_coding_visible")]
    pub coding_visible: bool,
    pub rank_score: Option<String>,
    pub lifecycle: String,
    pub release_stage: String,
    pub shelf_state: String,
    pub routing_state: String,
    pub replacement_model: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub strengths: Vec<String>,
    pub color_token: Option<String>,
    pub latency_p50_ms: Option<i32>,
    pub latency_p95_ms: Option<i32>,
    pub win_rate: Option<String>,
    pub trend_score: Option<String>,
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPricing {
    pub catalog_key: String,
    pub vendor_code: String,
    pub region_code: String,
    pub model_id: String,
    pub currency: String,
    #[serde(default)]
    pub prices: Vec<ModelPrice>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub price_id: String,
    #[serde(default)]
    pub rate_hash: String,
    #[serde(default)]
    pub price_book_code: String,
    #[serde(default)]
    pub product_code: String,
    #[serde(default)]
    pub operation_code: String,
    pub price_side: String,
    #[serde(default = "default_billability")]
    pub billability: String,
    #[serde(default = "default_charge_timing")]
    pub charge_timing: String,
    #[serde(default = "default_calculation_mode")]
    pub calculation_mode: String,
    pub pricing_scope: Option<String>,
    pub meter_code: String,
    #[serde(default = "default_quantity_aggregation")]
    pub quantity_aggregation: String,
    pub unit_size: String,
    pub unit_price: String,
    pub minimum_quantity: String,
    pub quantity_step: Option<String>,
    pub currency: Option<String>,
    pub effective_from: String,
    pub effective_to: Option<String>,
    #[serde(default = "default_pricing_priority")]
    pub priority: i32,
    #[serde(default = "default_rate_variant")]
    pub rate_variant: String,
    pub schedule: Option<PriceSchedule>,
    #[serde(default)]
    pub conditions: Vec<PriceRateCondition>,
    #[serde(default)]
    pub tiers: Vec<PriceRateTier>,
    pub formula: Option<PriceFormula>,
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceSchedule {
    pub time_zone: String,
    #[serde(default)]
    pub weekly_windows: Vec<WeeklyPriceWindow>,
    #[serde(default)]
    pub include_dates: Vec<String>,
    #[serde(default)]
    pub exclude_dates: Vec<String>,
    #[serde(default)]
    pub holiday_dates: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyPriceWindow {
    pub window_code: String,
    #[serde(default)]
    pub days_of_week: Vec<u8>,
    pub start_time: String,
    pub end_time: String,
    pub end_day_offset: u8,
    #[serde(default)]
    pub holiday_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceRateCondition {
    pub dimension_code: String,
    pub operator: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceRateTier {
    pub tier_code: String,
    pub lower_bound: String,
    pub upper_bound: Option<String>,
    pub unit_size: String,
    pub unit_price: String,
    #[serde(default = "default_zero_decimal")]
    pub flat_amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceFormula {
    pub formula_code: String,
    pub formula_version: String,
    #[serde(default = "default_zero_decimal")]
    pub constant_units: String,
    #[serde(default = "default_one_decimal")]
    pub quantity_coefficient: String,
    pub minimum_units: Option<String>,
    pub maximum_units: Option<String>,
    #[serde(default)]
    pub terms: Vec<PriceFormulaTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PriceFormulaTerm {
    pub term_code: String,
    pub dimension_code: String,
    pub coefficient: String,
}

fn default_billability() -> String {
    "unknown".to_owned()
}

fn default_charge_timing() -> String {
    "usage_reported".to_owned()
}

fn default_calculation_mode() -> String {
    "per_unit".to_owned()
}

fn default_quantity_aggregation() -> String {
    "sum".to_owned()
}

fn default_pricing_priority() -> i32 {
    100
}

fn default_rate_variant() -> String {
    "standard".to_owned()
}

fn default_zero_decimal() -> String {
    "0".to_owned()
}

fn default_one_decimal() -> String {
    "1".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceEvidence {
    pub source_url: String,
    pub observed_at: String,
    pub source_hash: Option<String>,
}

fn default_coding_visible() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankingFile {
    pub vendor_code: String,
    pub region_code: String,
    #[serde(default)]
    pub snapshots: Vec<RankingSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TtsVoice {
    pub schema_version: String,
    pub voice_id: String,
    pub display_name: String,
    pub vendor_code: String,
    pub region_code: String,
    pub catalog_key: String,
    pub primary_locale: String,
    #[serde(default)]
    pub supported_locales: Vec<String>,
    pub gender: String,
    pub voice_kind: String,
    pub provisioning_mode: String,
    pub wire_parameter: String,
    pub vendor_voice_namespace: Option<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    pub preview_audio_url: Option<String>,
    pub vendor_list_endpoint: Option<String>,
    pub description: Option<String>,
    pub lifecycle: String,
    pub release_stage: String,
    pub shelf_state: String,
    pub routing_state: String,
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelVoiceBinding {
    pub voice_key: String,
    pub voice_id: String,
    #[serde(default)]
    pub is_default: bool,
    pub compatibility: String,
    #[serde(default)]
    pub sort_order: Option<i32>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelVoiceBindingsFile {
    pub schema_version: String,
    pub vendor_code: String,
    pub region_code: String,
    pub model_id: String,
    pub catalog_key: String,
    #[serde(default)]
    pub bindings: Vec<ModelVoiceBinding>,
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VideoGenerationProfile {
    pub profile_code: String,
    pub catalog_key: String,
    pub display_name: String,
    pub generation_mode: String,
    pub duration_policy: String,
    pub duration_seconds: Option<i32>,
    #[serde(default)]
    pub duration_options: Vec<i32>,
    pub duration_tier_code: Option<String>,
    #[serde(default)]
    pub duration_tier_codes: Vec<String>,
    pub min_duration_seconds: Option<i32>,
    pub max_duration_seconds: Option<i32>,
    pub duration_step_seconds: Option<i32>,
    pub resolution: String,
    pub resolution_tier_code: Option<String>,
    #[serde(default)]
    pub aspect_ratios: Vec<String>,
    pub output_audio: Option<bool>,
    #[serde(default)]
    pub is_default: bool,
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub pricing_tier_codes: Vec<String>,
    #[serde(default)]
    pub wire_parameters: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelVideoProfilesFile {
    pub schema_version: String,
    pub vendor_code: String,
    pub region_code: String,
    pub model_id: String,
    pub catalog_key: String,
    #[serde(default)]
    pub profiles: Vec<VideoGenerationProfile>,
    pub source: SourceEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIndex {
    pub schema_version: String,
    pub catalog_version: String,
    #[serde(default)]
    pub vendors: Vec<CatalogIndexVendor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIndexVendor {
    pub vendor_code: String,
    pub region_code: String,
    pub path: String,
    pub families_path: String,
    #[serde(default)]
    pub model_files: Vec<String>,
    #[serde(default)]
    pub pricing_files: Vec<String>,
    pub voices_path: Option<String>,
    pub model_voices_path: Option<String>,
    #[serde(default)]
    pub model_voice_files: Vec<String>,
    pub model_video_profiles_path: Option<String>,
    #[serde(default)]
    pub model_video_profile_files: Vec<String>,
    pub rankings_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankingSnapshot {
    pub snapshot_date: String,
    pub snapshot_period: Option<String>,
    pub rank_scope: String,
    #[serde(default)]
    pub items: Vec<RankingItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankingItem {
    pub model_id: String,
    pub rank_no: i32,
    pub previous_rank_no: Option<i32>,
    pub pricing_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VendorCatalog {
    pub vendor_code: String,
    pub region_code: String,
    pub vendor: ModelVendor,
    pub families: Vec<ModelFamily>,
    pub models: Vec<ModelInfo>,
    pub pricing: Vec<ModelPricing>,
    pub rankings: Vec<RankingSnapshot>,
    #[serde(default)]
    pub voices: Vec<TtsVoice>,
    #[serde(default)]
    pub model_voice_bindings: Vec<ModelVoiceBindingsFile>,
    #[serde(default)]
    pub model_video_profiles: Vec<ModelVideoProfilesFile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelVendorIdentity {
    pub vendor_code: String,
    pub display_name: String,
    pub legal_name: Option<String>,
    pub vendor_type: String,
    pub capabilities: Vec<String>,
    pub supported_protocols: Vec<String>,
    pub client_api_compatibility: BTreeMap<String, ClientApiCompatibility>,
    pub open_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorRegionRef {
    pub vendor_code: String,
    pub region_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub manifest: CatalogManifest,
    pub meters: Vec<BillingMeter>,
    pub protocols: Vec<ProtocolStandard>,
    pub vendors: Vec<VendorCatalog>,
}
