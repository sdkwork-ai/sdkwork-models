pub mod admin_ai_resource;
pub mod admin_model_catalog;
pub mod admin_model_command;
pub mod app_models;
pub mod model_rankings;
pub mod page_info;
pub mod request_id;
pub mod response;
pub mod subject;
pub mod video_profile_catalog;
pub mod voice_catalog;

pub use admin_ai_resource::admin_ai_resource_router_with_store;
pub use admin_model_catalog::{
    admin_model_catalog_router, admin_model_catalog_router_with_api_key_hasher,
};
pub use admin_model_command::admin_model_management_router_with_store;
pub use app_models::{
    app_model_catalog_router, app_model_catalog_router_with_stores,
    app_model_catalog_router_with_stores_and_profiles,
};
pub use model_rankings::{
    admin_model_rankings_router, admin_model_rankings_router_with_read_store,
    admin_model_rankings_router_with_read_store_and_refresh_store, app_model_rankings_router,
    app_model_rankings_router_with_read_store,
};

pub use admin_model_catalog::admin_model_catalog_router as admin_model_catalog_router_with_store;
pub use admin_model_command::admin_model_management_router_with_store as admin_model_catalog_management_router_with_store;
pub use app_model_catalog_router as app_models_router;
pub use video_profile_catalog::{
    app_video_profile_catalog_router, backend_video_profile_catalog_router,
};
pub use voice_catalog::{app_voice_catalog_router, backend_voice_catalog_router};
