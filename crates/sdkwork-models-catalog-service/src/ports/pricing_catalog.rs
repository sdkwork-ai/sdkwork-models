use crate::domain::{
    AccountRateCard, AiModel, BillingMeter, GatewayAccessPolicy, GatewayApiKey, GatewayRiskRule,
    ModelMappingRule, ModelPrice, ModelUpstreamRoute, ModelVendorDefinition, PriceSide,
    PricingPlan, PricingRule, QuotaPolicy, ResolveModelMappingContext, UpstreamAccountGroup,
    UpstreamAccountGroupMetricSnapshot, UpstreamAccountRoute,
};

pub trait PricingCatalog {
    /// Visits the catalog's maintained model index without cloning the full
    /// collection. Return `false` from the visitor to stop early.
    fn visit_models(&self, vendor_code: Option<&str>, visitor: &mut dyn FnMut(&AiModel) -> bool);
    fn list_model_upstream_routes(&self, model: &str) -> Vec<ModelUpstreamRoute>;
    fn list_upstream_account_routes(&self) -> Vec<UpstreamAccountRoute>;
    fn list_model_mappings(&self) -> Vec<ModelMappingRule>;
    fn list_api_keys(&self) -> Vec<GatewayApiKey>;
    fn list_upstream_account_groups(&self) -> Vec<UpstreamAccountGroup>;
    fn list_model_prices(
        &self,
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
    ) -> Vec<ModelPrice>;
    fn list_model_prices_for_side(&self, model: &str, price_side: PriceSide) -> Vec<ModelPrice>;

    /// Returns prices visible from a tenant/organization scope. Implementations
    /// without persisted scope metadata retain legacy behavior by default.
    fn list_model_prices_for_scope(
        &self,
        tenant_id: i64,
        organization_id: i64,
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
    ) -> Vec<ModelPrice> {
        let _ = (tenant_id, organization_id);
        self.list_model_prices(model, price_side, billing_meter)
    }

    fn list_model_prices_for_scope_side(
        &self,
        tenant_id: i64,
        organization_id: i64,
        model: &str,
        price_side: PriceSide,
    ) -> Vec<ModelPrice> {
        let _ = (tenant_id, organization_id);
        self.list_model_prices_for_side(model, price_side)
    }
    fn find_api_key(&self, api_key_id: i64) -> Option<GatewayApiKey>;
    fn find_api_key_by_hash(&self, key_hash: &str) -> Option<GatewayApiKey>;
    fn find_upstream_account_group(&self, account_group_id: i64) -> Option<UpstreamAccountGroup>;
    fn find_access_policy(&self, policy_id: i64) -> Option<GatewayAccessPolicy>;
    fn find_quota_policy(&self, policy_id: i64) -> Option<QuotaPolicy>;
    fn list_gateway_risk_rules(&self) -> Vec<GatewayRiskRule>;
    fn find_latest_upstream_account_group_metric_snapshot(
        &self,
        account_group_id: i64,
    ) -> Option<UpstreamAccountGroupMetricSnapshot>;
    fn find_pricing_plan(&self, plan_code: &str) -> Option<PricingPlan>;

    fn list_pricing_rules(&self, _plan_code: &str) -> Vec<PricingRule> {
        Vec::new()
    }

    fn list_account_rate_cards(
        &self,
        _tenant_id: i64,
        _organization_id: i64,
    ) -> Vec<AccountRateCard> {
        Vec::new()
    }

    /// Returns every per-resource price an account group defines.
    ///
    /// Group prices are a distinct pricing layer that sits between the plan
    /// rules and the group-wide `sale_multiplier`. The default is empty so a
    /// catalog implementation that does not carry the layer keeps pricing
    /// exactly as before; an implementation that does carry it overrides this
    /// method and the resolver starts applying the layer.
    fn list_group_resource_prices(
        &self,
        _tenant_id: i64,
        _organization_id: i64,
        _account_group_id: i64,
    ) -> Vec<crate::domain::GroupResourcePrice> {
        Vec::new()
    }

    fn list_pricing_rules_for_plan(
        &self,
        _tenant_id: i64,
        _organization_id: i64,
        _pricing_plan_id: i64,
        plan_code: &str,
    ) -> Vec<PricingRule> {
        self.list_pricing_rules(plan_code)
    }

    fn find_pricing_plan_for_scope(
        &self,
        tenant_id: i64,
        organization_id: i64,
        plan_code: &str,
    ) -> Option<PricingPlan> {
        let _ = (tenant_id, organization_id);
        self.find_pricing_plan(plan_code)
    }

    /// Resolves the exact immutable plan selected by an external rate-card
    /// authority. Persisted catalogs override this method so a global plan is
    /// never replaced by a same-code tenant plan during rating.
    fn find_pricing_plan_by_identity(
        &self,
        tenant_id: i64,
        organization_id: i64,
        pricing_plan_id: i64,
        plan_code: &str,
    ) -> Option<PricingPlan> {
        let _ = pricing_plan_id;
        self.find_pricing_plan_for_scope(tenant_id, organization_id, plan_code)
    }
    fn find_model(&self, model: &str) -> Option<AiModel>;
    fn find_vendor(&self, vendor_code: &str) -> Option<ModelVendorDefinition>;
    fn resolve_model_mapping(
        &self,
        source_model: &str,
        context: &ResolveModelMappingContext,
    ) -> Option<ModelMappingRule>;
    fn find_model_upstream_route(
        &self,
        model: &str,
        supplier_code: &str,
    ) -> Option<ModelUpstreamRoute>;
    fn find_model_price(
        &self,
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
        supplier_code: Option<&str>,
        pricing_plan_code: Option<&str>,
    ) -> Option<ModelPrice>;

    fn find_model_price_for_scope(
        &self,
        tenant_id: i64,
        organization_id: i64,
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
        supplier_code: Option<&str>,
        pricing_plan_code: Option<&str>,
    ) -> Option<ModelPrice> {
        let _ = (tenant_id, organization_id);
        self.find_model_price(
            model,
            price_side,
            billing_meter,
            supplier_code,
            pricing_plan_code,
        )
    }
}
