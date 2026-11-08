use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::runtime_id::next_cloud_runtime_id;
use crate::sql_model_rankings::{add_seconds_to_timestamp, normalize_iso_timestamp, period_code};
use sdkwork_models_contract_service::{
    normalize_rank_scope, normalize_scope_ids, normalize_snapshot_period, DecimalValue,
    DomainError, ModelRankingRefreshAuditCommand, ModelRankingRefreshAuditFuture,
    ModelRankingRefreshCommand, ModelRankingRefreshFuture, ModelRankingRefreshOutcome,
    ModelRankingRefreshRunStatus, ModelRankingRefreshStore,
};

const MODEL_RANKING_REFRESH_JOB_TYPE: i64 = 20;

#[derive(Debug, Clone)]
pub struct PostgresModelRankingRefreshStore {
    pool: PgPool,
}

impl PostgresModelRankingRefreshStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ModelRankingRefreshStore for PostgresModelRankingRefreshStore {
    fn refresh_model_rankings<'a>(
        &'a self,
        command: ModelRankingRefreshCommand,
    ) -> ModelRankingRefreshFuture<'a> {
        Box::pin(async move { refresh_model_rankings(&self.pool, command).await })
    }

    fn record_model_ranking_refresh_audit<'a>(
        &'a self,
        command: ModelRankingRefreshAuditCommand,
    ) -> ModelRankingRefreshAuditFuture<'a> {
        Box::pin(async move { record_model_ranking_refresh_audit(&self.pool, command).await })
    }
}

#[derive(Debug, Clone)]
struct RankingAggregate {
    source_rows: i64,
    model_id: i64,
    catalog_key: String,
    model: String,
    vendor_code: String,
    region_code: String,
    vendor_name_snapshot: String,
    modality: i64,
    color_token: String,
    license_type: i64,
    context_tokens: i64,
    request_count: i64,
    token_count: i64,
    cost_amount: DecimalValue,
    currency: String,
    previous_rank_no: Option<i64>,
}

async fn refresh_model_rankings(
    pool: &PgPool,
    command: ModelRankingRefreshCommand,
) -> Result<ModelRankingRefreshOutcome, DomainError> {
    let command = normalize_refresh_command(command);
    let limit = command.limit.max(0);
    if limit == 0 {
        return Ok(ModelRankingRefreshOutcome {
            rank_scope: command.rank_scope,
            snapshot_date: command.snapshot_date,
            snapshot_period: command.snapshot_period,
            window_start: command.window_start,
            window_end: command.window_end,
            next_refresh_at: add_seconds_to_timestamp(
                &command.requested_at,
                command.refresh_interval_seconds,
            ),
            run_status: ModelRankingRefreshRunStatus::Empty,
            ..ModelRankingRefreshOutcome::default()
        });
    }

    let snapshot_period_code = period_code(&command.snapshot_period);
    let next_refresh_at =
        add_seconds_to_timestamp(&command.requested_at, command.refresh_interval_seconds);
    let metadata = ranking_metadata(&command, &next_refresh_at)?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|error| store_error("failed to begin model ranking refresh transaction", error))?;
    let rows = load_ranking_aggregates(&mut tx, &command, snapshot_period_code, limit).await?;
    if rows.is_empty() {
        tx.commit().await.map_err(|error| {
            store_error(
                "failed to commit empty model ranking refresh transaction",
                error,
            )
        })?;
        return Ok(ModelRankingRefreshOutcome {
            generated_count: 0,
            source_count: 0,
            rank_scope: command.rank_scope,
            snapshot_date: command.snapshot_date,
            snapshot_period: command.snapshot_period,
            window_start: command.window_start,
            window_end: command.window_end,
            next_refresh_at,
            run_status: ModelRankingRefreshRunStatus::Empty,
        });
    }
    deactivate_existing_snapshot(&mut tx, &command, snapshot_period_code).await?;

    let mut generated_count = 0;
    let mut source_count = 0;
    for (index, row) in rows.iter().enumerate() {
        let rank_no = index as i64 + 1;
        source_count += row.source_rows;
        upsert_ranking_snapshot(
            &mut tx,
            &command,
            snapshot_period_code,
            &metadata,
            row,
            rank_no,
        )
        .await?;
        generated_count += 1;
    }

    tx.commit().await.map_err(|error| {
        store_error("failed to commit model ranking refresh transaction", error)
    })?;

    Ok(ModelRankingRefreshOutcome {
        generated_count,
        source_count,
        rank_scope: command.rank_scope,
        snapshot_date: command.snapshot_date,
        snapshot_period: command.snapshot_period,
        window_start: command.window_start,
        window_end: command.window_end,
        next_refresh_at,
        run_status: ModelRankingRefreshRunStatus::Succeeded,
    })
}

