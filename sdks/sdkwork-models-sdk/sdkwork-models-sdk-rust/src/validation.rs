use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, NaiveTime};

use crate::types::ModelCatalog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogIssue {
    pub code: String,
    pub path: String,
    pub message: String,
}

pub fn validate_catalog(catalog: &ModelCatalog) -> Vec<CatalogIssue> {
    let meters = catalog
        .meters
        .iter()
        .map(|meter| meter.meter_code.as_str())
        .collect::<BTreeSet<_>>();
    let models = catalog
        .vendors
        .iter()
        .flat_map(|vendor| vendor.models.iter().map(|model| model.catalog_key.as_str()))
        .collect::<BTreeSet<_>>();
    let mut issues = Vec::new();
    for vendor in &catalog.vendors {
        for pricing in &vendor.pricing {
            if !models.contains(pricing.catalog_key.as_str()) {
                issues.push(CatalogIssue {
                    code: "pricing.model.missing".to_owned(),
                    path: pricing.catalog_key.clone(),
                    message: "pricing references an unknown model".to_owned(),
                });
            }
            for price in &pricing.prices {
                if !meters.contains(price.meter_code.as_str()) {
                    issues.push(CatalogIssue {
                        code: "pricing.meter.missing".to_owned(),
                        path: format!("{}/{}", pricing.catalog_key, price.meter_code),
                        message: "pricing references an unknown meter".to_owned(),
                    });
                }
                for (field, value) in [
                    ("unitSize", price.unit_size.as_str()),
                    ("unitPrice", price.unit_price.as_str()),
                    ("minimumQuantity", price.minimum_quantity.as_str()),
                ] {
                    if !is_decimal_string(value) {
                        issues.push(CatalogIssue {
                            code: "pricing.decimal.invalid".to_owned(),
                            path: format!("{}/{}", pricing.catalog_key, field),
                            message: "price quantity fields must be decimal strings".to_owned(),
                        });
                    }
                }
                for (field, value) in [
                    ("rateHash", price.rate_hash.as_str()),
                    ("priceBookCode", price.price_book_code.as_str()),
                    ("productCode", price.product_code.as_str()),
                    ("operationCode", price.operation_code.as_str()),
                ] {
                    if value.trim().is_empty() {
                        issues.push(CatalogIssue {
                            code: "pricing.identity.missing".to_owned(),
                            path: format!("{}/{}", pricing.catalog_key, field),
                            message: "pricing identity fields must not be empty".to_owned(),
                        });
                    }
                }
                if price.rate_hash.len() != 64
                    || !price
                        .rate_hash
                        .chars()
                        .all(|character| character.is_ascii_hexdigit())
                {
                    issues.push(CatalogIssue {
                        code: "pricing.rate_hash.invalid".to_owned(),
                        path: format!("{}/rateHash", pricing.catalog_key),
                        message: "rateHash must be a SHA-256 hex digest".to_owned(),
                    });
                }
                if !matches!(
                    price.billability.as_str(),
                    "chargeable" | "free" | "not_applicable" | "unknown"
                ) {
                    issues.push(CatalogIssue {
                        code: "pricing.billability.invalid".to_owned(),
                        path: format!("{}/billability", pricing.catalog_key),
                        message: "billability must be explicit".to_owned(),
                    });
                }
                let tiered_calculation =
                    matches!(price.calculation_mode.as_str(), "graduated" | "volume");
                if price.billability == "chargeable"
                    && !tiered_calculation
                    && is_zero_decimal(&price.unit_price)
                {
                    issues.push(CatalogIssue {
                        code: "pricing.chargeable.zero_price".to_owned(),
                        path: format!("{}/unitPrice", pricing.catalog_key),
                        message: "zero price cannot be inferred as chargeable".to_owned(),
                    });
                }
                if matches!(price.billability.as_str(), "free" | "not_applicable")
                    && !is_zero_decimal(&price.unit_price)
                {
                    issues.push(CatalogIssue {
                        code: "pricing.non_chargeable.positive_price".to_owned(),
                        path: format!("{}/unitPrice", pricing.catalog_key),
                        message: "free or not-applicable rates cannot have a positive price"
                            .to_owned(),
                    });
                }
                if !matches!(
                    price.calculation_mode.as_str(),
                    "per_unit" | "flat" | "graduated" | "volume" | "formula"
                ) {
                    issues.push(CatalogIssue {
                        code: "pricing.calculation_mode.invalid".to_owned(),
                        path: format!("{}/calculationMode", pricing.catalog_key),
                        message: "calculationMode is invalid".to_owned(),
                    });
                }
                if price.calculation_mode == "flat"
                    && compare_decimal_strings(&price.unit_size, "1")
                        != Some(std::cmp::Ordering::Equal)
                {
                    issues.push(CatalogIssue {
                        code: "pricing.flat.unit_size.invalid".to_owned(),
                        path: format!("{}/unitSize", pricing.catalog_key),
                        message: "flat pricing unitSize must equal one".to_owned(),
                    });
                }
                if price.priority < 0 {
                    issues.push(CatalogIssue {
                        code: "pricing.priority.invalid".to_owned(),
                        path: format!("{}/priority", pricing.catalog_key),
                        message: "priority must be zero or greater".to_owned(),
                    });
                }
                let currency = price.currency.as_deref().unwrap_or(&pricing.currency);
                if currency.len() != 3
                    || !currency
                        .chars()
                        .all(|character| character.is_ascii_uppercase())
                {
                    issues.push(CatalogIssue {
                        code: "pricing.currency.invalid".to_owned(),
                        path: format!("{}/currency", pricing.catalog_key),
                        message: "currency must be a three-letter uppercase ISO code".to_owned(),
                    });
                }
                validate_effective_window(pricing.catalog_key.as_str(), price, &mut issues);
                validate_schedule(pricing.catalog_key.as_str(), price, &mut issues);
                validate_tiers(pricing.catalog_key.as_str(), price, &mut issues);
                validate_formula(pricing.catalog_key.as_str(), price, &mut issues);
                let mut dimensions = BTreeSet::new();
                for condition in &price.conditions {
                    if condition.dimension_code.trim().is_empty()
                        || !matches!(
                            condition.operator.as_str(),
                            "eq" | "neq" | "gt" | "gte" | "lt" | "lte" | "in" | "not_in" | "exists"
                        )
                    {
                        issues.push(CatalogIssue {
                            code: "pricing.condition.invalid".to_owned(),
                            path: format!("{}/conditions", pricing.catalog_key),
                            message: "rate conditions require a dimension and operator".to_owned(),
                        });
                    }
                    if !dimensions.insert(condition.dimension_code.as_str()) {
                        issues.push(CatalogIssue {
                            code: "pricing.condition.duplicate".to_owned(),
                            path: format!("{}/conditions", pricing.catalog_key),
                            message: "a rate cannot repeat the same condition dimension".to_owned(),
                        });
                    }
                }
            }
        }
    }
    issues
}

