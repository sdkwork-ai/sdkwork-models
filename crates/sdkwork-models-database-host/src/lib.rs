use std::path::PathBuf;
use std::sync::Arc;

use sdkwork_database_config::DatabaseConfig;
use sdkwork_database_lifecycle::{lifecycle_options_from_env, LifecycleOrchestrator};
use sdkwork_database_spi::{
    DatabaseAssetProvider, DatabaseManifest, DefaultDatabaseModule, LocaleTag,
};
use sdkwork_database_sqlx::{create_pool_from_config, DatabasePool};

pub struct ModelsDatabaseHost {
    pool: DatabasePool,
    module: Arc<DefaultDatabaseModule>,
}

impl ModelsDatabaseHost {
    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    pub fn module(&self) -> Arc<DefaultDatabaseModule> {
        self.module.clone()
    }

    fn orchestrator(&self, applied_by: &str) -> LifecycleOrchestrator {
        LifecycleOrchestrator::new(self.pool.clone(), self.module.clone())
            .with_applied_by(applied_by)
    }

    pub async fn migrate(&self, applied_by: &str) -> Result<usize, String> {
        let orchestrator = self.orchestrator(applied_by);
        orchestrator
            .init()
            .await
            .map_err(|error| format!("models database init failed: {error}"))?;
        orchestrator
            .migrate()
            .await
            .map_err(|error| format!("models database migrate failed: {error}"))
    }

    pub async fn plan_migrations(&self) -> Result<usize, String> {
        self.orchestrator("sdkwork-models-plan")
            .plan_migrations()
            .await
            .map(|migrations| migrations.len())
            .map_err(|error| format!("models database migration plan failed: {error}"))
    }

    /// Applies the module's configured seed set for the selected locale and
    /// profile through the shared lifecycle seed pipeline.
    ///
    /// The seed set carries the catalog reference data that must exist for the
    /// console to open populated (for example the front-end channel
    /// initialization rows behind `/admin/model/frontend`). The runner records
    /// each file's content checksum, so repeated calls apply only new or
    /// changed seeds and never rewrite operator edits.
    ///
    /// `seed_locale` is the installer's resolved locale override (environment or
    /// runtime TOML). When omitted the module's own manifest/env default
    /// applies.
    pub async fn seed(&self, applied_by: &str, seed_locale: Option<&str>) -> Result<usize, String> {
        let manifest = DatabaseManifest::from_file(self.module.manifest_path())
            .map_err(|error| format!("read models database manifest failed: {error}"))?;
        let options = lifecycle_options_from_env("MODELS", &manifest);
        let locale = LocaleTag(
            seed_locale
                .map(str::to_owned)
                .unwrap_or_else(|| options.seed_locale.0.clone()),
        );
        self.orchestrator(applied_by)
            .seed(&locale, &options.seed_profile)
            .await
            .map_err(|error| format!("models database seed failed: {error}"))
    }
}

/// Loads the canonical models database module around an existing pool without
/// creating framework tables, applying schema, or seeding catalog data.
///
/// Both database roles are legitimate (ENVIRONMENT_SPEC §7.2): the server role
/// (workspace PostgreSQL profile) is resolved by `DatabaseConfig::from_env`,
/// while desktop hosts connect the client-local SQLite pool through
/// `DatabaseConfig::load_client_local_from_env`. The role-aware config loader
/// guarantees server modules never receive SQLite.
pub fn connect_models_database(pool: DatabasePool) -> Result<ModelsDatabaseHost, String> {
    let app_root = resolve_app_root();
    let module = Arc::new(
        DefaultDatabaseModule::from_app_root(&app_root)
            .map_err(|error| format!("load models database module failed: {error}"))?,
    );
    DatabaseManifest::from_file(module.manifest_path())
        .map_err(|error| format!("read models database manifest failed: {error}"))?;
    Ok(ModelsDatabaseHost { pool, module })
}

/// Runtime-safe models bootstrap. The manifest default is connection-only;
/// non-production auto-migration requires an explicit environment override.
/// The explicit lifecycle command (`--migrate-databases`, marked by
/// `SDKWORK_DATABASE_LIFECYCLE_COMMAND`) is the sanctioned production/staging
/// migration path and is admitted; plain runtime processes stay fail-closed.
pub async fn bootstrap_models_database(pool: DatabasePool) -> Result<ModelsDatabaseHost, String> {
    let host = connect_models_database(pool)?;
    let manifest = DatabaseManifest::from_file(host.module.manifest_path())
        .map_err(|error| format!("read models database manifest failed: {error}"))?;
    let options = lifecycle_options_from_env("MODELS", &manifest);
    if options.auto_migrate {
        let environment = std::env::var("SDKWORK_MODELS_ENVIRONMENT").unwrap_or_default();
        if production_like_environment(&environment) && !explicit_lifecycle_command() {
            return Err(
                "production/staging runtime must not auto-migrate the models database; run the explicit lifecycle migrate command before startup"
                    .to_owned(),
            );
        }
        host.migrate("sdkwork-models-runtime").await?;
    }
    Ok(host)
}