fn normalize_refresh_command(command: ModelRankingRefreshCommand) -> ModelRankingRefreshCommand {
    let (tenant_id, organization_id) =
        normalize_scope_ids(command.tenant_id, command.organization_id);
    let rank_scope = normalize_rank_scope(Some(&command.rank_scope));
    let snapshot_period = normalize_snapshot_period(Some(&command.snapshot_period));
    ModelRankingRefreshCommand {
        tenant_id,
        organization_id,
        rank_scope,
        snapshot_period,
        ..command
    }
}

async fn record_model_ranking_refresh_audit(
    pool: &PgPool,
    command: ModelRankingRefreshAuditCommand,
) -> Result<(), DomainError> {
    let command = normalize_audit_command(command);
    let payload = audit_payload(&command)?;
    let failure_reason = command
        .failure_reason
        .as_deref()
        .map(truncate_failure_reason);
    sqlx::query(
        r#"
        INSERT INTO ops_job_execution
            (id, uuid, tenant_id, organization_id, status, metadata, job_name, job_type, trigger_type,
             started_at, ended_at, duration_ms, execution_status, processed_count, success_count,
             failure_count, failure_reason, payload)
        VALUES
            ($1, $2, $3, $4, 1, $5::jsonb, $6, $7, $8,
             $9::timestamp AT TIME ZONE 'UTC', $10::timestamp AT TIME ZONE 'UTC',
             $11, $12, $13, $14, $15, $16, $17::jsonb)
        "#,
    )
    .bind(next_cloud_runtime_id("ops_job_execution")?)
    .bind(stable_uuid(
        "job",
        &[
            &command.job_name,
            &command.tenant_id.to_string(),
            &command.organization_id.to_string(),
            &command.rank_scope,
            &command.started_at,
            &command.ended_at,
        ],
    ))
    .bind(command.tenant_id)
    .bind(command.organization_id)
    .bind(r#"{"module":"model_rankings","component":"model_ranking_refresh_worker"}"#)
    .bind(&command.job_name)
    .bind(MODEL_RANKING_REFRESH_JOB_TYPE)
    .bind(normalize_trigger_type(command.trigger_type))
    .bind(sql_timestamp(&command.started_at))
    .bind(sql_timestamp(&command.ended_at))
    .bind(command.duration_ms.max(0))
    .bind(execution_status_code(&command.status))
    .bind(command.source_count.max(command.generated_count).max(0))
    .bind(command.success_count.max(0))
    .bind(command.failure_count.max(0))
    .bind(failure_reason)
    .bind(payload)
    .execute(pool)
    .await
    .map_err(|error| store_error("failed to record model ranking refresh audit job", error))?;
    Ok(())
}

fn normalize_audit_command(
    command: ModelRankingRefreshAuditCommand,
) -> ModelRankingRefreshAuditCommand {
    let (tenant_id, organization_id) =
        normalize_scope_ids(command.tenant_id, command.organization_id);
    let rank_scope = normalize_rank_scope(Some(&command.rank_scope));
    let snapshot_period = normalize_snapshot_period(Some(&command.snapshot_period));
    ModelRankingRefreshAuditCommand {
        tenant_id,
        organization_id,
        rank_scope,
        snapshot_period,
        ..command
    }
}

async fn load_ranking_aggregates(
    tx: &mut Transaction<'_, Postgres>,
    command: &ModelRankingRefreshCommand,
    snapshot_period_code: i64,
    limit: i64,
) -> Result<Vec<RankingAggregate>, DomainError> {
    let rows = sqlx::query(
        r#"
        WITH model_scope AS (
            SELECT
                m.id AS model_id,
                m.tenant_id,
                m.organization_id,
                m.catalog_key,
                m.model,
                COALESCE(NULLIF(m.display_name, ''), m.model, m.catalog_key) AS display_name,
                COALESCE(m.vendor_code, '') AS vendor_code,
                'global' AS region_code,
                COALESCE(NULLIF(m.vendor_name_snapshot, ''), m.vendor_code, 'Unknown') AS vendor_name_snapshot,
                COALESCE(m.capability, 1) AS modality,
                COALESCE(NULLIF(m.color_token, ''), '#64748b') AS color_token,
                COALESCE(m.license_type, 2) AS license_type,
                COALESCE(m.context_tokens, 0) AS context_tokens,
                ROW_NUMBER() OVER (
                    PARTITION BY m.catalog_key
                    ORDER BY
                        CASE
                            WHEN m.tenant_id = $1 AND m.organization_id = $2 THEN 3
                            WHEN m.tenant_id = $1 AND m.organization_id = 0 THEN 2
                            WHEN m.tenant_id = 0 AND m.organization_id = 0 THEN 1
                            ELSE 0
                        END DESC,
                        CAST(COALESCE(NULLIF(CAST(m.rank_score AS TEXT), ''), '0') AS NUMERIC) DESC,
                        m.id DESC
                ) AS model_row_no
            FROM ai_model m
            WHERE m.status = 1
              AND m.deleted_at IS NULL
              AND COALESCE(m.release_stage, 1) IN (1, 2)
              AND COALESCE(m.shelf_state, 1) = 1
              AND COALESCE(m.routing_state, 1) = 1
              AND (
                  ($1 > 0 AND m.tenant_id = $1 AND m.organization_id = $2)
                  OR ($1 > 0 AND $2 > 0 AND m.tenant_id = $1 AND m.organization_id = 0)
                  OR (m.tenant_id = 0 AND m.organization_id = 0)
              )
              AND COALESCE(NULLIF(m.catalog_key, ''), '') <> ''
        ),
        previous_rank AS (
            SELECT vendor_code, region_code, catalog_key, rank_no
            FROM (
                SELECT
                    r.vendor_code,
                    r.region_code,
                    r.catalog_key,
                    r.rank_no,
                    ROW_NUMBER() OVER (
                        PARTITION BY r.vendor_code, r.region_code, r.catalog_key
                        ORDER BY r.snapshot_date DESC NULLS LAST, r.snapshot_period DESC NULLS LAST, r.rank_no ASC
                    ) AS previous_row_no
                FROM ai_model_rank_snapshot r
                WHERE r.status = 1
                  AND r.tenant_id = $1
                  AND r.organization_id = $2
                  AND COALESCE(r.rank_scope, 'commercial-default') = $3
                  AND r.snapshot_period = $4
                  AND r.snapshot_date < $5::date
            ) previous_rows
            WHERE previous_row_no = 1
        ),
        billable_usage AS (
            SELECT
                c.id,
                c.tenant_id,
                c.organization_id,
                m.catalog_key,
                COALESCE(
                    NULLIF(d.pricing_snapshot #>> '{resource,requestedModel}', ''),
                    NULLIF(d.pricing_snapshot #>> '{model,model}', ''),
                    NULLIF(m.catalog_key, '')
                ) AS model,
                COALESCE((m.dimensions_json ->> 'modality')::integer, 0) AS modality,
                COALESCE((m.dimensions_json ->> 'requestCount')::bigint, 0) AS request_count,
                COALESCE((m.dimensions_json ->> 'totalTokens')::bigint, 0) AS total_tokens,
                c.amount AS customer_charge_amount,
                c.currency_code AS currency,
                c.charged_at AS occurred_at
            FROM cloudrouter_charge_line c
            JOIN cloudrouter_rating_decision d
              ON d.tenant_id = c.tenant_id
             AND d.organization_id = c.organization_id
             AND d.id = c.rating_decision_id
            JOIN cloudrouter_usage_measurement m
              ON m.tenant_id = d.tenant_id
             AND m.organization_id = d.organization_id
             AND m.id = d.measurement_id
            WHERE c.status = 1
              AND c.charge_status IN ('rated', 'settled')
              AND c.amount > 0
              AND d.status = 1
              AND d.decision_status = 'rated'
              AND d.billability = 'chargeable'
            UNION ALL
            SELECT
                legacy.id,
                legacy.tenant_id,
                legacy.organization_id,
                legacy.catalog_key,
                legacy.model,
                COALESCE(legacy.modality, 0),
                COALESCE(legacy.request_count, 1),
                COALESCE(legacy.total_tokens, 0),
                legacy.customer_charge_amount,
                legacy.currency,
                legacy.occurred_at
            FROM ai_metering_usage legacy
            WHERE legacy.status = 1
              AND COALESCE(legacy.customer_charge_amount, 0) > 0
              AND NOT EXISTS (
                  SELECT 1
                  FROM cloudrouter_rating_decision current_decision
                  WHERE current_decision.tenant_id = legacy.tenant_id
                    AND current_decision.organization_id = legacy.organization_id
                    AND current_decision.invocation_id = legacy.request_id
                    AND current_decision.status = 1
              )
        ),
        usage_aggregate AS (
            SELECT
                COUNT(u.id) AS source_rows,
                m.model_id,
                m.catalog_key,
                COALESCE(NULLIF(m.model, ''), NULLIF(u.model, ''), u.catalog_key) AS model,
                m.vendor_code,
                m.region_code,
                m.vendor_name_snapshot,
                COALESCE(u.modality, m.modality, 1) AS modality,
                m.color_token,
                m.license_type,
                m.context_tokens,
                SUM(COALESCE(u.request_count, 1)) AS request_count,
                SUM(COALESCE(u.total_tokens, 0)) AS token_count,
                SUM(COALESCE(u.customer_charge_amount, 0)) AS cost_amount,
                COALESCE(NULLIF(MAX(u.currency), ''), 'USD') AS currency
            FROM billable_usage u
            JOIN model_scope m
              ON m.catalog_key = u.catalog_key
             AND m.model_row_no = 1
            WHERE ($1 <= 0 OR u.tenant_id = $1)
              AND ($2 <= 0 OR u.organization_id = $2)
              AND COALESCE(NULLIF(u.catalog_key, ''), '') <> ''
              AND u.occurred_at >= $6::timestamp AT TIME ZONE 'UTC'
              AND u.occurred_at < $7::timestamp AT TIME ZONE 'UTC'
            GROUP BY
                m.model_id,
                m.catalog_key,
                COALESCE(NULLIF(m.model, ''), NULLIF(u.model, ''), u.catalog_key),
                m.vendor_code,
                m.region_code,
                m.vendor_name_snapshot,
                COALESCE(u.modality, m.modality, 1),
                m.color_token,
                m.license_type,
                m.context_tokens
        )
        SELECT
            CAST(a.source_rows AS TEXT) AS source_rows,
            CAST(a.model_id AS TEXT) AS model_id,
            a.catalog_key,
            a.model,
            a.vendor_code,
            a.region_code,
            a.vendor_name_snapshot,
            CAST(a.modality AS TEXT) AS modality,
            a.color_token,
            CAST(a.license_type AS TEXT) AS license_type,
            CAST(a.context_tokens AS TEXT) AS context_tokens,
            CAST(a.request_count AS TEXT) AS request_count,
            CAST(a.token_count AS TEXT) AS token_count,
            CAST(a.cost_amount AS TEXT) AS cost_amount,
            a.currency,
            CAST(p.rank_no AS TEXT) AS previous_rank_no
        FROM usage_aggregate a
        LEFT JOIN previous_rank p
          ON p.vendor_code = a.vendor_code
         AND p.region_code = a.region_code
         AND p.catalog_key = a.catalog_key
        ORDER BY
            a.request_count DESC,
            a.token_count DESC,
            a.cost_amount DESC,
            a.catalog_key ASC
        LIMIT $8
        "#,
    )
    .bind(command.tenant_id)
    .bind(command.organization_id)
    .bind(&command.rank_scope)
    .bind(snapshot_period_code)
    .bind(&command.snapshot_date)
    .bind(sql_timestamp(&command.window_start))
    .bind(sql_timestamp(&command.window_end))
    .bind(limit)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error("failed to aggregate model ranking usage facts", error))?;

    rows.iter()
        .map(|row| {
            Ok(RankingAggregate {
                source_rows: integer_cell(row, "source_rows"),
                model_id: integer_cell(row, "model_id"),
                catalog_key: string_cell(row, "catalog_key"),
                model: string_cell(row, "model"),
                vendor_code: string_cell(row, "vendor_code"),
                region_code: string_cell(row, "region_code"),
                vendor_name_snapshot: string_cell(row, "vendor_name_snapshot"),
                modality: integer_cell(row, "modality"),
                color_token: string_cell(row, "color_token"),
                license_type: integer_cell(row, "license_type"),
                context_tokens: integer_cell(row, "context_tokens"),
                request_count: integer_cell(row, "request_count"),
                token_count: integer_cell(row, "token_count"),
                cost_amount: decimal_cell(row, "cost_amount")?,
                currency: string_cell(row, "currency"),
                previous_rank_no: optional_integer_cell(row, "previous_rank_no"),
            })
        })
        .collect()
}

async fn deactivate_existing_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    command: &ModelRankingRefreshCommand,
    snapshot_period_code: i64,
) -> Result<(), DomainError> {
    sqlx::query(
        r#"
        UPDATE ai_model_rank_snapshot
        SET status = 0,
            updated_at = $1::timestamp AT TIME ZONE 'UTC'
        WHERE tenant_id = $2
          AND organization_id = $3
          AND snapshot_date = $4::date
          AND snapshot_period = $5
          AND COALESCE(rank_scope, 'commercial-default') = $6
        "#,
    )
    .bind(&command.requested_at)
    .bind(command.tenant_id)
    .bind(command.organization_id)
    .bind(&command.snapshot_date)
    .bind(snapshot_period_code)
    .bind(&command.rank_scope)
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        store_error(
            "failed to deactivate previous model ranking snapshot rows",
            error,
        )
    })?;
    Ok(())
}

