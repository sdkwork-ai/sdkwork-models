use chrono::{DateTime, Utc};

use crate::domain::{
    BillingMeter, DecimalValue, DomainError, DomainResult, ModelPrice, ModelVendor, Money,
    PriceSide, PricingDimensionContext, PricingRateMetadata,
};
use crate::ports::PricingCatalog;

const DEFAULT_PRICE_REGION_CODE: &str = "global";

pub struct PricingResolver<'a, C: PricingCatalog> {
    catalog: &'a C,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveModelPriceQuery {
    pub api_key_id: i64,
    pub account_group_id: Option<i64>,
    pub model: String,
    pub billing_meter: BillingMeter,
    pub supplier_code: Option<String>,
    pub account_id: Option<i64>,
    pub region_code: Option<String>,
    /// Configured default billing region for the resource (admin "default
    /// region" setting). The fallback chain probes it after the requested
    /// region and before the generic `global` bucket, so a multi-region model
    /// keeps rating against its default regional price even when the caller
    /// pins a region the price book does not carry.
    pub default_region_code: Option<String>,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedPriceSource {
    ExplicitCustomerCharge,
    DerivedFromOfficialReference,
}

/// Which layer finally determined the customer charge.
///
/// The resolver composes up to four layers in a fixed order. Recording the
/// winning layer on the resolution (and, through it, on the rating decision)
/// is what lets an operator answer "why was this request priced this way"
/// without re-deriving the whole chain from the current configuration, which
/// may already have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomerPriceLayer {
    /// The plan's own default multiplier/markup over the official reference
    /// (or an explicit customer-charge rate book) decided the price.
    PlanReference,
    /// A plan pricing rule (`multiplier_markup` or `unit_price_override`)
    /// decided the price.
    PlanRule,
    /// A per-resource price defined on the account group replaced the plan
    /// result for this exact (resource, operation, meter, region, provider).
    GroupResource,
    /// The account group's single `sale_multiplier` scaled the plan result.
    GroupMultiplier,
}

impl CustomerPriceLayer {
    pub fn code(self) -> &'static str {
        match self {
            Self::PlanReference => "plan_reference",
            Self::PlanRule => "plan_rule",
            Self::GroupResource => "group_resource",
            Self::GroupMultiplier => "group_multiplier",
        }
    }
}

/// The resource tuple one group price is resolved against.
#[derive(Debug, Clone, Copy)]
pub struct GroupPriceResourceKey<'a> {
    pub catalog_key: &'a str,
    pub operation_code: &'a str,
    pub meter_code: &'a str,
    pub region_code: &'a str,
    pub provider_code: &'a str,
}

