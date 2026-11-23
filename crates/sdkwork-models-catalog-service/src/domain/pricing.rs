use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc};
use chrono_tz::Tz;
use serde_json::Value;

use crate::domain::{BillingMeter, DecimalValue, Money};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PriceSide {
    OfficialReference,
    UpstreamCost,
    CustomerCharge,
    InternalTransfer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingPlan {
    pub id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub plan_code: String,
    pub base_price_side: PriceSide,
    pub default_multiplier: DecimalValue,
    pub default_markup_amount: Money,
    pub rounding_mode: String,
    pub minimum_charge_amount: Money,
    pub fail_closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopedPricingRecordIdentity {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub id: i64,
}

impl ScopedPricingRecordIdentity {
    pub fn persisted(tenant_id: i64, organization_id: i64, id: i64) -> Option<Self> {
        (id > 0).then_some(Self {
            tenant_id,
            organization_id,
            id,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PricingRateRecordIdentity {
    pub price_book_tenant_id: i64,
    pub price_book_organization_id: i64,
    pub price_book_id: i64,
    pub rate_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PricingPolicyRecordIdentity {
    pub account_rate_card: Option<ScopedPricingRecordIdentity>,
    pub pricing_plan: Option<ScopedPricingRecordIdentity>,
    pub pricing_rule: Option<ScopedPricingRecordIdentity>,
}

impl PricingPlan {
    pub fn new(
        plan_code: &str,
        base_price_side: PriceSide,
        default_multiplier: DecimalValue,
        default_markup_amount: Money,
    ) -> Self {
        Self {
            id: 0,
            tenant_id: 0,
            organization_id: 0,
            plan_code: plan_code.to_owned(),
            base_price_side,
            default_multiplier,
            default_markup_amount: default_markup_amount.clone(),
            minimum_charge_amount: Money {
                currency: default_markup_amount.currency.clone(),
                unit_price: DecimalValue::ZERO,
            },
            rounding_mode: "half_up".to_owned(),
            fail_closed: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPrice {
    pub catalog_key: String,
    pub model: String,
    pub region_code: String,
    pub price_side: PriceSide,
    pub billing_meter: BillingMeter,
    pub unit_size: DecimalValue,
    pub unit_price: Money,
    pub supplier_code: Option<String>,
    pub account_id: Option<i64>,
    pub pricing_plan_code: Option<String>,
    pub rate_metadata: Option<PricingRateMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingRateMetadata {
    pub record_identity: Option<PricingRateRecordIdentity>,
    pub price_book_code: String,
    pub rate_hash: String,
    pub product_code: String,
    pub operation_code: String,
    pub billability: String,
    pub charge_timing: String,
    pub calculation_mode: String,
    pub quantity_aggregation: String,
    pub minimum_quantity: DecimalValue,
    pub quantity_step: Option<DecimalValue>,
    pub priority: i32,
    pub effective_from: DateTime<Utc>,
    pub effective_to: Option<DateTime<Utc>>,
    pub rate_variant: PricingRateVariant,
    pub schedule: Option<PricingSchedule>,
    pub conditions: Vec<PricingRateCondition>,
    pub tiers: Vec<PricingRateTier>,
    pub formula: Option<PricingFormula>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PricingRateVariant {
    Standard,
    TimeWindow,
    /// Vendor batch/offline price (typically 50% of the real-time rate). It
    /// shares the no-schedule shape with `standard`; the catalog distinguishes
    /// it through the explicit `tier_code=batch` condition so an interactive
    /// call never selects it.
    Batch,
}

impl PricingRateVariant {
    pub fn code(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::TimeWindow => "time_window",
            Self::Batch => "batch",
        }
    }

    pub fn from_code(value: &str) -> Option<Self> {
        match value.trim() {
            "standard" => Some(Self::Standard),
            "time_window" => Some(Self::TimeWindow),
            "batch" => Some(Self::Batch),
            _ => None,
        }
    }

    pub fn selection_rank(self) -> u8 {
        match self {
            Self::Standard => 0,
            Self::Batch => 0,
            Self::TimeWindow => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingSchedule {
    pub time_zone: Tz,
    pub weekly_windows: Vec<PricingWeeklyWindow>,
    pub include_dates: Vec<NaiveDate>,
    pub exclude_dates: Vec<NaiveDate>,
    pub holiday_dates: Vec<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingWeeklyWindow {
    pub window_code: String,
    pub days_of_week: Vec<u8>,
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub end_day_offset: u8,
    pub holiday_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingRule {
    pub id: i64,
    pub pricing_plan_id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub rule_code: String,
    pub plan_code: String,
    pub product_code: Option<String>,
    pub operation_code: Option<String>,
    pub meter_code: Option<String>,
    pub provider_code: Option<String>,
    pub region_code: Option<String>,
    pub catalog_key: Option<String>,
    pub formula_mode: String,
    pub multiplier: DecimalValue,
    pub markup_amount: Money,
    pub unit_price_override: Option<Money>,
    pub priority: i32,
    pub effective_from: DateTime<Utc>,
    pub effective_to: Option<DateTime<Utc>>,
    pub conditions: Vec<PricingRateCondition>,
    pub schedule: Option<PricingSchedule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRateCard {
    pub id: i64,
    pub rate_card_code: String,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub subject_type: String,
    pub subject_id: Option<i64>,
    pub subject_code: Option<String>,
    pub pricing_plan_tenant_id: i64,
    pub pricing_plan_organization_id: i64,
    pub pricing_plan_id: i64,
    pub pricing_plan_code: String,
    pub priority: i32,
    pub effective_from: DateTime<Utc>,
    pub effective_to: Option<DateTime<Utc>>,
}

impl AccountRateCard {
    pub fn is_effective_at(&self, occurred_at: DateTime<Utc>) -> bool {
        self.effective_from <= occurred_at && self.effective_to.is_none_or(|end| occurred_at < end)
    }
}

impl PricingRule {
    /// Returns whether the rule's product scope matches the resolved runtime
    /// dimensions. Scope fields are part of the rule identity and must be
    /// enforced before conditions and priority are evaluated; otherwise a
    /// sales rule for one model can leak into every resource in the plan.
    pub fn scope_matches(&self, dimensions: &PricingDimensionContext) -> bool {
        [
            (self.product_code.as_deref(), "product_code"),
            (self.operation_code.as_deref(), "operation_code"),
            (self.meter_code.as_deref(), "meter_code"),
            (self.provider_code.as_deref(), "provider_code"),
            (self.region_code.as_deref(), "region_code"),
            (self.catalog_key.as_deref(), "catalog_key"),
        ]
        .into_iter()
        .all(|(expected, dimension_code)| {
            expected.is_none_or(|expected| {
                dimensions
                    .get(dimension_code)
                    .and_then(Value::as_str)
                    .is_some_and(|actual| actual.trim() == expected.trim())
            })
        })
    }

    pub fn matches_at(
        &self,
        dimensions: &PricingDimensionContext,
        occurred_at: DateTime<Utc>,
    ) -> bool {
        self.effective_from <= occurred_at
            && self.effective_to.is_none_or(|end| occurred_at < end)
            && self
                .conditions
                .iter()
                .all(|condition| condition.matches(dimensions))
            && self
                .schedule
                .as_ref()
                .is_none_or(|schedule| schedule.matched_window_code(occurred_at).is_some())
    }

    pub fn specificity(&self) -> usize {
        [
            self.product_code.as_ref(),
            self.operation_code.as_ref(),
            self.meter_code.as_ref(),
            self.provider_code.as_ref(),
            self.region_code.as_ref(),
            self.catalog_key.as_ref(),
        ]
        .into_iter()
        .flatten()
        .count()
            + self.conditions.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingRateTier {
    pub tier_code: String,
    pub lower_bound: DecimalValue,
    pub upper_bound: Option<DecimalValue>,
    pub unit_size: DecimalValue,
    pub unit_price: Money,
    pub flat_amount: Money,
}

/// A deliberately bounded formula contract. The formula first converts the
/// measured quantity and selected numeric dimensions into pricing units; each
/// resolved price side then multiplies those units by its unit price.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingFormula {
    pub formula_code: String,
    pub formula_version: String,
    pub constant_units: DecimalValue,
    pub quantity_coefficient: DecimalValue,
    pub minimum_units: Option<DecimalValue>,
    pub maximum_units: Option<DecimalValue>,
    pub terms: Vec<PricingFormulaTerm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingFormulaTerm {
    pub term_code: String,
    pub dimension_code: String,
    pub coefficient: DecimalValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricingRateCondition {
    pub dimension_code: String,
    pub operator_code: String,
    pub value: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PricingDimensionContext {
    values: BTreeMap<String, Value>,
}

impl PricingDimensionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, dimension_code: impl Into<String>, value: Value) {
        let dimension_code = dimension_code.into();
        let dimension_code = dimension_code.trim();
        if !dimension_code.is_empty() && !value.is_null() {
            self.values.insert(dimension_code.to_owned(), value);
        }
    }

    pub fn with_value(mut self, dimension_code: impl Into<String>, value: Value) -> Self {
        self.insert(dimension_code, value);
        self
    }

    pub fn get(&self, dimension_code: &str) -> Option<&Value> {
        self.values.get(dimension_code.trim())
    }

    /// Drops a dimension so the rate can be matched without it.
    ///
    /// Used by the billing-region fallback: the terminal "any region" probe
    /// must not impose the originally requested `region_code` on rates that
    /// legitimately belong to a different region.
    pub fn remove(&mut self, dimension_code: &str) {
        self.values.remove(dimension_code.trim());
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.values
            .iter()
            .map(|(dimension_code, value)| (dimension_code.as_str(), value))
    }

    pub fn decimal(&self, dimension_code: &str) -> Option<DecimalValue> {
        self.get(dimension_code).and_then(decimal_value)
    }
}

impl PricingRateMetadata {
    pub fn matches(&self, dimensions: &PricingDimensionContext) -> bool {
        self.conditions
            .iter()
            .all(|condition| condition.matches(dimensions))
    }

    pub fn condition_count(&self) -> usize {
        self.conditions.len()
    }

    pub fn is_effective_at(&self, occurred_at: DateTime<Utc>) -> bool {
        self.effective_from <= occurred_at
            && self
                .effective_to
                .is_none_or(|effective_to| occurred_at < effective_to)
    }

    pub fn matched_window_code(&self, occurred_at: DateTime<Utc>) -> Option<&str> {
        match self.rate_variant {
            PricingRateVariant::Standard | PricingRateVariant::Batch => None,
            PricingRateVariant::TimeWindow => self
                .schedule
                .as_ref()
                .and_then(|schedule| schedule.matched_window_code(occurred_at)),
        }
    }

    pub fn matches_at(
        &self,
        dimensions: &PricingDimensionContext,
        occurred_at: DateTime<Utc>,
    ) -> bool {
        self.is_effective_at(occurred_at)
            && self.matches(dimensions)
            && match self.rate_variant {
                PricingRateVariant::Standard | PricingRateVariant::Batch => {
                    self.schedule.is_none()
                }
                PricingRateVariant::TimeWindow => self
                    .schedule
                    .as_ref()
                    .and_then(|schedule| schedule.matched_window_code(occurred_at))
                    .is_some(),
            }
    }
}

impl PricingSchedule {
    pub fn matched_window_code(&self, occurred_at: DateTime<Utc>) -> Option<&str> {
        let local = occurred_at.with_timezone(&self.time_zone);
        let local_date = local.date_naive();
        if self.exclude_dates.contains(&local_date) {
            return None;
        }
        if self.holiday_dates.contains(&local_date) {
            return self
                .weekly_windows
                .iter()
                .find(|window| window.holiday_only)
                .map(|window| window.window_code.as_str());
        }
        let local_time = local.time();
        self.weekly_windows.iter().find_map(|window| {
            if window.holiday_only {
                return None;
            }
            let start_date = if window.end_day_offset == 1 && local_time < window.end_time {
                local_date.checked_sub_signed(Duration::days(1))?
            } else {
                local_date
            };
            if self.exclude_dates.contains(&start_date) {
                return None;
            }
            let scheduled_day = window
                .days_of_week
                .contains(&(start_date.weekday().number_from_monday() as u8));
            if !scheduled_day && !self.include_dates.contains(&start_date) {
                return None;
            }
            let time_matches = if window.end_day_offset == 0 {
                local_time >= window.start_time && local_time < window.end_time
            } else {
                local_time >= window.start_time || local_time < window.end_time
            };
            time_matches.then_some(window.window_code.as_str())
        })
    }
}

impl PricingRateCondition {
    pub fn matches(&self, dimensions: &PricingDimensionContext) -> bool {
        let actual = dimensions.get(&self.dimension_code);
        match self.operator_code.as_str() {
            "exists" => self.value.as_bool().unwrap_or(true) == actual.is_some(),
            "eq" => actual.is_some_and(|actual| pricing_values_equal(actual, &self.value)),
            "neq" => actual.is_some_and(|actual| !pricing_values_equal(actual, &self.value)),
            "gt" => compare_decimal_values(actual, &self.value).is_some_and(|order| order.is_gt()),
            "gte" => compare_decimal_values(actual, &self.value)
                .is_some_and(|order| order.is_gt() || order.is_eq()),
            "lt" => compare_decimal_values(actual, &self.value).is_some_and(|order| order.is_lt()),
            "lte" => compare_decimal_values(actual, &self.value)
                .is_some_and(|order| order.is_lt() || order.is_eq()),
            "in" => actual.is_some_and(|actual| pricing_value_in(actual, &self.value)),
            "not_in" => actual.is_some_and(|actual| !pricing_value_in(actual, &self.value)),
            _ => false,
        }
    }
}

fn pricing_values_equal(actual: &Value, expected: &Value) -> bool {
    match (decimal_value(actual), decimal_value(expected)) {
        (Some(actual), Some(expected)) => actual == expected,
        _ => actual == expected,
    }
}

fn pricing_value_in(actual: &Value, expected: &Value) -> bool {
    match expected.as_array() {
        Some(values) => values
            .iter()
            .any(|candidate| pricing_values_equal(actual, candidate)),
        None => actual.as_array().is_some_and(|values| {
            values
                .iter()
                .any(|value| pricing_values_equal(value, expected))
        }),
    }
}

fn compare_decimal_values(actual: Option<&Value>, expected: &Value) -> Option<std::cmp::Ordering> {
    decimal_value(actual?)?.partial_cmp(&decimal_value(expected)?)
}

fn decimal_value(value: &Value) -> Option<DecimalValue> {
    let value = match value {
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.trim().to_owned(),
        _ => return None,
    };
    DecimalValue::parse(&value).ok()
}

impl ModelPrice {
    pub fn new(
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
        unit_price: Money,
    ) -> Self {
        let unit_size = default_unit_size(&billing_meter);
        Self {
            catalog_key: model.to_owned(),
            model: model.to_owned(),
            region_code: "global".to_owned(),
            price_side,
            billing_meter,
            unit_size,
            unit_price,
            supplier_code: None,
            account_id: None,
            pricing_plan_code: None,
            rate_metadata: None,
        }
    }

    pub fn new_for_catalog_key(
        catalog_key: &str,
        model: &str,
        price_side: PriceSide,
        billing_meter: BillingMeter,
        unit_price: Money,
    ) -> Self {
        let unit_size = default_unit_size(&billing_meter);
        Self {
            catalog_key: catalog_key.to_owned(),
            model: model.to_owned(),
            region_code: "global".to_owned(),
            price_side,
            billing_meter,
            unit_size,
            unit_price,
            supplier_code: None,
            account_id: None,
            pricing_plan_code: None,
            rate_metadata: None,
        }
    }

    pub fn with_catalog_key(mut self, catalog_key: &str) -> Self {
        self.catalog_key = catalog_key.to_owned();
        self
    }

    pub fn with_region_code(mut self, region_code: &str) -> Self {
        self.region_code = normalized_region_code(region_code);
        self
    }

    pub fn with_unit_size(mut self, unit_size: DecimalValue) -> Self {
        self.unit_size = unit_size;
        self
    }

    pub fn for_upstream_account(mut self, supplier_code: &str, account_id: i64) -> Self {
        self.supplier_code = Some(supplier_code.to_owned());
        self.account_id = Some(account_id);
        self
    }

    pub fn for_pricing_plan(mut self, pricing_plan_code: &str) -> Self {
        self.pricing_plan_code = Some(pricing_plan_code.to_owned());
        self
    }

    pub fn with_rate_metadata(mut self, rate_metadata: PricingRateMetadata) -> Self {
        self.rate_metadata = Some(rate_metadata);
        self
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
    use serde_json::json;

    use super::{
        PricingDimensionContext, PricingRateCondition, PricingSchedule, PricingWeeklyWindow,
    };

    #[test]
    fn pricing_conditions_match_string_numeric_membership_and_existence() {
        let dimensions = PricingDimensionContext::new()
            .with_value("quality", json!("hd"))
            .with_value("duration_seconds", json!(10))
            .with_value("resolution", json!("1080p"));

        for condition in [
            PricingRateCondition {
                dimension_code: "quality".to_owned(),
                operator_code: "eq".to_owned(),
                value: json!("hd"),
            },
            PricingRateCondition {
                dimension_code: "duration_seconds".to_owned(),
                operator_code: "gte".to_owned(),
                value: json!("10.000000"),
            },
            PricingRateCondition {
                dimension_code: "resolution".to_owned(),
                operator_code: "in".to_owned(),
                value: json!(["720p", "1080p"]),
            },
            PricingRateCondition {
                dimension_code: "quality".to_owned(),
                operator_code: "exists".to_owned(),
                value: json!(true),
            },
        ] {
            assert!(condition.matches(&dimensions));
        }
    }

    #[test]
    fn missing_dimensions_never_satisfy_negative_conditions() {
        let dimensions = PricingDimensionContext::new();
        for operator_code in ["neq", "not_in", "gt"] {
            let condition = PricingRateCondition {
                dimension_code: "quality".to_owned(),
                operator_code: operator_code.to_owned(),
                value: json!("hd"),
            };
            assert!(!condition.matches(&dimensions));
        }
    }

    fn schedule(window: PricingWeeklyWindow) -> PricingSchedule {
        schedule_with(vec![window], Vec::new())
    }

    fn schedule_with(
        weekly_windows: Vec<PricingWeeklyWindow>,
        holiday_dates: Vec<NaiveDate>,
    ) -> PricingSchedule {
        PricingSchedule {
            time_zone: "Asia/Shanghai".parse().expect("valid timezone"),
            weekly_windows,
            include_dates: Vec::new(),
            exclude_dates: Vec::new(),
            holiday_dates,
        }
    }

    fn window(
        window_code: &str,
        days_of_week: Vec<u8>,
        start: (u32, u32, u32),
        end: (u32, u32, u32),
        end_day_offset: u8,
        holiday_only: bool,
    ) -> PricingWeeklyWindow {
        PricingWeeklyWindow {
            window_code: window_code.to_owned(),
            days_of_week,
            start_time: NaiveTime::from_hms_opt(start.0, start.1, start.2).expect("valid time"),
            end_time: NaiveTime::from_hms_opt(end.0, end.1, end.2).expect("valid time"),
            end_day_offset,
            holiday_only,
        }
    }

    fn shanghai_instant(year: i32, month: u32, day: u32, hour: u32) -> chrono::DateTime<Utc> {
        chrono::FixedOffset::east_opt(8 * 3600)
            .expect("valid offset")
            .with_ymd_and_hms(year, month, day, hour, 0, 0)
            .single()
            .expect("valid instant")
            .with_timezone(&Utc)
    }

    #[test]
    fn schedule_evaluates_the_occurrence_in_its_iana_timezone() {
        let schedule = schedule(PricingWeeklyWindow {
            window_code: "weekday-morning".to_owned(),
            days_of_week: vec![1],
            start_time: NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
            end_time: NaiveTime::from_hms_opt(12, 0, 0).expect("valid time"),
            end_day_offset: 0,
            holiday_only: false,
        });
        let occurred_at = Utc
            .with_ymd_and_hms(2026, 8, 17, 2, 0, 0)
            .single()
            .expect("valid instant");

        assert_eq!(
            Some("weekday-morning"),
            schedule.matched_window_code(occurred_at)
        );
    }

    #[test]
    fn cross_midnight_window_uses_the_start_day_and_excludes_the_end_boundary() {
        let schedule = schedule(PricingWeeklyWindow {
            window_code: "friday-night".to_owned(),
            days_of_week: vec![5],
            start_time: NaiveTime::from_hms_opt(22, 0, 0).expect("valid time"),
            end_time: NaiveTime::from_hms_opt(2, 0, 0).expect("valid time"),
            end_day_offset: 1,
            holiday_only: false,
        });
        let friday_2330 = Utc
            .with_ymd_and_hms(2026, 8, 21, 15, 30, 0)
            .single()
            .expect("valid instant");
        let saturday_0130 = Utc
            .with_ymd_and_hms(2026, 8, 21, 17, 30, 0)
            .single()
            .expect("valid instant");
        let saturday_0200 = Utc
            .with_ymd_and_hms(2026, 8, 21, 18, 0, 0)
            .single()
            .expect("valid instant");

        assert_eq!(
            Some("friday-night"),
            schedule.matched_window_code(friday_2330)
        );
        assert_eq!(
            Some("friday-night"),
            schedule.matched_window_code(saturday_0130)
        );
        assert_eq!(None, schedule.matched_window_code(saturday_0200));
    }

    #[test]
    fn include_dates_enable_exception_days_and_exclude_dates_take_precedence() {
        let mut schedule = schedule(PricingWeeklyWindow {
            window_code: "morning".to_owned(),
            days_of_week: vec![1],
            start_time: NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
            end_time: NaiveTime::from_hms_opt(12, 0, 0).expect("valid time"),
            end_day_offset: 0,
            holiday_only: false,
        });
        let sunday = NaiveDate::from_ymd_opt(2026, 8, 23).expect("valid date");
        let occurred_at = Utc
            .with_ymd_and_hms(2026, 8, 23, 2, 0, 0)
            .single()
            .expect("valid instant");

        schedule.include_dates.push(sunday);
        assert_eq!(Some("morning"), schedule.matched_window_code(occurred_at));
        schedule.exclude_dates.push(sunday);
        assert_eq!(None, schedule.matched_window_code(occurred_at));
    }

    #[test]
    fn weekday_morning_matches_the_peak_window() {
        let schedule = schedule(window(
            "peak_weekday_morning",
            vec![1, 2, 3, 4, 5],
            (9, 0, 0),
            (12, 0, 0),
            0,
            false,
        ));

        assert_eq!(
            Some("peak_weekday_morning"),
            schedule.matched_window_code(shanghai_instant(2026, 8, 17, 10))
        );
    }

    #[test]
    fn holiday_on_a_weekday_matches_only_the_holiday_window() {
        let holiday = NaiveDate::from_ymd_opt(2026, 10, 1).expect("valid date");
        let peak = window(
            "peak_weekday_morning",
            vec![1, 2, 3, 4, 5],
            (9, 0, 0),
            (12, 0, 0),
            0,
            false,
        );
        let holiday_window = window(
            "off_peak_holiday",
            vec![1, 2, 3, 4, 5, 6, 7],
            (0, 0, 0),
            (23, 59, 59),
            0,
            true,
        );

        let peak_only = schedule_with(vec![peak.clone()], vec![holiday]);
        assert_eq!(
            None,
            peak_only.matched_window_code(shanghai_instant(2026, 10, 1, 10))
        );

        let with_holiday = schedule_with(vec![peak, holiday_window], vec![holiday]);
        assert_eq!(
            Some("off_peak_holiday"),
            with_holiday.matched_window_code(shanghai_instant(2026, 10, 1, 10))
        );
    }

    #[test]
    fn holiday_only_window_never_matches_a_non_holiday() {
        let schedule = schedule(window(
            "off_peak_holiday",
            vec![1, 2, 3, 4, 5, 6, 7],
            (0, 0, 0),
            (23, 59, 59),
            0,
            true,
        ));

        assert_eq!(
            None,
            schedule.matched_window_code(shanghai_instant(2026, 8, 17, 10))
        );
    }

    #[test]
    fn exclude_dates_take_precedence_over_holiday_windows() {
        let holiday = NaiveDate::from_ymd_opt(2026, 10, 1).expect("valid date");
        let mut schedule = schedule_with(
            vec![window(
                "off_peak_holiday",
                vec![1, 2, 3, 4, 5, 6, 7],
                (0, 0, 0),
                (23, 59, 59),
                0,
                true,
            )],
            vec![holiday],
        );
        let occurred_at = shanghai_instant(2026, 10, 1, 10);

        assert_eq!(
            Some("off_peak_holiday"),
            schedule.matched_window_code(occurred_at)
        );
        schedule.exclude_dates.push(holiday);
        assert_eq!(None, schedule.matched_window_code(occurred_at));
    }
}

fn default_unit_size(meter: &BillingMeter) -> DecimalValue {
    if matches!(
        meter,
        BillingMeter::LlmInputToken
            | BillingMeter::LlmOutputToken
            | BillingMeter::LlmReasoningToken
            | BillingMeter::LlmCacheWriteToken
            | BillingMeter::LlmCacheReadToken
            | BillingMeter::EmbeddingInputToken
            | BillingMeter::ImageInputToken
            | BillingMeter::ImageOutputToken
            | BillingMeter::AudioInputToken
            | BillingMeter::AudioOutputToken
            | BillingMeter::VideoInputToken
            | BillingMeter::VideoOutputToken
    ) {
        DecimalValue::parse("1000000").expect("token billing unit size is valid")
    } else {
        DecimalValue::ONE
    }
}

fn normalized_region_code(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "global".to_owned()
    } else {
        value.to_owned()
    }
}
