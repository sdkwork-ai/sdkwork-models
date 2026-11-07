use axum::Router;
use sdkwork_api_models_assembly::assemble_api_router;
use sdkwork_api_models_standalone_gateway::{
    application_cors_layer, models_health_router_with_database_pool,
};
use sdkwork_web_bootstrap::ApiModuleRegistry;
use tracing::info;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    if iam_enabled() {
        info!("IAM session resolution enabled");
    }

    // PROCESS-DATABASE-POOL: install the process-shared pool before the first
    // database bootstrap. The assembly then owns the single canonical
    // PostgreSQL pool, and the readiness router below reuses it. Creating a
    // second pool here would double the process connection budget and run the
    // schema lifecycle twice against the same database.
    sdkwork_database_sqlx::enable_process_shared_database_pool();

    let mut module_registry = ApiModuleRegistry::new();
    module_registry.add_module(
        assemble_api_router()
            .await
            .expect("models gateway assembly failed"),
    );
    let assembly = module_registry
        .try_compose("SDKWork Models API")
        .unwrap_or_else(|error| panic!("SDKWork Models API module composition failed: {error}"));

    // Reuse the canonical pool installed by the assembly bootstrap above; the
    // readiness probe must never open its own connection pool.
    let database_pool = sdkwork_database_sqlx::process_shared_database_pool()
        .expect("models gateway requires SDKWORK_DATABASE_* environment configuration");

    let app = Router::new()
        .merge(models_health_router_with_database_pool(&database_pool))
        .merge(assembly.router)
        .layer(application_cors_layer());

    let addr = std::env::var("SDKWORK_MODELS_APPLICATION_PUBLIC_INGRESS_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    info!(%addr, "starting sdkwork-api-models-standalone-gateway");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("bind sdkwork-api-models-standalone-gateway listener");
    axum::serve(listener, app)
        .await
        .expect("serve sdkwork-api-models-standalone-gateway");
}

fn iam_enabled() -> bool {
    matches!(
        std::env::var("SDKWORK_MODELS_IAM_ENABLED").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}