impl GroupPriceResourceKey<'_> {
    fn matches(
        &self,
        catalog_key: &str,
        operation_code: &str,
        meter_code: &str,
        region_code: &str,
        provider_code: &str,
    ) -> bool {
        self.catalog_key.trim() == catalog_key.trim()
            && self.operation_code.trim() == operation_code.trim()
            && self.meter_code.trim() == meter_code.trim()
            && self.region_code.trim() == region_code.trim()
            && (provider_code.trim().is_empty()
                || self.provider_code.trim() == provider_code.trim())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelPrice {
    pub model: String,
    pub vendor_code: String,
    pub vendor: ModelVendor,
    pub group_code: String,
    pub pricing_plan_code: String,
    pub supplier_code: Option<String>,
    pub billing_meter: BillingMeter,
    pub official_reference: ModelPrice,
    pub raw_upstream_cost: Option<ModelPrice>,
    pub raw_customer_charge: Option<ModelPrice>,
    pub procurement_cost: Option<Money>,
    pub account_contract_cost_multiplier: Option<DecimalValue>,
    pub account_group_cost_multiplier: Option<DecimalValue>,
    pub procurement_cost_multiplier: Option<DecimalValue>,
    pub customer_charge_before_sale_multiplier: Money,
    pub sale_multiplier: DecimalValue,
    pub reference_multiplier: DecimalValue,
    pub default_markup_amount: Money,
    pub rounding_mode: String,
    pub minimum_charge_amount: Money,
    pub fail_closed: bool,
    pub pricing_rule_multiplier: DecimalValue,
    pub pricing_rule_markup_amount: Money,
    pub pricing_rule_unit_price_override: Option<Money>,
    pub pricing_record_identity: crate::domain::PricingPolicyRecordIdentity,
    pub customer_charge: Money,
    pub gross_margin_per_unit: Option<crate::domain::DecimalValue>,
    pub source: ResolvedPriceSource,
    /// Which layer finally decided `customer_charge`.
    pub customer_price_layer: CustomerPriceLayer,
    /// The per-resource group price that decided the customer charge, when
    /// [`CustomerPriceLayer::GroupResource`] won. `None` on every other layer.
    pub group_resource_price: Option<crate::domain::GroupResourcePrice>,
}

impl<'a, C: PricingCatalog> PricingResolver<'a, C> {
    pub fn new(catalog: &'a C) -> Self {
        Self { catalog }
    }

    pub fn resolve(&self, query: ResolveModelPriceQuery) -> DomainResult<ResolvedModelPrice> {
        self.resolve_with_dimensions(query, &PricingDimensionContext::default())
    }

    pub fn resolve_with_dimensions(
        &self,
        query: ResolveModelPriceQuery,
        dimensions: &PricingDimensionContext,
    ) -> DomainResult<ResolvedModelPrice> {
        // Auth-token sessions (api_key_id == 0) carry no gateway API key
        // record: the resolved account group is the authoritative
        // tenant/organization scope, mirroring the billing-subject semantics
        // of the open-api billing guard.
        let api_key = (query.api_key_id > 0)
            .then(|| self.find_api_key(query.api_key_id))
            .transpose()?;
        let group = self.find_group(
            query
                .account_group_id
                .or_else(|| api_key.as_ref().map(|key| key.default_account_group_id))
                .ok_or_else(|| {
                    DomainError::new(
                        "account group id is required when no API key backs the pricing session",
                    )
                })?,
        )?;
        // Verify the account group belongs to the same tenant/organization as the API key,
        // or is a global resource (tenant_id == 0); auth-token sessions have no
        // API key to compare against, so the group itself is authoritative.
        if let Some(api_key) = api_key.as_ref() {
            if group.tenant_id != 0 && group.tenant_id != api_key.tenant_id {
                return Err(DomainError::new(format!(
                    "account group {} does not belong to tenant {}",
                    group.id, api_key.tenant_id
                )));
            }
            if group.organization_id != 0 && group.organization_id != api_key.organization_id {
                return Err(DomainError::new(format!(
                    "account group {} does not belong to organization {}",
                    group.id, api_key.organization_id
                )));
            }
        }
        let (tenant_id, organization_id) = match api_key.as_ref() {
            Some(api_key) => (api_key.tenant_id, api_key.organization_id),
            None => (group.tenant_id, group.organization_id),
        };
        let (plan, account_rate_card) = self.find_plan(
            tenant_id,
            organization_id,
            &group,
            api_key.as_ref(),
            query.account_id,
            query.occurred_at,
        )?;
        let model = self.find_model(&query.model)?;
        let vendor = self.find_vendor(&model.vendor_code)?;
        let explicit_region_code = query
            .region_code
            .as_deref()
            .and_then(normalized_optional_region_code);
        // A missing route hint is not fatal: pricing rates against the
        // requested region and its price-book fallbacks.
        let route = match query.supplier_code.as_deref() {
            Some(supplier_code) => self.find_upstream_route(
                &query.model,
                supplier_code,
                query.account_id,
                explicit_region_code.as_deref(),
            )?,
            None => None,
        };
        let region_code = explicit_region_code
            .or_else(|| {
                route
                    .as_ref()
                    .map(|route| normalize_region_code(&route.region_code))
            })
            .unwrap_or_else(|| DEFAULT_PRICE_REGION_CODE.to_owned());
        // The admin default-region setting joins the region fallback chain
        // between the requested region and the generic `global` bucket: when
        // the price book has no rate for the requested region, the default
        // regional price answers before any global/other-region borrow.
        let default_region_code = query
            .default_region_code
            .as_deref()
            .and_then(normalized_optional_region_code);
        let mut rate_dimensions = dimensions.clone();
        rate_dimensions.insert(
            "vendor_code",
            serde_json::json!(vendor.vendor_code.as_str()),
        );
        rate_dimensions.insert("region_code", serde_json::json!(region_code.as_str()));
        rate_dimensions.insert("catalog_key", serde_json::json!(model.catalog_key.as_str()));
        rate_dimensions.insert("model", serde_json::json!(model.model.as_str()));
        rate_dimensions.insert("meter_code", serde_json::json!(query.billing_meter.code()));
        if let Some(supplier_code) = query.supplier_code.as_deref() {
            rate_dimensions.insert("provider_code", serde_json::json!(supplier_code));
        }
        let raw_upstream_cost = self.find_upstream_cost(
            &query,
            tenant_id,
            organization_id,
            &region_code,
            default_region_code.as_deref(),
            &rate_dimensions,
        )?;
        // The upstream cost anchors the resolution currency: the official
        // reference and the explicit customer charge prefer it so a fallback
        // region priced in another currency cannot split the two sides of the
        // margin into incompatible currencies.
        let preferred_currency = raw_upstream_cost
            .as_ref()
            .map(|price| price.unit_price.currency.clone());
        let price_scope = raw_upstream_cost
            .as_ref()
            .map(|price| price.catalog_key.as_str())
            .unwrap_or(query.model.as_str());
        let official = self.find_official_reference(
            &query,
            tenant_id,
            organization_id,
            price_scope,
            &region_code,
            default_region_code.as_deref(),
            preferred_currency.as_deref(),
            &rate_dimensions,
        )?;
        // Product and operation codes are authoritative on the selected
        // official rate. ResolveModelPriceQuery intentionally stays small and
        // does not duplicate those catalog fields, so add them before sales
        // rule scope matching.
        if let Some(metadata) = official.rate_metadata.as_ref() {
            rate_dimensions.insert(
                "product_code",
                serde_json::json!(metadata.product_code.as_str()),
            );
            rate_dimensions.insert(
                "operation_code",
                serde_json::json!(metadata.operation_code.as_str()),
            );
        }
        let official_currency = official.unit_price.currency.clone();
        // The upstream cost is the procurement side of a routed account: it
        // feeds gross-margin reporting and cost accounting only. A missing
        // cost price must never block customer billing — the customer charge
        // derives from the official reference, so downgrade to "no procurement
        // cost" with a diagnostic instead of failing the whole resolution.
        // Previously this surfaced as `price_not_found` (non-fatal), which
        // silently produced zero-priced usage records and free rides.
        if query.supplier_code.is_some() && raw_upstream_cost.is_none() {
            tracing::warn!(
                model = %query.model,
                supplier_code = ?query.supplier_code,
                account_id = ?query.account_id,
                meter = %query.billing_meter.code(),
                region_code = %region_code,
                "upstream cost price is missing for a routed account; procurement cost is not reported, customer billing continues on the official reference",
            );
        }
        let procurement_multipliers = if raw_upstream_cost.is_some() {
            self.resolve_procurement_multipliers(
                &query,
                group.id,
                group.cost_multiplier,
                &region_code,
                default_region_code.as_deref(),
            )?
        } else {
            None
        };
        let procurement_cost = match (&raw_upstream_cost, procurement_multipliers.as_ref()) {
            (Some(price), Some(multipliers)) => Some(
                price
                    .unit_price
                    .checked_multiply(multipliers.combined_multiplier)?,
            ),
            // Missing procurement context (no cost price, or no multiplier
            // bindings) only leaves gross margin unreported; it never blocks
            // the customer charge.
            (None, _) => None,
            _ => {
                return Err(DomainError::new(
                    "upstream price and procurement multiplier context must be resolved together",
                ));
            }
        };

        let explicit_customer_candidates = self
            .catalog
            .list_model_prices_for_scope(
                tenant_id,
                organization_id,
                price_scope,
                PriceSide::CustomerCharge,
                query.billing_meter.clone(),
            )
            .into_iter()
            .filter(|price| {
                price.supplier_code.is_none()
                    && price.pricing_plan_code.as_deref() == Some(plan.plan_code.as_str())
            })
            .collect::<Vec<_>>();
        let explicit_customer = select_rate_with_region_fallback(
            explicit_customer_candidates,
            &region_code,
            default_region_code.as_deref(),
            Some(&official_currency),
            &rate_dimensions,
            query.occurred_at,
            "customer charge",
        )?;
        if let Some(price) = explicit_customer.as_ref() {
            ensure_pricing_currency(
                &official_currency,
                &price.unit_price.currency,
                "customer charge",
            )?;
        }
        let reference_multiplier = plan.default_multiplier;
        let (mut customer_charge_before_sale_multiplier, source) = match explicit_customer.as_ref()
        {
            Some(price) => (
                price.unit_price.clone(),
                ResolvedPriceSource::ExplicitCustomerCharge,
            ),
            None => (
                add_default_markup(
                    official.unit_price.checked_multiply(reference_multiplier)?,
                    &plan.default_markup_amount,
                )?,
                ResolvedPriceSource::DerivedFromOfficialReference,
            ),
        };
        let pricing_rule = self.find_pricing_rule(&plan, &rate_dimensions, query.occurred_at)?;
        // A plan configured `fail_closed` must not silently price a request the
        // operator never wrote a rule for. The flag existed in the schema and
        // was carried into the resolution but never read, so every unmatched
        // resource quietly billed at the official reference and the strict mode
        // an operator selected had no effect.
        //
        // Strict mode is only authoritative for a PERSISTED plan: an ephemeral
        // plan (`id == 0`, constructed in memory or by a test catalog) carries
        // the constructor default, not an operator decision, and reinterpreting
        // that default as a refusal would turn every synthetic catalog into a
        // pricing outage.
        if pricing_rule.is_none() && plan.fail_closed && plan.id > 0 {
            return Err(DomainError::new(format!(
                "pricing plan `{}` is fail-closed and has no rule for model {} meter {} region {}",
                plan.plan_code,
                model.model,
                query.billing_meter.code(),
                region_code
            )));
        }
        let mut customer_price_layer = match explicit_customer.as_ref() {
            Some(_) => CustomerPriceLayer::PlanReference,
            None => CustomerPriceLayer::PlanReference,
        };
        if let Some(rule) = pricing_rule.as_ref() {
            if rule.formula_mode == "unit_price_override" {
                if let Some(unit_price) = rule.unit_price_override.as_ref() {
                    ensure_pricing_currency(
                        &official_currency,
                        &unit_price.currency,
                        "pricing rule unit price override",
                    )?;
                    customer_charge_before_sale_multiplier = unit_price.clone();
                }
            } else {
                let multiplied =
                    customer_charge_before_sale_multiplier.checked_multiply(rule.multiplier)?;
                customer_charge_before_sale_multiplier =
                    add_rule_markup(&multiplied, &rule.markup_amount, &rule.rule_code)?;
            }
            customer_price_layer = CustomerPriceLayer::PlanRule;
        }
        // Layer 3: a per-resource price defined on the account group. It
        // REPLACES the composed plan result for the tuple it covers and the
        // group multiplier is not applied on top, so the charged amount stays a
        // single predictable operation instead of a composition of two factors.
        let group_resource_price = self.find_group_resource_price(
            &group,
            tenant_id,
            organization_id,
            &model,
            official_operation_code(official.rate_metadata.as_ref()),
            &query,
            &region_code,
        )?;
        let customer_charge = match group_resource_price.as_ref() {
            Some(price) => {
                customer_price_layer = CustomerPriceLayer::GroupResource;
                price.apply(&customer_charge_before_sale_multiplier)?
            }
            None => {
                require_positive_multiplier(
                    "account group sale multiplier",
                    group.sale_multiplier,
                )?;
                if group.sale_multiplier != DecimalValue::ONE {
                    customer_price_layer = CustomerPriceLayer::GroupMultiplier;
                }
                customer_charge_before_sale_multiplier.checked_multiply(group.sale_multiplier)?
            }
        };
        // Gross margin is a reporting-only figure. A price book that configures
        // the upstream cost in a different currency than the official
        // reference must not fail billing because the two cannot be subtracted;
        // it only leaves the margin unreported for the catalog views.
        let gross_margin_per_unit = match (procurement_cost.as_ref(), &customer_charge) {
            (Some(cost), charge) if charge.currency != cost.currency => {
                tracing::warn!(
                    model = %model.model,
                    customer_charge_currency = %charge.currency,
                    procurement_cost_currency = %cost.currency,
                    "procurement cost currency differs from the customer charge currency; gross margin is not reported"
                );
                None
            }
            (Some(cost), charge) => Some(charge.subtract(cost)?),
            (None, _) => None,
        };

        Ok(ResolvedModelPrice {
            model: model.model,
            vendor_code: model.vendor_code,
            vendor: vendor.vendor,
            group_code: group.code,
            pricing_plan_code: plan.plan_code,
            supplier_code: query.supplier_code,
            billing_meter: query.billing_meter,
            official_reference: official,
            raw_upstream_cost,
            raw_customer_charge: explicit_customer,
            procurement_cost,
            account_contract_cost_multiplier: procurement_multipliers
                .as_ref()
                .map(|multipliers| multipliers.account_contract_multiplier),
            account_group_cost_multiplier: procurement_multipliers
                .as_ref()
                .map(|multipliers| multipliers.account_group_multiplier),
            procurement_cost_multiplier: procurement_multipliers
                .as_ref()
                .map(|multipliers| multipliers.combined_multiplier),
            customer_charge_before_sale_multiplier,
            sale_multiplier: group.sale_multiplier,
            reference_multiplier,
            default_markup_amount: plan.default_markup_amount,
            rounding_mode: plan.rounding_mode,
            minimum_charge_amount: plan.minimum_charge_amount,
            fail_closed: plan.fail_closed,
            pricing_rule_multiplier: pricing_rule
                .as_ref()
                .map(|rule| rule.multiplier)
                .unwrap_or(DecimalValue::ONE),
            pricing_rule_markup_amount: pricing_rule
                .as_ref()
                .map(|rule| rule.markup_amount.clone())
                .unwrap_or_else(|| Money {
                    currency: official_currency,
                    unit_price: DecimalValue::ZERO,
                }),
            pricing_rule_unit_price_override: pricing_rule
                .as_ref()
                .and_then(|rule| rule.unit_price_override.clone()),
            pricing_record_identity: crate::domain::PricingPolicyRecordIdentity {
                account_rate_card: account_rate_card.as_ref().and_then(|card| {
                    crate::domain::ScopedPricingRecordIdentity::persisted(
                        card.tenant_id,
                        card.organization_id,
                        card.id,
                    )
                }),
                pricing_plan: crate::domain::ScopedPricingRecordIdentity::persisted(
                    plan.tenant_id,
                    plan.organization_id,
                    plan.id,
                ),
                pricing_rule: pricing_rule.as_ref().and_then(|rule| {
                    crate::domain::ScopedPricingRecordIdentity::persisted(
                        rule.tenant_id,
                        rule.organization_id,
                        rule.id,
                    )
                }),
            },
            customer_charge,
            gross_margin_per_unit,
            source,
            customer_price_layer,
            group_resource_price,
        })
    }

    /// Resolves the account-group per-resource price that covers this request.
    ///
    /// Selection mirrors the rate selector: only effective rows are eligible,
    /// a provider-scoped row outranks the provider wildcard, and two rows that
    /// are equally specific for the same tuple are ambiguous and fail closed
    /// instead of silently picking one.
    fn find_group_resource_price(
        &self,
        group: &crate::domain::UpstreamAccountGroup,
        tenant_id: i64,
        organization_id: i64,
        model: &crate::domain::AiModel,
        official_operation_code: &str,
        query: &ResolveModelPriceQuery,
        region_code: &str,
    ) -> DomainResult<Option<crate::domain::GroupResourcePrice>> {
        if group.id <= 0 {
            return Ok(None);
        }
        let provider_code = query.supplier_code.as_deref().unwrap_or_default();
        let mut candidates = self
            .catalog
            .list_group_resource_prices(tenant_id, organization_id, group.id)
            .into_iter()
            .filter(|price| price.is_effective_at(query.occurred_at))
            .filter(|price| {
                price.covers(
                    &model.catalog_key,
                    official_operation_code,
                    query.billing_meter.code(),
                    region_code,
                    provider_code,
                )
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(None);
        }
        candidates.sort_by(|left, right| {
            right
                .provider_code
                .trim()
                .len()
                .cmp(&left.provider_code.trim().len())
                .then_with(|| left.effective_from.cmp(&right.effective_from))
        });
        if candidates.get(1).is_some_and(|next| {
            next.provider_code.trim().len() == candidates[0].provider_code.trim().len()
                && next.effective_from == candidates[0].effective_from
        }) {
            return Err(DomainError::new(format!(
                "group resource price ambiguous for account group {} resource {} meter {} at {}",
                group.id,
                model.catalog_key,
                query.billing_meter.code(),
                query.occurred_at.to_rfc3339()
            )));
        }
        Ok(candidates.into_iter().next())
    }

    fn find_api_key(&self, api_key_id: i64) -> DomainResult<crate::domain::GatewayApiKey> {
        self.catalog
            .find_api_key(api_key_id)
            .ok_or_else(|| DomainError::new(format!("api key not found: {api_key_id}")))
    }

    fn find_group(
        &self,
        account_group_id: i64,
    ) -> DomainResult<crate::domain::UpstreamAccountGroup> {
        self.catalog
            .find_upstream_account_group(account_group_id)
            .ok_or_else(|| DomainError::new(format!("account group not found: {account_group_id}")))
    }

    fn find_plan(
        &self,
        tenant_id: i64,
        organization_id: i64,
        group: &crate::domain::UpstreamAccountGroup,
        api_key: Option<&crate::domain::GatewayApiKey>,
        account_id: Option<i64>,
        occurred_at: DateTime<Utc>,
    ) -> DomainResult<(
        crate::domain::PricingPlan,
        Option<crate::domain::AccountRateCard>,
    )> {
        let mut cards = self
            .catalog
            .list_account_rate_cards(tenant_id, organization_id)
            .into_iter()
            .filter(|card| card.is_effective_at(occurred_at))
            .filter_map(|card| {
                let rank = match card.subject_type.as_str() {
                    "api_key"
                        if api_key.is_some_and(|api_key| card.subject_id == Some(api_key.id)) =>
                    {
                        5
                    }
                    "user"
                        if api_key
                            .is_some_and(|api_key| card.subject_id == Some(api_key.user_id)) =>
                    {
                        4
                    }
                    "account" if card.subject_id == account_id => 4,
                    "account_group" if card.subject_id == Some(group.id) => 3,
                    "organization" if card.subject_id == Some(organization_id) => 2,
                    "default" => 1,
                    _ => return None,
                };
                Some((rank, card))
            })
            .collect::<Vec<_>>();
        cards.sort_by(|(left_rank, left), (right_rank, right)| {
            right_rank
                .cmp(left_rank)
                .then_with(|| left.priority.cmp(&right.priority))
                .then_with(|| right.effective_from.cmp(&left.effective_from))
                .then_with(|| left.rate_card_code.cmp(&right.rate_card_code))
        });
        if let Some((rank, selected)) = cards.first() {
            if cards.get(1).is_some_and(|(next_rank, next)| {
                rank == next_rank
                    && selected.priority == next.priority
                    && selected.effective_from == next.effective_from
                    && selected.pricing_plan_id != next.pricing_plan_id
            }) {
                return Err(DomainError::new(format!(
                    "pricing rate card ambiguous for subject at {}",
                    occurred_at.to_rfc3339()
                )));
            }
            if let Some(plan) = self.catalog.find_pricing_plan_by_identity(
                selected.pricing_plan_tenant_id,
                selected.pricing_plan_organization_id,
                selected.pricing_plan_id,
                &selected.pricing_plan_code,
            ) {
                return Ok((plan, Some(selected.clone())));
            }
        }
        let plan = if group.pricing_plan_id > 0 {
            self.catalog.find_pricing_plan_by_identity(
                group.pricing_plan_tenant_id,
                group.pricing_plan_organization_id,
                group.pricing_plan_id,
                &group.pricing_plan_code,
            )
        } else {
            self.catalog.find_pricing_plan_for_scope(
                tenant_id,
                organization_id,
                &group.pricing_plan_code,
            )
        };
        plan.map(|plan| (plan, None)).ok_or_else(|| {
            DomainError::new(format!(
                "pricing plan not found: {}",
                group.pricing_plan_code
            ))
        })
    }

    fn find_model(&self, model: &str) -> DomainResult<crate::domain::AiModel> {
        self.catalog
            .find_model(model)
            .ok_or_else(|| DomainError::new(format!("model not found: {model}")))
    }

    fn find_vendor(&self, vendor_code: &str) -> DomainResult<crate::domain::ModelVendorDefinition> {
        self.catalog
            .find_vendor(vendor_code)
            .ok_or_else(|| DomainError::new(format!("model vendor not found: {vendor_code}")))
    }

    fn find_official_reference(
        &self,
        query: &ResolveModelPriceQuery,
        tenant_id: i64,
        organization_id: i64,
        price_scope: &str,
        region_code: &str,
        default_region_code: Option<&str>,
        preferred_currency: Option<&str>,
        dimensions: &PricingDimensionContext,
    ) -> DomainResult<ModelPrice> {
        let candidates = self
            .catalog
            .list_model_prices_for_scope(
                tenant_id,
                organization_id,
                price_scope,
                PriceSide::OfficialReference,
                query.billing_meter.clone(),
            )
            .into_iter()
            .filter(|price| price.pricing_plan_code.is_none())
            .collect::<Vec<_>>();
        select_rate_with_region_fallback(
            candidates,
            region_code,
            default_region_code,
            preferred_currency,
            dimensions,
            query.occurred_at,
            "official reference",
        )?
        .ok_or_else(|| {
            DomainError::new(format!(
                "official reference price not found for model {} meter {} and region {}",
                query.model,
                query.billing_meter.code(),
                region_code
            ))
        })
    }

    fn find_upstream_cost(
        &self,
        query: &ResolveModelPriceQuery,
        tenant_id: i64,
        organization_id: i64,
        region_code: &str,
        default_region_code: Option<&str>,
        dimensions: &PricingDimensionContext,
    ) -> DomainResult<Option<ModelPrice>> {
        let supplier_code = query.supplier_code.as_deref();
        if supplier_code.is_none() && query.account_id.is_none() {
            return Ok(None);
        }
        let supplier_code = supplier_code.ok_or_else(|| {
            DomainError::new("supplier code is required when an upstream account is selected")
        })?;
        let account_id = query.account_id.ok_or_else(|| {
            DomainError::new("upstream account id is required when a supplier is selected")
        })?;
        let candidates = self
            .catalog
            .list_model_prices_for_scope(
                tenant_id,
                organization_id,
                &query.model,
                PriceSide::UpstreamCost,
                query.billing_meter.clone(),
            )
            .into_iter()
            .filter(|price| {
                price.supplier_code.as_deref() == Some(supplier_code)
                    && price.account_id == Some(account_id)
                    && price.pricing_plan_code.is_none()
            })
            .collect::<Vec<_>>();
        select_rate_with_region_fallback(
            candidates,
            region_code,
            default_region_code,
            None,
            dimensions,
            query.occurred_at,
            "upstream cost",
        )
    }

    fn find_pricing_rule(
        &self,
        plan: &crate::domain::PricingPlan,
        dimensions: &PricingDimensionContext,
        occurred_at: DateTime<Utc>,
    ) -> DomainResult<Option<crate::domain::PricingRule>> {
        let rules = self
            .catalog
            .list_pricing_rules_for_plan(
                plan.tenant_id,
                plan.organization_id,
                plan.id,
                &plan.plan_code,
            )
            .into_iter()
            .filter(|rule| rule.pricing_plan_id == plan.id && rule.plan_code == plan.plan_code)
            .collect::<Vec<_>>();
        select_pricing_rule_for_dimensions(rules, dimensions, occurred_at, &plan.plan_code)
    }

    fn resolve_procurement_multipliers(
        &self,
        query: &ResolveModelPriceQuery,
        account_group_id: i64,
        default_group_multiplier: DecimalValue,
        region_code: &str,
        default_region_code: Option<&str>,
    ) -> DomainResult<Option<ProcurementMultipliers>> {
        if query.supplier_code.is_none() && query.account_id.is_none() {
            return Ok(None);
        }
        let supplier_code = query.supplier_code.as_deref().ok_or_else(|| {
            DomainError::new("supplier code is required when an upstream account is selected")
        })?;
        let account_id = query.account_id.ok_or_else(|| {
            DomainError::new("upstream account id is required when a supplier is selected")
        })?;

        let mut resolved: Option<ProcurementMultipliers> = None;
        // Walk the billing-region chain (requested region -> default region ->
        // `global` -> any) so an account bound in a different region still
        // yields multipliers instead of failing the whole resolution. The
        // account-group binding check below is what authorizes the account;
        // region is only a dimension of that binding.
        let excluded_regions = exact_probe_regions(region_code, default_region_code);
        for probe in billing_region_probes(region_code, default_region_code) {
            for route in self
                .catalog
                .list_upstream_account_routes()
                .into_iter()
                .filter(|route| {
                    route.supplier_code == supplier_code
                        && route.account_id == account_id
                        && match &probe {
                            RegionProbe::Exact(_) => probe.matches_region(&route.region_code),
                            RegionProbe::Any => !excluded_regions
                                .iter()
                                .any(|excluded| same_region(&route.region_code, excluded)),
                        }
                })
            {
                let Some(binding) = route
                    .account_group_bindings
                    .iter()
                    .find(|binding| binding.account_group_id == account_group_id)
                else {
                    continue;
                };
                let account_group_multiplier = binding
                    .cost_multiplier_override
                    .unwrap_or(default_group_multiplier);
                require_positive_multiplier(
                    "upstream account contract cost multiplier",
                    route.contract_cost_multiplier,
                )?;
                require_positive_multiplier(
                    "upstream account group cost multiplier",
                    account_group_multiplier,
                )?;
                let multipliers = ProcurementMultipliers {
                    account_contract_multiplier: route.contract_cost_multiplier,
                    account_group_multiplier,
                    combined_multiplier: route
                        .contract_cost_multiplier
                        .checked_multiply(account_group_multiplier)?,
                };
                if resolved
                    .as_ref()
                    .is_some_and(|current| current != &multipliers)
                {
                    return Err(DomainError::new(format!(
                        "inconsistent procurement multipliers for supplier {supplier_code}, account {account_id}, and account group {account_group_id}"
                    )));
                }
                resolved = Some(multipliers);
            }
            if resolved.is_some() {
                if probe == RegionProbe::Any {
                    tracing::warn!(
                        supplier_code = %supplier_code,
                        account_id = %account_id,
                        requested_region = %region_code,
                        "upstream account is not bound in the requested region or global; procurement multipliers resolved from the only bound region"
                    );
                }
                break;
            }
        }

        resolved.map(Some).ok_or_else(|| {
            DomainError::new(format!(
                "upstream account {account_id} is not bound to account group {account_group_id} for supplier {supplier_code} and region {region_code}"
            ))
        })
    }

    /// Resolves the upstream route backing a priced resource.
    ///
    /// The route is only a region hint: it supplies the effective region when
    /// the caller passed none. A missing hint must therefore degrade to
    /// `Ok(None)` instead of failing the whole price resolution - the request
    /// is already routed by the time pricing runs, and hard-failing here
    /// surfaced as an opaque `upstream route not found` 502 for catalogs whose
    /// route row simply carries a region outside the probe chain (or no model
    /// route row at all while the account route exists).
    ///
    /// Pricing then rates against the requested region and lets the price-book
    /// fallback chain (`region -> global -> any`) supply the rate.
    fn find_upstream_route(
        &self,
        model: &str,
        supplier_code: &str,
        account_id: Option<i64>,
        region_code: Option<&str>,
    ) -> DomainResult<Option<crate::domain::ModelUpstreamRoute>> {
        let probes = route_region_probes(region_code);
        for probe in &probes {
            let region_matches = |candidate: &str| match probe.as_deref() {
                Some(probe) => same_region(candidate, probe),
                None => true,
            };
            if let Some(route) = self
                .catalog
                .list_model_upstream_routes(model)
                .into_iter()
                .find(|route| {
                    route.supplier_code == supplier_code
                        && account_id
                            .map(|account_id| route.account_id == account_id)
                            .unwrap_or(true)
                        && region_matches(&route.region_code)
                })
            {
                return Ok(Some(route));
            }

            if let Some(route) = self
                .catalog
                .list_upstream_account_routes()
                .into_iter()
                .find(|route| {
                    route.supplier_code == supplier_code
                        && account_id
                            .map(|account_id| route.account_id == account_id)
                            .unwrap_or(true)
                        && region_matches(&route.region_code)
                })
            {
                return Ok(Some(
                    crate::domain::ModelUpstreamRoute::new_for_catalog_key(
                        model,
                        model,
                        &route.supplier_code,
                        route.account_id,
                        model,
                    )
                    .with_region_code(&route.region_code)
                    .with_upstream_endpoint(route.base_url, route.secret_ref)
                    .with_auth_profile(route.auth_profile),
                ));
            }
        }

        tracing::warn!(
            model = %model,
            supplier_code = %supplier_code,
            account_id = ?account_id,
            region_code = %region_code.unwrap_or(DEFAULT_PRICE_REGION_CODE),
            "upstream route not found for the priced resource; pricing keeps the requested region and falls back through the price book"
        );
        Ok(None)
    }
}

/// Selects the winning pricing rule for runtime dimensions.
///
/// This is the single authority for sales-rule precedence, shared by the
/// runtime resolver ([`PricingResolver::find_pricing_rule`]) and the admin
/// "effective price" preview so both surfaces can never disagree about which
/// rule wins. The ordering is: scope + schedule/window match first, then
/// specificity descending, priority ascending, effective_from descending, and
/// rule_code ascending as the final deterministic tiebreak. Two candidates
/// that still tie on every criterion except rule_code are ambiguous and fail
/// closed.
pub fn select_pricing_rule_for_dimensions(
    rules: Vec<crate::domain::PricingRule>,
    dimensions: &PricingDimensionContext,
    occurred_at: DateTime<Utc>,
    plan_code: &str,
) -> DomainResult<Option<crate::domain::PricingRule>> {
    let mut rules = rules
        .into_iter()
        .filter(|rule| rule.scope_matches(dimensions) && rule.matches_at(dimensions, occurred_at))
        .collect::<Vec<_>>();
    rules.sort_by(|left, right| {
        right
            .specificity()
            .cmp(&left.specificity())
            .then_with(|| left.priority.cmp(&right.priority))
            .then_with(|| right.effective_from.cmp(&left.effective_from))
            .then_with(|| left.rule_code.cmp(&right.rule_code))
    });
    let Some(selected) = rules.first().cloned() else {
        return Ok(None);
    };
    if rules.get(1).is_some_and(|next| {
        selected.specificity() == next.specificity()
            && selected.priority == next.priority
            && selected.effective_from == next.effective_from
            && selected.rule_code != next.rule_code
    }) {
        return Err(DomainError::new(format!(
            "pricing rule ambiguous for plan {plan_code} at {}",
            occurred_at.to_rfc3339()
        )));
    }
    Ok(Some(selected))
}

fn rate_specificity(price: &ModelPrice) -> usize {
    price
        .rate_metadata
        .as_ref()
        .map(|metadata| metadata.condition_count())
        .unwrap_or_default()
}

fn select_rate(
    candidates: Vec<ModelPrice>,
    dimensions: &PricingDimensionContext,
    occurred_at: DateTime<Utc>,
    rate_label: &str,
) -> DomainResult<Option<ModelPrice>> {
    let mut candidates = candidates
        .into_iter()
        .filter(|price| {
            price
                .rate_metadata
                .as_ref()
                .is_none_or(|metadata| metadata.matches_at(dimensions, occurred_at))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        rate_variant_rank(right)
            .cmp(&rate_variant_rank(left))
            .then_with(|| rate_specificity(right).cmp(&rate_specificity(left)))
            .then_with(|| rate_priority(left).cmp(&rate_priority(right)))
            .then_with(|| rate_effective_from(right).cmp(&rate_effective_from(left)))
            .then_with(|| rate_hash(left).cmp(rate_hash(right)))
    });
    let Some(selected) = candidates.first().cloned() else {
        return Ok(None);
    };
    if candidates.get(1).is_some_and(|next| {
        same_rate_rank(&selected, next) && rate_hash(&selected) != rate_hash(next)
    }) {
        return Err(DomainError::new(format!(
            "{rate_label} rate ambiguous at {}",
            occurred_at.to_rfc3339()
        )));
    }
    Ok(Some(selected))
}

fn same_rate_rank(left: &ModelPrice, right: &ModelPrice) -> bool {
    rate_variant_rank(left) == rate_variant_rank(right)
        && rate_specificity(left) == rate_specificity(right)
        && rate_priority(left) == rate_priority(right)
        && rate_effective_from(left) == rate_effective_from(right)
}

fn rate_variant_rank(price: &ModelPrice) -> u8 {
    price
        .rate_metadata
        .as_ref()
        .map(|metadata| metadata.rate_variant.selection_rank())
        .unwrap_or_default()
}

fn rate_effective_from(price: &ModelPrice) -> DateTime<Utc> {
    price
        .rate_metadata
        .as_ref()
        .map(|metadata| metadata.effective_from)
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}

fn rate_priority(price: &ModelPrice) -> i32 {
    price
        .rate_metadata
        .as_ref()
        .map(|metadata| metadata.priority)
        .unwrap_or(i32::MAX)
}

fn rate_hash(price: &ModelPrice) -> &str {
    price
        .rate_metadata
        .as_ref()
        .map(|metadata| metadata.rate_hash.as_str())
        .unwrap_or("")
}

/// Canonical operation code of the selected official rate.
///
/// Group resource prices are authored against the operation the official
/// catalog publishes, so the same value must be used when matching them. A
/// rate without metadata (a synthetic catalog in tests) yields an empty code,
/// which only matches a group price authored with an empty operation — an
/// unroutable configuration the database constraint already rejects.
fn official_operation_code(metadata: Option<&PricingRateMetadata>) -> &str {
    metadata
        .map(|metadata| metadata.operation_code.as_str())
        .unwrap_or("")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcurementMultipliers {
    account_contract_multiplier: DecimalValue,
    account_group_multiplier: DecimalValue,
    combined_multiplier: DecimalValue,
}

fn normalize_region_code(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "global".to_owned()
    } else {
        value.to_owned()
    }
}

fn normalized_optional_region_code(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(normalize_region_code(value))
    }
}

fn same_region(actual: &str, expected: &str) -> bool {
    normalize_region_code(actual).eq_ignore_ascii_case(&normalize_region_code(expected))
}

/// One step of the billing-region fallback chain.
///
/// The chain is the single authority for "which region may answer this
/// lookup". `PriceService` reuses it through [`region_matches_or_fallback`] so
/// a rate that the resolver deliberately fell back to is never rejected
/// downstream as a region mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegionProbe {
    /// Rates pinned to one exact region: the requested region first, then the
    /// generic `global` region.
    Exact(String),
    /// Terminal last-resort pass over the regions the exact probes did not
    /// cover. Guarantees a configured price book can never leave an otherwise
    /// routable request unpriced.
    Any,
}

