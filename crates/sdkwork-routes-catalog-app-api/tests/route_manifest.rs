use sdkwork_routes_models_catalog_app_api::{app_route_manifest, route_definitions};
use sdkwork_web_core::{HttpMethod, RouteAuth};

fn method_label(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

/// Reduce every `{param}` span to `{}` before comparing inventories. The
/// OpenAPI/SDK/route descriptors use camelCase parameter names while the axum
/// router and its manifest use snake_case (for example `{channelCode}` versus
/// `{channel_code}`); the name is never sent on the wire, so only the path
/// shape is authoritative.
fn normalized_path(template: &str) -> String {
    let mut resolved = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        resolved.push_str(&rest[..open]);
        resolved.push_str("{}");
        let Some(close_relative) = rest[open..].find('}') else {
            return resolved;
        };
        rest = &rest[open + close_relative + 1..];
    }
    resolved.push_str(rest);
    resolved
}

fn assert_app_auth(method: &str, path: &str, auth: RouteAuth) {
    if method == "GET" {
        // Public catalog reads are intentional (the console renders the model
        // directory before a session exists); write operations are not.
        assert!(
            matches!(
                auth,
                RouteAuth::Public | RouteAuth::DualToken | RouteAuth::DualTokenOrAnonymous
            ),
            "GET {path} has an unexpected auth profile {auth:?}"
        );
    } else {
        assert_eq!(
            auth,
            RouteAuth::DualToken,
            "mutating app route {method} {path} must require dual-token credentials"
        );
    }
}

fn concrete_path(template: &str) -> String {
    let mut resolved = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        resolved.push_str(&rest[..open]);
        let Some(close_relative) = rest[open..].find('}') else {
            resolved.push_str(&rest[open..]);
            return resolved;
        };
        resolved.push_str("parity-probe");
        rest = &rest[open + close_relative + 1..];
    }
    resolved.push_str(rest);
    resolved
}

#[test]
fn app_route_manifest_matches_route_definitions() {
    let manifest = app_route_manifest();
    for entry in route_definitions() {
        let matched = manifest
            .match_route(entry.method, entry.path)
            .unwrap_or_else(|| {
                panic!(
                    "missing http route manifest for {} {}",
                    entry.method, entry.path
                )
            });
        assert_app_auth(entry.method, entry.path, matched.auth);
        assert_eq!(matched.operation_id, entry.operation_id);
        assert_eq!(normalized_path(matched.path), normalized_path(entry.path));

        let concrete = concrete_path(entry.path);
        let matched_concrete = manifest
            .match_route(entry.method, &concrete)
            .unwrap_or_else(|| {
                panic!(
                    "manifest cannot resolve concrete request for {} {} (from {})",
                    entry.method, concrete, entry.path
                )
            });
        assert_eq!(
            normalized_path(matched_concrete.path),
            normalized_path(entry.path),
            "concrete request {concrete} resolved to the wrong template"
        );
        assert_eq!(matched_concrete.operation_id, entry.operation_id);
    }
}

#[test]
fn app_route_definitions_cover_every_manifest_route() {
    let manifest = app_route_manifest();
    let definitions = route_definitions();
    assert_eq!(
        manifest.routes().len(),
        definitions.len(),
        "app manifest and route descriptors must describe the same inventory"
    );

    for route in manifest.routes() {
        let method = method_label(route.method);
        let descriptor = definitions
            .iter()
            .find(|entry| {
                entry.method == method && normalized_path(entry.path) == normalized_path(route.path)
            })
            .unwrap_or_else(|| {
                panic!(
                    "app manifest declares {method} {} without a route descriptor",
                    route.path
                )
            });
        assert_eq!(
            route.operation_id, descriptor.operation_id,
            "{method} {} operation ids must agree between manifest and descriptor",
            route.path
        );
        assert_app_auth(method, route.path, route.auth);
    }
}