async fn upsert_ranking_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    command: &ModelRankingRefreshCommand,
    snapshot_period_code: i64,
    metadata: &str,
    row: &RankingAggregate,
    rank_no: i64,
) -> Result<(), DomainError> {
    let previous_rank_no = row.previous_rank_no.unwrap_or(rank_no);
    let trend_score = previous_rank_no - rank_no;
    let request_count = row.request_count.max(0);
    let cost_indicator = cost_indicator(row.cost_amount, request_count)?;
    let context_size_text = context_size_text(row.context_tokens);
    let pricing_text = pricing_text(&row.currency, row.cost_amount);
    let rank_payload = rank_payload(row, rank_no, previous_rank_no)?;
    sqlx::query(
        r#"
        INSERT INTO ai_model_rank_snapshot
            (id, uuid, tenant_id, organization_id, source_type, source_version, status, created_at, updated_at,
             rebuild_version, metadata, snapshot_date, snapshot_period, rank_scope, model_id, catalog_key,
             model, vendor_code, region_code, vendor_name_snapshot, modality, rank_no, previous_rank_no,
             base_volume, cost_indicator, context_size_text, is_new, color_token, pricing_text, license_type,
             strengths, request_count, token_count, cost_amount, currency, latency_p50_ms, latency_p95_ms,
             success_rate, win_rate, trend_score, rank_payload)
        VALUES
            ($1, $2, $3, $4, 'analytics-worker', 1, 1, $5::timestamp AT TIME ZONE 'UTC', $5::timestamp AT TIME ZONE 'UTC',
             0, $6::jsonb, $7::date, $8, $9, $10, $11,
             $12, $13, $14, $15, $16, $17, $18,
             $19, $20, $21, $22, $23, $24, $25,
             '[]'::jsonb, $26, $27, $28::text::numeric, $29, 0, 0,
             1.000000, $30::numeric, $31::numeric, $32::jsonb)
        ON CONFLICT (tenant_id, organization_id, snapshot_date, snapshot_period, rank_scope, vendor_code, region_code, catalog_key) DO UPDATE SET
            source_type = excluded.source_type,
            source_version = excluded.source_version,
            status = excluded.status,
            updated_at = excluded.updated_at,
            metadata = excluded.metadata,
            model_id = excluded.model_id,
            model = excluded.model,
            vendor_code = excluded.vendor_code,
            region_code = excluded.region_code,
            vendor_name_snapshot = excluded.vendor_name_snapshot,
            modality = excluded.modality,
            rank_no = excluded.rank_no,
            previous_rank_no = excluded.previous_rank_no,
            base_volume = excluded.base_volume,
            cost_indicator = excluded.cost_indicator,
            context_size_text = excluded.context_size_text,
            is_new = excluded.is_new,
            color_token = excluded.color_token,
            pricing_text = excluded.pricing_text,
            license_type = excluded.license_type,
            strengths = excluded.strengths,
            request_count = excluded.request_count,
            token_count = excluded.token_count,
            cost_amount = excluded.cost_amount,
            currency = excluded.currency,
            latency_p50_ms = excluded.latency_p50_ms,
            latency_p95_ms = excluded.latency_p95_ms,
            success_rate = excluded.success_rate,
            win_rate = excluded.win_rate,
            trend_score = excluded.trend_score,
            rank_payload = excluded.rank_payload
        "#,
    )
    .bind(next_cloud_runtime_id("ai_model_rank_snapshot")?)
    .bind(stable_uuid(
        "rank",
        &[
            &command.tenant_id.to_string(),
            &command.organization_id.to_string(),
            &command.snapshot_date,
            &snapshot_period_code.to_string(),
            &command.rank_scope,
            &row.vendor_code,
            &row.region_code,
            &row.catalog_key,
        ],
    ))
    .bind(command.tenant_id)
    .bind(command.organization_id)
    .bind(&command.requested_at)
    .bind(metadata)
    .bind(&command.snapshot_date)
    .bind(snapshot_period_code)
    .bind(&command.rank_scope)
    .bind(row.model_id)
    .bind(&row.catalog_key)
    .bind(&row.model)
    .bind(&row.vendor_code)
    .bind(&row.region_code)
    .bind(&row.vendor_name_snapshot)
    .bind(row.modality)
    .bind(rank_no)
    .bind(previous_rank_no)
    .bind(request_count)
    .bind(cost_indicator)
    .bind(context_size_text)
    .bind(row.previous_rank_no.is_none())
    .bind(&row.color_token)
    .bind(pricing_text)
    .bind(row.license_type)
    .bind(request_count)
    .bind(row.token_count.max(0))
    .bind(exact_decimal_text(row.cost_amount))
    .bind(&row.currency)
    .bind(win_rate(rank_no, previous_rank_no))
    .bind(decimal_text(trend_score as f64))
    .bind(rank_payload)
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error("failed to upsert model ranking snapshot row", error))?;
    Ok(())
}

