use std::collections::{BTreeMap, BTreeSet};

use sdkwork_utils_rust::{is_blank, snake_case};

use crate::application::{PricingResolver, ResolveModelPriceQuery};
use crate::domain::{
    provider_native_model_id, AiModel, BillingMeter, DomainResult, ModelPrice, ModelUpstreamRoute,
    ModelVendor, PriceSide, UpstreamAccountGroup, UpstreamAccountGroupBinding,
    UpstreamAccountRoute, UpstreamResourceEntitlement,
};
use crate::ports::PricingCatalog;

const MODEL_GROUP_DEFAULT: &str = "default";
const MODEL_GROUP_VIP: &str = "vip";
const MODEL_GROUP_ENTERPRISE: &str = "enterprise";
const MODEL_GROUP_BETA: &str = "beta";
const MODEL_CATEGORY_RECOMMENDED: &str = "Recommended";
const MODEL_CATEGORY_OPEN_SOURCE: &str = "Open Source";
const MODEL_CATEGORY_PROPRIETARY: &str = "Proprietary";
const MODEL_CATEGORY_FREE: &str = "Free";
const MODEL_CATEGORY_NEW: &str = "New";
const DEFAULT_MODEL_CATALOG_PAGE_SIZE: usize = 20;
const MAX_MODEL_CATALOG_PAGE_SIZE: usize = 200;
const INVALID_MODEL_CATEGORY_FILTER: &str = "__invalid_model_category__";
/// Synthetic capability emitted by the routing snapshot when a group grant and
/// an account's resource scope do not intersect. A binding carrying it must be
/// treated as a deny, exactly like the routing selector does.
const MODEL_GROUP_CAPABILITY_DENY: &str = "__deny__";

pub struct ModelCatalogQueryService<'a, C: PricingCatalog> {
    catalog: &'a C,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListModelCatalogQuery {
    pub api_key_id: Option<i64>,
    pub billing_meter: BillingMeter,
    pub vendor_code: Option<String>,
    pub vendor_codes: Vec<String>,
    pub modalities: Vec<String>,
    pub capabilities: Vec<String>,
    pub categories: Vec<String>,
    pub groups: Vec<String>,
    pub search_query: Option<String>,
    pub page_size: Option<usize>,
    pub offset: Option<usize>,
}

