use sdkwork_routes_models_catalog_backend_api::{backend_route_manifest, route_definitions};
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

/// Reduce every `{param}` span to `{}` before comparing inventories: axum may
/// name a path parameter differently from the OpenAPI/SDK contract without
/// changing the wire shape of the route.
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

/// Turn an OpenAPI-style `{param}` template into a concrete request path so
/// the manifest matcher is exercised the same way the Web Framework resolves
/// real traffic (matching a template against its own literal text can hide a
/// broken template rule).
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
fn backend_route_manifest_matches_route_definitions() {
    let manifest = backend_route_manifest();
    for entry in route_definitions() {
        let matched = manifest
            .match_route(entry.method, entry.path)
            .unwrap_or_else(|| {
                panic!(
                    "missing http route manifest for {} {}",
                    entry.method, entry.path
                )
            });
        assert_eq!(matched.auth, RouteAuth::DualToken);
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
fn backend_route_definitions_cover_every_manifest_route() {
    // Reverse direction: the manifest is what the Web Framework binds, so an
    // extra route would be advertised to clients without a registered handler
    // descriptor. Keep both inventories exactly equal (count, method, path,
    // operation id and auth profile), including parameterized templates.
    let manifest = backend_route_manifest();
    let definitions = route_definitions();
    assert_eq!(
        manifest.routes().len(),
        definitions.len(),
        "backend manifest and route descriptors must describe the same inventory"
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
                    "backend manifest declares {method} {} without a route descriptor",
                    route.path
                )
            });
        assert_eq!(
            route.operation_id, descriptor.operation_id,
            "{method} {} operation ids must agree between manifest and descriptor",
            route.path
        );
        assert_eq!(route.auth, RouteAuth::DualToken);
    }
}