fn ranking_metadata(
    command: &ModelRankingRefreshCommand,
    next_refresh_at: &str,
) -> Result<String, DomainError> {
    serde_json::to_string(&serde_json::json!({
        "snapshotDate": command.snapshot_date,
        "snapshotPeriod": command.snapshot_period,
        "windowStart": normalize_iso_timestamp(&command.window_start),
        "windowEnd": normalize_iso_timestamp(&command.window_end),
        "generatedAt": normalize_iso_timestamp(&command.requested_at),
        "refreshIntervalSeconds": command.refresh_interval_seconds,
        "nextRefreshAt": next_refresh_at,
        "cacheMaxAgeSeconds": command.cache_max_age_seconds,
        "sourceTables": ["cloudrouter_charge_line", "cloudrouter_rating_decision", "cloudrouter_usage_measurement", "ai_metering_usage", "ai_model", "ai_model_rank_snapshot"]
    }))
    .map_err(|error| DomainError::new(error.to_string()))
}

fn audit_payload(command: &ModelRankingRefreshAuditCommand) -> Result<String, DomainError> {
    serde_json::to_string(&serde_json::json!({
        "rankScope": command.rank_scope,
        "snapshotDate": command.snapshot_date,
        "snapshotPeriod": command.snapshot_period,
        "windowStart": command.window_start,
        "windowEnd": command.window_end,
        "generatedCount": command.generated_count.max(0),
        "sourceCount": command.source_count.max(0),
        "refreshIntervalSeconds": command.refresh_interval_seconds.max(1),
        "cacheMaxAgeSeconds": command.cache_max_age_seconds.max(1),
        "nextRefreshAt": command.next_refresh_at,
        "status": command.status,
        "attemptCount": command.attempt_count.max(0),
        "retryCount": command.retry_count.max(0),
        "consecutiveFailureCount": command.consecutive_failure_count.max(0),
        "alertRecommended": command.alert_recommended,
        "alertSeverity": command.alert_severity,
        "sourceTables": ["cloudrouter_charge_line", "cloudrouter_rating_decision", "cloudrouter_usage_measurement", "ai_metering_usage", "ai_model", "ai_model_rank_snapshot"]
    }))
    .map_err(|error| DomainError::new(error.to_string()))
}