impl RegionProbe {
    fn matches_region(&self, candidate_region_code: &str) -> bool {
        match self {
            Self::Exact(region_code) => same_region(candidate_region_code, region_code),
            Self::Any => true,
        }
    }
}

/// Ordered billing-region probe chain for a lookup.
///
/// A deployment started with a default region (for example
/// `SDKWORK_CLOUDROUTER_ROUTER_REGION_CODE=cn`) must still settle when the
/// price book only carries generic rates, and a resource that configures an
/// admin default billing region must rate against that regional price before
/// borrowing the generic bucket:
///
/// 1. the requested region — except the generic `global` bucket, which is a
///    borrowing fallback rather than a specific region: when a distinct
///    default billing region is configured, the default regional price is
///    probed *before* the `global` borrow, so the default region wins
///    regardless of whether the caller pre-replaced the region,
/// 2. the configured default billing region (when set and distinct),
/// 3. the generic `global` region - the documented `cn -> global` fallback,
/// 4. any remaining region, so the resolved price is never empty.
///
/// [`RegionProbe::Any`] is terminal and only runs when all exact probes found
/// nothing, so a regional rate always wins over a borrowed one.
fn billing_region_probes(region_code: &str, default_region_code: Option<&str>) -> Vec<RegionProbe> {
    let mut exact: Vec<String> = Vec::new();
    // The generic `global` bucket is a borrowing fallback, not a specific
    // region. When the admin default billing region is configured and
    // distinct, the default regional price must be probed before the `global`
    // borrow — otherwise a request whose routed region stays on the `global`
    // bucket would bill the generic price even though a regional default is
    // configured. Specific requested regions (e.g. `us`) keep their
    // first-probe position: the default region is only a fallback for regions
    // the price book does not carry.
    let normalized_requested = normalize_region_code(region_code);
    if normalized_requested.eq_ignore_ascii_case(DEFAULT_PRICE_REGION_CODE) {
        if let Some(default_region_code) = default_region_code {
            let default_region_code = normalize_region_code(default_region_code);
            if default_region_code != DEFAULT_PRICE_REGION_CODE {
                push_distinct_exact(&mut exact, &default_region_code);
            }
        }
    }
    push_distinct_exact(&mut exact, region_code);
    if let Some(default_region_code) = default_region_code {
        let default_region_code = normalize_region_code(default_region_code);
        if default_region_code != DEFAULT_PRICE_REGION_CODE {
            push_distinct_exact(&mut exact, &default_region_code);
        }
    }
    push_distinct_exact(&mut exact, DEFAULT_PRICE_REGION_CODE);
    exact
        .into_iter()
        .map(RegionProbe::Exact)
        .chain(std::iter::once(RegionProbe::Any))
        .collect()
}

