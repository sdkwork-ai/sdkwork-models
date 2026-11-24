//! Mount helpers for the portable-catalog profile routers.
//!
//! The models app-api surface ships as two halves:
//!
//! * the pricing-catalog router ([`sdkwork_models_catalog_service::app_model_catalog_router`])
//!   serves model vendors, models, access channels, and access-channel presets;
//! * the portable-catalog profile routers
//!   ([`sdkwork_models_catalog_service::app_voice_catalog_router`] and
//!   [`sdkwork_models_catalog_service::app_video_profile_catalog_router`]) serve
//!   `/app/v3/api/ai/voices`, `/app/v3/api/ai/models/{modelId}/voices`,
//!   `/app/v3/api/ai/video_profiles`, and
//!   `/app/v3/api/ai/models/{modelId}/video_profiles`.
//!
//! [`crate::app_route_manifest`] advertises both halves, so a host that mounts
//! only the pricing-catalog router authorizes the four profile paths and then
//! answers `404` from its dispatcher — the runtime symptom of a composed host
//! that under-mounts this capability. Merging through these helpers keeps the
//! dispatcher and the manifest in step without the host having to own the
//! router inventory itself.

use std::sync::{Arc, OnceLock};

use axum::Router;
use sdkwork_models::{load_bundled_catalog, CatalogError, ModelCatalog};

static PROFILE_CATALOG: OnceLock<Arc<ModelCatalog>> = OnceLock::new();

/// Loads the portable model catalog the voice and video profile routers read.
///
/// Resolution order is the catalog owner's own
/// ([`sdkwork_models::load_bundled_catalog`]): `SDKWORK_MODELS_CATALOG_ROOT`
/// first, then the workspace catalog tree, then `data/sdkwork-models`. The
/// first successful load is cached for the process because the voice and video
/// profile routers share one immutable catalog snapshot.
///
/// # Errors
///
/// Returns [`CatalogError`] when the configured or bundled catalog tree is
/// missing or malformed. Mounting must fail closed in that case: a host that
/// swallows the error silently republishes the same 404 the manifest denies.
pub fn load_profile_catalog() -> Result<Arc<ModelCatalog>, CatalogError> {
    if let Some(catalog) = PROFILE_CATALOG.get() {
        return Ok(Arc::clone(catalog));
    }
    let loaded = load_bundled_catalog()?;
    let catalog = PROFILE_CATALOG.get_or_init(|| Arc::new(loaded));
    Ok(Arc::clone(catalog))
}

/// Merges the voice-catalog and video-profile app routers onto `router`.
///
/// A host that already holds a portable catalog passes it directly so the
/// profile routes and any catalog-derived listing share one snapshot.
#[must_use]
pub fn merge_profile_catalog_routers(router: Router, catalog: Arc<ModelCatalog>) -> Router {
    router
        .merge(sdkwork_models_catalog_service::app_voice_catalog_router(
            Arc::clone(&catalog),
        ))
        .merge(sdkwork_models_catalog_service::app_video_profile_catalog_router(catalog))
}

/// [`merge_profile_catalog_routers`] against the process-wide portable catalog.
///
/// # Errors
///
/// Propagates [`load_profile_catalog`] failures so a host bootstrap fails
/// instead of publishing manifest-authorized routes that no handler answers.
pub fn merge_profile_catalog_routers_from_env(
    router: Router,
) -> Result<Router, CatalogError> {
    let catalog = load_profile_catalog()?;
    Ok(merge_profile_catalog_routers(router, catalog))
}