fn normalize_trigger_type(value: i64) -> i64 {
    match value {
        2 => 2,
        _ => 1,
    }
}

fn execution_status_code(status: &str) -> i64 {
    match status {
        "succeeded" => 2,
        "failed" => 3,
        "empty" => 4,
        "skipped" => 5,
        _ => 1,
    }
}

fn truncate_failure_reason(value: &str) -> String {
    value.chars().take(1024).collect()
}

fn rank_payload(
    row: &RankingAggregate,
    rank_no: i64,
    previous_rank_no: i64,
) -> Result<String, DomainError> {
    serde_json::to_string(&serde_json::json!({
        "catalogKey": row.catalog_key,
        "rank": rank_no,
        "previousRank": previous_rank_no,
        "sourceRows": row.source_rows,
        "requests": row.request_count,
        "tokens": row.token_count,
        "costAmount": exact_decimal_text(row.cost_amount),
        "currency": row.currency
    }))
    .map_err(|error| DomainError::new(error.to_string()))
}

fn sql_timestamp(value: &str) -> String {
    normalize_iso_timestamp(value)
        .replace('T', " ")
        .trim_end_matches('Z')
        .to_owned()
}

fn cost_indicator(cost_amount: DecimalValue, requests: i64) -> Result<i64, DomainError> {
    if requests <= 0 {
        return Ok(3);
    }
    let non_negative_cost = cost_amount.max(DecimalValue::ZERO);
    let cost_per_request = non_negative_cost.divide_i64(requests)?;
    let indicator = if cost_per_request <= DecimalValue::parse("0.01")? {
        1
    } else if cost_per_request <= DecimalValue::parse("0.05")? {
        2
    } else if cost_per_request <= DecimalValue::parse("0.25")? {
        3
    } else if cost_per_request <= DecimalValue::ONE {
        4
    } else {
        5
    };
    Ok(indicator)
}

