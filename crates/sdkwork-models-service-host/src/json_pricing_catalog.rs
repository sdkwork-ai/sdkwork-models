use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use sdkwork_models::{load_catalog, ModelCatalog, ModelInfo, ModelPricing, VendorCatalog};
use sdkwork_models_catalog_repository_sqlx::model_catalog_import::public_catalog_identity_models;
use sdkwork_models_catalog_service::domain::{
    AiModel, AiModelPublicMetadata, BillingMeter, DecimalValue, GatewayAccessPolicy, GatewayApiKey,
    GatewayRiskRule, ModelMappingRule, ModelPrice, ModelUpstreamRoute, ModelVendor,
    ModelVendorDefinition, Money, PriceSide, PricingFormula, PricingFormulaTerm, PricingPlan,
    PricingRateCondition, PricingRateMetadata, PricingRateTier, PricingRateVariant,
    PricingSchedule, PricingWeeklyWindow, QuotaPolicy, ResolveModelMappingContext,
    UpstreamAccountGroup, UpstreamAccountGroupMetricSnapshot, UpstreamAccountRoute,
};
use sdkwork_models_catalog_service::ports::PricingCatalog;

#[derive(Debug, Clone)]
pub struct JsonPricingCatalog {
    vendors: Vec<ModelVendorDefinition>,
    models: Vec<AiModel>,
    prices: Vec<ModelPrice>,
}

impl JsonPricingCatalog {
    pub fn from_catalog(catalog: &ModelCatalog) -> Self {
        let mut vendors = BTreeMap::new();
        for vendor in &catalog.vendors {
            vendors.insert(vendor.vendor_code.clone(), map_vendor_definition(vendor));
        }

        let mut models = Vec::new();
        for (_, (vendor, model)) in public_catalog_identity_models(catalog) {
            vendors
                .entry(vendor.vendor_code.clone())
                .or_insert_with(|| map_vendor_definition(vendor));
            models.push(map_model(vendor, model));
        }

        let mut prices = Vec::new();
        for vendor in &catalog.vendors {
            for pricing in &vendor.pricing {
                prices.extend(map_vendor_pricing(vendor, pricing));
            }
        }

        Self {
            vendors: vendors.into_values().collect(),
            models,
            prices,
        }
    }

    pub fn load_from_root(root: impl AsRef<Path>) -> Result<Arc<Self>, String> {
        let catalog =
            load_catalog(root).map_err(|error| format!("load catalog JSON failed: {error}"))?;
        Ok(Arc::new(Self::from_catalog(&catalog)))
    }
}

impl PricingCatalog for JsonPricingCatalog {
    fn visit_models(&self, vendor_code: Option<&str>, visitor: &mut dyn FnMut(&AiModel) -> bool) {
        for model in self.models.iter().filter(|model| {
            model.is_publicly_active()
                && vendor_code
                    .map(|code| model.vendor_code == code)
                    .unwrap_or(true)
        }) {
            if !visitor(model) {
                break;
            }
        }
    }

    fn list_model_upstream_routes(&self, _model: &str) -> Vec<ModelUpstreamRoute> {
        Vec::new()
    }

    fn list_upstream_account_routes(&self) -> Vec<UpstreamAccountRoute> {
        Vec::new()
    }

    fn list_model_mappings(&self) -> Vec<ModelMappingRule> {
        Vec::new()
    }

    fn list_api_keys(&self) -> Vec<GatewayApiKey> {
        Vec::new()
    }

    fn list_upstream_account_groups(&self) -> Vec<UpstreamAccountGroup> {
        Vec::new()
    }

    fn list_model_prices(
        &self,
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
    ) -> Vec<ModelPrice> {
        self.prices
            .iter()
            .filter(|price| {
                price.model == model
                    && price.price_side == price_side
                    && price.billing_meter == billing_meter
            })
            .cloned()
            .collect()
    }

    fn list_model_prices_for_side(&self, model: &str, price_side: PriceSide) -> Vec<ModelPrice> {
        self.prices
            .iter()
            .filter(|price| price.model == model && price.price_side == price_side)
            .cloned()
            .collect()
    }

    fn find_api_key(&self, _api_key_id: i64) -> Option<GatewayApiKey> {
        None
    }

    fn find_api_key_by_hash(&self, _key_hash: &str) -> Option<GatewayApiKey> {
        None
    }

    fn find_upstream_account_group(&self, _group_id: i64) -> Option<UpstreamAccountGroup> {
        None
    }

    fn find_access_policy(&self, _policy_id: i64) -> Option<GatewayAccessPolicy> {
        None
    }

    fn find_quota_policy(&self, _policy_id: i64) -> Option<QuotaPolicy> {
        None
    }

