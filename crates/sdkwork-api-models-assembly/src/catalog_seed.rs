//! One-time official catalog seeding for a greenfield Models database.
//!
//! The Models database module ships an intentionally empty bootstrap seed
//! (`SELECT 1`) with `seedOnBoot = false`. The authoritative vendor, model,
//! price, voice, and video-profile facts live in the `sdkwork-models.json` +
//! `models/` JSON bundle, which is installed alongside the server binary and
//! addressed through `SDKWORK_MODELS_CATALOG_ROOT`. Without this import a fresh
//! installation has an empty `ai_model_vendor` table, so the model-vendors
//! admin page stays blank until an operator manually runs "sync model catalog".
//!
//! The operator-driven `POST /backend/v3/api/ai/models/sync` endpoint remains
//! the explicit refresh path; this bootstrap only covers first install.

use std::path::PathBuf;

use sdkwork_database_sqlx::DatabasePool;
use sdkwork_models::ModelCatalog;
use sdkwork_models_catalog_repository_sqlx::{
    postgres::import_postgres_model_catalog_tx, ENV_MODELS_CATALOG_ROOT,
};
use sqlx::PgPool;

/// Transaction-scoped advisory lock serializing first-install imports across
/// gateway replicas that boot against the same PostgreSQL database at once.
/// Derived from the ASCII bytes of "SDKWMODELS" and kept stable so every
/// version of the gateway contends on the same key.
const CATALOG_SEED_ADVISORY_LOCK_KEY: i64 = 0x5344_4B57_4D4F_4445;

const VENDOR_COUNT_SQL: &str = "SELECT COUNT(*) FROM ai_model_vendor WHERE deleted_at IS NULL";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeedOutcome {
    Seeded,
    AlreadySeeded,
}

/// Imports the official catalog only when the vendor table is empty.
///
/// Every failure mode is non-fatal and logged: the schema may be managed by an
/// external lifecycle, the catalog bundle may be absent in a source checkout,
/// and the admin catalog sync endpoint remains a valid recovery path. Startup
/// must not fail closed on optional seed content.
pub async fn seed_model_catalog_if_empty(pool: &DatabasePool) {
    let Some(pg_pool) = pool.as_postgres().cloned() else {
        // Client-local SQLite hosts have no authoritative catalog tables.
        return;
    };
    let vendor_count = match sqlx::query_scalar::<_, i64>(VENDOR_COUNT_SQL)
        .fetch_one(&pg_pool)
        .await
    {
        Ok(count) => count,
        Err(error) => {
            // A missing/unmigrated schema is an installation defect, not a
            // normal startup condition: keep it visible in deployment logs so
            // an empty model-vendor page can be traced back to the database
            // lifecycle instead of looking like a silent no-op.
            tracing::warn!(
                target: "sdkwork.models.catalog_seed",
                %error,
                "model catalog seed skipped: ai_model_vendor is not queryable; run the database module lifecycle first"
            );
            return;
        }
    };
    if vendor_count > 0 {
        return;
    }
    let root = resolve_catalog_root();
    let catalog = match sdkwork_models::load_catalog(&root) {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::warn!(
                target: "sdkwork.models.catalog_seed",
                root = %root.display(),
                %error,
                "model catalog seed skipped: official catalog bundle is unavailable"
            );
            return;
        }
    };
    // The importer deactivates catalog rows that are absent from the bundle.
    // An accidentally mounted empty/incomplete root must therefore never be
    // treated as an authoritative catalog, even on a greenfield database.
    if catalog.vendors.is_empty() {
        tracing::warn!(
            target: "sdkwork.models.catalog_seed",
            root = %root.display(),
            "model catalog seed skipped: official bundle declares no vendors"
        );
        return;
    }
    let catalog_version = catalog.manifest.catalog_version.clone();
    let vendors = catalog.vendors.len();
    match import_catalog_once(&pg_pool, &catalog).await {
        Ok(SeedOutcome::Seeded) => tracing::info!(
            target: "sdkwork.models.catalog_seed",
            root = %root.display(),
            catalog_version = %catalog_version,
            vendors,
            "model catalog seeded from the official bundle"
        ),
        Ok(SeedOutcome::AlreadySeeded) => tracing::debug!(
            target: "sdkwork.models.catalog_seed",
            root = %root.display(),
            "model catalog seed skipped: another gateway replica seeded the database first"
        ),
        Err(error) => tracing::warn!(
            target: "sdkwork.models.catalog_seed",
            root = %root.display(),
            %error,
            "model catalog seed failed; use the admin catalog sync to retry"
        ),
    }
}