fn context_size_text(context_tokens: i64) -> Option<String> {
    if context_tokens <= 0 {
        None
    } else if context_tokens >= 1_000 {
        Some(format!("{}K", context_tokens / 1_000))
    } else {
        Some(context_tokens.to_string())
    }
}

fn pricing_text(currency: &str, cost_amount: DecimalValue) -> Option<String> {
    if currency.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{} {}/window",
        currency.trim(),
        exact_decimal_text(cost_amount)
    ))
}

fn win_rate(rank_no: i64, previous_rank_no: i64) -> String {
    if previous_rank_no <= 0 {
        return "0.500000".to_owned();
    }
    let movement = (previous_rank_no - rank_no) as f64 / previous_rank_no.max(1) as f64;
    decimal_text((0.5 + movement / 2.0).clamp(0.0, 1.0))
}

fn decimal_text(value: f64) -> String {
    let mut text = format!("{:.6}", value);
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text.is_empty() {
        "0".to_owned()
    } else {
        text
    }
}

fn exact_decimal_text(value: DecimalValue) -> String {
    value.to_fixed_string(12)
}

fn stable_uuid(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    let digest = hex::encode(hasher.finalize());
    format!("{prefix}-{}", &digest[..32])
}

fn optional_string_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(column).ok().flatten()
}