    fn list_gateway_risk_rules(&self) -> Vec<GatewayRiskRule> {
        Vec::new()
    }

    fn find_latest_upstream_account_group_metric_snapshot(
        &self,
        _group_id: i64,
    ) -> Option<UpstreamAccountGroupMetricSnapshot> {
        None
    }

    fn find_pricing_plan(&self, _plan_code: &str) -> Option<PricingPlan> {
        None
    }

    fn find_model(&self, model: &str) -> Option<AiModel> {
        self.models
            .iter()
            .find(|entry| entry.catalog_key == model || entry.model == model)
            .cloned()
    }

    fn find_vendor(&self, vendor_code: &str) -> Option<ModelVendorDefinition> {
        self.vendors
            .iter()
            .find(|vendor| vendor.vendor_code == vendor_code)
            .cloned()
    }

    fn resolve_model_mapping(
        &self,
        _source_model: &str,
        _context: &ResolveModelMappingContext,
    ) -> Option<ModelMappingRule> {
        None
    }

    fn find_model_upstream_route(
        &self,
        _model: &str,
        _supplier_code: &str,
    ) -> Option<ModelUpstreamRoute> {
        None
    }

    fn find_model_price(
        &self,
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
        supplier_code: Option<&str>,
        pricing_plan_code: Option<&str>,
    ) -> Option<ModelPrice> {
        self.prices
            .iter()
            .find(|price| {
                price.model == model
                    && price.price_side == price_side
                    && price.billing_meter == billing_meter
                    && price.supplier_code.as_deref() == supplier_code
                    && price.pricing_plan_code.as_deref() == pricing_plan_code
            })
            .cloned()
    }
}

fn map_vendor_definition(vendor: &VendorCatalog) -> ModelVendorDefinition {
    ModelVendorDefinition::new(
        &vendor.vendor_code,
        ModelVendor::from_code(&vendor.vendor_code),
        &vendor.vendor.display_name,
    )
}

fn map_model(vendor: &VendorCatalog, model: &ModelInfo) -> AiModel {
    AiModel::new(
        &model.model_id,
        &model.display_name,
        &vendor.vendor_code,
        model.capabilities.iter().map(String::as_str).collect(),
    )
    .with_catalog_key(&model.catalog_key)
    .with_public_metadata(AiModelPublicMetadata {
        description: model.description.clone(),
        modalities: model
            .capabilities
            .iter()
            .chain(model.input_modalities.iter())
            .chain(model.output_modalities.iter())
            .cloned()
            .collect(),
        input_modalities: model.input_modalities.clone(),
        output_modalities: model.output_modalities.clone(),
        api_format: Some(model.api_format.clone()),
        capability_intro: None,
        limitations: model.strengths.clone(),
        supported_languages: Vec::new(),
        use_cases: Vec::new(),
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
    })
}