/// Pushes a normalized region onto the exact probe list unless a
/// case-insensitive duplicate is already present.
fn push_distinct_exact(exact: &mut Vec<String>, region_code: &str) {
    let region_code = normalize_region_code(region_code);
    if !exact
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&region_code))
    {
        exact.push(region_code);
    }
}

/// Regions already covered by an [`RegionProbe::Exact`] step, so the terminal
/// `Any` pass cannot re-select a region that already failed.
fn exact_probe_regions(region_code: &str, default_region_code: Option<&str>) -> Vec<String> {
    billing_region_probes(region_code, default_region_code)
        .into_iter()
        .filter_map(|probe| match probe {
            RegionProbe::Exact(region_code) => Some(region_code),
            RegionProbe::Any => None,
        })
        .collect()
}

/// Reports whether a rate resolved in `actual_region` may bill a resource
/// requested in `expected_region`.
///
/// Delegates to [`billing_region_probes`] so the resolver and the resource
/// guard agree by construction: whatever the fallback chain can select, the
/// resolution accepts. The configured default billing region joins the chain
/// exactly as it does for the lookup, so a rate resolved through the default
/// region is never rejected downstream as a mismatch.
pub(crate) fn region_matches_or_fallback(
    expected_region: &str,
    actual_region: &str,
    default_region_code: Option<&str>,
) -> bool {
    billing_region_probes(expected_region, default_region_code)
        .iter()
        .any(|probe| probe.matches_region(actual_region))
}