pub async fn migrate_models_database(
    pool: DatabasePool,
    applied_by: &str,
) -> Result<(ModelsDatabaseHost, usize), String> {
    let host = connect_models_database(pool)?;
    let applied = host.migrate(applied_by).await?;
    Ok((host, applied))
}

/// Explicitly migrates the client-local SQLite models database for runtimes
/// without a lifecycle operator (desktop standalone). The module manifest
/// disables auto-migrate, so the host runtime performs the documented
/// lifecycle migrate itself before serving requests.
pub async fn migrate_client_local_models_database(applied_by: &str) -> Result<usize, String> {
    let _ = dotenvy::dotenv();
    if !sdkwork_database_config::client_local_sqlite_url_configured() {
        return Err(
            "client-local models database migration requires SDKWORK_DATABASE_SQLITE_URL"
                .to_owned(),
        );
    }
    let config = DatabaseConfig::load_client_local_from_env("MODELS")
        .map_err(|error| format!("read models database config failed: {error}"))?;
    let pool = create_pool_from_config(config)
        .await
        .map_err(|error| format!("create models database pool failed: {error}"))?;
    migrate_models_database(pool, applied_by)
        .await
        .map(|(_, applied)| applied)
}

pub async fn bootstrap_models_database_from_env() -> Result<ModelsDatabaseHost, String> {
    let _ = dotenvy::dotenv();
    // Server bootstrap resolves the workspace PostgreSQL profile exclusively;
    // SQLite is not a valid server persistence engine (DATABASE_SPEC §3.1:
    // authoritative-server persistence is PostgreSQL only).
    let config = DatabaseConfig::from_env("MODELS")
        .map_err(|error| format!("read models database config failed: {error}"))?;
    let pool = create_pool_from_config(config)
        .await
        .map_err(|error| format!("create models database pool failed: {error}"))?;
    bootstrap_models_database(pool).await
}

fn resolve_app_root() -> PathBuf {
    std::env::var("SDKWORK_MODELS_APP_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        })
}

fn production_like_environment(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "production" | "prod" | "staging"
    )
}

/// True when this process is the explicit release/operator lifecycle command
/// (for example `sdkwork-api-cloud-gateway --migrate-databases`, which sets
/// `SDKWORK_DATABASE_LIFECYCLE_COMMAND=migrate`). Plain runtime processes
/// never set the marker, so the production/staging auto-migration guard stays
/// fail-closed for them.
fn explicit_lifecycle_command() -> bool {
    matches!(
        std::env::var("SDKWORK_DATABASE_LIFECYCLE_COMMAND")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "migrate" | "migrate-databases" | "explicit" | "operator"
    )
}

#[cfg(test)]
mod tests {
    use sdkwork_database_spi::{SeedManifest, SeedProfile};

    use super::*;

    #[test]
    fn frontend_defaults_seed_is_registered_in_the_standard_profile() {
        let app_root = resolve_app_root();
        let seeds_dir = app_root.join("database").join("seeds");
        let manifest = SeedManifest::from_file(seeds_dir.join("seed.manifest.json"))
            .expect("read seed manifest");
        let plan = manifest
            .resolve_plan(
                &seeds_dir,
                &LocaleTag("zh-CN".to_owned()),
                &SeedProfile("standard".to_owned()),
            )
            .expect("resolve seed plan");
        assert!(
            plan.common_scripts
                .iter()
                .any(|path| path.ends_with("002_ai_model_frontend_defaults.sql")),
            "standard seed profile must apply the front-end defaults seed: {:?}",
            plan.common_scripts
        );
    }

    #[test]
    fn production_like_environment_rejects_production_aliases() {
        assert!(production_like_environment("production"));
        assert!(production_like_environment(" PROD "));
        assert!(production_like_environment("staging"));
        assert!(!production_like_environment("development"));
    }

    #[test]
    fn explicit_lifecycle_command_recognizes_operator_marker_only() {
        // The test process does not set the lifecycle-command marker.
        assert!(!explicit_lifecycle_command());
        unsafe { std::env::set_var("SDKWORK_DATABASE_LIFECYCLE_COMMAND", "migrate") };
        assert!(explicit_lifecycle_command());
        unsafe { std::env::set_var("SDKWORK_DATABASE_LIFECYCLE_COMMAND", " Migrate-Databases ") };
        assert!(explicit_lifecycle_command());
        unsafe { std::env::set_var("SDKWORK_DATABASE_LIFECYCLE_COMMAND", "serve") };
        assert!(!explicit_lifecycle_command());
        unsafe { std::env::remove_var("SDKWORK_DATABASE_LIFECYCLE_COMMAND") };
        assert!(!explicit_lifecycle_command());
    }
}
