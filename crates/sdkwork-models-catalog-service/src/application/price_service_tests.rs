use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use serde_json::json;

use super::{
    BillingStrategyKind, PriceResolutionFailureCode, PriceResolutionStatus, PriceService,
    PricingResolver, ResolveModelPriceQuery, ResourceBillability,
};
use crate::domain::{
    AccountRateCard, AiModel, BillingMeter, DecimalValue, GatewayAccessPolicy, GatewayApiKey,
    GatewayRiskRule, ModelMappingRule, ModelPrice, ModelUpstreamRoute, ModelVendor,
    ModelVendorDefinition, Money, PriceSide, PricingDimensionContext, PricingFormula,
    PricingFormulaTerm, PricingPlan, PricingRateCondition, PricingRateMetadata, PricingRateTier,
    PricingRateVariant, PricingRule, PricingSchedule, PricingWeeklyWindow, QuotaPolicy,
    ResolveModelMappingContext, ResourceDefinition, UpstreamAccountGroup,
    UpstreamAccountGroupMetricSnapshot, UpstreamAccountRoute,
};
use crate::ports::PricingCatalog;

const CATALOG_KEY: &str = "openai/test-model";
const MODEL_ID: &str = "test-model";
const API_CODE: &str = "openai.responses";
const PRODUCT_CODE: &str = "openai-model-api";
const OPERATION_CODE: &str = "responses.create";
const GROUP_ID: i64 = 1;
const SUPPLIER_CODE: &str = "supplier-test";
const ACCOUNT_ID: i64 = 9;

#[derive(Default)]
struct TestPricingCatalog {
    vendors: Vec<ModelVendorDefinition>,
    models: Vec<AiModel>,
    groups: Vec<UpstreamAccountGroup>,
    plans: Vec<PricingPlan>,
    rules: Vec<PricingRule>,
    rate_cards: Vec<AccountRateCard>,
    prices: Vec<ModelPrice>,
    account_routes: Vec<UpstreamAccountRoute>,
}

impl TestPricingCatalog {
    fn with_prices(prices: Vec<ModelPrice>) -> Self {
        Self {
            vendors: vec![ModelVendorDefinition::new(
                "openai",
                ModelVendor::OpenAi,
                "OpenAI",
            )],
            models: vec![AiModel::new(MODEL_ID, "Test model", "openai", vec!["chat"])
                .with_catalog_key(CATALOG_KEY)],
            groups: vec![UpstreamAccountGroup::new_scoped(
                GROUP_ID,
                10,
                20,
                "default",
                "standard",
                DecimalValue::ONE,
                DecimalValue::ONE,
            )],
            plans: vec![PricingPlan::new(
                "standard",
                PriceSide::OfficialReference,
                DecimalValue::ONE,
                Money::usd("0").expect("valid zero price"),
            )],
            rules: Vec::new(),
            rate_cards: Vec::new(),
            prices,
            account_routes: Vec::new(),
        }
    }

    fn with_account_route(mut self, route: UpstreamAccountRoute) -> Self {
        self.account_routes.push(route);
        self
    }
}

impl PricingCatalog for TestPricingCatalog {
    fn visit_models(&self, vendor_code: Option<&str>, visitor: &mut dyn FnMut(&AiModel) -> bool) {
        for model in self.models.iter().filter(|model| {
            vendor_code
                .map(|vendor_code| model.vendor_code == vendor_code)
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
        self.account_routes.clone()
    }

    fn list_model_mappings(&self) -> Vec<ModelMappingRule> {
        Vec::new()
    }

    fn list_api_keys(&self) -> Vec<GatewayApiKey> {
        Vec::new()
    }

    fn list_upstream_account_groups(&self) -> Vec<UpstreamAccountGroup> {
        self.groups.clone()
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
                price.catalog_key == model
                    && price.price_side == price_side
                    && price.billing_meter == billing_meter
            })
            .cloned()
            .collect()
    }

    fn list_model_prices_for_side(&self, model: &str, price_side: PriceSide) -> Vec<ModelPrice> {
        self.prices
            .iter()
            .filter(|price| price.catalog_key == model && price.price_side == price_side)
            .cloned()
            .collect()
    }

    fn find_api_key(&self, _api_key_id: i64) -> Option<GatewayApiKey> {
        None
    }

    fn find_api_key_by_hash(&self, _key_hash: &str) -> Option<GatewayApiKey> {
        None
    }

    fn find_upstream_account_group(&self, account_group_id: i64) -> Option<UpstreamAccountGroup> {
        self.groups
            .iter()
            .find(|group| group.id == account_group_id)
            .cloned()
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
        _account_group_id: i64,
    ) -> Option<UpstreamAccountGroupMetricSnapshot> {
        None
    }

    fn find_pricing_plan(&self, plan_code: &str) -> Option<PricingPlan> {
        self.plans
            .iter()
            .find(|plan| plan.plan_code == plan_code)
            .cloned()
    }

    fn list_pricing_rules(&self, plan_code: &str) -> Vec<PricingRule> {
        self.rules
            .iter()
            .filter(|rule| rule.plan_code == plan_code)
            .cloned()
            .collect()
    }

    fn list_account_rate_cards(
        &self,
        tenant_id: i64,
        organization_id: i64,
    ) -> Vec<AccountRateCard> {
        self.rate_cards
            .iter()
            .filter(|card| {
                (card.tenant_id == tenant_id && card.organization_id == organization_id)
                    || (card.tenant_id == 0 && card.organization_id == 0)
            })
            .cloned()
            .collect()
    }

    fn list_pricing_rules_for_plan(
        &self,
        tenant_id: i64,
        organization_id: i64,
        pricing_plan_id: i64,
        plan_code: &str,
    ) -> Vec<PricingRule> {
        self.rules
            .iter()
            .filter(|rule| {
                rule.tenant_id == tenant_id
                    && rule.organization_id == organization_id
                    && rule.pricing_plan_id == pricing_plan_id
                    && rule.plan_code == plan_code
            })
            .cloned()
            .collect()
    }

    fn find_pricing_plan_by_identity(
        &self,
        tenant_id: i64,
        organization_id: i64,
        pricing_plan_id: i64,
        plan_code: &str,
    ) -> Option<PricingPlan> {
        self.plans
            .iter()
            .find(|plan| {
                plan.id == pricing_plan_id
                    && plan.tenant_id == tenant_id
                    && plan.organization_id == organization_id
                    && plan.plan_code == plan_code
            })
            .cloned()
    }

    fn find_model(&self, model: &str) -> Option<AiModel> {
        self.models
            .iter()
            .find(|candidate| candidate.catalog_key == model)
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
                price.catalog_key == model
                    && price.price_side == price_side
                    && price.billing_meter == billing_meter
                    && price.supplier_code.as_deref() == supplier_code
                    && price.pricing_plan_code.as_deref() == pricing_plan_code
            })
            .cloned()
    }
}