/// Route probes. `None` preserves "match any region" semantics.
///
/// Account routes stay exact-probe only: a terminal "any region" pass would
/// let an unrelated region's credentials answer a region-scoped lookup. The
/// admin default billing region is intentionally not probed here — it steers
/// price selection, not credential binding.
fn route_region_probes(region_code: Option<&str>) -> Vec<Option<String>> {
    match region_code {
        Some(region_code) => billing_region_probes(region_code, None)
            .into_iter()
            .filter_map(|probe| match probe {
                RegionProbe::Exact(region_code) => Some(Some(region_code)),
                RegionProbe::Any => None,
            })
            .collect(),
        None => vec![None],
    }
}

/// Selects a rate through the billing-region fallback chain, preferring rates
/// in `preferred_currency` when the price book carries the model in several
/// currencies across regions.
///
/// The upstream cost anchors the resolution currency, and the official
/// reference plus the explicit customer charge follow it. Independent region
/// fallbacks could otherwise pick the two sides of the margin from different
/// regions priced in different currencies, and the derived customer charge
/// and procurement cost would then fail inside Money arithmetic with a bare
/// `money currency mismatch`. Preferring the anchored currency keeps the
/// resolution self-consistent; only when no rate in that currency exists does
/// the lookup fall back to the full candidate set, where the labeled currency
/// guards report the genuine configuration conflict.
fn select_rate_with_region_fallback(
    candidates: Vec<ModelPrice>,
    region_code: &str,
    default_region_code: Option<&str>,
    preferred_currency: Option<&str>,
    dimensions: &PricingDimensionContext,
    occurred_at: DateTime<Utc>,
    rate_label: &str,
) -> DomainResult<Option<ModelPrice>> {
    if let Some(currency) = preferred_currency {
        let same_currency = candidates
            .iter()
            .filter(|price| price.unit_price.currency == currency)
            .cloned()
            .collect::<Vec<_>>();
        if !same_currency.is_empty() {
            if let Some(rate) = select_rate_in_region_chain(
                same_currency,
                region_code,
                default_region_code,
                dimensions,
                occurred_at,
                rate_label,
            )? {
                return Ok(Some(rate));
            }
        }
    }
    select_rate_in_region_chain(
        candidates,
        region_code,
        default_region_code,
        dimensions,
        occurred_at,
        rate_label,
    )
}

