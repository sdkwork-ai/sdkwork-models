//! Application API assembly for sdkwork-models.
//! Application bootstrap lives in `bootstrap.rs`; route inventory is in `assembly-manifest.json`.
// SDKWORK-ASSEMBLY-LIB-CUSTOM: preserve the authored assembly bootstrap and UUID adapter.

mod bootstrap;
mod catalog_seed;
mod contribution;
mod entity_uuid_generator;
mod generated;

pub use bootstrap::{
    assemble_api_router, assemble_business_routes, web_module, ApiAssembly, ApiAssemblyContribution,
};
pub use contribution::assemble_app_api_contribution;

/// Runs Models-owned database lifecycle before dependent assemblies load the
/// shared model catalog.
pub async fn bootstrap_database_from_env() -> Result<(), String> {
    let host = sdkwork_models_database_host::bootstrap_models_database_from_env().await?;
    // The database module ships no catalog rows. Seed the official bundle once
    // on a greenfield database so a fresh install has model vendors available
    // before any admin-triggered sync.
    catalog_seed::seed_model_catalog_if_empty(host.pool()).await;
    Ok(())
}

/// App-api surface route manifest owned by the dependency assembly.
pub fn app_api_route_manifest() -> sdkwork_web_core::HttpRouteManifest {
    sdkwork_routes_models_catalog_app_api::app_route_manifest()
}

/// Backend-api surface route manifest owned by the dependency assembly.
///
/// The Cloud Router cloud assembly dispatches the model catalog backend router
/// itself, so its platform-gateway composition binds this manifest. Exporting
/// it here keeps the nested dependency surface contract's
/// `embeddedExecutableExport` pointing at a real owner-defined entrypoint
/// (API_ASSEMBLY_SPEC §6.2.1) instead of reaching into a route crate from the
/// gateway host.
pub fn backend_api_route_manifest() -> sdkwork_web_core::HttpRouteManifest {
    sdkwork_routes_models_catalog_backend_api::backend_route_manifest()
}

pub fn assembly_route_count() -> usize {
    generated::ROUTE_CRATE_COUNT
}