fn string_cell(row: &sqlx::postgres::PgRow, column: &str) -> String {
    optional_string_cell(row, column).unwrap_or_default()
}

fn optional_integer_cell(row: &sqlx::postgres::PgRow, column: &str) -> Option<i64> {
    row.try_get::<Option<i64>, _>(column)
        .ok()
        .flatten()
        .or_else(|| {
            // Snapshot tables declare INTEGER (int4) columns; decode them before
            // falling back to the text cast.
            row.try_get::<Option<i32>, _>(column)
                .ok()
                .flatten()
                .map(i64::from)
        })
        .or_else(|| {
            string_cell(row, column)
                .parse::<f64>()
                .ok()
                .map(|value| value as i64)
        })
}

fn integer_cell(row: &sqlx::postgres::PgRow, column: &str) -> i64 {
    optional_integer_cell(row, column).unwrap_or(0)
}

fn decimal_cell(row: &sqlx::postgres::PgRow, column: &str) -> Result<DecimalValue, DomainError> {
    let value = string_cell(row, column);
    DecimalValue::parse(&value).map_err(|error| {
        DomainError::new(format!(
            "invalid exact decimal in model ranking column {column}: {value}: {error}"
        ))
    })
}

fn store_error(context: &str, error: sqlx::Error) -> DomainError {
    DomainError::new(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const POSTGRES_TEST_DATABASE_URL: &str = "SDKWORK_DATABASE_URL";

    #[test]
    fn ranking_sql_keeps_usage_cost_numeric_until_the_text_mapping_boundary() {
        let production_source = include_str!("model_ranking_refresh_store.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source before test module");

        assert!(production_source.contains("FROM cloudrouter_charge_line c"));
        assert!(production_source.contains("d.decision_status = 'rated'"));
        assert!(production_source.contains("d.billability = 'chargeable'"));
        assert!(
            production_source.contains("SUM(COALESCE(u.customer_charge_amount, 0)) AS cost_amount")
        );
        assert!(production_source.contains("CAST(a.cost_amount AS TEXT) AS cost_amount"));
        assert!(production_source.contains("a.cost_amount DESC"));
        assert!(production_source.contains("$28::text::numeric"));
        assert!(!production_source.contains("NULLIF(u.customer_charge_amount, 0)"));
        assert!(!production_source.contains("u.cost_amount"));
        assert!(!production_source.contains("cost_amount: f64"));
        assert!(!production_source.contains(".bind(decimal_text(row.cost_amount))"));
        assert!(!production_source.contains("\"costAmount\": decimal_text(row.cost_amount)"));
    }

    #[test]
    fn exact_ranking_cost_remains_a_twelve_scale_decimal_in_snapshot_outputs() {
        let cost_amount = DecimalValue::parse("9007199254740992.000000000010")
            .expect("valid exact ranking amount");
        let row = ranking_aggregate(cost_amount, 10);

        assert_eq!(
            exact_decimal_text(cost_amount),
            "9007199254740992.000000000010"
        );
        assert_eq!(
            pricing_text("USD", cost_amount).as_deref(),
            Some("USD 9007199254740992.000000000010/window")
        );

        let payload: serde_json::Value =
            serde_json::from_str(&rank_payload(&row, 1, 2).expect("valid exact ranking payload"))
                .expect("ranking payload json");
        assert_eq!(
            payload
                .get("costAmount")
                .and_then(serde_json::Value::as_str),
            Some("9007199254740992.000000000010")
        );
    }

    #[test]
    fn exact_cost_indicator_compares_decimal_boundaries_without_binary_floats() {
        assert_eq!(
            cost_indicator(DecimalValue::parse("0.010000000000").unwrap(), 1).unwrap(),
            1
        );
        assert_eq!(
            cost_indicator(DecimalValue::parse("0.010000000001").unwrap(), 1).unwrap(),
            2
        );
        assert_eq!(
            cost_indicator(DecimalValue::parse("0.050000000001").unwrap(), 1).unwrap(),
            3
        );
        assert_eq!(
            cost_indicator(DecimalValue::parse("0.250000000001").unwrap(), 1).unwrap(),
            4
        );
        assert_eq!(
            cost_indicator(DecimalValue::parse("1.000000000001").unwrap(), 1).unwrap(),
            5
        );
        assert_eq!(
            cost_indicator(DecimalValue::parse("-1.000000000000").unwrap(), 1).unwrap(),
            1
        );
        assert_eq!(
            cost_indicator(DecimalValue::parse("1.000000000000").unwrap(), 0).unwrap(),
            3
        );
    }

    #[tokio::test]
    #[ignore = "requires SDKWORK_DATABASE_URL for an isolated workspace test database"]
    async fn postgres_numeric_aggregation_and_snapshot_cast_preserve_exact_ranking_cost() {
        let database_url = std::env::var(POSTGRES_TEST_DATABASE_URL).unwrap_or_else(|_| {
            panic!(
                "{POSTGRES_TEST_DATABASE_URL} is required for the PostgreSQL exact ranking decimal test"
            )
        });
        let pool = PgPool::connect(&database_url)
            .await
            .unwrap_or_else(|error| {
                panic!("connect PostgreSQL exact ranking test database: {error}")
            });

        let aggregate_row = sqlx::query(
            r#"
            SELECT CAST(SUM(amount) AS TEXT) AS cost_amount
            FROM (
                VALUES
                    ('9007199254740992.000000000001'::numeric(38, 12)),
                    ('0.000000000009'::numeric(38, 12))
            ) AS exact_usage(amount)
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("aggregate exact PostgreSQL ranking cost");
        let cost_amount = decimal_cell(&aggregate_row, "cost_amount")
            .expect("parse exact PostgreSQL ranking aggregate");
        assert_eq!(
            exact_decimal_text(cost_amount),
            "9007199254740992.000000000010"
        );

        let snapshot_row =
            sqlx::query("SELECT CAST($1::text::numeric(38, 12) AS TEXT) AS cost_amount")
                .bind(exact_decimal_text(cost_amount))
                .fetch_one(&pool)
                .await
                .expect("round-trip exact PostgreSQL ranking snapshot cost");
        let persisted_cost = decimal_cell(&snapshot_row, "cost_amount")
            .expect("parse exact PostgreSQL ranking snapshot cost");
        assert_eq!(persisted_cost, cost_amount);
        assert_eq!(
            exact_decimal_text(persisted_cost),
            "9007199254740992.000000000010"
        );
    }

    fn ranking_aggregate(cost_amount: DecimalValue, request_count: i64) -> RankingAggregate {
        RankingAggregate {
            source_rows: 2,
            model_id: 1,
            catalog_key: "test/model".to_owned(),
            model: "test-model".to_owned(),
            vendor_code: "test-vendor".to_owned(),
            region_code: "global".to_owned(),
            vendor_name_snapshot: "Test Vendor".to_owned(),
            modality: 1,
            color_token: "#64748b".to_owned(),
            license_type: 2,
            context_tokens: 128_000,
            request_count,
            token_count: 42,
            cost_amount,
            currency: "USD".to_owned(),
            previous_rank_no: Some(2),
        }
    }
}