/// Selects a rate through the billing-region fallback chain.
///
/// Every probe is matched against its own dimension context: the `region_code`
/// dimension is rewritten to the probed region so conditional rates
/// (`PricingRateMetadata` conditions) are evaluated against the region actually
/// being probed instead of the originally requested one. Without that rewrite
/// the `global` fallback silently rejected every conditional global rate and
/// reported "price not found" for a price book that did contain the rate.
fn select_rate_in_region_chain(
    candidates: Vec<ModelPrice>,
    region_code: &str,
    default_region_code: Option<&str>,
    dimensions: &PricingDimensionContext,
    occurred_at: DateTime<Utc>,
    rate_label: &str,
) -> DomainResult<Option<ModelPrice>> {
    let excluded_regions = exact_probe_regions(region_code, default_region_code);
    for probe in billing_region_probes(region_code, default_region_code) {
        let mut probe_dimensions = dimensions.clone();
        match &probe {
            RegionProbe::Exact(probe_region) => {
                probe_dimensions.insert("region_code", serde_json::json!(probe_region.as_str()));
            }
            RegionProbe::Any => {
                // Region-agnostic rates only: a rate pinned to a region
                // through a condition cannot be borrowed by the last resort.
                probe_dimensions.remove("region_code");
            }
        }
        let in_probe = select_rate(
            candidates
                .iter()
                .filter(|price| match &probe {
                    RegionProbe::Exact(_) => probe.matches_region(&price.region_code),
                    RegionProbe::Any => !excluded_regions
                        .iter()
                        .any(|excluded| same_region(&price.region_code, excluded)),
                })
                .cloned()
                .collect(),
            &probe_dimensions,
            occurred_at,
            rate_label,
        )?;
        if let Some(rate) = in_probe {
            if probe == RegionProbe::Any {
                tracing::warn!(
                    requested_region = %region_code,
                    default_region = ?default_region_code,
                    resolved_region = %rate.region_code,
                    rate = %rate_label,
                    "no {rate_label} rate exists for the requested region, the configured default region, or global; rated with the only available region instead of failing"
                );
            }
            return Ok(Some(rate));
        }
    }
    Ok(None)
}

