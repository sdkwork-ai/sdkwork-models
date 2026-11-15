use sdkwork_web_core::{HttpMethod, HttpRoute, HttpRouteManifest};

use crate::paths::SDK_DOMAIN;

const HTTP_ROUTES: &[HttpRoute] = &[
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/model_vendors",
        SDK_DOMAIN,
        "modelVendors.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/models",
        SDK_DOMAIN,
        "models.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/model_access_channels",
        SDK_DOMAIN,
        "modelAccessChannels.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/model_access_channel_presets",
        SDK_DOMAIN,
        "modelAccessChannelPresets.list",
    ),
    HttpRoute::dual_token(
        HttpMethod::Put,
        "/app/v3/api/ai/model_access_channels/{channel_code}",
        SDK_DOMAIN,
        "modelAccessChannels.upsert",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/model_rankings",
        SDK_DOMAIN,
        "modelRankings.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/voices",
        SDK_DOMAIN,
        "voices.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/models/{modelId}/voices",
        SDK_DOMAIN,
        "modelVoices.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/video_profiles",
        SDK_DOMAIN,
        "videoProfiles.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/models/{modelId}/video_profiles",
        SDK_DOMAIN,
        "modelVideoProfiles.list",
    ),
    HttpRoute::public(
        HttpMethod::Get,
        "/app/v3/api/ai/frontend_config",
        SDK_DOMAIN,
        "frontendConfig.retrieve",
    ),
];

pub fn app_route_manifest() -> HttpRouteManifest {
    HttpRouteManifest::new(HTTP_ROUTES)
}