fn validate_effective_window(
    catalog_key: &str,
    price: &crate::types::ModelPrice,
    issues: &mut Vec<CatalogIssue>,
) {
    let effective_from = parse_effective_instant(&price.effective_from);
    if effective_from.is_none() {
        issues.push(CatalogIssue {
            code: "pricing.effective_from.invalid".to_owned(),
            path: format!("{catalog_key}/effectiveFrom"),
            message: "effectiveFrom must be an RFC 3339 timestamp or ISO date".to_owned(),
        });
    }
    let effective_to = price
        .effective_to
        .as_deref()
        .and_then(parse_effective_instant);
    if price.effective_to.is_some() && effective_to.is_none() {
        issues.push(CatalogIssue {
            code: "pricing.effective_to.invalid".to_owned(),
            path: format!("{catalog_key}/effectiveTo"),
            message: "effectiveTo must be an RFC 3339 timestamp or ISO date".to_owned(),
        });
    }
    if effective_from
        .zip(effective_to)
        .is_some_and(|(start, end)| end <= start)
    {
        issues.push(CatalogIssue {
            code: "pricing.effective_window.invalid".to_owned(),
            path: format!("{catalog_key}/effectiveTo"),
            message: "effectiveTo must be later than effectiveFrom".to_owned(),
        });
    }
}