fn decimal(value: &str) -> DecimalValue {
    DecimalValue::parse(value).expect("valid decimal")
}

fn metadata(
    rate_hash: &str,
    billability: &str,
    calculation_mode: &str,
    minimum_quantity: &str,
    quantity_step: Option<&str>,
    priority: i32,
    conditions: Vec<PricingRateCondition>,
) -> PricingRateMetadata {
    PricingRateMetadata {
        record_identity: None,
        price_book_code: "openai-cn-usd".to_owned(),
        rate_hash: rate_hash.to_owned(),
        product_code: PRODUCT_CODE.to_owned(),
        operation_code: OPERATION_CODE.to_owned(),
        billability: billability.to_owned(),
        charge_timing: "usage_reported".to_owned(),
        calculation_mode: calculation_mode.to_owned(),
        quantity_aggregation: "sum".to_owned(),
        minimum_quantity: decimal(minimum_quantity),
        quantity_step: quantity_step.map(decimal),
        priority,
        effective_from: Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid timestamp"),
        effective_to: None,
        rate_variant: PricingRateVariant::Standard,
        schedule: None,
        conditions,
        tiers: Vec::new(),
        formula: None,
    }
}

fn official_price(
    meter: BillingMeter,
    unit_size: &str,
    unit_price: &str,
    metadata: PricingRateMetadata,
) -> ModelPrice {
    official_price_in_region(meter, unit_size, unit_price, "cn", metadata)
}

fn official_price_in_region(
    meter: BillingMeter,
    unit_size: &str,
    unit_price: &str,
    region_code: &str,
    metadata: PricingRateMetadata,
) -> ModelPrice {
    ModelPrice::new_for_catalog_key(
        CATALOG_KEY,
        MODEL_ID,
        PriceSide::OfficialReference,
        meter,
        Money::usd(unit_price).expect("valid price"),
    )
    .with_region_code(region_code)
    .with_unit_size(decimal(unit_size))
    .with_rate_metadata(metadata)
}

fn resource(meter: BillingMeter) -> ResourceDefinition {
    resource_in_region(meter, "cn")
}

fn resource_in_region(meter: BillingMeter, region_code: &str) -> ResourceDefinition {
    ResourceDefinition::new(
        CATALOG_KEY,
        meter,
        Utc.with_ymd_and_hms(2026, 8, 15, 0, 0, 0)
            .single()
            .expect("valid timestamp"),
    )
    .with_pricing_subject(0, Some(GROUP_ID))
    .with_vendor_code("openai")
    .with_region_code(region_code)
    .with_model(MODEL_ID)
    .with_api_code(API_CODE)
    .with_product_operation(PRODUCT_CODE, OPERATION_CODE)
}

/// A resource carrying the admin default billing region setting. The resolver
/// falls back to it when the requested region has no price, before `global`.
fn resource_in_region_with_default(
    meter: BillingMeter,
    region_code: &str,
    default_region_code: &str,
) -> ResourceDefinition {
    resource_in_region(meter, region_code)
        .with_default_billing_region(Some(default_region_code.to_owned()))
}

/// A deployment started with a default region (for example `cn`) must still
/// rate when the price book only carries `global` rates. The resolver's region
/// fallback selects the global rate, and the resource guard must accept it
/// instead of rejecting the resolution as a region mismatch - that rejection
/// is what made correctly configured catalogs fail with "cost price not
/// found".
#[test]
fn regional_price_missing_falls_back_to_the_global_region() {
    let global_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.001",
        "global",
        metadata(
            "global-input",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![global_rate]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource_in_region(BillingMeter::LlmInputToken, "cn"),
        )
        .expect("price resolution succeeds");

    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    assert!(
        resolution.failure.is_none(),
        "a region fallback is not a resource mismatch: {:?}",
        resolution.failure
    );
    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!("global", identity.region_code);
}

/// Each fallback probe must be matched against its own region dimension.
/// Probing `global` with the original `cn` dimension silently discarded every
/// conditional global rate, so the fallback existed but never fired.
#[test]
fn conditional_global_rate_matches_its_own_region_during_the_fallback() {
    let conditional = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.002",
        "global",
        metadata(
            "global-conditional",
            "chargeable",
            "per_unit",
            "0",
            None,
            10,
            vec![PricingRateCondition {
                dimension_code: "region_code".to_owned(),
                operator_code: "eq".to_owned(),
                value: json!("global"),
            }],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![conditional]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource_in_region(BillingMeter::LlmInputToken, "cn"),
        )
        .expect("price resolution succeeds");

    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    assert!(
        resolution.failure.is_none(),
        "conditional global rate must survive the fallback: {:?}",
        resolution.failure
    );
    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!(Some("global-conditional"), identity.rate_hash.as_deref());
    assert_eq!("global", identity.region_code);
}

/// The terminal "any region" probe guarantees the resolved price is never
/// empty, even when the price book carries neither the requested region nor
/// `global`.
#[test]
fn price_book_with_only_an_unrelated_region_still_rates() {
    let only_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.003",
        "us",
        metadata("us-input", "chargeable", "per_unit", "0", None, 100, vec![]),
    );
    let catalog = TestPricingCatalog::with_prices(vec![only_rate]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource_in_region(BillingMeter::LlmInputToken, "cn"),
        )
        .expect("price resolution succeeds");

    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!("us", identity.region_code);
}

/// The admin default billing region is the first fallback when the requested
/// region has no price: a request for `us` must rate against the configured
/// default `cn` rate before borrowing the generic `global` bucket.
#[test]
fn requested_region_without_a_price_falls_back_to_the_default_region() {
    let cn_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.002",
        "cn",
        metadata("cn-input", "chargeable", "per_unit", "0", None, 100, vec![]),
    );
    let global_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.001",
        "global",
        metadata(
            "global-input",
            "chargeable",
            "per_unit",
            "0",
            None,
            50,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![cn_rate, global_rate]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource_in_region_with_default(BillingMeter::LlmInputToken, "us", "cn"),
        )
        .expect("price resolution succeeds");

    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    assert!(
        resolution.failure.is_none(),
        "a default-region fallback is not a resource mismatch: {:?}",
        resolution.failure
    );
    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!("cn", identity.region_code);
    assert_eq!(Some("cn-input"), identity.rate_hash.as_deref());
}

/// A priced requested region always wins over the configured default region:
/// the default is a fallback, never an override for an explicitly requested
/// region that the price book carries.
#[test]
fn a_priced_requested_region_wins_over_the_default_region() {
    let us_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.004",
        "us",
        metadata("us-input", "chargeable", "per_unit", "0", None, 100, vec![]),
    );
    let cn_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.002",
        "cn",
        metadata("cn-input", "chargeable", "per_unit", "0", None, 100, vec![]),
    );
    let catalog = TestPricingCatalog::with_prices(vec![us_rate, cn_rate]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource_in_region_with_default(BillingMeter::LlmInputToken, "us", "cn"),
        )
        .expect("price resolution succeeds");

    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!("us", identity.region_code);
    assert_eq!(Some("us-input"), identity.rate_hash.as_deref());
}

