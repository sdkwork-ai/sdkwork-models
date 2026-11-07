//! Application API assembly bootstrap for sdkwork-models.

use axum::Router;
use sdkwork_database_sqlx::DatabasePool;
use sdkwork_models_catalog_repository_sqlx::{
    PostgresAdminAiResourceStore, PostgresModelCatalogAdminStore, PostgresModelRankingRefreshStore,
    PostgresModelRankingsReadStore,
};
use sdkwork_models_contract_service::{
    AdminAiResourceStore, EntityUuidGenerator, ModelCatalogAdminStore, ModelRankingRefreshStore,
    ModelRankingsReadModelStore,
};
use sdkwork_models_service_host::ModelsServiceHost;
pub use sdkwork_web_bootstrap::ApiAssemblyContribution;
use sdkwork_web_bootstrap::WebModule;
use sdkwork_web_core::HttpRouteManifest;
use std::sync::Arc;

use crate::entity_uuid_generator::CatalogEntityUuidGenerator;

/// Indivisible host-neutral API assembly contribution (web-bootstrap contract,
/// API_ASSEMBLY_SPEC.md section 4).
pub type ApiAssembly = ApiAssemblyContribution;

pub async fn assemble_business_routes() -> Result<ApiAssembly, String> {
    let host = Arc::new(ModelsServiceHost::new().await?);
    let entity_uuid_generator: Arc<dyn EntityUuidGenerator + Send + Sync> =
        CatalogEntityUuidGenerator::arc();
    let app_business = match host.database_pool() {
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
            )
        }
        _ => unreachable!(
            "models server assembly requires a PostgreSQL pool (DATABASE_SPEC: authoritative-server persistence is PostgreSQL only)"
        ),
    };
    let backend_business = match host.database_pool() {
        DatabasePool::Postgres(pool, _) => {
            sdkwork_routes_models_catalog_backend_api::gateway_mount(
                Arc::new(PostgresModelCatalogAdminStore::new(pool.clone()))
                    as Arc<dyn ModelCatalogAdminStore + Send + Sync>,
                Arc::new(PostgresModelRankingsReadStore::new(pool.clone()))
                    as Arc<dyn ModelRankingsReadModelStore + Send + Sync>,
                Arc::new(PostgresModelRankingRefreshStore::new(pool.clone()))
                    as Arc<dyn ModelRankingRefreshStore + Send + Sync>,
                Arc::new(PostgresAdminAiResourceStore::new(pool.clone()))
                    as Arc<dyn AdminAiResourceStore + Send + Sync>,
                Arc::clone(&entity_uuid_generator),
                host.voice_catalog(),
            )
        }
        _ => unreachable!(
            "models server assembly requires a PostgreSQL pool (DATABASE_SPEC: authoritative-server persistence is PostgreSQL only)"
        ),
    };
    let app = sdkwork_routes_models_catalog_app_api::wrap_router_with_web_framework_from_env(
        app_business,
    )
    .await;
    let backend =
        sdkwork_routes_models_catalog_backend_api::wrap_router_with_web_framework_from_env(
            backend_business,
        )
        .await;
    let router = Router::new().merge(app).merge(backend);

    let routes = [
        sdkwork_routes_models_catalog_app_api::gateway_route_manifest(),
        sdkwork_routes_models_catalog_backend_api::gateway_route_manifest(),
    ]
    .into_iter()
    .flat_map(|manifest| manifest.routes().to_vec())
    .collect();

    ApiAssemblyContribution::from_manifest(
        "sdkwork-models",
        "SDKWork Models API",
        router,
        HttpRouteManifest::from_owned_routes(routes),
        Vec::new(),
        Arc::new(crate::contribution::ModelsDatabaseReadinessCheck::new(host)),
    )
}

pub async fn assemble_api_router() -> Result<ApiAssembly, String> {
    assemble_business_routes().await
}

/// Canonical Web Module definition for this application
/// (API_ASSEMBLY_SPEC §4.1.1): the complete HTTP surface — every route,
/// manifest, and OpenAPI document of this owner — as one installable module.
pub async fn web_module() -> Result<WebModule, String> {
    Ok(WebModule::from_contribution(assemble_api_router().await?))
}