/// Runs the empty-table check and the full catalog import in one transaction.
///
/// The advisory lock is transaction scoped, so a crashed replica never leaves
/// a stale lock behind. Re-checking the vendor count *after* acquiring the lock
/// makes concurrent first-boot replicas converge on a single import instead of
/// all replaying the same upserts (and soft-deleting each other's rows).
async fn import_catalog_once(
    pg_pool: &PgPool,
    catalog: &ModelCatalog,
) -> Result<SeedOutcome, sqlx::Error> {
    let mut tx = pg_pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(CATALOG_SEED_ADVISORY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    let existing_vendors = sqlx::query_scalar::<_, i64>(VENDOR_COUNT_SQL)
        .fetch_one(&mut *tx)
        .await?;
    if existing_vendors > 0 {
        tx.rollback().await?;
        return Ok(SeedOutcome::AlreadySeeded);
    }
    import_postgres_model_catalog_tx(&mut tx, catalog).await?;
    tx.commit().await?;
    Ok(SeedOutcome::Seeded)
}

fn resolve_catalog_root() -> PathBuf {
    if let Ok(root) = std::env::var(ENV_MODELS_CATALOG_ROOT) {
        let trimmed = root.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    // Source checkout fallback: the catalog manifest lives at the repository
    // root (its `modelsRoot` points at `models/`).
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// The four files `sdkwork_models::load_catalog` reads unconditionally.
    /// A packaged install missing any one of them silently starts with an
    /// empty vendor table, so wiring them into the packaging contract is a
    /// release requirement rather than an implementation detail.
    #[test]
    fn official_catalog_bundle_loads_with_vendors() {
        let root = repository_root();
        for required in [
            "sdkwork-models.json",
            "models/index.json",
            "models/meters.json",
            "models/protocols.json",
        ] {
            assert!(
                root.join(required).is_file(),
                "official catalog bundle is missing {required}; a fresh install would seed zero vendors"
            );
        }
        let catalog = sdkwork_models::load_catalog(&root).unwrap_or_else(|error| {
            panic!(
                "official catalog bundle at {} is not loadable: {error}",
                root.display()
            )
        });
        assert!(
            !catalog.vendors.is_empty(),
            "official catalog bundle declares no vendors; seed would be skipped"
        );
        assert!(!catalog.manifest.catalog_version.is_empty());
        assert!(!catalog.meters.is_empty());
        assert!(!catalog.protocols.is_empty());
    }

    /// Every table written by `import_postgres_model_catalog_tx` must exist in
    /// the baseline DDL: `lifecycle.autoMigrate` is false, so a greenfield
    /// install only ever receives the consolidated baseline before seeding.
    #[test]
    fn baseline_ddl_declares_every_catalog_import_table() {
        const IMPORT_TABLES: &[&str] = &[
            "ai_billing_meter",
            "ai_model_vendor",
            "ai_model_family",
            "ai_model",
            "ai_model_capability",
            "ai_modality",
            "ai_api_endpoint",
            "ai_vendor_modality",
            "ai_vendor_api_endpoint",
            "ai_modality_api_endpoint",
            "ai_model_modality",
            "ai_model_api_endpoint",
            "ai_resource",
            "ai_model_pricing",
            "ai_model_rank_snapshot",
            "ai_model_voice",
            "ai_model_voice_binding",
            "ai_model_video_profile",
        ];
        let ddl_path = repository_root()
            .join("database/ddl/baseline/postgres/0001_sdkwork-models_baseline.sql");
        let ddl = std::fs::read_to_string(&ddl_path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", ddl_path.display()));
        for table in IMPORT_TABLES {
            let declared = ddl.contains(&format!("CREATE TABLE IF NOT EXISTS {table} ("))
                || ddl.contains(&format!("CREATE TABLE {table} ("));
            assert!(
                declared,
                "baseline DDL does not declare {table}; the first-install catalog import would fail"
            );
        }
    }
}