fn map_vendor_pricing(_vendor: &VendorCatalog, pricing: &ModelPricing) -> Vec<ModelPrice> {
    pricing
        .prices
        .iter()
        .filter_map(|price| {
            let price_side = map_price_side(&price.price_side)?;
            let billing_meter = BillingMeter::from_code(&price.meter_code);
            let currency = price
                .currency
                .clone()
                .unwrap_or_else(|| pricing.currency.clone());
            let unit_price = Money::new(&currency, &price.unit_price).ok()?;
            let unit_size = DecimalValue::parse(&price.unit_size).ok()?;
            let minimum_quantity = DecimalValue::parse(&price.minimum_quantity).ok()?;
            let quantity_step = price
                .quantity_step
                .as_deref()
                .map(DecimalValue::parse)
                .transpose()
                .ok()?;
            let effective_from = parse_effective_instant(&price.effective_from)?;
            let effective_to = match price.effective_to.as_deref() {
                Some(value) => Some(parse_effective_instant(value)?),
                None => None,
            };
            let rate_variant = PricingRateVariant::from_code(&price.rate_variant)?;
            let schedule = match price.schedule.as_ref() {
                Some(schedule) => Some(map_schedule(schedule)?),
                None => None,
            };
            let tiers = price
                .tiers
                .iter()
                .map(|tier| {
                    Some(PricingRateTier {
                        tier_code: tier.tier_code.clone(),
                        lower_bound: DecimalValue::parse(&tier.lower_bound).ok()?,
                        upper_bound: tier
                            .upper_bound
                            .as_deref()
                            .map(DecimalValue::parse)
                            .transpose()
                            .ok()?,
                        unit_size: DecimalValue::parse(&tier.unit_size).ok()?,
                        unit_price: Money::new(&currency, &tier.unit_price).ok()?,
                        flat_amount: Money::new(&currency, &tier.flat_amount).ok()?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            let formula = match price.formula.as_ref() {
                Some(formula) => Some(PricingFormula {
                    formula_code: formula.formula_code.clone(),
                    formula_version: formula.formula_version.clone(),
                    constant_units: DecimalValue::parse(&formula.constant_units).ok()?,
                    quantity_coefficient: DecimalValue::parse(&formula.quantity_coefficient)
                        .ok()?,
                    minimum_units: formula
                        .minimum_units
                        .as_deref()
                        .map(DecimalValue::parse)
                        .transpose()
                        .ok()?,
                    maximum_units: formula
                        .maximum_units
                        .as_deref()
                        .map(DecimalValue::parse)
                        .transpose()
                        .ok()?,
                    terms: formula
                        .terms
                        .iter()
                        .map(|term| {
                            Some(PricingFormulaTerm {
                                term_code: term.term_code.clone(),
                                dimension_code: term.dimension_code.clone(),
                                coefficient: DecimalValue::parse(&term.coefficient).ok()?,
                            })
                        })
                        .collect::<Option<Vec<_>>>()?,
                }),
                None => None,
            };
            Some(ModelPrice {
                catalog_key: pricing.catalog_key.clone(),
                model: pricing.catalog_key.clone(),
                region_code: pricing.region_code.clone(),
                price_side,
                billing_meter,
                unit_size,
                supplier_code: None,
                account_id: None,
                pricing_plan_code: None,
                unit_price,
                rate_metadata: Some(PricingRateMetadata {
                    record_identity: None,
                    price_book_code: price.price_book_code.clone(),
                    rate_hash: price.rate_hash.clone(),
                    product_code: price.product_code.clone(),
                    operation_code: price.operation_code.clone(),
                    billability: price.billability.clone(),
                    charge_timing: price.charge_timing.clone(),
                    calculation_mode: price.calculation_mode.clone(),
                    quantity_aggregation: price.quantity_aggregation.clone(),
                    minimum_quantity,
                    quantity_step,
                    priority: price.priority,
                    effective_from,
                    effective_to,
                    rate_variant,
                    schedule,
                    conditions: price
                        .conditions
                        .iter()
                        .map(|condition| PricingRateCondition {
                            dimension_code: condition.dimension_code.clone(),
                            operator_code: condition.operator.clone(),
                            value: condition.value.clone(),
                        })
                        .collect(),
                    tiers,
                    formula,
                }),
            })
        })
        .collect()
}

fn parse_effective_instant(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|value| value.and_utc())
        })
}

fn map_schedule(schedule: &sdkwork_models::PriceSchedule) -> Option<PricingSchedule> {
    Some(PricingSchedule {
        time_zone: schedule.time_zone.parse::<chrono_tz::Tz>().ok()?,
        weekly_windows: schedule
            .weekly_windows
            .iter()
            .map(|window| {
                Some(PricingWeeklyWindow {
                    window_code: window.window_code.clone(),
                    days_of_week: window.days_of_week.clone(),
                    start_time: NaiveTime::parse_from_str(&window.start_time, "%H:%M:%S").ok()?,
                    end_time: NaiveTime::parse_from_str(&window.end_time, "%H:%M:%S").ok()?,
                    end_day_offset: window.end_day_offset,
                    holiday_only: window.holiday_only,
                })
            })
            .collect::<Option<Vec<_>>>()?,
        include_dates: schedule
            .include_dates
            .iter()
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .collect::<Option<Vec<_>>>()?,
        exclude_dates: schedule
            .exclude_dates
            .iter()
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .collect::<Option<Vec<_>>>()?,
        holiday_dates: schedule
            .holiday_dates
            .iter()
            .map(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
            .collect::<Option<Vec<_>>>()?,
    })
}

fn map_price_side(value: &str) -> Option<PriceSide> {
    match value {
        "official" | "official_reference" | "reference" => Some(PriceSide::OfficialReference),
        "upstream" | "upstream_cost" => Some(PriceSide::UpstreamCost),
        "customer" | "customer_charge" => Some(PriceSide::CustomerCharge),
        "internal" | "internal_transfer" => Some(PriceSide::InternalTransfer),
        _ => None,
    }
}

fn release_stage_code(value: &str) -> i32 {
    match value {
        "preview" => 2,
        "deprecated" => 3,
        "retired" => 4,
        _ => 1,
    }
}

fn shelf_state_code(value: &str) -> i32 {
    match value {
        "hidden" => 2,
        "archived" => 3,
        _ => 1,
    }
}

fn routing_state_code(value: &str) -> i32 {
    match value {
        "enabled" => 1,
        _ => 0,
    }
}
