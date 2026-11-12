//! Route crate for intelligence catalog app API (`sdkwork-routes-catalog-app-api`).

pub mod http_route_manifest;
pub mod paths;
pub mod routes;
pub mod web_bootstrap;

pub use http_route_manifest::app_route_manifest;
pub use routes::{route_definitions, RouteDefinition};
pub use sdkwork_models_catalog_service::{
    app_model_catalog_router, app_model_catalog_router_with_stores, app_model_rankings_router,
    app_model_rankings_router_with_read_store, app_models_router,
};
pub use web_bootstrap::{
    intelligence_catalog_app_api_prefixes, intelligence_catalog_app_api_public_path_prefixes,
    wrap_router_with_web_framework, wrap_router_with_web_framework_from_env,
};

use sdkwork_web_core::HttpRouteManifest;

pub fn gateway_route_manifest() -> HttpRouteManifest {
    app_route_manifest()
}

pub fn gateway_mount<C>(
    pricing_catalog: std::sync::Arc<C>,
    voice_catalog: std::sync::Arc<sdkwork_models::ModelCatalog>,
    read_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::ModelRankingsReadModelStore + Send + Sync,
    >,
    model_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::ModelCatalogAdminStore + Send + Sync,
    >,
    resource_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::AdminAiResourceStore + Send + Sync,
    >,
    entity_uuid_generator: std::sync::Arc<
        dyn sdkwork_models_contract_service::EntityUuidGenerator + Send + Sync,
    >,
) -> axum::Router
where
    C: sdkwork_models_catalog_service::PricingCatalog + Send + Sync + 'static,
{
    sdkwork_models_catalog_service::app_model_catalog_router_with_stores_and_profiles(
        pricing_catalog,
        model_store,
        resource_store,
        entity_uuid_generator,
        Some(std::sync::Arc::clone(&voice_catalog)),
    )
    .merge(sdkwork_models_catalog_service::app_voice_catalog_router(
        std::sync::Arc::clone(&voice_catalog),
    ))
    .merge(sdkwork_models_catalog_service::app_video_profile_catalog_router(voice_catalog))
    .merge(sdkwork_models_catalog_service::app_model_rankings_router_with_read_store(read_store))
}