fn parse_effective_instant(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.timestamp())
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|value| value.and_utc().timestamp())
        })
}

fn validate_schedule(
    catalog_key: &str,
    price: &crate::types::ModelPrice,
    issues: &mut Vec<CatalogIssue>,
) {
    match (price.rate_variant.as_str(), price.schedule.as_ref()) {
        ("standard", None) => return,
        ("standard", Some(_)) => {
            issues.push(CatalogIssue {
                code: "pricing.schedule.unexpected".to_owned(),
                path: format!("{catalog_key}/schedule"),
                message: "standard rates must not define a schedule".to_owned(),
            });
            return;
        }
        ("time_window", None) => {
            issues.push(CatalogIssue {
                code: "pricing.schedule.missing".to_owned(),
                path: format!("{catalog_key}/schedule"),
                message: "time-window rates require a schedule".to_owned(),
            });
            return;
        }
        ("time_window", Some(_)) => {}
        _ => {
            issues.push(CatalogIssue {
                code: "pricing.rate_variant.invalid".to_owned(),
                path: format!("{catalog_key}/rateVariant"),
                message: "rateVariant must be standard or time_window".to_owned(),
            });
            return;
        }
    }

    let schedule = price.schedule.as_ref().expect("schedule checked above");
    if schedule.time_zone.parse::<chrono_tz::Tz>().is_err() {
        issues.push(CatalogIssue {
            code: "pricing.schedule.time_zone.invalid".to_owned(),
            path: format!("{catalog_key}/schedule/timeZone"),
            message: "schedule timeZone must be an IANA time-zone identifier".to_owned(),
        });
    }
    if schedule.weekly_windows.is_empty() {
        issues.push(CatalogIssue {
            code: "pricing.schedule.windows.missing".to_owned(),
            path: format!("{catalog_key}/schedule/weeklyWindows"),
            message: "a time-window schedule requires at least one weekly window".to_owned(),
        });
    }

    let mut window_codes = BTreeSet::new();
    for (index, window) in schedule.weekly_windows.iter().enumerate() {
        let path = format!("{catalog_key}/schedule/weeklyWindows/{index}");
        if window.window_code.trim().is_empty() || !window_codes.insert(window.window_code.as_str())
        {
            issues.push(CatalogIssue {
                code: "pricing.schedule.window_code.invalid".to_owned(),
                path: format!("{path}/windowCode"),
                message: "windowCode must be non-empty and unique within the schedule".to_owned(),
            });
        }
        let unique_days = window.days_of_week.iter().copied().collect::<BTreeSet<_>>();
        if window.days_of_week.is_empty()
            || unique_days.len() != window.days_of_week.len()
            || unique_days.iter().any(|day| !(1..=7).contains(day))
        {
            issues.push(CatalogIssue {
                code: "pricing.schedule.days.invalid".to_owned(),
                path: format!("{path}/daysOfWeek"),
                message: "daysOfWeek must contain unique ISO weekdays from 1 through 7".to_owned(),
            });
        }
        let start = NaiveTime::parse_from_str(&window.start_time, "%H:%M:%S").ok();
        let end = NaiveTime::parse_from_str(&window.end_time, "%H:%M:%S").ok();
        if start.is_none() || end.is_none() {
            issues.push(CatalogIssue {
                code: "pricing.schedule.time.invalid".to_owned(),
                path: path.clone(),
                message: "startTime and endTime must use HH:mm:ss".to_owned(),
            });
        }
        if window.holiday_only
            && (window.start_time != "00:00:00"
                || window.end_time != "23:59:59"
                || window.end_day_offset != 0)
        {
            issues.push(CatalogIssue {
                code: "pricing.schedule.holiday_only.range.invalid".to_owned(),
                path: path.clone(),
                message:
                    "holidayOnly windows must cover the whole day (00:00:00-23:59:59, endDayOffset 0)"
                        .to_owned(),
            });
        }
        if !matches!(window.end_day_offset, 0 | 1)
            || start.zip(end).is_some_and(|(start, end)| {
                (window.end_day_offset == 0 && end <= start)
                    || (window.end_day_offset == 1 && end >= start)
            })
        {
            issues.push(CatalogIssue {
                code: "pricing.schedule.range.invalid".to_owned(),
                path,
                message: "same-day windows require endTime after startTime; cross-midnight windows require endDayOffset 1 and endTime before startTime".to_owned(),
            });
        }
    }

    let include_dates = validate_schedule_dates(
        catalog_key,
        "includeDates",
        &schedule.include_dates,
        issues,
        "pricing.schedule.date.invalid",
    );
    let exclude_dates = validate_schedule_dates(
        catalog_key,
        "excludeDates",
        &schedule.exclude_dates,
        issues,
        "pricing.schedule.date.invalid",
    );
    let holiday_dates = validate_schedule_dates(
        catalog_key,
        "holidayDates",
        &schedule.holiday_dates,
        issues,
        "pricing.schedule.holiday_date.invalid",
    );
    if include_dates.intersection(&exclude_dates).next().is_some() {
        issues.push(CatalogIssue {
            code: "pricing.schedule.date_conflict".to_owned(),
            path: format!("{catalog_key}/schedule"),
            message: "a date cannot be both included and excluded".to_owned(),
        });
    }
    for (field, dates) in [
        ("includeDates", &include_dates),
        ("excludeDates", &exclude_dates),
    ] {
        if holiday_dates.intersection(dates).next().is_some() {
            issues.push(CatalogIssue {
                code: "pricing.schedule.holiday_date.conflict".to_owned(),
                path: format!("{catalog_key}/schedule"),
                message: format!("holidayDates must not overlap {field}"),
            });
        }
    }
}