/// Without a configured default region the chain keeps the legacy behavior:
/// the requested region falls back to `global` (never to an arbitrary
/// regional price before the generic bucket).
#[test]
fn without_a_default_region_the_chain_falls_back_to_global_only() {
    let cn_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.002",
        "cn",
        metadata("cn-input", "chargeable", "per_unit", "0", None, 100, vec![]),
    );
    let global_rate = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.001",
        "global",
        metadata(
            "global-input",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![cn_rate, global_rate]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource_in_region(BillingMeter::LlmInputToken, "us"),
        )
        .expect("price resolution succeeds");

    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!("global", identity.region_code);
    assert_eq!(Some("global-input"), identity.rate_hash.as_deref());
}

/// The upstream cost anchors the resolution currency. When the two sides of
/// the margin live in different regions priced in different currencies (a cn
/// CNY official reference and a global USD upstream cost), the official
/// reference follows the upstream cost's currency where possible, and a
/// residual cross-currency margin is simply not reported. The resolution must
/// keep a usable price instead of failing inside Money arithmetic with a bare
/// `money currency mismatch` - the failure that broke group-bound routes with
/// `pricing is not available ... money currency mismatch`.
#[test]
fn cross_currency_price_book_rates_without_a_money_mismatch() {
    let official = ModelPrice::new_for_catalog_key(
        CATALOG_KEY,
        MODEL_ID,
        PriceSide::OfficialReference,
        BillingMeter::ApiRequest,
        Money::cny("0.120000").expect("valid cny price"),
    )
    .with_region_code("cn")
    .with_unit_size(decimal("1"))
    .with_rate_metadata(metadata(
        "cn-api",
        "chargeable",
        "per_unit",
        "0",
        None,
        100,
        vec![],
    ));
    let upstream_cost = ModelPrice::new_for_catalog_key(
        CATALOG_KEY,
        MODEL_ID,
        PriceSide::UpstreamCost,
        BillingMeter::ApiRequest,
        Money::usd("0.080000").expect("valid usd price"),
    )
    .with_region_code("global")
    .with_unit_size(decimal("1"))
    .for_upstream_account(SUPPLIER_CODE, ACCOUNT_ID);
    let catalog = TestPricingCatalog::with_prices(vec![official, upstream_cost])
        .with_account_route(
            UpstreamAccountRoute::new(SUPPLIER_CODE, ACCOUNT_ID)
                .with_region_code("global")
                .with_account_group_binding(GROUP_ID, 100, 100),
        );

    let resolved = PricingResolver::new(&catalog)
        .resolve(ResolveModelPriceQuery {
            api_key_id: 0,
            account_group_id: Some(GROUP_ID),
            model: CATALOG_KEY.to_owned(),
            billing_meter: BillingMeter::ApiRequest,
            supplier_code: Some(SUPPLIER_CODE.to_owned()),
            account_id: Some(ACCOUNT_ID),
            region_code: Some("cn".to_owned()),
            default_region_code: None,
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 15, 0, 0, 0)
                .single()
                .expect("valid timestamp"),
        })
        .expect("a cross-currency price book must still rate");

    assert_eq!("CNY", resolved.official_reference.unit_price.currency);
    let procurement_cost = resolved.procurement_cost.expect("procurement cost");
    assert_eq!("USD", procurement_cost.currency);
    assert!(
        resolved.gross_margin_per_unit.is_none(),
        "a cross-currency margin is not reported, never a failure"
    );
    assert_eq!("CNY", resolved.customer_charge.currency);
}

/// The resource guard and the resolver's probe chain must agree by
/// construction: whatever the chain can select, the resolution accepts. The
/// configured default billing region joins the chain after the requested
/// region and before `global`, so a rate resolved through it is also accepted.
#[test]
fn region_guard_accepts_every_region_the_fallback_chain_can_select() {
    use super::pricing_resolver::region_matches_or_fallback;

    // No default region configured: requested -> global -> any.
    assert!(region_matches_or_fallback("cn", "cn", None));
    assert!(region_matches_or_fallback("cn", "global", None));
    assert!(
        region_matches_or_fallback("cn", "us", None),
        "the terminal fallback keeps the price non-empty"
    );
    assert!(region_matches_or_fallback("global", "global", None));
    assert!(region_matches_or_fallback("global", "cn", None));
    assert!(
        region_matches_or_fallback("", "cn", None),
        "no requested region accepts any rate"
    );
    assert!(
        region_matches_or_fallback("CN", "cn", None),
        "region codes compare case-insensitively"
    );

    // Default region configured: requested -> default -> global -> any.
    assert!(
        region_matches_or_fallback("us", "cn", Some("cn")),
        "a rate resolved through the configured default region is accepted"
    );
    assert!(
        region_matches_or_fallback("us", "global", Some("cn")),
        "global remains reachable after the default-region probe"
    );
    assert!(
        region_matches_or_fallback("us", "eu", Some("cn")),
        "the terminal fallback still accepts an unrelated region"
    );
}

/// The upstream route is only a region hint for pricing. A missing hint (and
/// a missing upstream cost price) must degrade to "no procurement cost"
/// rather than failing the whole resolution: the customer charge derives from
/// the official reference, and a zero-priced record / free ride was the old
/// failure mode of treating the missing cost as a classified-but-non-fatal
/// `price_not_found`.
#[test]
fn missing_upstream_route_hint_does_not_fail_the_resolution() {
    let official = official_price_in_region(
        BillingMeter::LlmInputToken,
        "1000",
        "0.001",
        "cn",
        metadata("cn-input", "chargeable", "per_unit", "0", None, 100, vec![]),
    );
    let catalog = TestPricingCatalog::with_prices(vec![official]);

    let resolved = PricingResolver::new(&catalog)
        .resolve(ResolveModelPriceQuery {
            api_key_id: 0,
            account_group_id: Some(GROUP_ID),
            model: CATALOG_KEY.to_owned(),
            billing_meter: BillingMeter::LlmInputToken,
            supplier_code: Some("supplier-test".to_owned()),
            account_id: Some(9),
            region_code: Some("cn".to_owned()),
            default_region_code: None,
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 15, 0, 0, 0)
                .single()
                .expect("valid timestamp"),
        })
        .expect("missing upstream cost must degrade, not fail");

    // Procurement cost is unreported, but the customer charge is derived from
    // the priced official reference — never a silent zero.
    assert!(resolved.procurement_cost.is_none());
    assert!(!resolved.official_reference.unit_price.is_zero());
    assert!(
        !resolved.customer_charge.unit_price.is_zero(),
        "customer charge must stay priced when only the upstream cost is missing"
    );
}