impl ListModelCatalogQuery {
    pub fn normalized_offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }

    pub fn normalized_vendor_codes(&self) -> Vec<String> {
        let mut values = normalize_filter_values(&self.vendor_codes);
        if let Some(vendor_code) = normalize_filter_value(self.vendor_code.as_deref()) {
            values.push(vendor_code);
        }
        values.sort();
        values.dedup();
        values
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogPage {
    pub items: Vec<ModelCatalogItem>,
    pub groups: Vec<ModelCatalogGroup>,
    pub page_size: usize,
    pub offset: usize,
    pub total_items: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogGroup {
    pub key: String,
    pub label: String,
    pub model_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogItem {
    pub catalog_key: String,
    pub model: String,
    pub display_name: String,
    pub vendor_code: String,
    pub vendor: ModelVendor,
    pub capabilities: Vec<String>,
    pub groups: Vec<String>,
    pub categories: Vec<String>,
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
    pub supplier_codes: Vec<String>,
    pub official_reference_prices: Vec<ModelCatalogReferencePriceView>,
    pub lowest_upstream_cost_unit_price: Option<String>,
    pub lowest_upstream_cost_currency: Option<String>,
    pub price_availability: PriceAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogReferencePriceView {
    pub region_code: String,
    pub billing_meter: String,
    pub unit_price: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriceAvailability {
    Available(ModelCatalogPriceView),
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCatalogPriceView {
    pub group_code: String,
    pub pricing_plan_code: String,
    pub customer_unit_price: String,
    pub gross_margin_per_unit: Option<String>,
}

impl<'a, C: PricingCatalog> ModelCatalogQueryService<'a, C> {
    pub fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    pub fn list_models(&self, query: ListModelCatalogQuery) -> DomainResult<ModelCatalogPage> {
        let resolver = PricingResolver::new(self.catalog);
        let (price_tenant_id, price_organization_id, price_account_group_id) = query
            .api_key_id
            .and_then(|api_key_id| self.catalog.find_api_key(api_key_id))
            .map(|api_key| {
                (
                    api_key.tenant_id,
                    api_key.organization_id,
                    Some(api_key.default_account_group_id),
                )
            })
            .unwrap_or((0, 0, None));
        let vendor_codes = query.normalized_vendor_codes();
        let modalities = normalize_modality_filter_values(&query.modalities);
        let capabilities = normalize_capability_filter_values(&query.capabilities);
        let categories = normalize_category_filter_values(&query.categories);
        let groups = normalize_filter_values(&query.groups);
        let search_query = normalize_filter_value(query.search_query.as_deref());
        let limit = query
            .page_size
            .unwrap_or(DEFAULT_MODEL_CATALOG_PAGE_SIZE)
            .clamp(1, MAX_MODEL_CATALOG_PAGE_SIZE);
        let offset = query.normalized_offset();
        let group_index = ConfiguredModelGroupIndex::from_catalog(self.catalog);
        let mut model_counts_by_group = BTreeMap::new();
        let mut total_items = 0_usize;
        let mut models = Vec::with_capacity(limit);

        self.catalog.visit_models(None, &mut |model| {
            if !model.is_publicly_active() {
                return true;
            }

            let model_groups =
                group_index.groups_for(model, &model_upstream_routes(self.catalog, model));
            count_model_groups(&mut model_counts_by_group, &model_groups);
            let vendor = self
                .catalog
                .find_vendor(&model.vendor_code)
                .map(|vendor| vendor.vendor)
                .unwrap_or(ModelVendor::Unknown);
            let model_lookup_key = model.catalog_key.as_str();
            let official_reference_prices = self.official_reference_prices(
                price_tenant_id,
                price_organization_id,
                model_lookup_key,
            );
            let model_categories =
                derive_model_categories(model, &vendor, &official_reference_prices);

            if !model_matches_filter(
                model,
                &model_groups,
                &model_categories,
                &vendor_codes,
                &modalities,
                &capabilities,
                &categories,
                &groups,
                search_query.as_deref(),
            ) {
                return true;
            }

            let matched_index = total_items;
            total_items = total_items.saturating_add(1);
            if matched_index < offset || models.len() >= limit {
                return true;
            }

            let supplier_codes = self.supplier_codes(model_lookup_key);
            let lowest_upstream_cost = self.lowest_upstream_cost(
                price_tenant_id,
                price_organization_id,
                price_account_group_id,
                model_lookup_key,
                query.billing_meter.clone(),
                &group_index,
            );
            let provider_for_resolve = lowest_upstream_cost
                .as_ref()
                .and_then(|price| price.supplier_code.clone())
                .or_else(|| supplier_codes.first().cloned());
            let price_availability = query
                .api_key_id
                .map(|api_key_id| {
                    resolver.resolve(ResolveModelPriceQuery {
                        api_key_id,
                        account_group_id: None,
                        model: model_lookup_key.to_owned(),
                        billing_meter: query.billing_meter.clone(),
                        supplier_code: provider_for_resolve,
                        account_id: lowest_upstream_cost
                            .as_ref()
                            .and_then(|price| price.account_id),
                        region_code: lowest_upstream_cost
                            .as_ref()
                            .map(|price| price.region_code.clone()),
                        default_region_code: None,
                        occurred_at: chrono::Utc::now(),
                    })
                })
                .map(to_price_availability)
                .unwrap_or_else(|| PriceAvailability::Unavailable {
                    reason: "api key context is required for customer price".to_owned(),
                });

            models.push(ModelCatalogItem {
                catalog_key: model.catalog_key.clone(),
                model: model.model.clone(),
                display_name: model.display_name.clone(),
                vendor_code: model.vendor_code.clone(),
                vendor,
                capabilities: model.capabilities.clone(),
                groups: model_groups,
                categories: model_categories,
                description: model.description.clone(),
                modalities: model.modalities.clone(),
                input_modalities: model.input_modalities.clone(),
                output_modalities: model.output_modalities.clone(),
                api_format: model.api_format.clone(),
                capability_intro: model.capability_intro.clone(),
                limitations: model.limitations.clone(),
                supported_languages: model.supported_languages.clone(),
                use_cases: model.use_cases.clone(),
                training_data_cutoff: model.training_data_cutoff.clone(),
                context_tokens: model.context_tokens,
                max_output_tokens: model.max_output_tokens,
                supports_streaming: model.supports_streaming,
                supports_tools: model.supports_tools,
                supports_json_schema: model.supports_json_schema,
                usage_scopes: model.usage_scopes.clone(),
                coding_visible: model.coding_visible,
                release_stage: model.release_stage,
                shelf_state: model.shelf_state,
                routing_state: model.routing_state,
                replacement_model: model.replacement_model.clone(),
                supplier_codes,
                official_reference_prices,
                lowest_upstream_cost_unit_price: lowest_upstream_cost
                    .as_ref()
                    .map(|price| price.unit_price.to_fixed_string(6)),
                lowest_upstream_cost_currency: lowest_upstream_cost
                    .as_ref()
                    .map(|price| price.unit_price.currency.clone()),
                price_availability,
            });
            true
        });
        let group_catalog = group_index.catalog(&model_counts_by_group);

        Ok(ModelCatalogPage {
            items: models,
            groups: group_catalog,
            page_size: limit,
            offset,
            total_items,
        })
    }

    fn supplier_codes(&self, model: &str) -> Vec<String> {
        let mut supplier_codes: Vec<String> = self
            .catalog
            .list_model_upstream_routes(model)
            .into_iter()
            .map(|route| route.supplier_code)
            .collect();
        supplier_codes.sort();
        supplier_codes.dedup();
        supplier_codes
    }

    fn official_reference_prices(
        &self,
        tenant_id: i64,
        organization_id: i64,
        model: &str,
    ) -> Vec<ModelCatalogReferencePriceView> {
        let mut prices: Vec<ModelCatalogReferencePriceView> = self
            .catalog
            .list_model_prices_for_scope_side(
                tenant_id,
                organization_id,
                model,
                PriceSide::OfficialReference,
            )
            .into_iter()
            .filter(|price| {
                price.supplier_code.is_none()
                    && price.account_id.is_none()
                    && price.pricing_plan_code.is_none()
            })
            .map(|price| ModelCatalogReferencePriceView {
                region_code: price.region_code,
                billing_meter: price.billing_meter.code().to_owned(),
                unit_price: price.unit_price.to_fixed_string(6),
                currency: price.unit_price.currency,
            })
            .collect();
        prices.sort_by(|left, right| {
            model_region_sort_key(&left.region_code)
                .cmp(&model_region_sort_key(&right.region_code))
                .then_with(|| left.region_code.cmp(&right.region_code))
                .then_with(|| {
                    billing_meter_sort_key(&left.billing_meter)
                        .cmp(&billing_meter_sort_key(&right.billing_meter))
                })
                .then_with(|| left.billing_meter.cmp(&right.billing_meter))
        });
        prices.dedup_by(|left, right| {
            left.region_code == right.region_code && left.billing_meter == right.billing_meter
        });
        prices
    }

    fn lowest_upstream_cost(
        &self,
        tenant_id: i64,
        organization_id: i64,
        account_group_id: Option<i64>,
        model: &str,
        billing_meter: BillingMeter,
        group_index: &ConfiguredModelGroupIndex,
    ) -> Option<ModelPrice> {
        self.catalog
            .list_model_prices_for_scope(
                tenant_id,
                organization_id,
                model,
                PriceSide::UpstreamCost,
                billing_meter,
            )
            .into_iter()
            .filter(|price| group_index.price_is_visible_to_group(price, account_group_id))
            .min_by_key(|price| price.unit_price.unit_price)
    }
}

struct ConfiguredModelGroupDefinition {
    key: String,
    label: String,
}

struct ConfiguredModelGroupIndex {
    definitions: Vec<ConfiguredModelGroupDefinition>,
    groups_by_id: BTreeMap<i64, String>,
    account_routes: Vec<UpstreamAccountRoute>,
    has_group_bindings: bool,
}

impl ConfiguredModelGroupIndex {
    fn from_catalog<C: PricingCatalog>(catalog: &C) -> Self {
        let mut definitions = Vec::new();
        let mut groups_by_id = BTreeMap::new();
        for group in catalog.list_upstream_account_groups() {
            let Some(key) = configured_group_code(&group) else {
                continue;
            };
            definitions.push(ConfiguredModelGroupDefinition {
                key: key.clone(),
                label: group.display_name(),
            });
            groups_by_id.insert(group.id, key);
        }
        let account_routes = catalog.list_upstream_account_routes();
        let has_group_bindings = account_routes
            .iter()
            .any(|route| !route.account_group_bindings.is_empty());
        Self {
            definitions,
            groups_by_id,
            account_routes,
            has_group_bindings,
        }
    }

    fn groups_for(&self, model: &AiModel, model_routes: &[ModelUpstreamRoute]) -> Vec<String> {
        model_group_keys(
            model,
            model_routes,
            &self.account_routes,
            &self.groups_by_id,
            self.has_group_bindings,
        )
    }

    fn price_is_visible_to_group(&self, price: &ModelPrice, account_group_id: Option<i64>) -> bool {
        let Some(account_group_id) = account_group_id else {
            return true;
        };
        let (Some(supplier_code), Some(account_id)) =
            (price.supplier_code.as_deref(), price.account_id)
        else {
            return false;
        };
        self.account_routes.iter().any(|route| {
            route.supplier_code == supplier_code
                && route.account_id == account_id
                && same_model_region(&route.region_code, &price.region_code)
                && route
                    .account_group_bindings
                    .iter()
                    .any(|binding| binding.account_group_id == account_group_id)
        })
    }

    fn catalog(&self, model_counts_by_group: &BTreeMap<String, usize>) -> Vec<ModelCatalogGroup> {
        let mut groups = self
            .definitions
            .iter()
            .map(|definition| ModelCatalogGroup {
                key: definition.key.clone(),
                label: definition.label.clone(),
                model_count: model_counts_by_group
                    .get(&normalize_semantic_token(&definition.key))
                    .copied()
                    .unwrap_or(0),
            })
            .collect::<Vec<_>>();
        groups.sort_by(|left, right| {
            group_has_models_sort_key(right)
                .cmp(&group_has_models_sort_key(left))
                .then_with(|| {
                    model_group_sort_key(&left.key).cmp(&model_group_sort_key(&right.key))
                })
                .then_with(|| {
                    normalize_semantic_token(&left.key).cmp(&normalize_semantic_token(&right.key))
                })
        });
        groups.dedup_by(|left, right| {
            normalize_semantic_token(&left.key) == normalize_semantic_token(&right.key)
        });
        groups
    }
}

/// Group keys a model belongs to, mirroring the routing authority:
///
/// * a group only contains a model when one of its accounts is bound to the
///   group and the binding's resource scope covers the model. The routing
///   snapshot's effective scope (group × supplier × account) is preferred; the
///   account's own entitlements are the fallback and a binding without either
///   is unscoped and serves every model, matching
///   `account_route_allows_model_request`;
/// * `Some([])` is a configured scope whose intersection is empty and must
///   fail closed;
/// * the binding capability list narrows the granted surface (`llm`, `image`,
///   …); the synthetic `__deny__` entry denies the whole binding.
///
/// When the catalog carries explicit model→account routes they are
/// authoritative for which accounts can serve the model; catalogs that derive
/// routes from entitlements (the SQL snapshot) carry none and every account
/// route is considered.
fn model_group_keys(
    model: &AiModel,
    model_routes: &[ModelUpstreamRoute],
    account_routes: &[UpstreamAccountRoute],
    groups_by_id: &BTreeMap<i64, String>,
    has_group_bindings: bool,
) -> Vec<String> {
    if groups_by_id.is_empty() {
        return Vec::new();
    }

    let mut selected_group_ids = BTreeSet::new();
    if !has_group_bindings {
        // No group configuration exists, so every group shares the same
        // unscoped routing behavior (see `AiRoutingIndex::matching_accounts`).
        selected_group_ids.extend(groups_by_id.keys().copied());
    } else {
        let model_capability_codes = model_group_capability_codes(model);
        let routed_accounts = routed_account_keys(model_routes);
        for route in account_routes {
            if let Some(routed_accounts) = routed_accounts.as_ref() {
                let account_key = account_route_key(route);
                if !routed_accounts.contains(&account_key) {
                    continue;
                }
            }
            for binding in &route.account_group_bindings {
                if !groups_by_id.contains_key(&binding.account_group_id) {
                    continue;
                }
                if binding_is_denied(binding) {
                    continue;
                }
                if !binding_matches_model_capability(binding, &model_capability_codes) {
                    continue;
                }
                if !binding_grants_model(binding, model, &model_capability_codes) {
                    continue;
                }
                selected_group_ids.insert(binding.account_group_id);
            }
        }
    }

    let mut groups = selected_group_ids
        .into_iter()
        .filter_map(|group_id| groups_by_id.get(&group_id).cloned())
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        model_group_sort_key(left)
            .cmp(&model_group_sort_key(right))
            .then_with(|| normalize_semantic_token(left).cmp(&normalize_semantic_token(right)))
    });
    groups
        .dedup_by(|left, right| normalize_semantic_token(left) == normalize_semantic_token(right));
    groups
}

fn routed_account_keys(model_routes: &[ModelUpstreamRoute]) -> Option<BTreeSet<(String, i64)>> {
    if model_routes.is_empty() {
        return None;
    }
    Some(
        model_routes
            .iter()
            .map(|route| {
                (
                    normalize_semantic_token(&route.supplier_code),
                    route.account_id,
                )
            })
            .collect(),
    )
}

fn account_route_key(route: &UpstreamAccountRoute) -> (String, i64) {
    (
        normalize_semantic_token(&route.supplier_code),
        route.account_id,
    )
}

fn binding_is_denied(binding: &UpstreamAccountGroupBinding) -> bool {
    binding.capabilities.iter().any(|capability| {
        capability
            .trim()
            .eq_ignore_ascii_case(MODEL_GROUP_CAPABILITY_DENY)
    })
}

fn binding_grants_model(
    binding: &UpstreamAccountGroupBinding,
    model: &AiModel,
    model_capability_codes: &BTreeSet<String>,
) -> bool {
    // The routing snapshot's effective scope (group × supplier × account) is
    // the most precise projection. It is absent only when no resource scope is
    // configured at all, in which case the account-scoped entitlements (or the
    // unscoped fallback) apply.
    let Some(entitlements) = binding
        .effective_resource_entitlements
        .as_deref()
        .or(binding.resource_entitlements.as_deref())
    else {
        // No configured resource scope: the account is unscoped and serves
        // every model, matching the routing selector.
        return true;
    };

    // API endpoint grants describe a callable surface, not a model set. Only a
    // binding without any model-scoped grant falls back to surface-level
    // membership (the binding capability scope narrows it further).
    let mut has_model_scope = false;
    let mut has_api_scope = false;
    for entitlement in entitlements {
        if entitlement_has_model_scope(entitlement) {
            has_model_scope = true;
            if entitlement_matches_model(entitlement, model, model_capability_codes) {
                return true;
            }
        } else if non_blank(entitlement.api_code.as_deref()).is_some() {
            has_api_scope = true;
        }
    }

    !has_model_scope && has_api_scope
}

fn entitlement_has_model_scope(entitlement: &UpstreamResourceEntitlement) -> bool {
    non_blank(entitlement.catalog_key.as_deref()).is_some()
        || non_blank(entitlement.model.as_deref()).is_some()
        || non_blank(entitlement.provider_native_model.as_deref()).is_some()
        || non_blank(entitlement.vendor_code.as_deref()).is_some()
        || non_blank(entitlement.modality_code.as_deref()).is_some()
}

fn entitlement_matches_model(
    entitlement: &UpstreamResourceEntitlement,
    model: &AiModel,
    model_capability_codes: &BTreeSet<String>,
) -> bool {
    let catalog_key = model.catalog_key.trim();
    let model_name = model.model.trim();
    let native_model = provider_native_model_id(catalog_key);
    let mut constrained = false;

    if let Some(value) = non_blank(entitlement.catalog_key.as_deref()) {
        constrained = true;
        if !value.eq_ignore_ascii_case(catalog_key) {
            return false;
        }
    }
    if let Some(value) = non_blank(entitlement.model.as_deref()) {
        constrained = true;
        if !value.eq_ignore_ascii_case(model_name)
            && !value.eq_ignore_ascii_case(catalog_key)
            && !value.eq_ignore_ascii_case(&native_model)
        {
            return false;
        }
    }
    if let Some(value) = non_blank(entitlement.provider_native_model.as_deref()) {
        constrained = true;
        if !value.eq_ignore_ascii_case(&native_model) && !value.eq_ignore_ascii_case(model_name) {
            return false;
        }
    }
    if let Some(value) = non_blank(entitlement.vendor_code.as_deref()) {
        constrained = true;
        if !value.eq_ignore_ascii_case(model.vendor_code.trim()) {
            return false;
        }
    }
    if let Some(value) = non_blank(entitlement.modality_code.as_deref()) {
        constrained = true;
        let modality = normalize_semantic_token(value);
        if !model_capability_codes.contains(&modality) {
            return false;
        }
    }

    constrained
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn model_upstream_routes<C: PricingCatalog + ?Sized>(
    catalog: &C,
    model: &AiModel,
) -> Vec<ModelUpstreamRoute> {
    let by_catalog_key = catalog.list_model_upstream_routes(model.catalog_key.trim());
    if !by_catalog_key.is_empty() {
        return by_catalog_key;
    }
    let model_name = model.model.trim();
    if model_name.is_empty() || model_name.eq_ignore_ascii_case(model.catalog_key.trim()) {
        return Vec::new();
    }
    catalog.list_model_upstream_routes(model_name)
}

fn count_model_groups(counts: &mut BTreeMap<String, usize>, groups: &[String]) {
    let mut counted_groups = BTreeSet::new();
    for group in groups {
        let normalized = normalize_semantic_token(group);
        if !normalized.is_empty() && counted_groups.insert(normalized.clone()) {
            *counts.entry(normalized).or_insert(0) += 1;
        }
    }
}

fn group_has_models_sort_key(group: &ModelCatalogGroup) -> usize {
    usize::from(group.model_count > 0)
}

fn configured_group_code(group: &UpstreamAccountGroup) -> Option<String> {
    let code = group.code.trim();
    if !code.is_empty() {
        return Some(code.to_owned());
    }
    let name = group.name.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn binding_matches_model_capability(
    binding: &UpstreamAccountGroupBinding,
    model_capability_codes: &BTreeSet<String>,
) -> bool {
    if binding.capabilities.is_empty() {
        return true;
    }
    let mut binding_codes = BTreeSet::new();
    for capability in &binding.capabilities {
        add_model_group_capability_code(&mut binding_codes, &normalize_semantic_token(capability));
    }
    binding_codes
        .iter()
        .any(|capability| model_capability_codes.contains(capability))
}

fn model_group_capability_codes(model: &AiModel) -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    for capability in &model.capabilities {
        add_model_group_capability_code(&mut codes, &normalize_semantic_token(capability));
    }
    for modality in normalized_model_modalities(model) {
        add_model_group_capability_code(&mut codes, &modality);
    }
    codes
}

fn add_model_group_capability_code(codes: &mut BTreeSet<String>, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    match value {
        "text" | "chat" | "llm" => {
            codes.insert("text".to_owned());
            codes.insert("chat".to_owned());
            codes.insert("llm".to_owned());
        }
        "speech" | "voice" | "audio" => {
            codes.insert("speech".to_owned());
            codes.insert("voice".to_owned());
            codes.insert("audio".to_owned());
        }
        "embedding" | "embeddings" => {
            codes.insert("embedding".to_owned());
            codes.insert("embeddings".to_owned());
            codes.insert("llm".to_owned());
        }
        "rerank" | "ranking" => {
            codes.insert("rerank".to_owned());
            codes.insert("ranking".to_owned());
            codes.insert("llm".to_owned());
        }
        "function_calling" | "function_call" | "tool_calling" | "tools" => {
            codes.insert("function_calling".to_owned());
            codes.insert("function_call".to_owned());
            codes.insert("tool_calling".to_owned());
            codes.insert("tools".to_owned());
        }
        "json" | "json_mode" | "json_schema" => {
            codes.insert("json".to_owned());
            codes.insert("json_mode".to_owned());
            codes.insert("json_schema".to_owned());
        }
        _ => {
            codes.insert(value.to_owned());
        }
    }
}

fn derive_model_categories(
    model: &crate::domain::AiModel,
    vendor: &ModelVendor,
    reference_prices: &[ModelCatalogReferencePriceView],
) -> Vec<String> {
    let mut categories = Vec::new();
    if model_is_public_default(model) {
        categories.push(MODEL_CATEGORY_RECOMMENDED.to_owned());
    }
    if is_open_source_vendor(vendor, &model.vendor_code) {
        categories.push(MODEL_CATEGORY_OPEN_SOURCE.to_owned());
    } else if vendor != &ModelVendor::Unknown {
        categories.push(MODEL_CATEGORY_PROPRIETARY.to_owned());
    }
    if model_is_free(reference_prices) {
        categories.push(MODEL_CATEGORY_FREE.to_owned());
    }
    if model_is_beta(model) {
        categories.push(MODEL_CATEGORY_NEW.to_owned());
    }
    categories.sort_by_key(|category| model_category_sort_key(category));
    categories.dedup();
    categories
}

fn model_is_public_default(model: &crate::domain::AiModel) -> bool {
    model.shelf_state.unwrap_or(1) == 1 && model.routing_state.unwrap_or(1) == 1
}

fn model_is_beta(model: &crate::domain::AiModel) -> bool {
    model.release_stage.unwrap_or(1) == 2
}

fn is_open_source_vendor(vendor: &ModelVendor, vendor_code: &str) -> bool {
    vendor == &ModelVendor::DeepSeek
        || matches!(
            normalize_filter_value(Some(vendor_code)).as_deref(),
            Some("meta" | "mistral" | "deepseek" | "01ai")
        )
}

fn model_is_free(reference_prices: &[ModelCatalogReferencePriceView]) -> bool {
    !reference_prices.is_empty()
        && reference_prices.iter().all(|price| {
            price
                .unit_price
                .trim()
                .parse::<f64>()
                .map(|value| value == 0.0)
                .unwrap_or(false)
        })
}

fn model_matches_filter(
    model: &AiModel,
    model_groups: &[String],
    model_categories: &[String],
    vendor_codes: &[String],
    modalities: &[String],
    capabilities: &[String],
    categories: &[String],
    groups: &[String],
    search_query: Option<&str>,
) -> bool {
    (vendor_codes.is_empty()
        || vendor_codes
            .iter()
            .any(|value| value == &normalize_semantic_token(&model.vendor_code)))
        && (modalities.is_empty()
            || normalized_model_modalities(model)
                .iter()
                .any(|value| modalities.contains(value)))
        && (capabilities.is_empty()
            || capabilities
                .iter()
                .all(|capability| normalized_model_capabilities(model).contains(capability)))
        && (categories.is_empty()
            || categories
                .iter()
                .all(|category| model_categories.iter().any(|value| value == category)))
        && (groups.is_empty()
            || groups.iter().any(|group| {
                model_groups
                    .iter()
                    .any(|value| normalize_semantic_token(value) == *group)
            }))
        && search_query_matches(model, search_query)
}

fn search_query_matches(model: &AiModel, search_query: Option<&str>) -> bool {
    let Some(search_query) = search_query else {
        return true;
    };
    let haystack = [
        model.catalog_key.as_str(),
        model.model.as_str(),
        model.display_name.as_str(),
        model.vendor_code.as_str(),
        model.description.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    haystack.contains(search_query)
}

fn normalized_model_capabilities(model: &AiModel) -> Vec<String> {
    let mut values = model
        .capabilities
        .iter()
        .map(|value| normalize_semantic_token(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn normalize_capability_filter_values(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .flat_map(|value| value.split(','))
        .filter_map(|value| normalize_filter_value(Some(value)))
        .map(|value| match value.as_str() {
            "function_calling" | "function_call" | "tool_calling" => "tools".to_owned(),
            "json" | "json_mode" => "json_schema".to_owned(),
            "vision" => "image".to_owned(),
            value => value.to_owned(),
        })
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalized_model_modalities(model: &crate::domain::AiModel) -> Vec<String> {
    let mut values = model
        .modalities
        .iter()
        .chain(model.input_modalities.iter())
        .chain(model.output_modalities.iter())
        .flat_map(|value| normalize_modality_token(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        values = model
            .capabilities
            .iter()
            .map(|value| normalize_semantic_token(value))
            .filter(|value| !value.is_empty())
            .collect();
    }
    values.sort();
    values.dedup();
    values
}

fn normalize_modality_filter_values(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .flat_map(|value| value.split(','))
        .flat_map(normalize_modality_token)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_modality_token(value: &str) -> Vec<String> {
    match normalize_semantic_token(value).as_str() {
        "text" | "chat" | "llm" => vec!["text".to_owned(), "chat".to_owned(), "llm".to_owned()],
        "speech" | "voice" | "audio" => {
            vec!["audio".to_owned(), "speech".to_owned(), "voice".to_owned()]
        }
        value => vec![value.to_owned()],
    }
}

fn normalize_filter_values(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .flat_map(|value| value.split(','))
        .filter_map(|value| normalize_filter_value(Some(value)))
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn normalize_category_filter_values(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .flat_map(|value| value.split(','))
        .filter_map(normalize_category_filter_value)
        .collect::<Vec<_>>();
    normalized.sort_by_key(|category| model_category_sort_key(category));
    normalized.dedup();
    normalized
}

fn normalize_filter_value(value: Option<&str>) -> Option<String> {
    if is_blank(value) {
        return None;
    }
    Some(normalize_semantic_token(value.unwrap().trim()))
}

fn normalize_category_filter_value(value: &str) -> Option<String> {
    match value
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '-'], " ")
        .as_str()
    {
        "recommended" => Some(MODEL_CATEGORY_RECOMMENDED.to_owned()),
        "open source" => Some(MODEL_CATEGORY_OPEN_SOURCE.to_owned()),
        "proprietary" => Some(MODEL_CATEGORY_PROPRIETARY.to_owned()),
        "free" => Some(MODEL_CATEGORY_FREE.to_owned()),
        "new" => Some(MODEL_CATEGORY_NEW.to_owned()),
        value if value.trim().is_empty() => None,
        _ => Some(INVALID_MODEL_CATEGORY_FILTER.to_owned()),
    }
}

fn normalize_semantic_token(value: &str) -> String {
    snake_case(value.trim())
}

fn model_group_sort_key(group: &str) -> usize {
    match group {
        MODEL_GROUP_DEFAULT => 10,
        MODEL_GROUP_VIP => 20,
        MODEL_GROUP_ENTERPRISE => 30,
        MODEL_GROUP_BETA => 40,
        _ => usize::MAX,
    }
}

fn model_category_sort_key(category: &str) -> usize {
    match category {
        MODEL_CATEGORY_RECOMMENDED => 10,
        MODEL_CATEGORY_OPEN_SOURCE => 20,
        MODEL_CATEGORY_PROPRIETARY => 30,
        MODEL_CATEGORY_FREE => 40,
        MODEL_CATEGORY_NEW => 50,
        _ => usize::MAX,
    }
}

fn billing_meter_sort_key(billing_meter: &str) -> usize {
    match billing_meter {
        "llm_input_token" => 10,
        "llm_output_token" => 20,
        "llm_reasoning_token" => 30,
        "llm_cache_write_token" => 40,
        "llm_cache_read_token" => 50,
        "embedding_input_token" => 100,
        "image_input_token" => 200,
        "image_output_token" => 210,
        "image_result" => 220,
        "audio_input_token" => 300,
        "audio_output_token" => 310,
        "audio_input_second" => 320,
        "audio_output_second" => 330,
        "stt_audio_minute" => 340,
        "tts_input_character" => 350,
        "video_input_token" => 400,
        "video_output_token" => 410,
        "video_input_second" => 420,
        "video_output_second" => 430,
        "video_result" => 440,
        "music_output_second" => 500,
        "sfx_result" => 510,
        _ => usize::MAX,
    }
}

fn model_region_sort_key(region_code: &str) -> usize {
    match region_code.trim().to_ascii_lowercase().as_str() {
        "global" => 0,
        "cn" | "china" | "mainland" => 10,
        _ => 20,
    }
}

fn same_model_region(left: &str, right: &str) -> bool {
    normalized_model_region(left).eq_ignore_ascii_case(normalized_model_region(right))
}

fn normalized_model_region(value: &str) -> &str {
    let value = value.trim();
    if value.is_empty() {
        "global"
    } else {
        value
    }
}

fn to_price_availability(
    resolved: DomainResult<crate::application::ResolvedModelPrice>,
) -> PriceAvailability {
    match resolved {
        Ok(price) => PriceAvailability::Available(ModelCatalogPriceView {
            group_code: price.group_code,
            pricing_plan_code: price.pricing_plan_code,
            customer_unit_price: price.customer_charge.to_fixed_string(6),
            gross_margin_per_unit: price
                .gross_margin_per_unit
                .map(|margin| margin.to_fixed_string(6)),
        }),
        Err(error) => PriceAvailability::Unavailable {
            reason: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::model_group_keys;
    use crate::domain::{
        AiModel, DecimalValue, ModelUpstreamRoute, UpstreamAccountGroup,
        UpstreamAccountGroupBinding, UpstreamAccountRoute, UpstreamResourceEntitlement,
    };
    use std::collections::BTreeMap;

    fn group(id: i64, code: &str) -> UpstreamAccountGroup {
        UpstreamAccountGroup::new(id, code, "standard", DecimalValue::ONE, DecimalValue::ONE)
    }

    fn groups_by_id(groups: &[UpstreamAccountGroup]) -> BTreeMap<i64, String> {
        groups
            .iter()
            .map(|group| (group.id, group.code.clone()))
            .collect()
    }

    fn text_model(model: &str, vendor_code: &str) -> AiModel {
        AiModel::new(model, model, vendor_code, vec!["chat", "llm"])
    }

    fn vendor_entitlement(vendor_code: &str) -> UpstreamResourceEntitlement {
        let mut entitlement =
            UpstreamResourceEntitlement::new(format!("vendor.{vendor_code}"), "vendor");
        entitlement.vendor_code = Some(vendor_code.to_owned());
        entitlement
    }

    fn modality_entitlement(modality_code: &str) -> UpstreamResourceEntitlement {
        let mut entitlement =
            UpstreamResourceEntitlement::new(format!("modality.{modality_code}"), "modality");
        entitlement.modality_code = Some(modality_code.to_owned());
        entitlement
    }

    fn api_entitlement(api_code: &str) -> UpstreamResourceEntitlement {
        let mut entitlement =
            UpstreamResourceEntitlement::new(format!("api.{api_code}"), "api_endpoint");
        entitlement.api_code = Some(api_code.to_owned());
        entitlement
    }

    fn scoped_binding(
        account_group_id: i64,
        capabilities: &[&str],
        entitlements: Option<Vec<UpstreamResourceEntitlement>>,
    ) -> UpstreamAccountGroupBinding {
        let binding = UpstreamAccountGroupBinding::new_resource_scoped(
            account_group_id,
            100,
            100,
            Vec::<String>::new(),
            capabilities.iter().map(|value| (*value).to_owned()),
        );
        match entitlements {
            Some(entitlements) => binding.with_resource_entitlements(entitlements),
            None => binding,
        }
    }

    fn account_route(
        supplier_code: &str,
        account_id: i64,
        bindings: Vec<UpstreamAccountGroupBinding>,
    ) -> UpstreamAccountRoute {
        UpstreamAccountRoute::new(supplier_code, account_id).with_account_group_bindings(bindings)
    }

    #[test]
    fn group_membership_prefers_the_effective_scope_for_unscoped_accounts() {
        let groups = [group(10, "deepseek.text")];
        let by_id = groups_by_id(&groups);
        let routes = vec![account_route(
            "deepseek",
            1,
            vec![scoped_binding(10, &["llm"], None)
                .with_effective_resource_entitlements(vec![vendor_entitlement("deepseek")])],
        )];

        assert_eq!(
            vec!["deepseek.text"],
            model_group_keys(
                &text_model("deepseek-chat", "deepseek"),
                &[],
                &routes,
                &by_id,
                true
            )
        );
        assert!(
            model_group_keys(&text_model("gpt-4o", "openai"), &[], &routes, &by_id, true)
                .is_empty()
        );
    }

    #[test]
    fn group_membership_respects_vendor_resource_entitlements() {
        let groups = [group(10, "deepseek.text"), group(11, "openai.text")];
        let by_id = groups_by_id(&groups);
        let routes = vec![
            account_route(
                "deepseek",
                1,
                vec![scoped_binding(
                    10,
                    &["llm"],
                    Some(vec![vendor_entitlement("deepseek")]),
                )],
            ),
            account_route(
                "openai",
                2,
                vec![scoped_binding(
                    11,
                    &["llm"],
                    Some(vec![vendor_entitlement("openai")]),
                )],
            ),
        ];

        assert_eq!(
            vec!["deepseek.text"],
            model_group_keys(
                &text_model("deepseek-chat", "deepseek"),
                &[],
                &routes,
                &by_id,
                true
            )
        );
        assert_eq!(
            vec!["openai.text"],
            model_group_keys(&text_model("gpt-4o", "openai"), &[], &routes, &by_id, true)
        );
    }

    #[test]
    fn group_membership_does_not_count_models_outside_the_granted_modality() {
        let groups = [group(10, "aliyun.audio")];
        let by_id = groups_by_id(&groups);
        let routes = vec![account_route(
            "alibaba",
            1,
            vec![scoped_binding(
                10,
                &["audio"],
                Some(vec![modality_entitlement("audio")]),
            )],
        )];
        let mut audio_model = AiModel::new("qwen-tts", "Qwen TTS", "alibaba", vec!["audio"]);
        audio_model.modalities = vec!["audio".to_owned()];

        assert_eq!(
            vec!["aliyun.audio"],
            model_group_keys(&audio_model, &[], &routes, &by_id, true)
        );
        assert!(model_group_keys(
            &text_model("qwen-max", "alibaba"),
            &[],
            &routes,
            &by_id,
            true
        )
        .is_empty());
    }

    #[test]
    fn group_membership_uses_api_surface_only_when_no_model_scope_is_granted() {
        let groups = [group(10, "relay.text"), group(11, "deepseek.text")];
        let by_id = groups_by_id(&groups);
        let routes = vec![
            account_route(
                "relay",
                1,
                vec![scoped_binding(
                    10,
                    &["llm"],
                    Some(vec![api_entitlement("openai.chat_completions")]),
                )],
            ),
            account_route(
                "deepseek",
                2,
                vec![scoped_binding(
                    11,
                    &["llm"],
                    Some(vec![
                        vendor_entitlement("deepseek"),
                        api_entitlement("openai.chat_completions"),
                    ]),
                )],
            ),
        ];

        // The relay account only grants a callable surface, so any model whose
        // capability fits that surface is reachable through it.
        assert_eq!(
            vec!["relay.text"],
            model_group_keys(&text_model("gpt-4o", "openai"), &[], &routes, &by_id, true)
        );
        // The deepseek account also carries a model-scoped vendor grant, which
        // stays authoritative over the co-granted chat surface.
        assert_eq!(
            vec!["deepseek.text", "relay.text"],
            model_group_keys(
                &text_model("deepseek-chat", "deepseek"),
                &[],
                &routes,
                &by_id,
                true
            )
        );
        let mut image_model =
            AiModel::new("flux-pro", "FLUX Pro", "black_forest_labs", vec!["image"]);
        image_model.modalities = vec!["image".to_owned()];
        assert!(model_group_keys(&image_model, &[], &routes, &by_id, true).is_empty());
    }

    #[test]
    fn group_membership_fails_closed_for_denied_bindings() {
        let groups = [group(10, "deepseek.text")];
        let by_id = groups_by_id(&groups);
        let routes = vec![account_route(
            "deepseek",
            1,
            vec![scoped_binding(
                10,
                &["llm", "__deny__"],
                Some(vec![vendor_entitlement("deepseek")]),
            )],
        )];

        assert!(model_group_keys(
            &text_model("deepseek-chat", "deepseek"),
            &[],
            &routes,
            &by_id,
            true
        )
        .is_empty());
    }

    #[test]
    fn group_membership_treats_missing_entitlements_as_unscoped() {
        let groups = [group(10, "standard-group")];
        let by_id = groups_by_id(&groups);
        let routes = vec![account_route(
            "openrouter",
            1,
            vec![scoped_binding(10, &["tools"], None)],
        )];
        let mut tools_model = text_model("gpt-4o", "openai");
        tools_model.capabilities = vec!["chat".to_owned(), "tools".to_owned()];

        assert_eq!(
            vec!["standard-group"],
            model_group_keys(&tools_model, &[], &routes, &by_id, true)
        );
        assert!(
            model_group_keys(&text_model("o1-mini", "openai"), &[], &routes, &by_id, true)
                .is_empty()
        );
    }

    #[test]
    fn group_membership_keeps_unscoped_accounts_out_of_groups_without_a_binding() {
        let groups = [group(10, "standard-group"), group(11, "premium-lab")];
        let by_id = groups_by_id(&groups);
        let routes = vec![
            account_route("openrouter", 1, vec![scoped_binding(10, &[], None)]),
            account_route("azure_openai", 2, vec![scoped_binding(11, &[], None)]),
        ];
        let model_routes = vec![ModelUpstreamRoute::new("gpt-4o", "openrouter", 1, "gpt-4o")];

        assert_eq!(
            vec!["standard-group"],
            model_group_keys(
                &text_model("gpt-4o", "openai"),
                &model_routes,
                &routes,
                &by_id,
                true
            )
        );
    }

    #[test]
    fn group_membership_returns_every_group_when_no_binding_is_configured() {
        let groups = [group(10, "standard-group"), group(11, "premium-lab")];
        let by_id = groups_by_id(&groups);
        let model = text_model("gpt-4o", "openai");

        assert_eq!(
            vec!["premium-lab", "standard-group"],
            model_group_keys(&model, &[], &[], &by_id, false)
        );
    }
}
