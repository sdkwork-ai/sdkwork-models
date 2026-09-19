use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use sha2::{Digest, Sha256};

use sdkwork_models::{ClientApiCompatibility, ModelCatalog, ModelInfo, TtsVoice, VendorApiEndpoint, VendorCatalog};

use sdkwork_models_contract_service::{
    AdminAiModelItem, AdminAiModelRegionPriceCommand, AdminModelSubject, AdminModelVendorItem,
};

pub(crate) const SYSTEM_TENANT_ID: i64 = 0;
pub(crate) const SYSTEM_ORGANIZATION_ID: i64 = 0;
pub(crate) const SYSTEM_DATA_SCOPE: i32 = 1;
pub(crate) const ACTIVE_STATUS: i32 = 1;
pub(crate) const INACTIVE_STATUS: i32 = 0;
pub(crate) const SYNC_MODE_DRY_RUN: &str = "dry_run";
const AI_RESOURCE_DESCRIPTION_MAX_CHARS: usize = 512;

pub(crate) fn pricing_catalog_key(vendor_code: &str, model_id: &str) -> String {
    model_catalog_key(vendor_code, model_id)
}

pub(crate) fn model_catalog_key(vendor_code: &str, model_id: &str) -> String {
    format!("{vendor_code}/{model_id}")
}

pub(crate) fn voice_catalog_key(vendor_code: &str, voice_id: &str) -> String {
    format!("{vendor_code}/{voice_id}")
}

pub(crate) fn catalog_identity_models(
    catalog: &ModelCatalog,
) -> BTreeMap<String, (&VendorCatalog, &ModelInfo)> {
    let mut models: BTreeMap<String, (&VendorCatalog, &ModelInfo)> = BTreeMap::new();
    for vendor in &catalog.vendors {
        for model in &vendor.models {
            let key = model_catalog_key(&model.vendor_code, &model.model_id);
            let replace = models
                .get(&key)
                .map(|(existing_vendor, existing_model)| {
                    model_identity_score(vendor, model)
                        > model_identity_score(existing_vendor, existing_model)
                })
                .unwrap_or(true);
            if replace {
                models.insert(key, (vendor, model));
            }
        }
    }
    models
}

pub fn public_catalog_identity_models(
    catalog: &ModelCatalog,
) -> BTreeMap<String, (&VendorCatalog, &ModelInfo)> {
    catalog_identity_models(catalog)
        .into_iter()
        .filter(|(_, (_, model))| sdkwork_model_is_publicly_active(model))
        .collect()
}

fn model_identity_score(vendor: &VendorCatalog, model: &ModelInfo) -> i32 {
    let has_region_pricing = vendor
        .pricing
        .iter()
        .any(|pricing| pricing.model_id == model.model_id && !pricing.prices.is_empty());
    let mut score = 0;
    if has_region_pricing {
        score += 100;
    }
    if model.routing_state == "enabled" {
        score += 40;
    }
    if model.shelf_state == "listed" {
        score += 20;
    }
    if model.release_stage == "active" {
        score += 10;
    }
    if matches!(model.lifecycle.as_str(), "current" | "preview") {
        score += 5;
    }
    if vendor.region_code == "global" {
        score += 1;
    }
    score
}

pub(crate) fn sdkwork_model_is_publicly_active(model: &ModelInfo) -> bool {
    matches!(model.release_stage.as_str(), "active" | "preview")
        && model.shelf_state == "listed"
        && model.routing_state == "enabled"
        && !matches!(
            model.lifecycle.as_str(),
            "deprecated" | "catalog_only" | "retired"
        )
}

pub(crate) fn sdkwork_voice_is_publicly_active(voice: &TtsVoice) -> bool {
    matches!(voice.release_stage.as_str(), "active" | "preview")
        && voice.shelf_state == "listed"
        && voice.routing_state == "enabled"
        && !matches!(
            voice.lifecycle.as_str(),
            "deprecated" | "catalog_only" | "retired"
        )
}

pub(crate) fn voice_catalog_status(voice: &TtsVoice) -> i32 {
    if sdkwork_voice_is_publicly_active(voice) {
        ACTIVE_STATUS
    } else {
        INACTIVE_STATUS
    }
}

pub(crate) fn catalog_model_status(model: &ModelInfo) -> i32 {
    if sdkwork_model_is_publicly_active(model) {
        ACTIVE_STATUS
    } else {
        INACTIVE_STATUS
    }
}

#[derive(Debug)]
pub(crate) enum CatalogImportError {
    Catalog(sdkwork_models::CatalogError),
    CatalogVersionMismatch { expected: String, actual: String },
    UnknownVendors(Vec<String>),
}

impl Display for CatalogImportError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "{error}"),
            Self::CatalogVersionMismatch { expected, actual } => write!(
                formatter,
                "sdkwork-models catalog version mismatch: expected {expected}, loaded {actual}"
            ),
            Self::UnknownVendors(vendors) => {
                write!(
                    formatter,
                    "sdkwork-models catalog does not define vendor(s): {}",
                    vendors.join(", ")
                )
            }
        }
    }
}

impl Error for CatalogImportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Catalog(error) => Some(error),
            Self::CatalogVersionMismatch { .. } | Self::UnknownVendors(_) => None,
        }
    }
}

impl From<sdkwork_models::CatalogError> for CatalogImportError {
    fn from(value: sdkwork_models::CatalogError) -> Self {
        Self::Catalog(value)
    }
}

pub(crate) fn load_catalog_root_with_pin(
    catalog_root: Option<&str>,
    catalog_version: Option<&str>,
) -> Result<ModelCatalog, CatalogImportError> {
    let catalog = match catalog_root
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(root) => sdkwork_models::load_catalog(root)?,
        None => sdkwork_models::load_bundled_catalog()?,
    };
    validate_catalog_version_pin(&catalog, catalog_version)?;
    Ok(catalog)
}

pub(crate) fn catalog_with_selected_vendors(
    catalog: &ModelCatalog,
    vendor_codes: &[String],
) -> Result<ModelCatalog, CatalogImportError> {
    let requested = normalized_vendor_set(vendor_codes);
    if requested.is_empty() {
        return Ok(catalog.clone());
    }

    let available = catalog
        .vendors
        .iter()
        .map(|vendor| vendor.vendor.vendor_code.clone())
        .collect::<BTreeSet<_>>();
    let missing = requested
        .iter()
        .filter(|vendor_code| !available.contains(*vendor_code))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(CatalogImportError::UnknownVendors(missing));
    }

    Ok(ModelCatalog {
        manifest: catalog.manifest.clone(),
        meters: catalog.meters.clone(),
        protocols: catalog.protocols.clone(),
        vendors: catalog
            .vendors
            .iter()
            .filter(|vendor| requested.contains(&vendor.vendor.vendor_code))
            .cloned()
            .collect(),
    })
}

pub(crate) fn catalog_scope_vendor_codes(catalog: &ModelCatalog) -> Vec<String> {
    catalog_vendor_records(catalog)
        .into_iter()
        .map(|vendor| vendor.vendor_code)
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CatalogVendorRecord {
    pub vendor_code: String,
    pub display_name: String,
    pub legal_name: Option<String>,
    pub description: Option<String>,
    pub website_url: Option<String>,
    pub docs_url: Option<String>,
    pub country_region: Option<String>,
    pub vendor_type: String,
    pub model_families: Vec<String>,
    pub capabilities: Vec<String>,
    pub supported_protocols: Vec<String>,
    pub api_endpoints: BTreeMap<String, VendorApiEndpoint>,
    pub client_api_compatibility: BTreeMap<String, ClientApiCompatibility>,
    pub open_source: bool,
    pub sort_order: i32,
    pub source_url: String,
}

pub(crate) fn catalog_vendor_records(catalog: &ModelCatalog) -> Vec<CatalogVendorRecord> {
    let mut vendors = BTreeMap::<String, CatalogVendorRecord>::new();
    for region_catalog in &catalog.vendors {
        let vendor = &region_catalog.vendor;
        let record = vendors
            .entry(vendor.vendor_code.clone())
            .or_insert_with(|| CatalogVendorRecord {
                vendor_code: vendor.vendor_code.clone(),
                display_name: vendor.display_name.clone(),
                legal_name: vendor.legal_name.clone(),
                description: vendor.description.clone(),
                website_url: vendor.website_url.clone(),
                docs_url: vendor.docs_url.clone(),
                country_region: vendor.country_region.clone(),
                vendor_type: vendor.vendor_type.clone(),
                model_families: Vec::new(),
                capabilities: Vec::new(),
                supported_protocols: Vec::new(),
                api_endpoints: BTreeMap::new(),
                client_api_compatibility: BTreeMap::new(),
                open_source: vendor.open_source.unwrap_or(false),
                sort_order: vendor.sort_order.unwrap_or(1_000_000),
                source_url: vendor.source.source_url.clone(),
            });
        append_unique(&mut record.model_families, vendor.model_families.iter());
        append_unique(&mut record.capabilities, vendor.capabilities.iter());
        append_unique(
            &mut record.supported_protocols,
            vendor.supported_protocols.iter(),
        );
        for (family, endpoint) in &vendor.api_endpoints {
            record
                .api_endpoints
                .entry(family.clone())
                .or_insert_with(|| endpoint.clone());
        }
        for (client_api_code, compatibility) in &vendor.client_api_compatibility {
            match record.client_api_compatibility.get(client_api_code) {
                Some(existing)
                    if client_api_support_rank(&existing.support_status)
                        >= client_api_support_rank(&compatibility.support_status) => {}
                _ => {
                    record
                        .client_api_compatibility
                        .insert(client_api_code.clone(), compatibility.clone());
                }
            }
        }
    }
    vendors.into_values().collect()
}

fn append_unique<'a>(target: &mut Vec<String>, values: impl IntoIterator<Item = &'a String>) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn client_api_support_rank(value: &str) -> i32 {
    match value {
        "supported" => 3,
        "compatible" => 2,
        "unsupported" => 1,
        _ => 0,
    }
}

