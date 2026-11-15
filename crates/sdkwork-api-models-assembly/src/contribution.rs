//! App API contribution for gateways that own a single Web Framework layer.

use std::sync::Arc;

use sdkwork_database_sqlx::DatabasePool;
use sdkwork_models_catalog_repository_sqlx::{
    PostgresAdminAiResourceStore, PostgresFrontendConfigStore, PostgresModelCatalogAdminStore,
    PostgresModelRankingsReadStore,
};
use sdkwork_models_contract_service::{
    AdminAiResourceStore, EntityUuidGenerator, FrontendConfigStore, ModelCatalogAdminStore,
    ModelRankingsReadModelStore,
};
use sdkwork_models_service_host::ModelsServiceHost;
pub use sdkwork_web_bootstrap::ApiAssemblyContribution;
use sdkwork_web_bootstrap::{ReadinessCheck, ReadinessFuture};

use crate::entity_uuid_generator::CatalogEntityUuidGenerator;

/// Builds the unwrapped Models App API for a gateway that owns the single Web
/// Framework layer. The backing database pool is role-resolved from the
/// `SDKWORK_DATABASE_*` environment contract: client-local SQLite through
/// `SDKWORK_DATABASE_SQLITE_URL` for standalone desktop hosts, otherwise the
/// workspace PostgreSQL profile (ENVIRONMENT_SPEC §7.2).
pub async fn assemble_app_api_contribution() -> Result<ApiAssemblyContribution, String> {
    let host = Arc::new(
        ModelsServiceHost::new()
            .await
            .map_err(|error| format!("bootstrap models service host failed: {error}"))?,
    );
    let entity_uuid_generator: Arc<dyn EntityUuidGenerator + Send + Sync> =
        CatalogEntityUuidGenerator::arc();
    let router = match host.database_pool() {
        DatabasePool::Postgres(pool, _) => {
            let read_store: Arc<dyn ModelRankingsReadModelStore + Send + Sync> =
                Arc::new(PostgresModelRankingsReadStore::new(pool.clone()));
            sdkwork_routes_models_catalog_app_api::gateway_mount(
                host.pricing_catalog(),
                host.voice_catalog(),
                read_store,
                Arc::new(PostgresModelCatalogAdminStore::new(pool.clone()))
                    as Arc<dyn ModelCatalogAdminStore + Send + Sync>,
                Arc::new(PostgresAdminAiResourceStore::new(pool.clone()))
                    as Arc<dyn AdminAiResourceStore + Send + Sync>,
                Arc::clone(&entity_uuid_generator),
                Arc::new(PostgresFrontendConfigStore::new(pool.clone()))
                    as Arc<dyn FrontendConfigStore + Send + Sync>,
            )
        }
        _ => unreachable!(
            "models server assembly requires a PostgreSQL pool (DATABASE_SPEC: authoritative-server persistence is PostgreSQL only)"
        ),
    };
    let route_manifest = sdkwork_routes_models_catalog_app_api::app_route_manifest();
    ApiAssemblyContribution::from_manifest(
        "sdkwork-models",
        "SDKWork Models App API",
        router,
        route_manifest,
        Vec::new(),
        Arc::new(ModelsDatabaseReadinessCheck::new(host)),
    )
}

/// Readiness probe for the Models App API database pool.
#[derive(Clone)]
pub(crate) struct ModelsDatabaseReadinessCheck {
    host: Arc<ModelsServiceHost>,
}

impl ModelsDatabaseReadinessCheck {
    pub(crate) fn new(host: Arc<ModelsServiceHost>) -> Self {
        Self { host }
    }
}

impl ReadinessCheck for ModelsDatabaseReadinessCheck {
    fn check(&self) -> ReadinessFuture<'_> {
        let host = Arc::clone(&self.host);
        Box::pin(async move {
            let result = match host.database_pool() {
                DatabasePool::Postgres(pool, _) => {
                    sqlx::query("SELECT 1").execute(pool).await.map(|_| ())
                }
                _ => unreachable!(
                    "models server assembly requires a PostgreSQL pool (DATABASE_SPEC: authoritative-server persistence is PostgreSQL only)"
                ),
            };
            result.map_err(|error| format!("models database readiness probe failed: {error}"))
        })
    }
}