/// Adds the plan's default markup to a derived customer charge.
///
/// A markup authored in a currency different from the price book is skipped
/// (with a warning) instead of failing the resolution: the charge keeps the
/// price-book currency, the skipped adjustment stays visible through the
/// audit fields, and the request keeps a usable price. Adding across
/// currencies is meaningless and previously surfaced as a bare `money
/// currency mismatch` 502.
fn add_default_markup(base: Money, markup: &Money) -> DomainResult<Money> {
    if markup.is_zero() {
        return Ok(base);
    }
    if base.currency != markup.currency {
        tracing::warn!(
            charge_currency = %base.currency,
            markup_currency = %markup.currency,
            "pricing plan default markup is configured in a different currency than the price book; the markup is skipped so billing keeps a usable price"
        );
        return Ok(base);
    }
    base.add(markup)
}

/// Adds a pricing-rule markup to a derived customer charge.
///
/// Same currency policy as [`add_default_markup`]: a cross-currency rule
/// markup is skipped with a warning rather than failing the request.
fn add_rule_markup(charge: &Money, markup: &Money, rule_code: &str) -> DomainResult<Money> {
    if markup.is_zero() {
        return Ok(charge.clone());
    }
    if charge.currency != markup.currency {
        tracing::warn!(
            rule_code = %rule_code,
            charge_currency = %charge.currency,
            markup_currency = %markup.currency,
            "pricing rule markup is configured in a different currency than the customer charge; the markup is skipped so billing keeps a usable price"
        );
        return Ok(charge.clone());
    }
    charge.add(markup)
}