pub(crate) fn catalog_scope_model_count(catalog: &ModelCatalog) -> usize {
    catalog_identity_models(catalog).len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CatalogScopeCounts {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogAuthorityKeys {
    pub vendor_codes: Vec<String>,
    pub catalog_keys: Vec<String>,
    pub family_uuids: Vec<String>,
    pub capability_uuids: Vec<String>,
    pub price_uuids: Vec<String>,
    pub ranking_uuids: Vec<String>,
    pub voice_uuids: Vec<String>,
    pub voice_binding_uuids: Vec<String>,
    pub video_profile_uuids: Vec<String>,
    pub vendor_modality_uuids: Vec<String>,
    pub vendor_api_endpoint_uuids: Vec<String>,
    pub model_modality_uuids: Vec<String>,
    pub model_api_endpoint_uuids: Vec<String>,
    pub ai_resource_codes: Vec<String>,
}

impl CatalogScopeCounts {
    pub fn accepted_count(self) -> i64 {
        (self.meter_count
            + self.vendor_count
            + self.family_count
            + self.model_count
            + self.capability_count
            + self.price_count
            + self.ranking_count
            + self.voice_count
            + self.voice_binding_count
            + self.video_profile_count) as i64
    }
}

pub(crate) fn catalog_scope_counts(catalog: &ModelCatalog) -> CatalogScopeCounts {
    let identity_models = public_catalog_identity_models(catalog);
    let model_catalog_keys = identity_models.keys().cloned().collect::<BTreeSet<_>>();
    let capability_count = identity_models
        .values()
        .flat_map(|(_, model)| {
            let capabilities = if model.capabilities.is_empty() {
                vec![model.primary_capability.clone()]
            } else {
                model.capabilities.clone()
            };
            capabilities.into_iter().map(move |capability| {
                model_catalog_key(&model.vendor_code, &model.model_id) + "/" + &capability
            })
        })
        .collect::<BTreeSet<_>>()
        .len();
    let public_model_keys = public_catalog_identity_models(catalog)
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let price_count = catalog
        .vendors
        .iter()
        .flat_map(|vendor| vendor.pricing.iter())
        .filter(|pricing| {
            public_model_keys.contains(&model_catalog_key(&pricing.vendor_code, &pricing.model_id))
        })
        .map(|pricing| pricing.prices.len())
        .sum();
    let ranking_count = catalog
        .vendors
        .iter()
        .flat_map(|vendor| {
            let vendor_code = vendor.vendor.vendor_code.as_str();
            vendor.rankings.iter().flat_map(move |snapshot| {
                snapshot.items.iter().map(move |item| {
                    (
                        model_catalog_key(vendor_code, &item.model_id),
                        pricing_catalog_key(vendor_code, &item.model_id),
                    )
                })
            })
        })
        .filter(|(model_catalog_key, _)| model_catalog_keys.contains(model_catalog_key))
        .count();
    let voice_count = catalog
        .vendors
        .iter()
        .map(|vendor| vendor.voices.len())
        .sum();
    let voice_binding_count = catalog
        .vendors
        .iter()
        .flat_map(|vendor| vendor.model_voice_bindings.iter())
        .map(|binding| binding.bindings.len())
        .sum();
    CatalogScopeCounts {
        meter_count: catalog.meters.len(),
        vendor_count: catalog_scope_vendor_codes(catalog).len(),
        family_count: catalog
            .vendors
            .iter()
            .flat_map(|vendor| {
                vendor.families.iter().map(|family| {
                    (
                        vendor.vendor.vendor_code.clone(),
                        family.family_code.clone(),
                    )
                })
            })
            .collect::<BTreeSet<_>>()
            .len(),
        model_count: catalog_scope_model_count(catalog),
        capability_count,
        price_count,
        ranking_count,
        voice_count,
        voice_binding_count,
        video_profile_count: catalog
            .vendors
            .iter()
            .flat_map(|vendor| vendor.model_video_profiles.iter())
            .map(|file| file.profiles.len())
            .sum(),
    }
}

pub(crate) fn catalog_authority_keys(catalog: &ModelCatalog) -> CatalogAuthorityKeys {
    let vendor_codes = catalog
        .vendors
        .iter()
        .map(|vendor| vendor.vendor.vendor_code.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let catalog_keys = catalog_identity_models(catalog)
        .keys()
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();
    let public_catalog_keys = public_catalog_identity_models(catalog)
        .keys()
        .cloned()
        .into_iter()
        .collect::<Vec<_>>();
    let model_catalog_key_set = public_catalog_keys.iter().cloned().collect::<BTreeSet<_>>();
    let family_uuids = catalog
        .vendors
        .iter()
        .flat_map(|vendor| {
            vendor.families.iter().map(|family| {
                stable_uuid(
                    "sdk-family",
                    &[&vendor.vendor.vendor_code, &family.family_code],
                )
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let capability_uuids = public_catalog_identity_models(catalog)
        .values()
        .flat_map(|(_, model)| {
            let capabilities = if model.capabilities.is_empty() {
                vec![model.primary_capability.clone()]
            } else {
                model.capabilities.clone()
            };
            capabilities.into_iter().map(move |capability| {
                stable_uuid(
                    "sdk-cap",
                    &[&model.vendor_code, &model.model_id, &capability],
                )
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let price_uuids = catalog
        .vendors
        .iter()
        .flat_map(|vendor| {
            vendor.pricing.iter().flat_map(|pricing| {
                pricing.prices.iter().map(|price| {
                    (
                        model_catalog_key(&pricing.vendor_code, &pricing.model_id),
                        stable_uuid(
                            "sdk-price",
                            &[
                                &pricing.vendor_code,
                                &pricing.region_code,
                                &pricing.model_id,
                                &price.price_id,
                            ],
                        ),
                    )
                })
            })
        })
        .filter_map(|(catalog_key, uuid)| {
            if model_catalog_key_set.contains(&catalog_key) {
                Some(uuid)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let ranking_uuids = catalog
        .vendors
        .iter()
        .flat_map(|vendor| {
            let vendor_code = vendor.vendor.vendor_code.clone();
            let region_code = vendor.vendor.region_code.clone();
            let model_catalog_key_set = model_catalog_key_set.clone();
            vendor.rankings.iter().flat_map(move |snapshot| {
                let vendor_code = vendor_code.clone();
                let region_code = region_code.clone();
                let model_catalog_key_set = model_catalog_key_set.clone();
                snapshot.items.iter().filter_map(move |item| {
                    let model_catalog_key = model_catalog_key(&vendor_code, &item.model_id);
                    if model_catalog_key_set.contains(&model_catalog_key) {
                        Some(stable_uuid(
                            "sdk-rank",
                            &[
                                &snapshot.snapshot_date,
                                &snapshot.rank_scope,
                                &vendor_code,
                                &region_code,
                                &item.model_id,
                            ],
                        ))
                    } else {
                        None
                    }
                })
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let voice_uuids = catalog
        .vendors
        .iter()
        .flat_map(|vendor| {
            vendor
                .voices
                .iter()
                .map(|voice| stable_uuid("sdk-voice", &[&voice.vendor_code, &voice.voice_id]))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let voice_binding_uuids = catalog
        .vendors
        .iter()
        .flat_map(|vendor| {
            vendor.model_voice_bindings.iter().flat_map(|binding_file| {
                binding_file.bindings.iter().map(|binding| {
                    stable_uuid(
                        "sdk-voice-bind",
                        &[&binding_file.catalog_key, &binding.voice_key],
                    )
                })
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let video_profile_uuids = catalog
        .vendors
        .iter()
        .flat_map(|vendor| {
            vendor.model_video_profiles.iter().flat_map(|profile_file| {
                profile_file.profiles.iter().map(|profile| {
                    stable_uuid(
                        "sdk-video-profile",
                        &[&profile_file.catalog_key, &profile.profile_code],
                    )
                })
            })
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    CatalogAuthorityKeys {
        vendor_codes,
        catalog_keys,
        family_uuids,
        capability_uuids,
        price_uuids,
        ranking_uuids,
        voice_uuids,
        voice_binding_uuids,
        video_profile_uuids,
        vendor_modality_uuids: catalog_vendor_modality_projections(catalog)
            .into_iter()
            .map(|item| item.uuid)
            .collect(),
        vendor_api_endpoint_uuids: catalog_vendor_api_endpoint_projections(catalog)
            .into_iter()
            .map(|item| item.uuid)
            .collect(),
        model_modality_uuids: catalog_model_modality_projections(catalog)
            .into_iter()
            .map(|item| item.uuid)
            .collect(),
        model_api_endpoint_uuids: catalog_model_api_endpoint_projections(catalog)
            .into_iter()
            .map(|item| item.uuid)
            .collect(),
        ai_resource_codes: catalog_ai_resource_projections(catalog)
            .into_iter()
            .map(|item| item.resource_code)
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogModalityProjection {
    pub uuid: String,
    pub modality_code: String,
    pub display_name: String,
    pub modality_group: String,
    pub description: String,
    pub input_supported: bool,
    pub output_supported: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogApiEndpointProjection {
    pub uuid: String,
    pub endpoint_code: String,
    pub protocol_code: String,
    pub display_name: String,
    pub method: String,
    pub path_template: String,
    pub streaming_supported: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogVendorModalityProjection {
    pub uuid: String,
    pub vendor_code: String,
    pub modality_code: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogVendorApiEndpointProjection {
    pub uuid: String,
    pub vendor_code: String,
    pub endpoint_code: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogModalityApiEndpointProjection {
    pub uuid: String,
    pub modality_code: String,
    pub endpoint_code: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogModelModalityProjection {
    pub uuid: String,
    pub catalog_key: String,
    pub model: String,
    pub vendor_code: String,
    pub modality_code: String,
    pub direction: String,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogModelApiEndpointProjection {
    pub uuid: String,
    pub catalog_key: String,
    pub model: String,
    pub vendor_code: String,
    pub endpoint_code: String,
    pub provider_native_model: String,
    pub default_parameters: String,
    pub supports_streaming: bool,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogAiResourceProjection {
    pub uuid: String,
    pub resource_code: String,
    pub resource_kind: String,
    pub display_name: String,
    pub vendor_code: Option<String>,
    pub modality_code: Option<String>,
    pub api_endpoint_code: Option<String>,
    pub catalog_key: Option<String>,
    pub model: Option<String>,
    pub provider_native_model: Option<String>,
    pub composition_mode: String,
    pub capability_schema: String,
    pub metadata_schema: String,
    pub description: Option<String>,
    pub sort_order: i32,
    /// Vendor compatibility API endpoints; only vendor-kind projections carry it.
    pub api_endpoints: Option<BTreeMap<String, VendorApiEndpoint>>,
}

pub(crate) fn catalog_modality_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogModalityProjection> {
    let mut usage = public_catalog_identity_models(catalog)
        .into_values()
        .map(|(_, model)| model)
        .fold(
            BTreeMap::<String, (bool, bool)>::new(),
            |mut usage, model| {
                for modality in &model.input_modalities {
                    let entry = usage.entry(modality.clone()).or_insert((false, false));
                    entry.0 = true;
                }
                for modality in &model.output_modalities {
                    let entry = usage.entry(modality.clone()).or_insert((false, false));
                    entry.1 = true;
                }
                usage
                    .entry(model.primary_capability.clone())
                    .or_insert((true, true));
                usage
            },
        );
    for meter in &catalog.meters {
        usage.entry(meter.modality.clone()).or_insert((true, true));
    }
    usage
        .into_iter()
        .enumerate()
        .map(
            |(index, (modality_code, (input_supported, output_supported)))| {
                CatalogModalityProjection {
                    uuid: stable_uuid("sdk-modality", &[&modality_code]),
                    display_name: modality_display_name(&modality_code),
                    modality_group: modality_group(&modality_code).to_owned(),
                    description: modality_description(&modality_code),
                    sort_order: modality_sort_order(&modality_code)
                        .unwrap_or((index as i32) + 1000),
                    modality_code,
                    input_supported,
                    output_supported,
                }
            },
        )
        .collect()
}

pub(crate) fn catalog_api_endpoint_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogApiEndpointProjection> {
    public_catalog_identity_models(catalog)
        .into_values()
        .map(|(_, model)| model)
        .map(model_endpoint_descriptor)
        .map(|descriptor| (descriptor.endpoint_code.to_owned(), descriptor))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .map(|descriptor| CatalogApiEndpointProjection {
            uuid: stable_uuid("sdk-api-endpoint", &[descriptor.endpoint_code]),
            endpoint_code: descriptor.endpoint_code.to_owned(),
            protocol_code: descriptor.protocol_code.to_owned(),
            display_name: descriptor.display_name.to_owned(),
            method: descriptor.method.to_owned(),
            path_template: descriptor.path_template.to_owned(),
            streaming_supported: descriptor.streaming_supported,
            sort_order: descriptor.sort_order,
        })
        .collect()
}

pub(crate) fn catalog_vendor_modality_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogVendorModalityProjection> {
    public_catalog_identity_models(catalog)
        .into_values()
        .flat_map(|(_, model)| {
            model_modality_codes(model)
                .into_iter()
                .map(move |modality_code| (model.vendor_code.clone(), modality_code))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(
            |(index, (vendor_code, modality_code))| CatalogVendorModalityProjection {
                uuid: stable_uuid("sdk-vendor-modality", &[&vendor_code, &modality_code]),
                vendor_code,
                modality_code,
                sort_order: (index as i32) + 1,
            },
        )
        .collect()
}

pub(crate) fn catalog_vendor_api_endpoint_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogVendorApiEndpointProjection> {
    public_catalog_identity_models(catalog)
        .into_values()
        .map(|(_, model)| {
            (
                model.vendor_code.clone(),
                model_endpoint_descriptor(model).endpoint_code.to_owned(),
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(
            |(index, (vendor_code, endpoint_code))| CatalogVendorApiEndpointProjection {
                uuid: stable_uuid("sdk-vendor-endpoint", &[&vendor_code, &endpoint_code]),
                vendor_code,
                endpoint_code,
                sort_order: (index as i32) + 1,
            },
        )
        .collect()
}

pub(crate) fn catalog_modality_api_endpoint_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogModalityApiEndpointProjection> {
    public_catalog_identity_models(catalog)
        .into_values()
        .map(|(_, model)| model)
        .flat_map(|model| {
            let endpoint_code = model_endpoint_descriptor(model).endpoint_code.to_owned();
            model_endpoint_modalities(model)
                .into_iter()
                .map(move |modality_code| (modality_code, endpoint_code.clone()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(
            |(index, (modality_code, endpoint_code))| CatalogModalityApiEndpointProjection {
                uuid: stable_uuid("sdk-modality-endpoint", &[&modality_code, &endpoint_code]),
                modality_code,
                endpoint_code,
                sort_order: (index as i32) + 1,
            },
        )
        .collect()
}

pub(crate) fn catalog_model_modality_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogModelModalityProjection> {
    public_catalog_identity_models(catalog)
        .into_iter()
        .flat_map(|(catalog_key, (_, model))| {
            model_modality_directions(model)
                .into_iter()
                .map(
                    move |(modality_code, direction)| CatalogModelModalityProjection {
                        uuid: stable_uuid(
                            "sdk-model-modality",
                            &[&catalog_key, &modality_code, &direction],
                        ),
                        catalog_key: catalog_key.clone(),
                        model: model.model_id.clone(),
                        vendor_code: model.vendor_code.clone(),
                        modality_code,
                        direction,
                        sort_order: 1,
                    },
                )
        })
        .collect()
}

pub(crate) fn catalog_model_api_endpoint_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogModelApiEndpointProjection> {
    public_catalog_identity_models(catalog)
        .into_iter()
        .enumerate()
        .map(|(index, (catalog_key, (_, model)))| {
            let endpoint = model_endpoint_descriptor(model);
            CatalogModelApiEndpointProjection {
                uuid: stable_uuid(
                    "sdk-model-endpoint",
                    &[&catalog_key, endpoint.endpoint_code],
                ),
                catalog_key,
                model: model.model_id.clone(),
                vendor_code: model.vendor_code.clone(),
                endpoint_code: endpoint.endpoint_code.to_owned(),
                provider_native_model: model.model_id.clone(),
                default_parameters: "{}".to_owned(),
                supports_streaming: model.supports_streaming,
                sort_order: (index as i32) + 1,
            }
        })
        .collect()
}

pub(crate) fn catalog_ai_resource_projections(
    catalog: &ModelCatalog,
) -> Vec<CatalogAiResourceProjection> {
    let mut resources = BTreeMap::<String, CatalogAiResourceProjection>::new();
    for (index, vendor) in catalog
        .vendors
        .iter()
        .map(|vendor| &vendor.vendor)
        .map(|vendor| (vendor.vendor_code.clone(), vendor))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .enumerate()
    {
        let resource_code = format!("vendor.{}", vendor.vendor_code);
        resources.insert(
            resource_code.clone(),
            CatalogAiResourceProjection {
                uuid: stable_uuid("sdk-cap-resource", &[&resource_code]),
                resource_code,
                resource_kind: "vendor".to_owned(),
                display_name: vendor.display_name.clone(),
                vendor_code: Some(vendor.vendor_code.clone()),
                modality_code: None,
                api_endpoint_code: None,
                catalog_key: None,
                model: None,
                provider_native_model: None,
                composition_mode: "single".to_owned(),
                capability_schema: "{}".to_owned(),
                metadata_schema: "{}".to_owned(),
                description: ai_resource_description(vendor.description.clone()),
                sort_order: (index as i32) + 1,
                api_endpoints: if vendor.api_endpoints.is_empty() {
                    None
                } else {
                    Some(vendor.api_endpoints.clone())
                },
            },
        );
    }

    for (index, modality) in catalog_modality_resource_projections(catalog)
        .into_iter()
        .enumerate()
    {
        let resource_code = format!("modality.{}", modality.modality_code);
        resources.insert(
            resource_code.clone(),
            CatalogAiResourceProjection {
                uuid: stable_uuid("sdk-cap-resource", &[&resource_code]),
                resource_code,
                resource_kind: "modality".to_owned(),
                display_name: modality.display_name,
                vendor_code: None,
                modality_code: Some(modality.modality_code),
                api_endpoint_code: None,
                catalog_key: None,
                model: None,
                provider_native_model: None,
                composition_mode: "single".to_owned(),
                capability_schema: serde_json::json!({
                    "modalityGroup": modality.modality_group,
                    "inputSupported": modality.input_supported,
                    "outputSupported": modality.output_supported
                })
                .to_string(),
                metadata_schema: "{}".to_owned(),
                description: ai_resource_description(Some(modality.description)),
                sort_order: 5_000 + modality.sort_order + (index as i32),
                api_endpoints: None,
            },
        );
    }

    for endpoint in catalog_api_endpoint_projections(catalog) {
        let resource_code = format!("api.{}", endpoint.endpoint_code);
        resources.insert(
            resource_code.clone(),
            CatalogAiResourceProjection {
                uuid: stable_uuid("sdk-cap-resource", &[&resource_code]),
                resource_code,
                resource_kind: "api_endpoint".to_owned(),
                display_name: endpoint.display_name,
                vendor_code: endpoint_vendor_code(&endpoint.endpoint_code),
                modality_code: endpoint_modality_code(&endpoint.endpoint_code),
                api_endpoint_code: Some(endpoint.endpoint_code),
                catalog_key: None,
                model: None,
                provider_native_model: None,
                composition_mode: "single".to_owned(),
                capability_schema: "{}".to_owned(),
                metadata_schema: "{}".to_owned(),
                description: ai_resource_description(Some(
                    "Model catalog API endpoint capability".to_owned(),
                )),
                sort_order: 10_000 + endpoint.sort_order,
                api_endpoints: None,
            },
        );
    }

    for (index, (catalog_key, (_, model))) in public_catalog_identity_models(catalog)
        .into_iter()
        .enumerate()
    {
        let endpoint = model_endpoint_descriptor(model);
        let modality_code = model_resource_suffix(model);
        let resource_code = format!(
            "model.{}.{}.{}",
            model.vendor_code, model.model_id, modality_code
        );
        resources.insert(
            resource_code.clone(),
            CatalogAiResourceProjection {
                uuid: stable_uuid("sdk-cap-resource", &[&resource_code]),
                resource_code,
                resource_kind: "model_api".to_owned(),
                display_name: if model.display_name.trim().is_empty() {
                    model.model_id.clone()
                } else {
                    model.display_name.clone()
                },
                vendor_code: Some(model.vendor_code.clone()),
                modality_code: Some(modality_code),
                api_endpoint_code: Some(endpoint.endpoint_code.to_owned()),
                catalog_key: Some(catalog_key),
                model: Some(model.model_id.clone()),
                provider_native_model: Some(model.model_id.clone()),
                composition_mode: "single".to_owned(),
                capability_schema: serde_json::json!({
                    "capability": &model.primary_capability,
                    "capabilities": if model.capabilities.is_empty() {
                        vec![model.primary_capability.clone()]
                    } else {
                        model.capabilities.clone()
                    },
                    "inputModalities": &model.input_modalities,
                    "outputModalities": &model.output_modalities,
                    "apiFormat": &model.api_format,
                    "supportsStreaming": model.supports_streaming,
                    "supportsTools": model.supports_tools,
                    "supportsJsonSchema": model.supports_json_schema
                })
                .to_string(),
                metadata_schema: "{}".to_owned(),
                description: ai_resource_description(model.description.clone()),
                sort_order: 20_000 + (index as i32) + 1,
                api_endpoints: None,
            },
        );
    }

    resources.into_values().collect()
}

fn ai_resource_description(description: Option<String>) -> Option<String> {
    description.map(|mut value| {
        if let Some((byte_index, _)) = value.char_indices().nth(AI_RESOURCE_DESCRIPTION_MAX_CHARS) {
            value.truncate(byte_index);
        }
        value
    })
}

fn catalog_modality_resource_projections(catalog: &ModelCatalog) -> Vec<CatalogModalityProjection> {
    let mut modalities = catalog_modality_projections(catalog);
    if modalities.iter().any(|modality| {
        matches!(
            modality.modality_code.as_str(),
            "chat" | "text" | "embedding" | "rerank"
        )
    }) && !modalities
        .iter()
        .any(|modality| modality.modality_code == "llm")
    {
        modalities.push(CatalogModalityProjection {
            uuid: stable_uuid("sdk-modality", &["llm"]),
            modality_code: "llm".to_owned(),
            display_name: modality_display_name("llm"),
            modality_group: modality_group("llm").to_owned(),
            input_supported: true,
            output_supported: true,
            description: modality_description("llm"),
            sort_order: modality_sort_order("llm").unwrap_or(5),
        });
    }
    modalities
}

pub(crate) fn model_resource_suffix(model: &ModelInfo) -> String {
    if model.primary_capability == "chat" {
        "chat".to_owned()
    } else {
        model.primary_capability.clone()
    }
}

#[derive(Debug, Clone, Copy)]
struct EndpointDescriptor {
    endpoint_code: &'static str,
    protocol_code: &'static str,
    display_name: &'static str,
    method: &'static str,
    path_template: &'static str,
    streaming_supported: bool,
    sort_order: i32,
}

/// The vendor-native image endpoint of a vendor, when the vendor publishes one.
///
/// `primaryCapability` alone cannot choose an image endpoint: `image` is carried
/// by 11 catalog vendors whose HTTP surfaces are **not** interchangeable. Before
/// this table existed every one of the 49 bound image models landed on
/// `openai.images` — including the 42 whose own `apiFormat` says
/// `vendor_native` — so an image request for, say, `google/gemini-3-pro-image`
/// was planned on an OpenAI-compatible endpoint even though the catalog declares
/// `google_gemini` and the cloud router's classifier
/// (`provider_native_classifier.rs`) would replay it to
/// `/v1beta/models/{model}:generateImages`.
///
/// Keys are the *catalog* `vendorCode` (the model's own `vendorCode`), which is
/// not always the account-side/endpoint-side vendor name — `google` publishes
/// `gemini.*` endpoints, `bytedance` publishes `jimeng.*`, `kuaishou` publishes
/// `kling.*`. Vendors absent from this table keep the OpenAI-compatible
/// `openai.images` surface, which is the honest answer for them: the catalog
/// declares no native image API for those vendors.
///
/// This function is duplicated in the cloud router's own copy of
/// `model_catalog_import`. Keep both copies in step: this one *writes* the rows
/// and the router's copy *reads* them for `catalog_expectations`.
fn vendor_native_image_descriptor(vendor_code: &str) -> Option<EndpointDescriptor> {
    let descriptor = match vendor_code {
        // `google` is the catalog vendor code; its native image surface is the
        // Gemini `:generateImages` action.
        "google" | "gemini" => EndpointDescriptor {
            endpoint_code: "gemini.image_generation",
            protocol_code: "vendor_native",
            display_name: "Gemini Image Generation",
            method: "POST",
            path_template: "/v1beta/models/{model}:generateImages",
            streaming_supported: false,
            sort_order: 440,
        },
        "bytedance" | "jimeng" => EndpointDescriptor {
            endpoint_code: "jimeng.image_generation",
            protocol_code: "vendor_native",
            display_name: "Jimeng Image Generation",
            method: "POST",
            path_template: "/v1/images/generations",
            streaming_supported: false,
            sort_order: 510,
        },
        "kuaishou" | "kling" => EndpointDescriptor {
            endpoint_code: "kling.image_generation",
            protocol_code: "vendor_native",
            display_name: "Kling Image Generation",
            method: "POST",
            path_template: "/v1/images/generations",
            streaming_supported: false,
            sort_order: 490,
        },
        "volcengine" => EndpointDescriptor {
            endpoint_code: "volcengine.image_generation",
            protocol_code: "vendor_native",
            display_name: "Volcengine Image Generation",
            method: "POST",
            path_template: "/api/v3/images/generations",
            streaming_supported: false,
            sort_order: 540,
        },
        "vidu" => EndpointDescriptor {
            endpoint_code: "vidu.reference_to_image",
            protocol_code: "vendor_native",
            display_name: "Vidu Reference To Image",
            method: "POST",
            path_template: "/ent/v2/reference2image",
            streaming_supported: false,
            sort_order: 580,
        },
        // FLUX publishes one path per model (`POST /v1/{model}`), then hands back
        // a `polling_url`; `GET /v1/get_result?id=` is the documented result
        // route.
        "black_forest_labs" | "bfl" => EndpointDescriptor {
            endpoint_code: "black_forest_labs.image_generation",
            protocol_code: "vendor_native",
            display_name: "FLUX Image Generation",
            method: "POST",
            path_template: "/v1/flux-{model}",
            streaming_supported: false,
            sort_order: 600,
        },
        // Runway's Text/Image-to-Image surface: one endpoint carrying every
        // image model, discriminated by the body's `model` field.
        "runway" | "runwayml" => EndpointDescriptor {
            endpoint_code: "runway.image_generation",
            protocol_code: "vendor_native",
            display_name: "Runway Text/Image To Image",
            method: "POST",
            path_template: "/v1/text_to_image",
            streaming_supported: false,
            sort_order: 610,
        },
        // Stability distinguishes the model by the last path segment
        // (`core` / `ultra` / `sd3`) rather than by a body field.
        "stability_ai" | "stability" => EndpointDescriptor {
            endpoint_code: "stability_ai.image_generation",
            protocol_code: "vendor_native",
            display_name: "Stability Image Generation",
            method: "POST",
            path_template: "/v2beta/stable-image/generate/{mode}",
            streaming_supported: false,
            sort_order: 620,
        },
        _ => return None,
    };
    Some(descriptor)
}

/// The vendor-native image endpoint for a model, honouring both the model's
/// declared `apiFormat` and its catalog vendor code.
///
/// `apiFormat = "google_gemini"` is itself the native Gemini surface, so a
/// Gemini image model must land on `gemini.image_generation` even when its
/// vendor code were to drift. A `vendor_native` image model only moves off
/// `openai.images` when its vendor publishes a native image endpoint; otherwise
/// the call would be planned against a route the vendor never registered.
fn model_image_endpoint_descriptor(model: &ModelInfo) -> EndpointDescriptor {
    let native = vendor_native_image_descriptor(model.vendor_code.trim());
    if model.api_format == "google_gemini" {
        return native.unwrap_or_else(|| EndpointDescriptor {
            endpoint_code: "gemini.image_generation",
            protocol_code: "vendor_native",
            display_name: "Gemini Image Generation",
            method: "POST",
            path_template: "/v1beta/models/{model}:generateImages",
            streaming_supported: false,
            sort_order: 440,
        });
    }
    match native {
        // A native endpoint exists *and* the model claims a native surface.
        Some(descriptor) if model.api_format == "vendor_native" => descriptor,
        // A native endpoint exists but the model is declared OpenAI-compatible:
        // the model's own declaration wins, because the request body it will
        // receive is the OpenAI one.
        _ => EndpointDescriptor {
            endpoint_code: "openai.images",
            protocol_code: "openai_compatible",
            display_name: "OpenAI Images",
            method: "POST",
            path_template: "/v1/images/generations",
            streaming_supported: false,
            sort_order: 30,
        },
    }
}

/// The vendor-native video endpoint of a vendor, when the vendor publishes one.
///
/// Same defect as the image arm, one capability later. `primaryCapability =
/// "video"` is carried by 13 catalog vendors whose HTTP surfaces are not
/// interchangeable, yet every one of the 60 active video bindings landed on the
/// single generic `openai.video` (`POST /v1/videos`) — including the 55 whose
/// own `apiFormat` says `vendor_native`. Meanwhile the bundled catalog already
/// declared nine vendor-native video endpoints
/// (`ai_resource.api_code` / `data/ai-routing/resources/vendor-native-resources.json`)
/// and `provider_native_classifier.rs` already routed every one of their paths:
/// they simply had **zero** `ai_model_api_endpoint` rows, so no model could ever
/// reach them.
///
/// Keys are the *catalog* `vendorCode`, which is not the endpoint-side name —
/// `bytedance` publishes `jimeng.*`, `kuaishou` publishes `kling.*`, `google`
/// publishes `gemini.*`.
///
/// Vendors absent from this table keep the OpenAI-compatible `openai.video`
/// surface, which is the honest answer for them: the catalog declares no native
/// video API for those vendors. This function is duplicated in the cloud router
/// (`sdkwork-cloudrouter-router-service`'s `model_catalog_import`); keep both
/// copies in step, because this one *writes* the rows the router *reads*.
fn vendor_native_video_descriptor(vendor_code: &str) -> Option<EndpointDescriptor> {
    let descriptor = match vendor_code {
        // Gemini's `:generateVideos` action (Veo family).
        "google" | "gemini" => EndpointDescriptor {
            endpoint_code: "gemini.video_generation",
            protocol_code: "vendor_native",
            display_name: "Gemini Video Generation",
            method: "POST",
            path_template: "/v1beta/models/{model}:generateVideos",
            streaming_supported: false,
            sort_order: 460,
        },
        // Kling's text-to-video surface. Its sibling operations
        // (`image2video` / `avatar` / `motion-control`) stay addressable through
        // the classifier but are not what a *video* model binds to; a video
        // model's entry point is text-to-video.
        "kuaishou" | "kling" => EndpointDescriptor {
            endpoint_code: "kling.text_to_video",
            protocol_code: "vendor_native",
            display_name: "Kling Text To Video",
            method: "POST",
            path_template: "/v1/videos/text2video",
            streaming_supported: false,
            sort_order: 470,
        },
        "bytedance" | "jimeng" => EndpointDescriptor {
            endpoint_code: "jimeng.video_generation",
            protocol_code: "vendor_native",
            display_name: "Jimeng Video Generation",
            method: "POST",
            path_template: "/v1/videos/generations",
            streaming_supported: false,
            sort_order: 520,
        },
        // Volcengine Ark submits an async task and polls it.
        "volcengine" => EndpointDescriptor {
            endpoint_code: "volcengine.video_generation",
            protocol_code: "vendor_native",
            display_name: "Volcengine Video Generation",
            method: "POST",
            path_template: "/api/v3/contents/generations/tasks",
            streaming_supported: false,
            sort_order: 550,
        },
        "vidu" => EndpointDescriptor {
            endpoint_code: "vidu.start_end_to_video",
            protocol_code: "vendor_native",
            display_name: "Vidu Start-End To Video",
            method: "POST",
            path_template: "/ent/v2/start-end2video",
            streaming_supported: false,
            sort_order: 590,
        },
        // The four vendors below each publish a video surface the catalog
        // actually declares and whose models actually declare
        // `apiFormat = vendor_native`, yet they had no arm here — so their
        // native declaration was silently discarded and every model was bound
        // to `openai.video` (`POST /v1/videos`), a route their vendor never
        // registered. The paths are the ones the project already ships in
        // `data/ai-routing/resources/vendor-native-resources.json`, not new
        // claims: `alibaba.video_generation` (`wan2.6-t2v`/`i2v`/`r2v`),
        // `luma_ai.video_generation` (Ray family), `pixverse.video_generation`
        // and `zhipu.video_generation`.
        //
        // These are `vendor_native`-only on purpose: each vendor's *other*
        // models (`alibaba` chat/embedding/image, `zhipu` chat/embedding/image)
        // declare `openai_compatible` and stay on the generic face, so the arm
        // is gated on `api_format == "vendor_native"` by
        // `model_video_endpoint_descriptor` and cannot capture them.
        "alibaba" => EndpointDescriptor {
            endpoint_code: "alibaba.video_generation",
            protocol_code: "vendor_native",
            display_name: "Alibaba Video Generation",
            method: "POST",
            path_template: "/api/v1/services/aigc/video-generation/video-synthesis",
            streaming_supported: false,
            sort_order: 630,
        },
        "luma_ai" => EndpointDescriptor {
            endpoint_code: "luma_ai.video_generation",
            protocol_code: "vendor_native",
            display_name: "Luma Video Generation",
            method: "POST",
            path_template: "/dream-machine/v1/generations",
            streaming_supported: false,
            sort_order: 640,
        },
        "pixverse" => EndpointDescriptor {
            endpoint_code: "pixverse.video_generation",
            protocol_code: "vendor_native",
            display_name: "PixVerse Video Generation",
            method: "POST",
            path_template: "/openapi/v2/video/text/generate",
            streaming_supported: false,
            sort_order: 650,
        },
        "zhipu" => EndpointDescriptor {
            endpoint_code: "zhipu.video_generation",
            protocol_code: "vendor_native",
            display_name: "Zhipu Video Generation",
            method: "POST",
            path_template: "/api/paas/v4/videos/generations",
            streaming_supported: false,
            sort_order: 660,
        },
        _ => return None,
    };
    Some(descriptor)
}

/// The vendor-native video endpoint for a model, honouring both the model's
/// declared `apiFormat` and its catalog vendor code.
///
/// `apiFormat = "google_gemini"` is itself the native Gemini surface, so a
/// Gemini video model (Veo) must land on `gemini.video_generation` even when its
/// vendor code were to drift. A `vendor_native` video model only moves off
/// `openai.video` when its vendor publishes a native video endpoint; otherwise
/// the call would be planned against a route the vendor never registered.
fn model_video_endpoint_descriptor(model: &ModelInfo) -> EndpointDescriptor {
    let native = vendor_native_video_descriptor(model.vendor_code.trim());
    if model.api_format == "google_gemini" {
        return native.unwrap_or_else(|| EndpointDescriptor {
            endpoint_code: "gemini.video_generation",
            protocol_code: "vendor_native",
            display_name: "Gemini Video Generation",
            method: "POST",
            path_template: "/v1beta/models/{model}:generateVideos",
            streaming_supported: false,
            sort_order: 460,
        });
    }
    match native {
        Some(descriptor) if model.api_format == "vendor_native" => descriptor,
        _ => EndpointDescriptor {
            endpoint_code: "openai.video",
            protocol_code: "openai_compatible",
            display_name: "Video Generation",
            method: "POST",
            path_template: "/v1/videos",
            streaming_supported: false,
            sort_order: 60,
        },
    }
}

/// The vendor-native audio endpoint of a vendor, when the vendor publishes one.
///
/// Same defect as the image arm and the video arm, one capability later.
/// `primaryCapability = "audio"` is carried by 46 catalog models across 6
/// vendors whose HTTP surfaces are not interchangeable, yet **every one of the
/// 40 bound audio models landed on the single generic `openai.audio`**
/// (`POST /v1/audio`) — including the 14 whose own `apiFormat` says
/// `vendor_native` (`bytedance` ×2, `elevenlabs` ×8, `minimax` ×6).
///
/// Consequences that are not cosmetic:
///
/// * `elevenlabs` text-to-speech replayed an OpenAI audio body to
///   `/v1/audio`, but the classifier only recognises
///   `/v1/text-to-speech/{voice_id}` for that vendor — so an ElevenLabs TTS
///   call could never be classified onto `elevenlabs.text_to_speech` and
///   reached no declared route at all.
/// * `volcengine.speech` (`/api/v3/audio/speech`) is declared as a resource,
///   routed by the classifier, granted by `official.volcengine.full` — and had
///   **zero** models bound to it.
///
/// The table below is deliberately restricted to endpoints that are already
/// wired **end to end**: present in
/// `data/ai-routing/resources/vendor-native-resources.json`, classified by
/// `provider_native_api_code_from_standard_path` (both copies), and granted by
/// an `official.<vendor>.full` resource group. Inventing a path for a vendor
/// whose native surface the project has not declared would create a route with
/// no resource, no grant and no classifier arm — the exact "declared but
/// unreachable" shape this audit exists to eliminate. So `minimax`,
/// `bytedance` and `xiaomi` stay on `openai.audio`: for them that is the honest
/// answer, because the catalog declares no native audio endpoint for those
/// vendors (they publish OpenAI-compatible audio surfaces).
///
/// Keys are the *catalog* `vendorCode`, which is not the endpoint-side name
/// (`bytedance` publishes `jimeng.*` for media, `kuaishou` publishes
/// `kling.*`). Keep both copies of this function in step (this one *writes* the
/// rows; the cloud router's copy *reads* them for `catalog_expectations`).
fn vendor_native_audio_descriptor(vendor_code: &str) -> Option<EndpointDescriptor> {
    let descriptor = match vendor_code {
        // ElevenLabs' speech synthesis surface. Transcription models
        // (`scribe_*`) are excluded before this table is consulted — see
        // `model_audio_endpoint_descriptor`.
        "elevenlabs" => EndpointDescriptor {
            endpoint_code: "elevenlabs.text_to_speech",
            protocol_code: "vendor_native",
            display_name: "ElevenLabs Text To Speech",
            method: "POST",
            path_template: "/v1/text-to-speech/{voice_id}",
            streaming_supported: true,
            sort_order: 410,
        },
        // Volcengine Ark's speech surface.
        "volcengine" => EndpointDescriptor {
            endpoint_code: "volcengine.speech",
            protocol_code: "vendor_native",
            display_name: "Volcengine Speech",
            method: "POST",
            path_template: "/api/v3/audio/speech",
            streaming_supported: true,
            sort_order: 420,
        },
        _ => return None,
    };
    Some(descriptor)
}

/// The vendor-native audio endpoint for a model, honouring both the model's
/// declared `apiFormat` and its catalog vendor code.
///
/// `apiFormat = "google_gemini"` is itself the native Gemini surface. Google's
/// audio models split into two kinds: the live/translate family, which the
/// classifier routes to `gemini.live` (`/v1beta/live/sessions`), and the plain
/// TTS family, for which the catalog publishes no native speech endpoint — so
/// the TTS family keeps `openai.audio` rather than being pointed at the live
/// session route it cannot speak.
///
/// **Transcription models are a deliberate exception.** `scribe_v2`,
/// `scribe_v2_medical`, `scribe_v2_realtime` and the Gemini `*-transcribe*`
/// models are `primaryCapability = "audio"` with `apiFormat = vendor_native`,
/// but they take audio *input* and emit text; the speech (synthesis) natives do
/// not answer them and the project declares no native transcription route. They
/// keep `openai.audio`, which the vendors' OpenAI-compatible surface does
/// answer.
fn model_audio_endpoint_descriptor(model: &ModelInfo) -> EndpointDescriptor {
    let openai_compatible_audio = || EndpointDescriptor {
        endpoint_code: "openai.audio",
        protocol_code: "openai_compatible",
        display_name: "OpenAI Audio",
        method: "POST",
        path_template: "/v1/audio",
        streaming_supported: true,
        sort_order: 40,
    };

    // `apiFormat = "google_gemini"` is itself the native Gemini surface. Of
    // Google's audio models only the live/translate family has a declared
    // native route (`gemini.live`, audio-in/audio-out); the plain TTS family has
    // none, so it keeps `openai.audio` rather than being pointed at a live
    // session route it cannot speak.
    if model.api_format == "google_gemini"
        && model.input_modalities.iter().any(|m| m == "audio")
        && model.output_modalities.iter().any(|m| m == "audio")
    {
        return EndpointDescriptor {
            endpoint_code: "gemini.live",
            protocol_code: "vendor_native",
            display_name: "Gemini Live Session",
            method: "POST",
            path_template: "/v1beta/live/sessions",
            streaming_supported: true,
            sort_order: 415,
        };
    }

    // A transcription model has audio *input* and text output; the speech
    // (synthesis) natives do not answer it.
    let is_transcription = model.output_modalities.iter().any(|m| m == "text")
        && model.input_modalities.iter().any(|m| m == "audio");
    if is_transcription {
        return openai_compatible_audio();
    }

    match vendor_native_audio_descriptor(model.vendor_code.trim()) {
        // A native endpoint exists *and* the model claims a native surface.
        Some(descriptor) if model.api_format == "vendor_native" => descriptor,
        // A native endpoint exists but the model is declared OpenAI-compatible:
        // the model's own declaration wins, because the request body it will
        // receive is the OpenAI one.
        _ => openai_compatible_audio(),
    }
}

/// The sound-effect endpoint for a model (音效).
///
/// Four sfx vendors (`kuaishou`, `stability_ai`, `vidu`, and the generic case)
/// each answer a different path, and all of them resolve onto the one
/// `sfx.sound` route so the capability has a single reachable endpoint code.
///
/// **`elevenlabs` is the exception.** Its classifier arms deliberately omit the
/// sfx routes: `/v1/sound-generation` classifies to
/// `elevenlabs.sound_generation`, which is the more specific code and must win.
/// Binding its sfx model to the generic `sfx.sound` would plan the call against
/// `/v1/sound/generate` — a path ElevenLabs answers with `elevenlabs.*` codes,
/// not `sfx.sound` — so the binding could never be honoured.
fn model_sfx_endpoint_descriptor(model: &ModelInfo) -> EndpointDescriptor {
    if model.api_format == "vendor_native" && model.vendor_code.trim() == "elevenlabs" {
        return EndpointDescriptor {
            endpoint_code: "elevenlabs.sound_generation",
            protocol_code: "vendor_native",
            display_name: "ElevenLabs Sound Generation",
            method: "POST",
            path_template: "/v1/sound-generation",
            streaming_supported: false,
            sort_order: 415,
        };
    }
    EndpointDescriptor {
        endpoint_code: "sfx.sound",
        protocol_code: "vendor_native",
        display_name: "Sound Effects",
        method: "POST",
        path_template: "/v1/sound/generate",
        streaming_supported: false,
        sort_order: 55,
    }
}

/// The music endpoint for a model.
///
/// Same defect as the image, video and audio arms, one capability later:
/// `primaryCapability = "music"` was the only input, so **all 15 active music
/// bindings** — `elevenlabs/music_*`, `google/lyria-*`, `mureka/*`,
/// `stability_ai/stable-audio-*`, `bytedance/seed-music-gensong-v4`,
/// `minimax/music-cover` — collapsed onto `suno.music`, a route owned by a
/// vendor none of them are.
///
/// **`suno.music` is a compatibility surface, not a vendor endpoint.** Its
/// resource is `api.suno.music`, display name "Music Generation
/// (Suno-protocol)", and it is granted by `api.openai_compatible.all` (the
/// compat group) with `defaultBillingMeter = music_output_second` — same shape
/// as `openai.audio` / `openai.videos`. So models whose vendor publishes no
/// native music API legitimately stay there, which is the honest answer for
/// `elevenlabs`, `google`, `mureka`, `stability_ai` and `bytedance`.
///
/// The one vendor with a declared native music endpoint is `minimax`
/// (`minimax.music_generation`, `/v1/music/generations`, granted by
/// `official.minimax.music`) — and it had **zero** models bound. `suno` itself
/// is deliberately parked in the catalog (`lifecycle = catalog_only` /
/// `deprecated`, `shelfState = hidden`, `routingState = catalog_only`), so its
/// models are retired and bind nothing.
fn model_music_endpoint_descriptor(model: &ModelInfo) -> EndpointDescriptor {
    let suno_compatible_music = || EndpointDescriptor {
        endpoint_code: "suno.music",
        protocol_code: "vendor_native",
        display_name: "Suno Music",
        method: "POST",
        path_template: "/v1/music",
        streaming_supported: false,
        sort_order: 50,
    };

    // MiniMax publishes its own music surface; a model declaring
    // `vendor_native` must reach it rather than the Suno compatibility face.
    if model.api_format == "vendor_native" && model.vendor_code.trim() == "minimax" {
        return EndpointDescriptor {
            endpoint_code: "minimax.music_generation",
            protocol_code: "vendor_native",
            display_name: "MiniMax Music Generation",
            method: "POST",
            path_template: "/v1/music/generations",
            streaming_supported: false,
            sort_order: 430,
        };
    }

    // Mureka publishes its own song surface. Same defect as the MiniMax arm
    // one branch up: all 8 `mureka` music models declare
    // `apiFormat = vendor_native` and the project declares
    // `mureka.music_generation` (`POST /v1/song/generate`), but with no arm
    // here every one of them collapsed onto the Suno compatibility face — a
    // route that speaks Suno's song protocol, not Mureka's.
    if model.api_format == "vendor_native" && model.vendor_code.trim() == "mureka" {
        return EndpointDescriptor {
            endpoint_code: "mureka.music_generation",
            protocol_code: "vendor_native",
            display_name: "Mureka Music Generation",
            method: "POST",
            path_template: "/v1/song/generate",
            streaming_supported: false,
            sort_order: 435,
        };
    }

    suno_compatible_music()
}

/// The vendor-native chat endpoint of a vendor, when the vendor publishes one
/// that the catalog actually commits to.
///
/// Chat is the fallback face: `model_endpoint_descriptor`'s catch-all arm
/// returns `openai.chat_completions` for `llm` / `chat` / `code` / `reasoning`
/// and for any capability it does not recognise. That is correct for the
/// overwhelming majority of vendors, whose chat models declare
/// `apiFormat = openai_compatible` and whose request bodies therefore really are
/// OpenAI-shaped.
///
/// `baidu` is the exception the sweep found. Its four chat models and one
/// reasoning model (`ernie-5.0`, `ernie-5.1`, `ernie-4.5-turbo-128k`,
/// `ernie-x1.1`, `ernie-5.0-thinking-preview`) declare
/// `apiFormat = vendor_native`, and the project declares
/// `baidu.chat_completions` at `POST /v2/chat/completions` — the 千帆 v2
/// surface, which is **not** `/v1/chat/completions`. Binding them to the
/// generic face would replay an OpenAI body to a path Baidu does not serve and
/// discard the models' own declaration.
///
/// Deliberately a one-vendor table. Adding a vendor here is a claim that its
/// models *declare* `vendor_native`; every other vendor's chat models say
/// `openai_compatible` and must keep the generic fallback, which is why the
/// sweep in `every_capability_binds_only_to_a_declared_vendor_native_endpoint`
/// asserts the generic face is still reached at all.
fn vendor_native_chat_descriptor(vendor_code: &str) -> Option<EndpointDescriptor> {
    let descriptor = match vendor_code {
        "baidu" => EndpointDescriptor {
            endpoint_code: "baidu.chat_completions",
            protocol_code: "vendor_native",
            display_name: "Baidu Chat Completions",
            method: "POST",
            path_template: "/v2/chat/completions",
            streaming_supported: true,
            sort_order: 420,
        },
        _ => return None,
    };
    Some(descriptor)
}

fn model_endpoint_descriptor(model: &ModelInfo) -> EndpointDescriptor {
    match model.primary_capability.as_str() {
        "image" => model_image_endpoint_descriptor(model),
        "audio" => model_audio_endpoint_descriptor(model),
        "music" => model_music_endpoint_descriptor(model),
        "video" => model_video_endpoint_descriptor(model),
        // Sound effects.
        //
        // Without this arm `primaryCapability = "sfx"` falls through to the
        // generic chat branch below and every sfx model is bound to
        // `openai.chat_completions` — so a sound-effect request is replayed to
        // the vendor's *chat* surface and can never produce audio. The catalog
        // declares 10 sfx models across 4 vendors (`elevenlabs`, `kuaishou`,
        // `stability_ai`, `vidu`); this arm gives them their own endpoint so the
        // vendor-native classifier can reach `sound.generate`.
        //
        // This function is duplicated in the cloud router
        // (`sdkwork-cloudrouter-router-service`'s `model_catalog_import`). The
        // two are independent: this one projects the rows the router later
        // *reads*, so a capability missing from only this copy makes the router
        // report `ai_vendor_api_endpoint is missing N bundled key(s)` and stall
        // the bootstrap at `UpgradeRequired` — the router cannot heal a
        // projection it does not own.
        "sfx" => model_sfx_endpoint_descriptor(model),
        "embedding" => EndpointDescriptor {
            endpoint_code: "openai.embeddings",
            protocol_code: "openai_compatible",
            display_name: "OpenAI Embeddings",
            method: "POST",
            path_template: "/v1/embeddings",
            streaming_supported: false,
            sort_order: 20,
        },
        "rerank" => EndpointDescriptor {
            endpoint_code: "rerank",
            protocol_code: "vendor_native",
            display_name: "Rerank",
            method: "POST",
            path_template: "/v1/rerank",
            streaming_supported: false,
            sort_order: 70,
        },
        _ if model.api_format == "vendor_native" => vendor_native_chat_descriptor(
            model.vendor_code.trim(),
        )
        .unwrap_or(EndpointDescriptor {
            endpoint_code: "openai.chat_completions",
            protocol_code: "openai_compatible",
            display_name: "OpenAI Chat Completions",
            method: "POST",
            path_template: "/v1/chat/completions",
            streaming_supported: model.supports_streaming,
            sort_order: 10,
        }),
        _ if model.api_format == "openai_responses" => EndpointDescriptor {
            endpoint_code: "openai.chat_completions",
            protocol_code: "openai_compatible",
            display_name: "OpenAI Chat Completions",
            method: "POST",
            path_template: "/v1/chat/completions",
            streaming_supported: model.supports_streaming,
            sort_order: 10,
        },
        _ => EndpointDescriptor {
            endpoint_code: "openai.chat_completions",
            protocol_code: "openai_compatible",
            display_name: "OpenAI Chat Completions",
            method: "POST",
            path_template: "/v1/chat/completions",
            streaming_supported: model.supports_streaming,
            sort_order: 10,
        },
    }
}

fn model_modality_codes(model: &ModelInfo) -> BTreeSet<String> {
    model
        .input_modalities
        .iter()
        .chain(model.output_modalities.iter())
        .chain(std::iter::once(&model.primary_capability))
        .filter_map(|value| {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_owned())
            }
        })
        .collect()
}

fn model_endpoint_modalities(model: &ModelInfo) -> BTreeSet<String> {
    model
        .input_modalities
        .iter()
        .chain(model.output_modalities.iter())
        .chain(std::iter::once(&model.primary_capability))
        .filter_map(|value| {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_owned())
            }
        })
        .collect()
}

fn model_modality_directions(model: &ModelInfo) -> Vec<(String, String)> {
    let input = model
        .input_modalities
        .iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    let output = model
        .output_modalities
        .iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>();
    input
        .union(&output)
        .map(|modality_code| {
            let direction = match (
                input.contains(modality_code),
                output.contains(modality_code),
            ) {
                (true, true) => "input_output",
                (true, false) => "input",
                (false, true) => "output",
                (false, false) => "input_output",
            };
            (modality_code.clone(), direction.to_owned())
        })
        .collect()
}

fn endpoint_vendor_code(endpoint_code: &str) -> Option<String> {
    endpoint_code
        .split_once('.')
        .map(|(vendor_code, _)| vendor_code.to_owned())
        .filter(|vendor_code| vendor_code != "rerank")
}

fn endpoint_modality_code(endpoint_code: &str) -> Option<String> {
    match endpoint_code {
        "openai.images" => Some("image"),
        "openai.audio" => Some("audio"),
        "suno.music" => Some("music"),
        "openai.video" => Some("video"),
        "openai.embeddings" => Some("embedding"),
        "rerank" => Some("rerank"),
        "openai.chat_completions" => Some("chat"),
        // Vendor-native endpoints whose code does not spell out the modality.
        // Without these the endpoint is still importable but the
        // `ai_modality_api_endpoint` projection loses the link, so the modality
        // view of the catalog under-reports which endpoints serve it.
        "gemini.image_generation" | "gemini.nano_banana.image_generation" => Some("image"),
        "jimeng.image_generation" | "kling.image_generation" | "volcengine.image_generation"
        | "vidu.reference_to_image" | "black_forest_labs.image_generation"
        | "runway.image_generation" | "stability_ai.image_generation" => Some("image"),
        "gemini.video_generation" | "jimeng.video_generation" | "volcengine.video_generation"
        | "kling.text_to_video" | "kling.image_to_video" | "kling.avatar"
        | "kling.motion_control" | "vidu.start_end_to_video" | "vidu.motion_sync" => Some("video"),
        "minimax.music_generation" | "suno.music_generation" => Some("music"),
        "elevenlabs.text_to_speech" | "volcengine.speech" => Some("audio"),
        "elevenlabs.sound_generation" | "sfx.sound" => Some("audio"),
        // Gemini's live/translate session surface is audio-in/audio-out.
        "gemini.live" => Some("audio"),
        "gemini.generate_content" | "gemini.stream_generate_content" | "anthropic.messages"
        | "anthropic.claude_code" => Some("chat"),
        "gemini.embed_content" => Some("embedding"),
        // The thirteen vendors that had no `api.*` entitlement at all. Their
        // endpoints are declared in `vendor-native-resources.json` and every
        // one of them must resolve to a modality, otherwise the
        // `ai_modality_api_endpoint` projection loses the link and the modality
        // view of the catalog under-reports which endpoints serve it. The code
        // spells the modality out for all but the two obvious cases, but the
        // map is explicit on purpose — it is the accounting side's only way to
        // pick a meter, and a `None` here is a silent billing degradation.
        "alibaba.image_generation" | "xai.image_generation" | "xiaomi.image_generation"
        | "zhipu.image_generation" => Some("image"),
        "alibaba.video_generation" | "luma_ai.video_generation" | "pixverse.video_generation"
        | "xai.video_generation" | "xiaomi.video_generation" | "zhipu.video_generation" => {
            Some("video")
        }
        "mureka.music_generation" => Some("music"),
        "xiaomi.speech" => Some("audio"),
        "alibaba.chat_completions" | "baidu.chat_completions" | "deepseek.chat_completions"
        | "meituan.chat_completions" | "moonshot.chat_completions" | "stepfun.chat_completions"
        | "tencent.chat_completions" | "xai.chat_completions" | "xiaomi.chat_completions"
        | "zhipu.chat_completions" => Some("chat"),
        "alibaba.embeddings" | "zhipu.embeddings" => Some("embedding"),
        _ => None,
    }
    .map(str::to_owned)
}

fn modality_display_name(modality_code: &str) -> String {
    match modality_code {
        "llm" => "LLM",
        "text" => "Text",
        "chat" => "Chat",
        "image" => "Image",
        "audio" => "Audio",
        "music" => "Music",
        "video" => "Video",
        "embedding" => "Embedding",
        "rerank" => "Rerank",
        "tool" => "Tool",
        "storage" => "Storage",
        "network" => "Network",
        value => value,
    }
    .to_owned()
}

fn modality_group(modality_code: &str) -> &'static str {
    match modality_code {
        "llm" | "chat" | "text" | "embedding" | "rerank" => "language",
        "image" | "video" => "visual",
        "audio" | "music" => "audio",
        "tool" | "storage" | "network" => "tooling",
        _ => "custom",
    }
}

fn modality_description(modality_code: &str) -> String {
    format!("SDKWork model catalog {modality_code} modality")
}

fn modality_sort_order(modality_code: &str) -> Option<i32> {
    match modality_code {
        "llm" => Some(5),
        "chat" => Some(10),
        "text" => Some(20),
        "embedding" => Some(30),
        "image" => Some(40),
        "audio" => Some(50),
        "music" => Some(60),
        "video" => Some(70),
        "rerank" => Some(80),
        "tool" => Some(90),
        "storage" => Some(100),
        "network" => Some(110),
        _ => None,
    }
}

pub(crate) fn is_dry_run_mode(mode: &str) -> bool {
    mode == SYNC_MODE_DRY_RUN
}

pub(crate) fn catalog_preview_admin_items(
    catalog: &ModelCatalog,
    subject: AdminModelSubject,
) -> (Vec<AdminModelVendorItem>, Vec<AdminAiModelItem>) {
    let vendors = catalog_vendor_records(catalog)
        .into_iter()
        .map(|vendor| AdminModelVendorItem {
            id: 0,
            uuid: stable_uuid("sdk-vendor-preview", &[&vendor.vendor_code]),
            tenant_id: subject.tenant_id,
            organization_id: subject.organization_id,
            vendor_code: vendor.vendor_code,
            name: vendor.display_name,
            status: "active".to_owned(),
            color: "bg-slate-700".to_owned(),
            description: vendor.description.unwrap_or_default(),
            supported_protocols: json_array(&vendor.supported_protocols),
            client_api_compatibility: serde_json::to_string(&vendor.client_api_compatibility)
                .unwrap_or_else(|_| "{}".to_owned()),
            deleted_at: None,
        })
        .map(|item| (item.vendor_code.clone(), item))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .enumerate()
        .map(|(index, mut item)| {
            item.id = (index as i64) + 1;
            item
        })
        .collect::<Vec<_>>();
    let models = public_catalog_identity_models(catalog)
        .into_iter()
        .map(|(catalog_key, (vendor, model))| {
            let prices = vendor
                .pricing
                .iter()
                .find(|pricing| pricing.model_id == model.model_id);
            let item = AdminAiModelItem {
                id: 0,
                uuid: stable_uuid(
                    "sdk-model-preview",
                    &[&vendor.vendor.vendor_code, &model.model_id],
                ),
                tenant_id: subject.tenant_id,
                organization_id: subject.organization_id,
                vendor_id: vendor.vendor.vendor_code.clone(),
                vendor_code: vendor.vendor.vendor_code.clone(),
                vendor_name: vendor.vendor.display_name.clone(),
                catalog_key: catalog_key.clone(),
                model: model.model_id.clone(),
                display_name: model.display_name.clone(),
                name: if model.display_name.trim().is_empty() {
                    model.model_id.clone()
                } else {
                    model.display_name.clone()
                },
                model_type: preview_model_type(model),
                region_prices: vec![AdminAiModelRegionPriceCommand {
                    region_code: vendor.vendor.region_code.clone(),
                    currency: preview_currency(prices, &vendor.vendor.region_code),
                    price_in: preview_price(prices, true),
                    price_out: preview_price(prices, false),
                    cache_read_price: non_empty_preview_cache_price(prices, "llm_cache_read_token"),
                    cache_write_price: non_empty_preview_cache_price(
                        prices,
                        "llm_cache_write_token",
                    ),
                }],
                status: "active".to_owned(),
                calls: "0".to_owned(),
                description: model.description.clone(),
                modalities: preview_modalities(model),
                input_modalities: model.input_modalities.clone(),
                output_modalities: model.output_modalities.clone(),
                api_format: Some(model.api_format.clone()),
                capability_intro: None,
                limitations: Vec::new(),
                supported_languages: Vec::new(),
                use_cases: model.strengths.clone(),
                training_data_cutoff: None,
                context_tokens: model.context_tokens,
                max_output_tokens: model.max_output_tokens,
                supports_streaming: model.supports_streaming,
                supports_tools: model.supports_tools,
                supports_json_schema: model.supports_json_schema,
                usage_scopes: model.usage_scopes.clone(),
                coding_visible: model.coding_visible,
                release_stage: Some(release_stage_code(&model.release_stage)),
                shelf_state: Some(shelf_state_code(&model.shelf_state)),
                routing_state: Some(routing_state_code(&model.routing_state)),
                replacement_model: model.replacement_model.clone(),
                deleted_at: None,
            };
            (catalog_key, item)
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .enumerate()
        .map(|(index, mut item)| {
            item.id = (index as i64) + 1;
            item
        })
        .collect::<Vec<_>>();
    (vendors, models)
}

fn validate_catalog_version_pin(
    catalog: &ModelCatalog,
    catalog_version: Option<&str>,
) -> Result<(), CatalogImportError> {
    let Some(expected) = catalog_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if expected != catalog.manifest.catalog_version {
        return Err(CatalogImportError::CatalogVersionMismatch {
            expected: expected.to_owned(),
            actual: catalog.manifest.catalog_version.clone(),
        });
    }
    Ok(())
}

fn normalized_vendor_set(vendor_codes: &[String]) -> BTreeSet<String> {
    vendor_codes
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

pub(crate) fn catalog_scope_source_hash(source_code: &str, catalog: &ModelCatalog) -> String {
    let payload = serde_json::json!({
        "hashKind": "sdkwork-models.catalog-scope.v1",
        "sourceCode": source_code,
        "catalog": catalog,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_else(|_| {
        format!(
            "{}:{}:{}:{}:{}",
            source_code,
            catalog.manifest.schema_version,
            catalog.manifest.catalog_version,
            catalog.manifest.generated_at,
            catalog.vendors.len()
        )
        .into_bytes()
    });
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    hex::encode(hasher.finalize())
}

fn preview_model_type(model: &ModelInfo) -> String {
    if model
        .input_modalities
        .iter()
        .chain(model.output_modalities.iter())
        .any(|modality| modality == "embedding")
    {
        return "Embedding".to_owned();
    }
    match model.primary_capability.as_str() {
        "image" => "Image",
        "audio" => "Audio",
        "music" => "Music",
        "sfx" | "sound_effect" => "SoundEffect",
        "video" => "Video",
        "embedding" => "Embedding",
        _ => "Chat",
    }
    .to_owned()
}

fn preview_currency(pricing: Option<&sdkwork_models::ModelPricing>, region_code: &str) -> String {
    pricing
        .map(|pricing| pricing.currency.trim().to_ascii_uppercase())
        .filter(|currency| {
            currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase())
        })
        .unwrap_or_else(|| match region_code {
            "cn" => "CNY".to_owned(),
            _ => "USD".to_owned(),
        })
}

fn preview_modalities(model: &ModelInfo) -> Vec<String> {
    let mut values = model.input_modalities.clone();
    for modality in &model.output_modalities {
        if !values.contains(modality) {
            values.push(modality.clone());
        }
    }
    values
}

fn preview_price(pricing: Option<&sdkwork_models::ModelPricing>, input: bool) -> String {
    let Some(pricing) = pricing else {
        return String::new();
    };
    let meters: &[&str] = if input {
        &[
            "llm_input_token",
            "embedding_input_token",
            "image_input_token",
            "image_megapixel",
            "audio_input_token",
            "audio_input_second",
            "audio_input_minute",
            "stt_audio_minute",
            "tts_input_character",
            "api_request",
            "video_input_token",
        ]
    } else {
        &[
            "llm_output_token",
            "image_output_token",
            "image_result",
            "image_megapixel",
            "audio_output_token",
            "audio_output_second",
            "music_output_second",
            "sfx_result",
            "video_output_token",
            "video_output_second",
            "video_result",
            "api_result",
        ]
    };
    pricing
        .prices
        .iter()
        .find(|price| meters.contains(&price.meter_code.as_str()))
        .map(|price| price.unit_price.clone())
        .unwrap_or_default()
}

fn preview_cache_price(pricing: Option<&sdkwork_models::ModelPricing>, meter_code: &str) -> String {
    pricing
        .and_then(|pricing| {
            pricing
                .prices
                .iter()
                .find(|price| price.meter_code == meter_code)
        })
        .map(|price| price.unit_price.clone())
        .unwrap_or_default()
}

fn non_empty_preview_cache_price(
    pricing: Option<&sdkwork_models::ModelPricing>,
    meter_code: &str,
) -> Option<String> {
    let value = preview_cache_price(pricing, meter_code);
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub(crate) fn stable_uuid(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    for part in parts {
        hasher.update(b":");
        hasher.update(part.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    format!("{prefix}-{}", &digest[..40])
}

pub(crate) fn catalog_sync_run_uuid(snapshot_uuid: &str) -> String {
    const MAX_LEN: usize = 64;
    let prefixed = format!("catalog-sync-{snapshot_uuid}");
    if prefixed.len() <= MAX_LEN {
        prefixed
    } else {
        snapshot_uuid.to_owned()
    }
}

pub(crate) fn stable_catalog_id(prefix: &str, parts: &[&str]) -> i64 {
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    for part in parts {
        hasher.update(b":");
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    let value = u64::from_be_bytes(bytes) & 0x3fff_ffff_ffff_ffff;
    (value as i64) + 1
}

pub(crate) fn metadata_json(
    catalog: &ModelCatalog,
    source: &str,
    extra: serde_json::Value,
) -> String {
    serde_json::json!({
        "source": source,
        "catalogVersion": catalog.manifest.catalog_version,
        "schemaVersion": catalog.manifest.schema_version,
        "generatedAt": catalog.manifest.generated_at,
        "extra": extra,
    })
    .to_string()
}

pub(crate) fn modality_code(value: &str) -> i32 {
    match value {
        "text" => 1,
        "image" => 2,
        "audio" => 3,
        "music" => 4,
        "video" => 5,
        "embedding" => 6,
        "rerank" => 7,
        "tool" => 8,
        "storage" => 9,
        "network" => 10,
        _ => 0,
    }
}

pub(crate) fn capability_code(value: &str) -> i32 {
    match value {
        "image" => 2,
        "audio" => 3,
        "music" => 4,
        "video" => 5,
        "embedding" => 6,
        "rerank" => 7,
        "tool" => 8,
        _ => 1,
    }
}

pub(crate) fn family_type_code(value: &str) -> i32 {
    match value {
        "embedding" => 2,
        "image" => 3,
        "audio" => 4,
        "music" => 5,
        "video" => 6,
        "rerank" => 7,
        "multimodal" => 8,
        _ => 1,
    }
}

pub(crate) fn vendor_type_code(value: &str) -> i32 {
    match value {
        "open_source" => 2,
        "research" => 3,
        "community" => 4,
        _ => 1,
    }
}

pub(crate) fn release_stage_code(value: &str) -> i32 {
    match value {
        "preview" => 2,
        "deprecated" => 3,
        "retired" => 4,
        _ => 1,
    }
}

pub(crate) fn shelf_state_code(value: &str) -> i32 {
    match value {
        "hidden" => 2,
        "archived" => 3,
        _ => 1,
    }
}

pub(crate) fn routing_state_code(value: &str) -> i32 {
    match value {
        "enabled" => 1,
        _ => 0,
    }
}

pub(crate) fn lifecycle_code(value: &str) -> i32 {
    match value {
        "preview" => 2,
        "deprecated" => 3,
        "catalog_only" => 4,
        "retired" => 5,
        _ => 1,
    }
}

pub(crate) fn voice_gender_code(value: &str) -> i32 {
    match value {
        "male" => 1,
        "female" => 2,
        "neutral" => 3,
        _ => 0,
    }
}

pub(crate) fn price_side_code(value: &str) -> i32 {
    match value {
        "upstream" => 2,
        "customer" => 3,
        _ => 1,
    }
}

pub(crate) fn price_supplier_code(
    vendor_code: &str,
    _region_code: &str,
    price_side: &str,
    pricing_scope: Option<&str>,
) -> Option<String> {
    if price_side == "upstream" || matches!(pricing_scope, Some("provider" | "channel")) {
        Some(format!("{vendor_code}_direct"))
    } else {
        None
    }
}

pub(crate) fn pricing_scope_code(value: Option<&str>) -> i32 {
    match value {
        Some("provider") => 2,
        Some("channel") => 3,
        Some("plan") => 4,
        _ => 1,
    }
}

pub(crate) fn primary_modality(model: &ModelInfo) -> i32 {
    model
        .output_modalities
        .first()
        .or_else(|| model.input_modalities.first())
        .map(|value| modality_code(value))
        .unwrap_or_else(|| capability_code(&model.primary_capability))
}

pub(crate) fn model_modalities_json(model: &ModelInfo) -> String {
    let mut values = model.input_modalities.clone();
    for output in &model.output_modalities {
        if !values.contains(output) {
            values.push(output.clone());
        }
    }
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_owned())
}

pub(crate) fn model_capabilities_json(model: &ModelInfo) -> String {
    let capabilities;
    let values = if model.capabilities.is_empty() {
        capabilities = vec![model.primary_capability.clone()];
        capabilities.as_slice()
    } else {
        model.capabilities.as_slice()
    };
    json_array(values)
}

pub(crate) fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        ai_resource_description, catalog_sync_run_uuid, endpoint_modality_code,
        model_endpoint_descriptor, price_supplier_code, ModelInfo,
    };

    #[test]
    fn ai_resource_description_preserves_none_and_short_values() {
        assert_eq!(None, ai_resource_description(None));
        assert_eq!(
            Some("short description".to_owned()),
            ai_resource_description(Some("short description".to_owned()))
        );
    }

    #[test]
    fn ai_resource_description_preserves_exact_character_limit() {
        let description = "a".repeat(512);
        assert_eq!(
            Some(description.clone()),
            ai_resource_description(Some(description))
        );
    }

    #[test]
    fn ai_resource_description_truncates_values_over_character_limit() {
        let description = format!("{}b", "a".repeat(512));
        assert_eq!(
            Some("a".repeat(512)),
            ai_resource_description(Some(description))
        );
    }

    #[test]
    fn ai_resource_description_truncates_multibyte_values_at_utf8_boundary() {
        let description = format!("{}{}", "界".repeat(512), "文".repeat(48));
        let projected = ai_resource_description(Some(description)).expect("description");
        assert_eq!(512, projected.chars().count());
        assert_eq!("界".repeat(512), projected);
    }

    #[test]
    fn catalog_sync_run_uuid_fits_varchar_64_for_installer_style_snapshot_uuid() {
        let snapshot_uuid = format!("catalog-refresh-{}", "11111111-1111-4111-8111-111111111111");
        let sync_run_uuid = catalog_sync_run_uuid(&snapshot_uuid);
        assert!(
            sync_run_uuid.len() <= 64,
            "sync run uuid must fit VARCHAR(64): len={} value={sync_run_uuid}",
            sync_run_uuid.len()
        );
        assert_eq!(snapshot_uuid, sync_run_uuid);
    }

    #[test]
    fn catalog_sync_run_uuid_keeps_prefix_for_standard_snapshot_uuid() {
        let snapshot_uuid = "11111111-1111-4111-8111-111111111111".to_owned();
        assert_eq!(
            "catalog-sync-11111111-1111-4111-8111-111111111111",
            catalog_sync_run_uuid(&snapshot_uuid)
        );
    }

    #[test]
    fn price_supplier_code_keeps_vendor_identity_separate_from_region() {
        assert_eq!(
            Some("minimax_direct".to_owned()),
            price_supplier_code("minimax", "cn", "upstream", None)
        );
        assert_eq!(
            Some("minimax_direct".to_owned()),
            price_supplier_code("minimax", "global", "official", Some("provider"))
        );
        assert_eq!(
            Some("kuaishou_direct".to_owned()),
            price_supplier_code("kuaishou", "global", "official", Some("channel"))
        );
        assert_eq!(
            None,
            price_supplier_code("minimax", "cn", "official", Some("model"))
        );
    }

    /// Every capability the catalog declares must land on a descriptor that can
    /// actually carry it.
    ///
    /// This pins the defect that let sound effects be routed as chat, and it
    /// pins it in *this* copy on purpose: this module is the one that writes
    /// `ai_vendor_api_endpoint` / `ai_api_endpoint` / `ai_model_api_endpoint`, so
    /// a capability missing here cannot be healed by the router's own copy of
    /// the same function — the router only *reads* the rows, so a projection
    /// this side never emits shows up as
    /// `ai_vendor_api_endpoint is missing N bundled key(s)` and stalls the
    /// bootstrap at `UpgradeRequired`.
    #[test]
    fn each_media_capability_binds_to_its_own_endpoint_not_chat() {
        // Built through serde rather than a field list: `ModelInfo` is a large
        // type and transcribing its fields here would make this guard fail to
        // compile every time it grows one — obscuring the one thing it asserts.
        let model_of = |capability: &str| -> ModelInfo {
            serde_json::from_value(serde_json::json!({
                "catalogKey": "test/model",
                "modelId": "model",
                "displayName": "Test Model",
                "vendorCode": "test",
                "regionCode": "global",
                "familyCode": "test",
                "primaryCapability": capability,
                "apiFormat": "openai_compatible",
                "lifecycle": "active",
                "releaseStage": "active",
                "shelfState": "listed",
                "routingState": "enabled",
                "source": { "sourceUrl": "https://example.test/models", "observedAt": "2026-01-01T00:00:00Z" },
            }))
            .expect("test model must deserialize")
        };

        for (capability, expected) in [
            ("image", "openai.images"),
            ("audio", "openai.audio"),
            ("music", "suno.music"),
            ("video", "openai.video"),
            ("embedding", "openai.embeddings"),
            ("rerank", "rerank"),
            ("sfx", "sfx.sound"),
        ] {
            let descriptor = model_endpoint_descriptor(&model_of(capability));
            assert_eq!(
                descriptor.endpoint_code, expected,
                "capability {capability} bound to {} instead of {expected}",
                descriptor.endpoint_code
            );
        }

        // `chat` is what the fallback produces, and it must still do so.
        assert_eq!(
            model_endpoint_descriptor(&model_of("chat")).endpoint_code,
            "openai.chat_completions"
        );
        assert_eq!(
            model_endpoint_descriptor(&model_of("something-new")).endpoint_code,
            "openai.chat_completions"
        );

        // Regressions that matter most: no media capability may be chat.
        for capability in ["image", "audio", "music", "video", "embedding", "sfx"] {
            assert_ne!(
                model_endpoint_descriptor(&model_of(capability)).endpoint_code,
                "openai.chat_completions",
                "capability {capability} collapsed onto the chat endpoint"
            );
        }
    }

    /// An image model must be bound to *its own vendor's* image surface, not to
    /// whichever vendor happened to define the generic one.
    ///
    /// Pins the defect that collapsed every bound image model onto
    /// `openai.images`: `primaryCapability` was the only input to
    /// `model_endpoint_descriptor`, so a `google/gemini-3-pro-image` request was
    /// planned against an OpenAI-compatible route while the cloud router's
    /// classifier would have replayed it to
    /// `/v1beta/models/{model}:generateImages`. The two halves of the chain
    /// disagreed about which `api_code` an image request carries.
    #[test]
    fn image_models_bind_to_their_vendor_native_endpoint() {
        let image_model_of = |vendor_code: &str, api_format: &str| -> ModelInfo {
            serde_json::from_value(serde_json::json!({
                "catalogKey": format!("{vendor_code}/test-image"),
                "modelId": "test-image",
                "displayName": "Test Image Model",
                "vendorCode": vendor_code,
                "regionCode": "global",
                "familyCode": "test",
                "primaryCapability": "image",
                "apiFormat": api_format,
                "lifecycle": "active",
                "releaseStage": "active",
                "shelfState": "listed",
                "routingState": "enabled",
                "source": { "sourceUrl": "https://example.test/models", "observedAt": "2026-01-01T00:00:00Z" },
            }))
            .expect("test image model must deserialize")
        };

        for (vendor_code, expected) in [
            ("google", "gemini.image_generation"),
            ("bytedance", "jimeng.image_generation"),
            ("kuaishou", "kling.image_generation"),
            ("volcengine", "volcengine.image_generation"),
            ("vidu", "vidu.reference_to_image"),
            ("black_forest_labs", "black_forest_labs.image_generation"),
            ("runway", "runway.image_generation"),
            ("stability_ai", "stability_ai.image_generation"),
        ] {
            let descriptor =
                model_endpoint_descriptor(&image_model_of(vendor_code, "vendor_native"));
            assert_eq!(
                descriptor.endpoint_code, expected,
                "vendor {vendor_code} image model bound to {} instead of {expected}",
                descriptor.endpoint_code
            );
            assert_eq!(
                descriptor.protocol_code, "vendor_native",
                "vendor {vendor_code} bound to a non-native protocol"
            );
        }

        // `apiFormat = "google_gemini"` is itself the native Gemini surface.
        for vendor_spelling in ["google", "gemini"] {
            assert_eq!(
                model_endpoint_descriptor(&image_model_of(
                    vendor_spelling,
                    "google_gemini"
                ))
                .endpoint_code,
                "gemini.image_generation",
                "apiFormat google_gemini must reach the native Gemini endpoint"
            );
        }

        // Vendors with no declared native image API keep the generic surface.
        for vendor_code in ["openai", "xai", "zhipu", "minimax", "alibaba"] {
            assert_eq!(
                model_endpoint_descriptor(&image_model_of(vendor_code, "vendor_native"))
                    .endpoint_code,
                "openai.images",
                "vendor {vendor_code} has no native image endpoint and must stay generic"
            );
        }

        // A model declaring an OpenAI-compatible surface keeps it even when its
        // vendor does publish a native one: the body it receives is the OpenAI one.
        for vendor_code in [
            "google",
            "bytedance",
            "kuaishou",
            "volcengine",
            "vidu",
            "black_forest_labs",
            "runway",
            "stability_ai",
        ] {
            assert_eq!(
                model_endpoint_descriptor(&image_model_of(vendor_code, "openai_compatible"))
                    .endpoint_code,
                "openai.images",
                "vendor {vendor_code} declared openai_compatible and must not be forced native"
            );
        }
    }

    /// A video model must be bound to *its own vendor's* video surface, not to
    /// whichever vendor happened to define the generic one.
    ///
    /// Pins the video half of the same defect the image guard covers:
    /// `primaryCapability = "video"` was the only input, so all 60 active video
    /// bindings — including the 55 whose `apiFormat` says `vendor_native` —
    /// collapsed onto `openai.video` (`POST /v1/videos`). The bundled catalog
    /// had already declared nine native video endpoints and
    /// `provider_native_classifier` already routed their paths, but with zero
    /// `ai_model_api_endpoint` rows no video model could ever reach them.
    #[test]
    fn video_models_bind_to_their_vendor_native_endpoint() {
        let video_model_of = |vendor_code: &str, api_format: &str| -> ModelInfo {
            serde_json::from_value(serde_json::json!({
                "catalogKey": format!("{vendor_code}/test-video"),
                "modelId": "test-video",
                "displayName": "Test Video Model",
                "vendorCode": vendor_code,
                "regionCode": "global",
                "familyCode": "test",
                "primaryCapability": "video",
                "apiFormat": api_format,
                "lifecycle": "active",
                "releaseStage": "active",
                "shelfState": "listed",
                "routingState": "enabled",
                "source": { "sourceUrl": "https://example.test/models", "observedAt": "2026-01-01T00:00:00Z" },
            }))
            .expect("test video model must deserialize")
        };

        // Catalog vendor code -> the native endpoint its video models must use.
        for (vendor_code, expected) in [
            ("google", "gemini.video_generation"),
            ("kuaishou", "kling.text_to_video"),
            ("bytedance", "jimeng.video_generation"),
            ("volcengine", "volcengine.video_generation"),
            ("vidu", "vidu.start_end_to_video"),
            ("alibaba", "alibaba.video_generation"),
            ("luma_ai", "luma_ai.video_generation"),
            ("pixverse", "pixverse.video_generation"),
            ("zhipu", "zhipu.video_generation"),
        ] {
            let descriptor =
                model_endpoint_descriptor(&video_model_of(vendor_code, "vendor_native"));
            assert_eq!(
                descriptor.endpoint_code, expected,
                "vendor {vendor_code} video model bound to {} instead of {expected}",
                descriptor.endpoint_code
            );
            assert_eq!(
                descriptor.protocol_code, "vendor_native",
                "vendor {vendor_code} bound to a non-native protocol"
            );
        }

        // `apiFormat = "google_gemini"` is itself the native Gemini surface, so
        // a Veo model must reach `gemini.video_generation` regardless of the
        // vendor code spelling used by the catalog.
        for vendor_spelling in ["google", "gemini"] {
            assert_eq!(
                model_endpoint_descriptor(&video_model_of(vendor_spelling, "google_gemini"))
                    .endpoint_code,
                "gemini.video_generation",
                "apiFormat google_gemini must reach the native Gemini video endpoint"
            );
        }

        // A vendor with no declared native video API keeps the generic surface:
        // sending it to a native route the vendor never registered would plan
        // against a non-existent endpoint.
        for vendor_code in ["black_forest_labs", "minimax", "runway", "openai", "xai"] {
            assert_eq!(
                model_endpoint_descriptor(&video_model_of(vendor_code, "vendor_native"))
                    .endpoint_code,
                "openai.video",
                "vendor {vendor_code} has no native video endpoint and must stay generic"
            );
        }

        // A model that declares an OpenAI-compatible video surface must keep it
        // even when its vendor does publish a native one: the request body it
        // will receive is the OpenAI one.
        for vendor_code in ["google", "kuaishou", "bytedance", "volcengine", "vidu"] {
            assert_eq!(
                model_endpoint_descriptor(&video_model_of(vendor_code, "openai_compatible"))
                    .endpoint_code,
                "openai.video",
                "vendor {vendor_code} declared openai_compatible and must not be forced native"
            );
        }

        // The native video endpoints must resolve to the `video` modality, so
        // the `ai_modality_api_endpoint` projection keeps the link.
        for endpoint_code in [
            "gemini.video_generation",
            "kling.text_to_video",
            "kling.image_to_video",
            "kling.avatar",
            "kling.motion_control",
            "jimeng.video_generation",
            "volcengine.video_generation",
            "vidu.start_end_to_video",
            "vidu.motion_sync",
        ] {
            assert_eq!(
                endpoint_modality_code(endpoint_code).as_deref(),
                Some("video"),
                "endpoint {endpoint_code} must map to the video modality"
            );
        }
    }

    /// The closure invariant behind the per-capability guards above.
    ///
    /// Every per-capability guard enumerates the vendors it knows about by
    /// hand, which is what let the same defect (one generic endpoint per
    /// capability, chosen without looking at `apiFormat` or `vendor_code`)
    /// survive five times. This test closes the space instead: it sweeps every
    /// `(vendor_code, api_format, capability)` combination the catalog can
    /// carry through `model_endpoint_descriptor` and asserts that no
    /// `vendor_native` descriptor names a route the project has not declared.
    ///
    /// A descriptor that names an undeclared path is an orphan route: bound in
    /// the catalog, but with no resource, no group grant, no classifier arm and
    /// no price, so routing answers `50201` for it. A declared endpoint no
    /// descriptor can bind is the mirror defect — a model that can never reach
    /// a route the project ships.
    ///
    /// Keep [`DECLARED_VENDOR_NATIVE_ENDPOINTS`] in step with
    /// `data/ai-routing/resources/vendor-native-resources.json`.
    #[test]
    fn every_capability_binds_only_to_a_declared_vendor_native_endpoint() {
        const DECLARED_VENDOR_NATIVE_ENDPOINTS: &[(&str, &str)] = &[
            ("anthropic.claude_code", "/v1/claude-code/sessions"),
            ("anthropic.messages", "/v1/messages"),
            ("black_forest_labs.image_generation", "/v1/flux-{model}"),
            ("black_forest_labs.task_query", "/v1/get_result"),
            ("elevenlabs.sound_generation", "/v1/sound-generation"),
            ("elevenlabs.text_to_speech", "/v1/text-to-speech/{voice_id}"),
            ("gemini.embed_content", "/v1beta/models/{model}:embedContent"),
            ("gemini.generate_content", "/v1beta/models/{model}:generateContent"),
            ("gemini.image_generation", "/v1beta/models/{model}:generateImages"),
            ("gemini.live", "/v1beta/live/sessions"),
            (
                "gemini.nano_banana.image_generation",
                "/v1beta/models/nano-banana:generateImages",
            ),
            (
                "gemini.stream_generate_content",
                "/v1beta/models/{model}:streamGenerateContent",
            ),
            ("gemini.video_generation", "/v1beta/models/{model}:generateVideos"),
            ("jimeng.image_generation", "/v1/images/generations"),
            ("jimeng.task_query", "/v1/tasks/{taskId}"),
            ("jimeng.video_generation", "/v1/videos/generations"),
            ("kling.avatar", "/v1/videos/avatar"),
            ("kling.image_generation", "/v1/images/generations"),
            ("kling.image_to_video", "/v1/videos/image2video"),
            ("kling.motion_control", "/v1/videos/motion-control"),
            ("kling.task_query", "/v1/videos/generations/{taskId}"),
            ("kling.text_to_video", "/v1/videos/text2video"),
            ("minimax.music_generation", "/v1/music/generations"),
            ("runway.image_generation", "/v1/text_to_image"),
            ("runway.task_query", "/v1/tasks/{id}"),
            ("sfx.sound", "/v1/sound/generate"),
            (
                "stability_ai.image_generation",
                "/v2beta/stable-image/generate/{mode}",
            ),
            ("suno.music", "/v1/music"),
            ("suno.music_generation", "/v1/music/generations"),
            ("suno.music_task_query", "/v1/music/generations/{taskId}"),
            ("vidu.motion_sync", "/ent/v2/template"),
            ("vidu.reference_to_image", "/ent/v2/reference2image"),
            ("vidu.start_end_to_video", "/ent/v2/start-end2video"),
            ("volcengine.image_generation", "/api/v3/images/generations"),
            ("volcengine.speech", "/api/v3/audio/speech"),
            (
                "volcengine.task_query",
                "/api/v3/contents/generations/tasks/{taskId}",
            ),
            (
                "volcengine.video_generation",
                "/api/v3/contents/generations/tasks",
            ),
            // The thirteen catalog vendors whose `official.<vendor>.full`
            // resource group reached its bundled account with no `api.*`
            // entitlement, so `account_route_allows_api_resource` failed the
            // whole group closed on every api code it did not grant. Each
            // endpoint below is a real declaration in the seed, not a route
            // invented to make this table agree with it.
            (
                "alibaba.chat_completions",
                "/compatible-mode/v1/chat/completions",
            ),
            ("alibaba.embeddings", "/compatible-mode/v1/embeddings"),
            (
                "alibaba.image_generation",
                "/api/v1/services/aigc/text2image/image-synthesis",
            ),
            (
                "alibaba.video_generation",
                "/api/v1/services/aigc/video-generation/video-synthesis",
            ),
            (
                "alibaba.video_generation_task_query",
                "/api/v1/tasks/{task_id}",
            ),
            ("baidu.chat_completions", "/v2/chat/completions"),
            ("deepseek.chat_completions", "/v1/chat/completions"),
            (
                "luma_ai.video_generation",
                "/dream-machine/v1/generations",
            ),
            (
                "luma_ai.video_generation_task_query",
                "/dream-machine/v1/generations/{id}",
            ),
            ("meituan.chat_completions", "/v1/chat/completions"),
            ("moonshot.chat_completions", "/v1/chat/completions"),
            ("mureka.music_generation", "/v1/song/generate"),
            (
                "mureka.music_generation_task_query",
                "/v1/song/query/{task_id}",
            ),
            (
                "pixverse.video_generation",
                "/openapi/v2/video/text/generate",
            ),
            (
                "pixverse.video_generation_task_query",
                "/openapi/v2/video/result/{video_id}",
            ),
            ("stepfun.chat_completions", "/v1/chat/completions"),
            ("tencent.chat_completions", "/v1/chat/completions"),
            ("xai.chat_completions", "/v1/chat/completions"),
            ("xai.image_generation", "/v1/images/generations"),
            ("xai.video_generation", "/v1/videos/generations"),
            ("xiaomi.chat_completions", "/v1/chat/completions"),
            ("xiaomi.image_generation", "/v1/images/generations"),
            ("xiaomi.speech", "/v1/audio/speech"),
            ("xiaomi.video_generation", "/v1/videos/generations"),
            ("zhipu.chat_completions", "/api/paas/v4/chat/completions"),
            ("zhipu.embeddings", "/api/paas/v4/embeddings"),
            (
                "zhipu.image_generation",
                "/api/paas/v4/images/generations",
            ),
            (
                "zhipu.video_generation",
                "/api/paas/v4/videos/generations",
            ),
            (
                "zhipu.video_generation_task_query",
                "/api/paas/v4/async-result/{id}",
            ),
        ];

        const GENERIC_ENDPOINTS: &[&str] = &[
            "openai.images",
            "openai.video",
            "openai.audio",
            "openai.embeddings",
            "openai.chat_completions",
            "rerank",
        ];

        // Catalog vendors (25) plus the endpoint-side aliases the classifier
        // also accepts for the three renamed ones.
        const CATALOG_VENDORS: &[&str] = &[
            "alibaba",
            "anthropic",
            "baidu",
            "black_forest_labs",
            "bytedance",
            "deepseek",
            "elevenlabs",
            "google",
            "kuaishou",
            "luma_ai",
            "meituan",
            "minimax",
            "moonshot",
            "mureka",
            "openai",
            "pixverse",
            "runway",
            "stability_ai",
            "stepfun",
            "suno",
            "tencent",
            "vidu",
            "volcengine",
            "xai",
            "xiaomi",
            "zhipu",
        ];
        const VENDOR_ALIASES: &[&str] = &["gemini", "kling", "jimeng"];
        const API_FORMATS: &[&str] = &["vendor_native", "google_gemini", "openai_compatible"];
        const NATIVE_CAPABILITIES: &[(&str, &[&str], &[&str])] = &[
            // `chat` is swept only because `baidu` publishes a
            // `vendor_native` chat surface; every other vendor stays generic.
            ("chat", &["text"], &["text"]),
            ("image", &["text"], &["image"]),
            ("video", &["text"], &["video"]),
            ("audio", &["text"], &["audio"]),
            ("audio", &["audio"], &["text"]),
            ("audio", &["audio"], &["audio"]),
            ("music", &["text"], &["audio"]),
            ("sfx", &["text"], &["audio"]),
        ];
        // Utility, poll and feature-entry surfaces: declared for the classifier
        // and granted by their resource groups, but not bound by any
        // *capability* descriptor. The feature-entry ones are driven by the
        // generation service with an explicit api code.
        const NOT_BOUND_BY_DESCRIPTOR: &[&str] = &[
            "anthropic.claude_code",
            "anthropic.messages",
            "gemini.embed_content",
            "gemini.generate_content",
            "gemini.stream_generate_content",
            "black_forest_labs.task_query",
            "jimeng.task_query",
            "kling.task_query",
            "runway.task_query",
            "suno.music_generation",
            "suno.music_task_query",
            "volcengine.task_query",
            // The five async poll surfaces added alongside their create
            // endpoints. Reached holding a task id produced by a prior create
            // call, so no model's `primaryCapability` selects them. Keep in
            // step with the cloud router's copy.
            "alibaba.video_generation_task_query",
            "luma_ai.video_generation_task_query",
            "mureka.music_generation_task_query",
            "pixverse.video_generation_task_query",
            "zhipu.video_generation_task_query",
            "gemini.nano_banana.image_generation",
            "kling.avatar",
            "kling.image_to_video",
            "kling.motion_control",
            "vidu.motion_sync",
            // Compatibility-face surfaces. Every endpoint below is a real
            // declaration, classified and granted, but no model binds to it
            // because every model of its vendor declares
            // `apiFormat = openai_compatible` — and a model's own declaration
            // wins (`model_image_endpoint_descriptor` / `_video_` / `_audio_`).
            // Forcing a descriptor arm would have to overwrite that
            // declaration, which would send an OpenAI-shaped body to a vendor
            // path that does not answer it. So the endpoint stays reachable by
            // api code for callers that drive it explicitly, and the models
            // stay on the generic face — the honest projection of the catalog.
            "alibaba.chat_completions",
            "alibaba.embeddings",
            "alibaba.image_generation",
            "deepseek.chat_completions",
            "meituan.chat_completions",
            "moonshot.chat_completions",
            "stepfun.chat_completions",
            "tencent.chat_completions",
            "xai.chat_completions",
            "xai.image_generation",
            "xai.video_generation",
            "xiaomi.chat_completions",
            "xiaomi.image_generation",
            "xiaomi.speech",
            "xiaomi.video_generation",
            "zhipu.chat_completions",
            "zhipu.embeddings",
            "zhipu.image_generation",
        ];

        let declared: Vec<(&str, &str)> = DECLARED_VENDOR_NATIVE_ENDPOINTS.to_vec();
        let known_endpoint_codes: Vec<&str> = declared
            .iter()
            .map(|(code, _)| *code)
            .chain(GENERIC_ENDPOINTS.iter().copied())
            .collect();

        let model_of = |vendor_code: &str,
                        api_format: &str,
                        capability: &str,
                        input_modalities: &[&str],
                        output_modalities: &[&str]|
         -> ModelInfo {
            serde_json::from_value(serde_json::json!({
                "catalogKey": format!("{vendor_code}/closure-{capability}"),
                "modelId": format!("closure-{capability}"),
                "displayName": "Closure Probe",
                "vendorCode": vendor_code,
                "regionCode": "global",
                "familyCode": "test",
                "primaryCapability": capability,
                "apiFormat": api_format,
                "inputModalities": input_modalities,
                "outputModalities": output_modalities,
                "lifecycle": "active",
                "releaseStage": "active",
                "shelfState": "listed",
                "routingState": "enabled",
                "source": { "sourceUrl": "https://example.test/models", "observedAt": "2026-01-01T00:00:00Z" },
            }))
            .expect("closure probe model must deserialize")
        };

        let all_vendors: Vec<&str> = CATALOG_VENDORS
            .iter()
            .chain(VENDOR_ALIASES.iter())
            .copied()
            .collect();

        let mut native_seen = 0usize;
        let mut generic_seen = 0usize;
        let mut reachable = std::collections::BTreeSet::new();

        for vendor_code in &all_vendors {
            for api_format in API_FORMATS {
                for (capability, input_modalities, output_modalities) in NATIVE_CAPABILITIES {
                    let descriptor = model_endpoint_descriptor(&model_of(
                        vendor_code,
                        api_format,
                        capability,
                        input_modalities,
                        output_modalities,
                    ));
                    let context = format!(
                        "{vendor_code} / {api_format} / {capability} (in={input_modalities:?}, out={output_modalities:?})"
                    );

                    if descriptor.protocol_code == "vendor_native" {
                        native_seen += 1;
                        reachable.insert(descriptor.endpoint_code.clone());
                        let declared_path = declared
                            .iter()
                            .find(|(code, _)| *code == descriptor.endpoint_code)
                            .map(|(_, path)| *path);
                        assert_eq!(
                            declared_path,
                            Some(descriptor.path_template),
                            "invented vendor-native route: {context} binds {} at {} but no seeded \
                             api_endpoint declares that code (declared: {declared_path:?})",
                            descriptor.endpoint_code,
                            descriptor.path_template,
                        );
                    } else {
                        generic_seen += 1;
                    }

                    assert!(
                        known_endpoint_codes.contains(&descriptor.endpoint_code),
                        "{context} produced endpoint code {} which is neither a declared \
                         vendor-native endpoint nor a known generic face",
                        descriptor.endpoint_code,
                    );
                    assert!(
                        endpoint_modality_code(&descriptor.endpoint_code).is_some(),
                        "{context} produced endpoint code {} which resolves to no modality",
                        descriptor.endpoint_code,
                    );
                }
            }
        }

        assert!(
            native_seen > 0 && generic_seen > 0,
            "the sweep produced native={native_seen} generic={generic_seen}; the closure assertion \
             is vacuous"
        );

        let unbound: Vec<&str> = declared
            .iter()
            .map(|(code, _)| *code)
            .filter(|code| !reachable.contains(*code))
            .filter(|code| !NOT_BOUND_BY_DESCRIPTOR.contains(code))
            .collect();
        assert!(
            unbound.is_empty(),
            "declared vendor-native endpoints no descriptor can bind ({unbound:?}); a \
             declared-but-unbindable endpoint is a model that can never reach it"
        );
    }
}
