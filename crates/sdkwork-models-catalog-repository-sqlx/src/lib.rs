pub const ENV_MODELS_CATALOG_ROOT: &str = "SDKWORK_MODELS_CATALOG_ROOT";

mod admin_ai_resource_hierarchy;
mod admin_models_list;
pub mod model_catalog_import;
pub mod model_modality;
pub mod routing_config_change;
pub mod runtime_id;
pub mod sql_model_rankings;

pub mod postgres;

pub use postgres::{
    PostgresAdminAiResourceStore, PostgresFrontendConfigStore, PostgresModelCatalogAdminStore,
    PostgresModelRankingRefreshStore, PostgresModelRankingsReadStore,
};

pub use sdkwork_models_contract_service::DEFAULT_CATALOG_REFRESH_SOURCE;