fn ensure_pricing_currency(expected: &str, actual: &str, label: &str) -> DomainResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(DomainError::new(format!(
            "{label} currency mismatch: official reference uses {expected}, received {actual}"
        )))
    }
}

fn require_positive_multiplier(field: &str, value: DecimalValue) -> DomainResult<()> {
    if value <= DecimalValue::ZERO {
        return Err(DomainError::new(format!("{field} must be positive")));
    }
    Ok(())
}

#[cfg(test)]
mod region_probe_chain_tests {
    use super::{billing_region_probes, RegionProbe, DEFAULT_PRICE_REGION_CODE};

    fn exact_regions(region_code: &str, default_region_code: Option<&str>) -> Vec<String> {
        billing_region_probes(region_code, default_region_code)
            .into_iter()
            .filter_map(|probe| match probe {
                RegionProbe::Exact(region) => Some(region),
                RegionProbe::Any => None,
            })
            .collect()
    }

    #[test]
    fn default_region_is_probed_before_the_global_borrow() {
        // The admin default billing region must win over the generic `global`
        // bucket even when the routed region itself stays on `global` — this
        // is the resolver-side guarantee behind "a configured default region
        // is preferred over the global borrow".
        assert_eq!(
            exact_regions(DEFAULT_PRICE_REGION_CODE, Some("cn")),
            vec!["cn".to_owned(), "global".to_owned()]
        );
        assert_eq!(
            exact_regions(DEFAULT_PRICE_REGION_CODE, Some("CN")),
            vec!["CN".to_owned(), "global".to_owned()],
            "probe labels keep the authored case; matching is case-insensitive"
        );
        assert_eq!(
            exact_regions("GLOBAL", Some("cn")),
            vec!["cn".to_owned(), "GLOBAL".to_owned()],
            "the generic-bucket classification is case-insensitive; probe labels keep the authored case"
        );
    }

    #[test]
    fn a_specific_requested_region_stays_first() {
        assert_eq!(
            exact_regions("us", Some("cn")),
            vec!["us".to_owned(), "cn".to_owned(), "global".to_owned()]
        );
    }

    #[test]
    fn no_default_keeps_the_legacy_chain() {
        assert_eq!(exact_regions("global", None), vec!["global".to_owned()]);
        assert_eq!(
            exact_regions("us", None),
            vec!["us".to_owned(), "global".to_owned()]
        );
    }

    #[test]
    fn a_global_default_never_shadows_the_requested_region() {
        // `global` as the configured default is a no-op: it must not reorder
        // the chain or shadow a specific requested region.
        assert_eq!(
            exact_regions("us", Some("global")),
            vec!["us".to_owned(), "global".to_owned()]
        );
        assert_eq!(
            exact_regions("global", Some("global")),
            vec!["global".to_owned()]
        );
    }
}
