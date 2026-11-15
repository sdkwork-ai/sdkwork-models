#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteDefinition {
    pub method: &'static str,
    pub path: &'static str,
    pub operation_id: &'static str,
    pub handler: &'static str,
    pub service_method: &'static str,
}

pub const ROUTES: &[RouteDefinition] = &[
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/model_vendors",
        operation_id: "modelVendors.list",
        handler: "list_model_vendors",
        service_method: "list_model_vendors",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/models",
        operation_id: "models.list",
        handler: "list_models",
        service_method: "list_models",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/model_access_channels",
        operation_id: "modelAccessChannels.list",
        handler: "list_model_access_channels",
        service_method: "list_model_access_channels",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/model_access_channel_presets",
        operation_id: "modelAccessChannelPresets.list",
        handler: "list_model_access_channel_presets",
        service_method: "list_model_access_channel_presets",
    },
    RouteDefinition {
        method: "PUT",
        path: "/app/v3/api/ai/model_access_channels/{channel_code}",
        operation_id: "modelAccessChannels.upsert",
        handler: "upsert_model_access_channel",
        service_method: "upsert_model_access_channel",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/model_rankings",
        operation_id: "modelRankings.list",
        handler: "list_model_rankings",
        service_method: "list_model_rankings",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/voices",
        operation_id: "voices.list",
        handler: "list_voices",
        service_method: "list_voices",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/models/{modelId}/voices",
        operation_id: "modelVoices.list",
        handler: "list_model_voices",
        service_method: "list_model_voices",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/video_profiles",
        operation_id: "videoProfiles.list",
        handler: "list_video_profiles",
        service_method: "list_video_profiles",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/models/{modelId}/video_profiles",
        operation_id: "modelVideoProfiles.list",
        handler: "list_model_video_profiles",
        service_method: "list_model_video_profiles",
    },
    RouteDefinition {
        method: "GET",
        path: "/app/v3/api/ai/frontend_config",
        operation_id: "frontendConfig.retrieve",
        handler: "fetch_app_frontend_config",
        service_method: "load_frontend_config",
    },
];

pub fn route_definitions() -> &'static [RouteDefinition] {
    ROUTES
}