#[test]
fn resolves_condition_specific_rate_by_vendor_region_api_and_model() {
    let generic = official_price(
        BillingMeter::LlmInputToken,
        "1000",
        "0.001",
        metadata("generic", "chargeable", "per_unit", "0", None, 100, vec![]),
    );
    let conditional = official_price(
        BillingMeter::LlmInputToken,
        "1000",
        "0.002",
        metadata(
            "responses-cn",
            "chargeable",
            "per_unit",
            "0",
            None,
            10,
            vec![PricingRateCondition {
                dimension_code: "api_code".to_owned(),
                operator_code: "eq".to_owned(),
                value: json!(API_CODE),
            }],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![generic, conditional]);

    let resolution = PriceService::new()
        .resolve(&catalog, resource(BillingMeter::LlmInputToken))
        .expect("price resolution succeeds");

    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    assert_eq!(ResourceBillability::Chargeable, resolution.billability);
    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!(Some("responses-cn"), identity.rate_hash.as_deref());
    assert_eq!("openai", identity.vendor_code);
    assert_eq!("cn", identity.region_code);
    assert_eq!(CATALOG_KEY, identity.catalog_key);
}

/// The digital-human catalog prices two quality modes off one meter: an unconditional rate at
/// the vendor's documented default (`std`) and a `quality eq pro` rate for the dearer mode,
/// both at priority 100. Condition count is therefore the *only* thing separating them, so
/// this pins the ordering in `select_rate` that lets the override win. Without it a pro request
/// settles at the default-mode price — half the published rate — and nothing else in the
/// pipeline would notice, because a rate did resolve.
#[test]
fn a_conditioned_rate_outranks_an_unconditional_one_at_equal_priority() {
    let default_mode = official_price(
        BillingMeter::VideoOutputSecond,
        "1",
        "0.400000",
        metadata(
            "avatar-default-mode-std",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let pro_mode = official_price(
        BillingMeter::VideoOutputSecond,
        "1",
        "0.800000",
        metadata(
            "avatar-mode-pro",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![PricingRateCondition {
                dimension_code: "quality".to_owned(),
                operator_code: "eq".to_owned(),
                value: json!("pro"),
            }],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![default_mode, pro_mode]);

    let with_pro = resource(BillingMeter::VideoOutputSecond)
        .with_dimensions(PricingDimensionContext::new().with_value("quality", json!("pro")));
    let resolution = PriceService::new()
        .resolve(&catalog, with_pro)
        .expect("price resolution succeeds");
    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!(Some("avatar-mode-pro"), identity.rate_hash.as_deref());

    // An omitted `mode` is the vendor's default, so it must keep matching the default-mode
    // rate rather than the override.
    let without_mode =
        resource(BillingMeter::VideoOutputSecond).with_dimensions(PricingDimensionContext::new());
    let resolution = PriceService::new()
        .resolve(&catalog, without_mode)
        .expect("price resolution succeeds");
    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    let identity = resolution.rate_identity.expect("resolved rate identity");
    assert_eq!(
        Some("avatar-default-mode-std"),
        identity.rate_hash.as_deref()
    );
}

#[test]
fn active_sales_rule_overrides_the_official_reference_price() {
    let official = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.010000000000",
        metadata(
            "official-api",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let mut catalog = TestPricingCatalog::with_prices(vec![official]);
    let plan = catalog.plans[0].clone();
    let mut sales_rule = default_rule(
        42,
        &plan,
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid rule timestamp"),
    );
    sales_rule.rule_code = "sales-api-request".to_owned();
    sales_rule.product_code = Some(PRODUCT_CODE.to_owned());
    sales_rule.operation_code = Some(OPERATION_CODE.to_owned());
    sales_rule.meter_code = Some(BillingMeter::ApiRequest.code().to_owned());
    sales_rule.catalog_key = Some(CATALOG_KEY.to_owned());
    sales_rule.formula_mode = "unit_price_override".to_owned();
    sales_rule.unit_price_override = Some(Money::usd("0.025000000000").expect("valid sales price"));
    catalog.rules.push(sales_rule);

    let resolved = PricingResolver::new(&catalog)
        .resolve(ResolveModelPriceQuery {
            api_key_id: 0,
            account_group_id: Some(GROUP_ID),
            model: CATALOG_KEY.to_owned(),
            billing_meter: BillingMeter::ApiRequest,
            supplier_code: None,
            account_id: None,
            region_code: Some("cn".to_owned()),
            default_region_code: None,
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 18, 0, 0, 0)
                .single()
                .expect("valid occurrence"),
        })
        .expect("sales price resolves");

    assert_eq!(
        decimal("0.010000000000"),
        resolved.official_reference.unit_price.unit_price
    );
    assert_eq!(
        decimal("0.025000000000"),
        resolved.customer_charge.unit_price
    );
    assert_eq!(
        Some(42),
        resolved
            .pricing_record_identity
            .pricing_rule
            .map(|identity| identity.id)
    );
}

#[test]
fn expired_sales_rule_falls_back_to_the_official_reference_price() {
    let official = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.010000000000",
        metadata(
            "official-api",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let mut catalog = TestPricingCatalog::with_prices(vec![official]);
    let plan = catalog.plans[0].clone();
    let mut expired_rule = default_rule(
        43,
        &plan,
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid rule timestamp"),
    );
    expired_rule.rule_code = "expired-sales-api-request".to_owned();
    expired_rule.product_code = Some(PRODUCT_CODE.to_owned());
    expired_rule.operation_code = Some(OPERATION_CODE.to_owned());
    expired_rule.meter_code = Some(BillingMeter::ApiRequest.code().to_owned());
    expired_rule.catalog_key = Some(CATALOG_KEY.to_owned());
    expired_rule.formula_mode = "unit_price_override".to_owned();
    expired_rule.unit_price_override =
        Some(Money::usd("0.025000000000").expect("valid sales price"));
    expired_rule.effective_to = Some(
        Utc.with_ymd_and_hms(2026, 8, 17, 0, 0, 0)
            .single()
            .expect("valid expiry"),
    );
    catalog.rules.push(expired_rule);

    let resolved = PricingResolver::new(&catalog)
        .resolve(ResolveModelPriceQuery {
            api_key_id: 0,
            account_group_id: Some(GROUP_ID),
            model: CATALOG_KEY.to_owned(),
            billing_meter: BillingMeter::ApiRequest,
            supplier_code: None,
            account_id: None,
            region_code: Some("cn".to_owned()),
            default_region_code: None,
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 18, 0, 0, 0)
                .single()
                .expect("valid occurrence"),
        })
        .expect("official fallback resolves");

    assert_eq!(
        decimal("0.010000000000"),
        resolved.customer_charge.unit_price
    );
    assert!(resolved.pricing_record_identity.pricing_rule.is_none());
}

#[test]
fn product_scoped_sales_rule_does_not_leak_to_another_model() {
    let official = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.010000000000",
        metadata(
            "official-api",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let mut catalog = TestPricingCatalog::with_prices(vec![official]);
    let plan = catalog.plans[0].clone();
    let mut other_model_rule = default_rule(
        44,
        &plan,
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid rule timestamp"),
    );
    other_model_rule.rule_code = "other-model-sales-price".to_owned();
    other_model_rule.product_code = Some("another-model".to_owned());
    other_model_rule.meter_code = Some(BillingMeter::ApiRequest.code().to_owned());
    other_model_rule.catalog_key = Some("openai/cn/another-model".to_owned());
    other_model_rule.formula_mode = "unit_price_override".to_owned();
    other_model_rule.unit_price_override =
        Some(Money::usd("0.025000000000").expect("valid sales price"));
    catalog.rules.push(other_model_rule);

    let resolved = PricingResolver::new(&catalog)
        .resolve(ResolveModelPriceQuery {
            api_key_id: 0,
            account_group_id: Some(GROUP_ID),
            model: CATALOG_KEY.to_owned(),
            billing_meter: BillingMeter::ApiRequest,
            supplier_code: None,
            account_id: None,
            region_code: Some("cn".to_owned()),
            default_region_code: None,
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 18, 0, 0, 0)
                .single()
                .expect("valid occurrence"),
        })
        .expect("official fallback resolves");

    assert_eq!(
        decimal("0.010000000000"),
        resolved.customer_charge.unit_price
    );
    assert!(resolved.pricing_record_identity.pricing_rule.is_none());
}

#[test]
fn sales_price_with_a_different_currency_fails_before_billing() {
    let official = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.010000000000",
        metadata(
            "official-api",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let mut catalog = TestPricingCatalog::with_prices(vec![official]);
    let plan = catalog.plans[0].clone();
    let mut sales_rule = default_rule(
        45,
        &plan,
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid rule timestamp"),
    );
    sales_rule.product_code = Some(PRODUCT_CODE.to_owned());
    sales_rule.operation_code = Some(OPERATION_CODE.to_owned());
    sales_rule.meter_code = Some(BillingMeter::ApiRequest.code().to_owned());
    sales_rule.catalog_key = Some(CATALOG_KEY.to_owned());
    sales_rule.formula_mode = "unit_price_override".to_owned();
    sales_rule.unit_price_override =
        Some(Money::new("CNY", "0.025000000000").expect("valid sales price"));
    catalog.rules.push(sales_rule);

    let error = PricingResolver::new(&catalog)
        .resolve(ResolveModelPriceQuery {
            api_key_id: 0,
            account_group_id: Some(GROUP_ID),
            model: CATALOG_KEY.to_owned(),
            billing_meter: BillingMeter::ApiRequest,
            supplier_code: None,
            account_id: None,
            region_code: Some("cn".to_owned()),
            default_region_code: None,
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 18, 0, 0, 0)
                .single()
                .expect("valid occurrence"),
        })
        .expect_err("currency mismatch must fail closed");

    assert!(error
        .to_string()
        .contains("pricing rule unit price override currency mismatch"));
}

#[test]
fn token_strategy_applies_minimum_step_unit_size_and_exact_decimal_amount() {
    let price = official_price(
        BillingMeter::LlmInputToken,
        "1000",
        "0.002",
        metadata(
            "token-stepped",
            "chargeable",
            "per_unit",
            "1000",
            Some("500"),
            10,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![price]);
    let resource = resource(BillingMeter::LlmInputToken).with_measured_quantity(decimal("1001"));

    let resolution = PriceService::new()
        .resolve(&catalog, resource)
        .expect("rating succeeds");
    let billing = resolution.billing.expect("rated billing structure");

    assert_eq!(PriceResolutionStatus::Rated, resolution.status);
    assert_eq!(BillingStrategyKind::TokenUsage, billing.strategy);
    assert_eq!(decimal("1001"), billing.measured_quantity);
    assert_eq!(decimal("1500"), billing.rated_quantity);
    assert_eq!(decimal("1000"), billing.unit_size);
    assert_eq!(decimal("0.003"), billing.customer_charge_amount.unit_price);
}

#[test]
fn standard_registry_selects_independent_api_image_duration_unit_and_flat_strategies() {
    for (meter, quantity, calculation_mode, expected_strategy) in [
        (
            BillingMeter::ApiRequest,
            "2",
            "per_unit",
            BillingStrategyKind::ApiCall,
        ),
        (
            BillingMeter::ImageResult,
            "3",
            "per_unit",
            BillingStrategyKind::ImageQuantity,
        ),
        (
            BillingMeter::VideoOutputSecond,
            "2.5",
            "per_unit",
            BillingStrategyKind::Duration,
        ),
        (
            BillingMeter::TtsInputCharacter,
            "250",
            "per_unit",
            BillingStrategyKind::UnitQuantity,
        ),
        (
            BillingMeter::ApiItem,
            "7",
            "flat",
            BillingStrategyKind::FlatFee,
        ),
    ] {
        let price = official_price(
            meter.clone(),
            "1",
            "0.01",
            metadata(
                &format!("{}-{calculation_mode}", meter.code()),
                "chargeable",
                calculation_mode,
                "0",
                None,
                10,
                vec![],
            ),
        );
        let catalog = TestPricingCatalog::with_prices(vec![price]);
        let resource = resource(meter).with_measured_quantity(decimal(quantity));

        let resolution = PriceService::new()
            .resolve(&catalog, resource)
            .expect("rating succeeds");

        assert_eq!(PriceResolutionStatus::Rated, resolution.status);
        assert_eq!(
            expected_strategy,
            resolution.billing.expect("billing structure").strategy
        );
    }
}

#[test]
fn flat_strategy_rejects_a_non_unit_catalog_base() {
    let price = official_price(
        BillingMeter::ApiRequest,
        "2",
        "0.01",
        metadata(
            "invalid-flat-unit-size",
            "chargeable",
            "flat",
            "0",
            None,
            10,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![price]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource(BillingMeter::ApiRequest).with_measured_quantity(DecimalValue::ONE),
        )
        .expect("invalid flat fee resolves to an unrated decision");

    assert_eq!(PriceResolutionStatus::Unrated, resolution.status);
    assert!(resolution.billing.is_none());
    assert_eq!(
        Some(PriceResolutionFailureCode::UnsupportedBillingStrategy),
        resolution.failure.as_ref().map(|failure| failure.code)
    );
    assert!(resolution
        .failure
        .expect("classified failure")
        .message
        .contains("flat fee pricing unit size must equal one"));
}

#[test]
fn explicit_free_and_not_applicable_rates_are_non_chargeable() {
    for (billability, expected) in [
        ("free", ResourceBillability::Free),
        ("not_applicable", ResourceBillability::NotApplicable),
    ] {
        let price = official_price(
            BillingMeter::ApiRequest,
            "1",
            "0",
            metadata(billability, billability, "per_unit", "0", None, 10, vec![]),
        );
        let catalog = TestPricingCatalog::with_prices(vec![price]);

        let resolution = PriceService::new()
            .resolve(
                &catalog,
                resource(BillingMeter::ApiRequest).with_measured_quantity(DecimalValue::ONE),
            )
            .expect("resolution succeeds");

        assert_eq!(PriceResolutionStatus::NonChargeable, resolution.status);
        assert_eq!(expected, resolution.billability);
        assert!(resolution.billing.is_none());
    }
}

#[test]
fn unknown_billability_fails_closed_without_a_billing_structure() {
    let price = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.01",
        metadata(
            "unknown-billability",
            "vendor_specific",
            "per_unit",
            "0",
            None,
            10,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![price]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource(BillingMeter::ApiRequest).with_measured_quantity(DecimalValue::ONE),
        )
        .expect("resolution returns a classified failure");

    assert_eq!(PriceResolutionStatus::Unrated, resolution.status);
    assert_eq!(ResourceBillability::Unknown, resolution.billability);
    assert_eq!(
        Some(PriceResolutionFailureCode::UnknownBillability),
        resolution.failure.map(|failure| failure.code)
    );
    assert!(resolution.billing.is_none());
}

#[test]
fn equally_specific_distinct_rates_fail_closed_as_ambiguous() {
    let first = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.01",
        metadata("rate-a", "chargeable", "per_unit", "0", None, 10, vec![]),
    );
    let second = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.02",
        metadata("rate-b", "chargeable", "per_unit", "0", None, 10, vec![]),
    );
    let catalog = TestPricingCatalog::with_prices(vec![first, second]);

    let resolution = PriceService::new()
        .resolve(&catalog, resource(BillingMeter::ApiRequest))
        .expect("ambiguity is classified instead of charged");

    assert_eq!(PriceResolutionStatus::Unrated, resolution.status);
    assert_eq!(
        Some(PriceResolutionFailureCode::AmbiguousRate),
        resolution.failure.map(|failure| failure.code)
    );
    assert!(resolution.billing.is_none());
}

#[test]
fn unsupported_calculation_mode_fails_closed() {
    let price = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.01",
        metadata("script-rate", "chargeable", "script", "0", None, 10, vec![]),
    );
    let catalog = TestPricingCatalog::with_prices(vec![price]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource(BillingMeter::ApiRequest).with_measured_quantity(DecimalValue::ONE),
        )
        .expect("unsupported strategy is classified");

    assert_eq!(PriceResolutionStatus::Unrated, resolution.status);
    assert_eq!(ResourceBillability::Chargeable, resolution.billability);
    assert_eq!(
        Some(PriceResolutionFailureCode::UnsupportedBillingStrategy),
        resolution.failure.map(|failure| failure.code)
    );
    assert!(resolution.billing.is_none());
}

#[test]
fn graduated_tier_strategy_rates_each_contiguous_band_exactly() {
    let mut rate_metadata = metadata(
        "graduated-rate",
        "chargeable",
        "graduated",
        "0",
        None,
        10,
        vec![],
    );
    rate_metadata.tiers = vec![
        PricingRateTier {
            tier_code: "first-10".to_owned(),
            lower_bound: decimal("0"),
            upper_bound: Some(decimal("10")),
            unit_size: decimal("1"),
            unit_price: Money::usd("1").expect("valid tier price"),
            flat_amount: Money::usd("0").expect("valid flat amount"),
        },
        PricingRateTier {
            tier_code: "over-10".to_owned(),
            lower_bound: decimal("10"),
            upper_bound: None,
            unit_size: decimal("1"),
            unit_price: Money::usd("0.5").expect("valid tier price"),
            flat_amount: Money::usd("0").expect("valid flat amount"),
        },
    ];
    let catalog = TestPricingCatalog::with_prices(vec![official_price(
        BillingMeter::ApiItem,
        "1",
        "0",
        rate_metadata,
    )]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource(BillingMeter::ApiItem).with_measured_quantity(decimal("15")),
        )
        .expect("graduated pricing succeeds");
    let billing = resolution.billing.expect("rated billing structure");

    assert_eq!(BillingStrategyKind::GraduatedTier, billing.strategy);
    assert_eq!(
        decimal("12.5"),
        billing.official_reference_amount.unit_price
    );
    assert_eq!(
        2,
        billing
            .components
            .iter()
            .filter(|item| item.price_side == PriceSide::OfficialReference)
            .count()
    );
}

#[test]
fn volume_tier_strategy_applies_the_selected_tier_to_the_whole_quantity() {
    let mut rate_metadata = metadata("volume-rate", "chargeable", "volume", "0", None, 10, vec![]);
    rate_metadata.tiers = vec![
        PricingRateTier {
            tier_code: "small".to_owned(),
            lower_bound: decimal("0"),
            upper_bound: Some(decimal("10")),
            unit_size: decimal("1"),
            unit_price: Money::usd("1").expect("valid tier price"),
            flat_amount: Money::usd("0").expect("valid flat amount"),
        },
        PricingRateTier {
            tier_code: "large".to_owned(),
            lower_bound: decimal("10"),
            upper_bound: None,
            unit_size: decimal("1"),
            unit_price: Money::usd("0.5").expect("valid tier price"),
            flat_amount: Money::usd("0").expect("valid flat amount"),
        },
    ];
    let catalog = TestPricingCatalog::with_prices(vec![official_price(
        BillingMeter::ApiItem,
        "1",
        "0",
        rate_metadata,
    )]);

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource(BillingMeter::ApiItem).with_measured_quantity(decimal("15")),
        )
        .expect("volume pricing succeeds");
    let billing = resolution.billing.expect("rated billing structure");

    assert_eq!(BillingStrategyKind::VolumeTier, billing.strategy);
    assert_eq!(decimal("7.5"), billing.official_reference_amount.unit_price);
}

#[test]
fn formula_strategy_uses_only_bounded_typed_numeric_terms() {
    let mut rate_metadata = metadata(
        "formula-rate",
        "chargeable",
        "formula",
        "0",
        None,
        10,
        vec![],
    );
    rate_metadata.formula = Some(PricingFormula {
        formula_code: "duration-weighted".to_owned(),
        formula_version: "1".to_owned(),
        constant_units: decimal("2"),
        quantity_coefficient: decimal("1.5"),
        minimum_units: None,
        maximum_units: Some(decimal("10")),
        terms: vec![PricingFormulaTerm {
            term_code: "duration".to_owned(),
            dimension_code: "duration_seconds".to_owned(),
            coefficient: decimal("0.25"),
        }],
    });
    let catalog = TestPricingCatalog::with_prices(vec![official_price(
        BillingMeter::ApiItem,
        "1",
        "0.2",
        rate_metadata,
    )]);
    let dimensions = PricingDimensionContext::new().with_value("duration_seconds", json!("8"));

    let resolution = PriceService::new()
        .resolve(
            &catalog,
            resource(BillingMeter::ApiItem)
                .with_dimensions(dimensions)
                .with_measured_quantity(decimal("4")),
        )
        .expect("formula pricing succeeds");
    let billing = resolution.billing.expect("rated billing structure");

    assert_eq!(BillingStrategyKind::Formula, billing.strategy);
    assert_eq!(decimal("10"), billing.rated_quantity);
    assert_eq!(decimal("2"), billing.official_reference_amount.unit_price);
}

#[test]
fn resolved_rate_rejects_resource_identity_mismatches() {
    let price = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.01",
        metadata(
            "matched-rate",
            "chargeable",
            "per_unit",
            "0",
            None,
            10,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![price]);
    let quoted = PriceService::new()
        .resolve(&catalog, resource(BillingMeter::ApiRequest))
        .expect("quote succeeds");
    let resolved_price = quoted.resolved_price.expect("resolved price");

    let mut mismatches = Vec::new();
    let mut vendor = resource(BillingMeter::ApiRequest);
    vendor.vendor_code = Some("google".to_owned());
    mismatches.push(vendor);
    let mut product = resource(BillingMeter::ApiRequest);
    product.product_code = Some("other-product".to_owned());
    mismatches.push(product);
    let mut operation = resource(BillingMeter::ApiRequest);
    operation.operation_code = Some("other.operation".to_owned());
    mismatches.push(operation);
    let mut catalog_key = resource(BillingMeter::ApiRequest);
    catalog_key.catalog_key = "openai/other-model".to_owned();
    mismatches.push(catalog_key);
    let meter = resource(BillingMeter::LlmInputToken);
    mismatches.push(meter);
    let mut provider = resource(BillingMeter::ApiRequest);
    provider.provider_code = Some("other-provider".to_owned());
    mismatches.push(provider);

    for mismatch in mismatches {
        let resolution = PriceService::new()
            .rate_resolved(mismatch, resolved_price.clone())
            .expect("mismatch is classified");
        assert_eq!(PriceResolutionStatus::Unrated, resolution.status);
        assert_eq!(
            Some(PriceResolutionFailureCode::ResourceMismatch),
            resolution.failure.map(|failure| failure.code)
        );
        assert!(resolution.billing.is_none());
    }
}

/// Region is deliberately not part of the identity guard: the resolver may
/// legally fall back across regions (`cn` -> `global` -> any) to keep the
/// resolved price non-empty, so a resource pinned to another region still
/// rates. Cross-region borrowing is reported by the rate identity and the
/// resolver's warning, not by failing the billing.
#[test]
fn resolved_rate_accepts_a_region_fallback() {
    let price = official_price(
        BillingMeter::ApiRequest,
        "1",
        "0.01",
        metadata(
            "matched-rate",
            "chargeable",
            "per_unit",
            "0",
            None,
            10,
            vec![],
        ),
    );
    let catalog = TestPricingCatalog::with_prices(vec![price]);
    let quoted = PriceService::new()
        .resolve(&catalog, resource(BillingMeter::ApiRequest))
        .expect("quote succeeds");
    let resolved_price = quoted.resolved_price.expect("resolved price");

    let mut fallback = resource(BillingMeter::ApiRequest);
    fallback.region_code = Some("global".to_owned());
    let resolution = PriceService::new()
        .rate_resolved(fallback, resolved_price)
        .expect("region fallback rates normally");

    assert_eq!(PriceResolutionStatus::Quoted, resolution.status);
    assert!(resolution.failure.is_none());
}

#[test]
fn time_window_rate_overrides_standard_rate_only_inside_local_window() {
    let mut time_window = metadata(
        "time-window-rate",
        "chargeable",
        "per_unit",
        "0",
        None,
        100,
        vec![],
    );
    time_window.rate_variant = PricingRateVariant::TimeWindow;
    time_window.schedule = Some(PricingSchedule {
        time_zone: "Asia/Shanghai".parse().expect("valid IANA timezone"),
        weekly_windows: vec![PricingWeeklyWindow {
            window_code: "weekday-morning".to_owned(),
            days_of_week: vec![1, 2, 3, 4, 5],
            start_time: NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
            end_time: NaiveTime::from_hms_opt(12, 0, 0).expect("valid time"),
            end_day_offset: 0,
        }],
        include_dates: vec![NaiveDate::from_ymd_opt(2026, 8, 17).expect("valid date")],
        exclude_dates: Vec::new(),
    });
    let standard = official_price(
        BillingMeter::ApiRequest,
        "1",
        "5",
        metadata(
            "standard-rate",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let scheduled = official_price(BillingMeter::ApiRequest, "1", "3", time_window);
    let catalog = TestPricingCatalog::with_prices(vec![standard, scheduled]);

    let inside = resource(BillingMeter::ApiRequest);
    let mut inside = inside;
    inside.occurred_at = Utc
        .with_ymd_and_hms(2026, 8, 17, 2, 30, 0)
        .single()
        .expect("valid timestamp");
    let inside_resolution = PriceService::new()
        .resolve(&catalog, inside)
        .expect("scheduled rate resolves");
    assert_eq!(
        decimal("3"),
        inside_resolution
            .resolved_price
            .expect("resolved price")
            .official_reference
            .unit_price
            .unit_price
    );
    assert_eq!(
        Some("weekday-morning".to_owned()),
        inside_resolution
            .rate_identity
            .expect("rate identity")
            .matched_window_code
    );

    let outside = resource(BillingMeter::ApiRequest);
    let outside_resolution = PriceService::new()
        .resolve(&catalog, outside)
        .expect("standard rate resolves");
    assert_eq!(
        decimal("5"),
        outside_resolution
            .resolved_price
            .expect("resolved price")
            .official_reference
            .unit_price
            .unit_price
    );
}

#[test]
fn historical_occurrence_selects_the_rate_effective_at_that_instant() {
    let mut historical = metadata(
        "historical-rate",
        "chargeable",
        "per_unit",
        "0",
        None,
        100,
        vec![],
    );
    historical.effective_to = Some(
        Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0)
            .single()
            .expect("valid timestamp"),
    );
    let mut current = metadata(
        "current-rate",
        "chargeable",
        "per_unit",
        "0",
        None,
        100,
        vec![],
    );
    current.effective_from = Utc
        .with_ymd_and_hms(2026, 7, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    let catalog = TestPricingCatalog::with_prices(vec![
        official_price(BillingMeter::ApiRequest, "1", "2", historical),
        official_price(BillingMeter::ApiRequest, "1", "4", current),
    ]);
    let mut resource = resource(BillingMeter::ApiRequest);
    resource.occurred_at = Utc
        .with_ymd_and_hms(2026, 6, 30, 23, 59, 59)
        .single()
        .expect("valid timestamp");

    let historical = PriceService::new()
        .resolve(&catalog, resource.clone())
        .expect("historical rate resolves");
    assert_eq!(
        Some("historical-rate"),
        historical
            .rate_identity
            .as_ref()
            .and_then(|identity| identity.rate_hash.as_deref())
    );

    resource.occurred_at = Utc
        .with_ymd_and_hms(2026, 7, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    let current = PriceService::new()
        .resolve(&catalog, resource)
        .expect("current rate resolves");
    assert_eq!(
        Some("current-rate"),
        current
            .rate_identity
            .as_ref()
            .and_then(|identity| identity.rate_hash.as_deref())
    );
}

fn scoped_plan(id: i64, plan_code: &str, multiplier: &str) -> PricingPlan {
    let mut plan = PricingPlan::new(
        plan_code,
        PriceSide::OfficialReference,
        decimal(multiplier),
        Money::usd("0").expect("valid money"),
    );
    plan.id = id;
    plan.tenant_id = 10;
    plan.organization_id = 20;
    plan
}

fn default_rule(id: i64, plan: &PricingPlan, effective_from: chrono::DateTime<Utc>) -> PricingRule {
    PricingRule {
        id,
        pricing_plan_id: plan.id,
        tenant_id: plan.tenant_id,
        organization_id: plan.organization_id,
        rule_code: format!("{}-{id}", plan.plan_code),
        plan_code: plan.plan_code.clone(),
        product_code: None,
        operation_code: None,
        meter_code: None,
        provider_code: None,
        region_code: None,
        catalog_key: None,
        formula_mode: "multiplier_markup".to_owned(),
        multiplier: DecimalValue::ONE,
        markup_amount: Money::usd("0").expect("valid money"),
        unit_price_override: None,
        priority: 100,
        effective_from,
        effective_to: None,
        conditions: Vec::new(),
        schedule: None,
    }
}

#[test]
fn account_group_rate_card_overrides_the_default_subject_and_preserves_identity() {
    let price = official_price(
        BillingMeter::ApiRequest,
        "1",
        "2",
        metadata(
            "subject-rate",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let mut catalog = TestPricingCatalog::with_prices(vec![price]);
    let standard = scoped_plan(11, "standard", "1");
    let premium = scoped_plan(12, "premium", "2");
    let effective_from = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    catalog.plans = vec![standard.clone(), premium.clone()];
    catalog.rules = vec![
        default_rule(31, &standard, effective_from),
        default_rule(32, &premium, effective_from),
    ];
    catalog.rate_cards = vec![
        AccountRateCard {
            id: 21,
            rate_card_code: "default".to_owned(),
            tenant_id: 10,
            organization_id: 20,
            subject_type: "default".to_owned(),
            subject_id: None,
            subject_code: None,
            pricing_plan_tenant_id: 10,
            pricing_plan_organization_id: 20,
            pricing_plan_id: standard.id,
            pricing_plan_code: standard.plan_code.clone(),
            priority: 100,
            effective_from,
            effective_to: None,
        },
        AccountRateCard {
            id: 22,
            rate_card_code: "group".to_owned(),
            tenant_id: 10,
            organization_id: 20,
            subject_type: "account_group".to_owned(),
            subject_id: Some(GROUP_ID),
            subject_code: None,
            pricing_plan_tenant_id: 10,
            pricing_plan_organization_id: 20,
            pricing_plan_id: premium.id,
            pricing_plan_code: premium.plan_code.clone(),
            priority: 100,
            effective_from,
            effective_to: None,
        },
    ];

    let resolution = PriceService::new()
        .resolve(&catalog, resource(BillingMeter::ApiRequest))
        .expect("group rate card resolves");
    let resolved = resolution.resolved_price.expect("resolved price");
    assert_eq!("premium", resolved.pricing_plan_code);
    assert_eq!(
        Some(22),
        resolved
            .pricing_record_identity
            .account_rate_card
            .map(|identity| identity.id)
    );
    assert_eq!(
        Some(12),
        resolved
            .pricing_record_identity
            .pricing_plan
            .map(|identity| identity.id)
    );
    assert_eq!(
        Some(32),
        resolved
            .pricing_record_identity
            .pricing_rule
            .map(|identity| identity.id)
    );
}

#[test]
fn equally_ranked_pricing_rules_fail_closed() {
    let price = official_price(
        BillingMeter::ApiRequest,
        "1",
        "2",
        metadata(
            "rule-conflict-rate",
            "chargeable",
            "per_unit",
            "0",
            None,
            100,
            vec![],
        ),
    );
    let mut catalog = TestPricingCatalog::with_prices(vec![price]);
    let plan = scoped_plan(11, "standard", "1");
    let effective_from = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    catalog.plans = vec![plan.clone()];
    catalog.groups[0].pricing_plan_id = plan.id;
    catalog.groups[0].pricing_plan_tenant_id = plan.tenant_id;
    catalog.groups[0].pricing_plan_organization_id = plan.organization_id;
    catalog.rules = vec![
        default_rule(31, &plan, effective_from),
        default_rule(32, &plan, effective_from),
    ];

    let error = PriceService::new()
        .resolve(&catalog, resource(BillingMeter::ApiRequest))
        .expect_err("ambiguous pricing rules must fail closed");
    assert!(error.to_string().contains("pricing rule ambiguous"));
}

/// Expired price rows (effective_to in the past) must never be selected:
/// a stale price book is equivalent to "no price", and the caller is
/// responsible for failing the invocation instead of billing nothing.
#[test]
fn expired_rates_are_never_selected() {
    let mut metadata = metadata("expired", "chargeable", "per_unit", "0", None, 100, vec![]);
    metadata.effective_from = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp");
    metadata.effective_to = Some(
        Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0)
            .single()
            .expect("valid timestamp"),
    );
    let price = official_price(BillingMeter::LlmInputToken, "1000", "0.001", metadata);
    let catalog = TestPricingCatalog::with_prices(vec![price]);

    let error = PricingResolver::new(&catalog)
        .resolve(ResolveModelPriceQuery {
            api_key_id: 0,
            account_group_id: Some(GROUP_ID),
            model: CATALOG_KEY.to_owned(),
            billing_meter: BillingMeter::LlmInputToken,
            supplier_code: None,
            account_id: None,
            region_code: Some("cn".to_owned()),
            default_region_code: None,
            occurred_at: Utc
                .with_ymd_and_hms(2026, 8, 31, 0, 0, 0)
                .single()
                .expect("valid timestamp"),
        })
        .expect_err("an expired rate must not be selected");

    let message = error.to_string();
    assert!(
        message.contains("price not found")
            || message.contains("official reference price not found"),
        "unexpected error: {message}"
    );
}
