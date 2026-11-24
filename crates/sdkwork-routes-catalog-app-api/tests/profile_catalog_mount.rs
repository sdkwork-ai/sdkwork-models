//! Mount parity for the portable-catalog profile routers.
//!
//! These four routes are the profile half of the models app-api surface: they
//! live in the voice-catalog and video-profile routers, not in the
//! pricing-catalog router, yet [`app_route_manifest`] advertises them. A host
//! that mounts only the pricing-catalog router therefore authorizes the paths
//! and answers `404` from its dispatcher — the regression these tests exist to
//! keep closed.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use sdkwork_routes_models_catalog_app_api::{
    app_route_manifest, merge_profile_catalog_routers_from_env,
};
use sdkwork_web_core::{
    ServerRequestId, WebApiSurface, WebAuthLevel, WebAuthMode, WebDeploymentMode, WebEnvironment,
    WebLoginScope, WebRequestContext, WebRequestPrincipal, WebTransportFacts,
};
use tower::ServiceExt;

/// Concrete request paths for the four profile routes. `openai%2Fgpt-4o` is one
/// URL segment that percent-decodes to the `vendor/model` catalog key the
/// per-model handlers expect.
const PROFILE_REQUEST_PATHS: &[&str] = &[
    "/app/v3/api/ai/voices",
    "/app/v3/api/ai/models/openai%2Fgpt-4o/voices",
    "/app/v3/api/ai/video_profiles",
    "/app/v3/api/ai/models/openai%2Fgpt-4o/video_profiles",
];

/// Template paths the same requests must resolve to in the route manifest.
const PROFILE_TEMPLATE_PATHS: &[&str] = &[
    "/app/v3/api/ai/voices",
    "/app/v3/api/ai/models/{modelId}/voices",
    "/app/v3/api/ai/video_profiles",
    "/app/v3/api/ai/models/{modelId}/video_profiles",
];

fn app_request(uri: &str) -> Request<Body> {
    let principal = WebRequestPrincipal::builder()
        .tenant_id("100001")
        .organization_id(Some("0".to_owned()))
        .user_id("30")
        .login_scope(WebLoginScope::Organization)
        .session_id(Some("session-1".to_owned()))
        .app_id("sdkwork-models")
        .environment(WebEnvironment::Dev)
        .deployment_mode(WebDeploymentMode::Private)
        .auth_level(WebAuthLevel::Password)
        .build();
    let context = WebRequestContext {
        request_id: ServerRequestId("profile-catalog-test".to_owned()),
        trace_id: None,
        api_surface: WebApiSurface::AppApi,
        auth_mode: WebAuthMode::Public,
        transport: WebTransportFacts {
            path: uri.to_owned(),
            method: "GET".to_owned(),
            auth_token_present: false,
            access_token_present: false,
            api_key_present: false,
            ingress_token_present: false,
            oauth_bearer_present: false,
            agent_token_present: false,
        },
        principal: Some(principal),
        locale: None,
        client_kind: None,
        operation: None,
        idempotency_key: None,
    };

    let mut request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .expect("profile catalog request");
    request.extensions_mut().insert(context);
    request
}

#[test]
fn manifest_advertises_every_profile_route_the_mount_serves() {
    let manifest = app_route_manifest();
    for template in PROFILE_TEMPLATE_PATHS {
        assert!(
            manifest.match_route("GET", template).is_some(),
            "GET {template} must stay advertised by the models app-api manifest"
        );
    }
}

#[tokio::test]
async fn merged_profile_routers_serve_every_advertised_profile_route() {
    let router = merge_profile_catalog_routers_from_env(axum::Router::new())
        .expect("load the portable models catalog for the profile routers");

    for uri in PROFILE_REQUEST_PATHS {
        let response = router
            .clone()
            .oneshot(app_request(uri))
            .await
            .expect("profile catalog response");

        assert_eq!(
            StatusCode::OK,
            response.status(),
            "GET {uri} must be served once the profile routers are mounted"
        );

        let body = response
            .into_body()
            .collect()
            .await
            .expect("profile catalog body")
            .to_bytes();
        let payload: Value = serde_json::from_slice(&body).expect("profile catalog envelope json");
        assert_eq!(
            0, payload["code"],
            "GET {uri} must answer the success envelope"
        );
        assert!(
            payload["data"]["items"].is_array(),
            "GET {uri} must answer data.items from the portable catalog"
        );
    }
}