fn validate_schedule_dates(
    catalog_key: &str,
    field: &str,
    values: &[String],
    issues: &mut Vec<CatalogIssue>,
    invalid_code: &str,
) -> BTreeSet<NaiveDate> {
    let mut dates = BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            Ok(date) if dates.insert(date) => {}
            _ => issues.push(CatalogIssue {
                code: invalid_code.to_owned(),
                path: format!("{catalog_key}/schedule/{field}/{index}"),
                message: "schedule dates must be unique ISO dates".to_owned(),
            }),
        }
    }
    dates
}

pub fn is_decimal_string(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let (integer, fraction) = match value.split_once('.') {
        Some((integer, fraction)) if !fraction.is_empty() => (integer, Some(fraction)),
        Some(_) => return false,
        None => (value, None),
    };
    if integer != "0" && integer.starts_with('0') {
        return false;
    }
    if integer.is_empty() || !integer.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    fraction
        .map(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        .unwrap_or(true)
}

fn is_zero_decimal(value: &str) -> bool {
    compare_decimal_strings(value, "0") == Some(std::cmp::Ordering::Equal)
}

fn validate_tiers(
    catalog_key: &str,
    price: &crate::types::ModelPrice,
    issues: &mut Vec<CatalogIssue>,
) {
    let tiered = matches!(price.calculation_mode.as_str(), "graduated" | "volume");
    if tiered && price.tiers.is_empty() {
        issues.push(CatalogIssue {
            code: "pricing.tiers.missing".to_owned(),
            path: format!("{catalog_key}/tiers"),
            message: "graduated and volume rates require at least one tier".to_owned(),
        });
    }
    if !tiered && !price.tiers.is_empty() {
        issues.push(CatalogIssue {
            code: "pricing.tiers.unexpected".to_owned(),
            path: format!("{catalog_key}/tiers"),
            message: "tiers are allowed only for graduated and volume rates".to_owned(),
        });
    }

    let mut expected_lower = Some("0");
    let mut tier_codes = BTreeSet::new();
    for (index, tier) in price.tiers.iter().enumerate() {
        let path = format!("{catalog_key}/tiers/{index}");
        for (field, value) in [
            ("lowerBound", tier.lower_bound.as_str()),
            ("unitSize", tier.unit_size.as_str()),
            ("unitPrice", tier.unit_price.as_str()),
            ("flatAmount", tier.flat_amount.as_str()),
        ] {
            if !is_decimal_string(value) {
                issues.push(CatalogIssue {
                    code: "pricing.tier.decimal.invalid".to_owned(),
                    path: format!("{path}/{field}"),
                    message: format!("{field} must be a decimal string"),
                });
            }
        }
        if tier
            .upper_bound
            .as_deref()
            .is_some_and(|value| !is_decimal_string(value))
        {
            issues.push(CatalogIssue {
                code: "pricing.tier.decimal.invalid".to_owned(),
                path: format!("{path}/upperBound"),
                message: "upperBound must be a decimal string or null".to_owned(),
            });
        }
        if tier.tier_code.trim().is_empty() || !tier_codes.insert(tier.tier_code.as_str()) {
            issues.push(CatalogIssue {
                code: "pricing.tier.code.invalid".to_owned(),
                path: format!("{path}/tierCode"),
                message: "tierCode must be non-empty and unique within the rate".to_owned(),
            });
        }
        if expected_lower.is_none()
            || expected_lower.is_some_and(|expected| {
                compare_decimal_strings(&tier.lower_bound, expected)
                    != Some(std::cmp::Ordering::Equal)
            })
        {
            issues.push(CatalogIssue {
                code: "pricing.tier.range.gap".to_owned(),
                path: format!("{path}/lowerBound"),
                message: "tier ranges must start at zero and remain contiguous".to_owned(),
            });
        }
        if tier.upper_bound.as_deref().is_some_and(|upper| {
            compare_decimal_strings(upper, &tier.lower_bound)
                .is_some_and(|ordering| !ordering.is_gt())
        }) {
            issues.push(CatalogIssue {
                code: "pricing.tier.range.invalid".to_owned(),
                path: format!("{path}/upperBound"),
                message: "upperBound must be greater than lowerBound".to_owned(),
            });
        }
        if tier.upper_bound.is_none() && index + 1 != price.tiers.len() {
            issues.push(CatalogIssue {
                code: "pricing.tier.range.open".to_owned(),
                path: format!("{path}/upperBound"),
                message: "only the final tier may have an open upper bound".to_owned(),
            });
        }
        if !is_positive_decimal(&tier.unit_size) {
            issues.push(CatalogIssue {
                code: "pricing.tier.unit_size.invalid".to_owned(),
                path: format!("{path}/unitSize"),
                message: "tier unitSize must be positive".to_owned(),
            });
        }
        let tier_has_amount =
            is_positive_decimal(&tier.unit_price) || is_positive_decimal(&tier.flat_amount);
        if price.billability == "chargeable" && !tier_has_amount {
            issues.push(CatalogIssue {
                code: "pricing.tier.chargeable.zero_price".to_owned(),
                path: path.clone(),
                message: "each chargeable tier must have a positive unitPrice or flatAmount"
                    .to_owned(),
            });
        }
        if matches!(price.billability.as_str(), "free" | "not_applicable") && tier_has_amount {
            issues.push(CatalogIssue {
                code: "pricing.tier.non_chargeable.positive_price".to_owned(),
                path,
                message: "non-chargeable tiers cannot contain positive amounts".to_owned(),
            });
        }
        expected_lower = tier.upper_bound.as_deref();
    }
    if tiered
        && price
            .tiers
            .last()
            .is_some_and(|tier| tier.upper_bound.is_some())
    {
        issues.push(CatalogIssue {
            code: "pricing.tier.range.unbounded".to_owned(),
            path: format!("{catalog_key}/tiers"),
            message: "the final tier must have a null upperBound".to_owned(),
        });
    }
}

fn validate_formula(
    catalog_key: &str,
    price: &crate::types::ModelPrice,
    issues: &mut Vec<CatalogIssue>,
) {
    if price.calculation_mode == "formula" && price.formula.is_none() {
        issues.push(CatalogIssue {
            code: "pricing.formula.missing".to_owned(),
            path: format!("{catalog_key}/formula"),
            message: "formula rates require a formula definition".to_owned(),
        });
    }
    if price.calculation_mode != "formula" && price.formula.is_some() {
        issues.push(CatalogIssue {
            code: "pricing.formula.unexpected".to_owned(),
            path: format!("{catalog_key}/formula"),
            message: "formula is allowed only for formula rates".to_owned(),
        });
    }
    let Some(formula) = price.formula.as_ref() else {
        return;
    };
    if formula.formula_code.trim().is_empty() || formula.formula_version.trim().is_empty() {
        issues.push(CatalogIssue {
            code: "pricing.formula.identity.invalid".to_owned(),
            path: format!("{catalog_key}/formula"),
            message: "formulaCode and formulaVersion must not be empty".to_owned(),
        });
    }
    for (field, value) in [
        ("constantUnits", Some(formula.constant_units.as_str())),
        (
            "quantityCoefficient",
            Some(formula.quantity_coefficient.as_str()),
        ),
        ("minimumUnits", formula.minimum_units.as_deref()),
        ("maximumUnits", formula.maximum_units.as_deref()),
    ] {
        if value.is_some_and(|value| !is_decimal_string(value)) {
            issues.push(CatalogIssue {
                code: "pricing.formula.decimal.invalid".to_owned(),
                path: format!("{catalog_key}/formula/{field}"),
                message: format!("{field} must be a decimal string"),
            });
        }
    }
    if formula
        .minimum_units
        .as_deref()
        .zip(formula.maximum_units.as_deref())
        .is_some_and(|(minimum, maximum)| {
            compare_decimal_strings(maximum, minimum) == Some(std::cmp::Ordering::Less)
        })
    {
        issues.push(CatalogIssue {
            code: "pricing.formula.bounds.invalid".to_owned(),
            path: format!("{catalog_key}/formula"),
            message: "maximumUnits must be greater than or equal to minimumUnits".to_owned(),
        });
    }
    let mut term_codes = BTreeSet::new();
    let mut term_dimensions = BTreeSet::new();
    for (index, term) in formula.terms.iter().enumerate() {
        let path = format!("{catalog_key}/formula/terms/{index}");
        if term.term_code.trim().is_empty()
            || term.dimension_code.trim().is_empty()
            || !term_codes.insert(term.term_code.as_str())
            || !term_dimensions.insert(term.dimension_code.as_str())
        {
            issues.push(CatalogIssue {
                code: "pricing.formula.term.invalid".to_owned(),
                path: path.clone(),
                message: "formula term codes and dimensions must be non-empty and unique"
                    .to_owned(),
            });
        }
        if !is_decimal_string(&term.coefficient) {
            issues.push(CatalogIssue {
                code: "pricing.formula.term.coefficient.invalid".to_owned(),
                path: format!("{path}/coefficient"),
                message: "formula coefficient must be a decimal string".to_owned(),
            });
        }
    }
}

fn is_positive_decimal(value: &str) -> bool {
    compare_decimal_strings(value, "0") == Some(std::cmp::Ordering::Greater)
}

fn compare_decimal_strings(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    if !is_decimal_string(left) || !is_decimal_string(right) {
        return None;
    }
    let (left_whole, left_fraction) = decimal_parts(left);
    let (right_whole, right_fraction) = decimal_parts(right);
    let whole_order = left_whole
        .len()
        .cmp(&right_whole.len())
        .then_with(|| left_whole.cmp(right_whole));
    if !whole_order.is_eq() {
        return Some(whole_order);
    }
    let width = left_fraction.len().max(right_fraction.len());
    for index in 0..width {
        let left_digit = left_fraction.as_bytes().get(index).copied().unwrap_or(b'0');
        let right_digit = right_fraction
            .as_bytes()
            .get(index)
            .copied()
            .unwrap_or(b'0');
        let order = left_digit.cmp(&right_digit);
        if !order.is_eq() {
            return Some(order);
        }
    }
    Some(std::cmp::Ordering::Equal)
}

fn decimal_parts(value: &str) -> (&str, &str) {
    value.split_once('.').unwrap_or((value, ""))
}
