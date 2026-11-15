//! Route crate for intelligence catalog backend API (`sdkwork-routes-catalog-backend-api`).

pub mod http_route_manifest;
pub mod paths;
pub mod routes;
pub mod web_bootstrap;

pub use http_route_manifest::backend_route_manifest;
pub use routes::{route_definitions, RouteDefinition};
pub use sdkwork_models_catalog_service::{
    admin_ai_resource_router_with_store, admin_model_catalog_management_router_with_store,
    admin_model_catalog_router, admin_model_catalog_router_with_api_key_hasher,
    admin_model_catalog_router_with_store, admin_model_management_router_with_store,
    admin_model_rankings_router, admin_model_rankings_router_with_read_store,
    admin_model_rankings_router_with_read_store_and_refresh_store,
    app_video_profile_catalog_router, app_voice_catalog_router,
    backend_frontend_config_router_with_store, backend_video_profile_catalog_router,
    backend_voice_catalog_router,
};
pub use web_bootstrap::{
    intelligence_catalog_backend_api_prefixes,
    intelligence_catalog_backend_api_public_path_prefixes, wrap_router_with_web_framework,
    wrap_router_with_web_framework_from_env,
};

use sdkwork_web_core::HttpRouteManifest;

pub fn gateway_route_manifest() -> HttpRouteManifest {
    backend_route_manifest()
}

pub fn gateway_mount(
    admin_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::ModelCatalogAdminStore + Send + Sync,
    >,
    read_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::ModelRankingsReadModelStore + Send + Sync,
    >,
    refresh_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::ModelRankingRefreshStore + Send + Sync,
    >,
    ai_resource_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::AdminAiResourceStore + Send + Sync,
    >,
    entity_uuid_generator: std::sync::Arc<
        dyn sdkwork_models_contract_service::EntityUuidGenerator + Send + Sync,
    >,
    frontend_config_store: std::sync::Arc<
        dyn sdkwork_models_contract_service::FrontendConfigStore + Send + Sync,
    >,
    voice_catalog: std::sync::Arc<sdkwork_models::ModelCatalog>,
) -> axum::Router {
    sdkwork_models_catalog_service::admin_model_management_router_with_store(
        admin_store,
        std::sync::Arc::clone(&entity_uuid_generator),
    )
    .merge(
        sdkwork_models_catalog_service::admin_model_rankings_router_with_read_store_and_refresh_store(
            read_store,
            refresh_store,
        ),
    )
    .merge(sdkwork_models_catalog_service::admin_ai_resource_router_with_store(
        ai_resource_store,
        std::sync::Arc::clone(&entity_uuid_generator),
    ))
    .merge(
        sdkwork_models_catalog_service::backend_frontend_config_router_with_store(
            frontend_config_store,
            entity_uuid_generator,
        ),
    )
    .merge(sdkwork_models_catalog_service::backend_voice_catalog_router(
        std::sync::Arc::clone(&voice_catalog),
    ))
    .merge(sdkwork_models_catalog_service::backend_video_profile_catalog_router(
        voice_catalog,
    ))
}
