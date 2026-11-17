//! Real-PostgreSQL contract test for the front-end configuration store.
//!
//! Run against a database migrated with the models module:
//!
//! ```text
//! SDKWORK_DATABASE_URL=postgresql://... \
//!   cargo test -p sdkwork-models-catalog-repository-sqlx \
//!   --test frontend_config_store_postgres -- --ignored
//! ```

use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use sdkwork_models_catalog_repository_sqlx::PostgresFrontendConfigStore;
use sdkwork_models_contract_service::{
    FrontendChannelCommand, FrontendChannelFieldCommand, FrontendChannelModelCommand,
    FrontendConfigStore, FrontendConfigSubject, LoadFrontendConfigQuery,
    ReplaceFrontendConfigCommand,
};

const TEST_TENANT_ID: i64 = 900_001;
const HASH_TENANT_ID: i64 = 900_002;
const TEST_ORGANIZATION_ID: i64 = 0;

/// The workspace schema is the database name; the runtime normalizes the URL
/// to set `search_path`, so mirror that for a raw pool.
fn workspace_schema(database_url: &str) -> String {
    database_url
        .split('?')
        .next()
        .unwrap_or(database_url)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

async fn connect_pool(database_url: &str) -> PgPool {
    let schema = workspace_schema(database_url);
    PgPoolOptions::new()
        .max_connections(2)
        .after_connect(move |connection, _metadata| {
            let statement = format!("SET search_path TO \"{}\"", schema.replace('"', "\"\""));
            Box::pin(async move {
                sqlx::query(sqlx::AssertSqlSafe(statement))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
        .expect("connect models database")
}

fn subject() -> FrontendConfigSubject {
    FrontendConfigSubject {
        tenant_id: TEST_TENANT_ID,
        organization_id: TEST_ORGANIZATION_ID,
        operator_id: 1,
        operator_type: 1,
    }
}

fn model(catalog_key: &str, model_id: Option<i64>) -> FrontendChannelModelCommand {
    FrontendChannelModelCommand {
        uuid: format!("test-model-{catalog_key}"),
        catalog_key: catalog_key.to_owned(),
        model_id,
        visible: true,
        featured: false,
        sort_order: 10,
        badge: None,
        config: json!({}),
    }
}

fn channel(channel_code: &str, models: Vec<FrontendChannelModelCommand>) -> FrontendChannelCommand {
    FrontendChannelCommand {
        uuid: format!("test-channel-{channel_code}"),
        channel_code: channel_code.to_owned(),
        display_name: channel_code.to_uppercase(),
        display_name_i18n: json!({ "en-US": channel_code.to_uppercase() }),
        description: None,
        enabled: true,
        sort_order: 10,
        default_catalog_key: models.first().map(|model| model.catalog_key.clone()),
        capability_codes: vec!["chat".to_owned()],
        config: json!({ "pageSize": 100 }),
        schema_version: "1.0.0".to_owned(),
        models,
        fields: vec![FrontendChannelFieldCommand {
            uuid: format!("test-field-{channel_code}-price"),
            field_key: "price".to_owned(),
            visible: true,
            sort_order: 10,
            config: json!({}),
        }],
    }
}

fn replace_command(
    channels: Vec<FrontendChannelCommand>,
    sequence: u32,
) -> ReplaceFrontendConfigCommand {
    ReplaceFrontendConfigCommand {
        subject: subject(),
        channels,
        source: "test".to_owned(),
        request_id: format!("test-request-{sequence}"),
        snapshot_uuid: format!("test-snapshot-{sequence}"),
        requested_at: "2026-11-17T12:00:00Z".to_owned(),
    }
}

async fn clean_tenant(pool: &PgPool, tenant_id: i64) {
    for table in [
        "ai_model_frontend_channel_model",
        "ai_model_frontend_field",
        "ai_model_frontend_channel",
        "ai_model_frontend_revision",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {table} WHERE tenant_id = $1 AND organization_id = $2"
        )))
        .bind(tenant_id)
        .bind(TEST_ORGANIZATION_ID)
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("clean {table} failed: {error}"));
    }
}

#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_URL pointing to a migrated models database"]
async fn store_round_trips_and_replaces_the_frontend_configuration() {
    let database_url = std::env::var("SDKWORK_DATABASE_URL").expect("SDKWORK_DATABASE_URL");
    let pool = connect_pool(&database_url).await;
    clean_tenant(&pool, TEST_TENANT_ID).await;
    let store = PostgresFrontendConfigStore::new(pool.clone());

    let first = store
        .replace_frontend_config(replace_command(
            vec![channel(
                "llm",
                vec![
                    model("test-vendor/test-model-a", Some(42)),
                    model("test-vendor/test-model-b", None),
                ],
            )],
            1,
        ))
        .await
        .expect("replace frontend config");

    assert_eq!(first.channels.len(), 1);
    assert_eq!(first.channels[0].channel_code, "llm");
    assert_eq!(first.channels[0].models.len(), 2);
    assert_eq!(first.channels[0].fields.len(), 1);

    let loaded = store
        .load_frontend_config(LoadFrontendConfigQuery { subject: subject() })
        .await
        .expect("load frontend config");
    assert_eq!(loaded.revision, first.revision);
    assert_eq!(loaded.channels.len(), 1);
    assert_eq!(loaded.channels[0].channel_code, "llm");
    assert_eq!(
        loaded.channels[0].default_catalog_key.as_deref(),
        Some("test-vendor/test-model-a")
    );
    assert_eq!(loaded.channels[0].models.len(), 2);
    assert_eq!(loaded.channels[0].models[0].model_id, Some(42));
    assert_eq!(loaded.channels[0].models[1].model_id, None);
    assert_eq!(loaded.channels[0].fields[0].field_key, "price");

    // A second replace retires the previous revision's rows.
    let second = store
        .replace_frontend_config(replace_command(
            vec![channel(
                "video",
                vec![model("test-vendor/test-video", Some(7))],
            )],
            2,
        ))
        .await
        .expect("replace frontend config again");
    assert!(second.revision > first.revision);

    let replaced = store
        .load_frontend_config(LoadFrontendConfigQuery { subject: subject() })
        .await
        .expect("load replaced frontend config");
    assert_eq!(replaced.revision, second.revision);
    assert_eq!(replaced.channels.len(), 1);
    assert_eq!(replaced.channels[0].channel_code, "video");
    assert_eq!(replaced.channels[0].models[0].model_id, Some(7));

    clean_tenant(&pool, TEST_TENANT_ID).await;
}

/// Run after `migrate` + `seed` against a fresh models database: the persisted
/// initialization document must load through the real store, including the
/// `INTEGER` sort-order columns and the model/field children.
#[tokio::test]
#[ignore = "requires SDKWORK_DATABASE_URL pointing to a migrated and seeded models database"]
async fn seeded_console_tenant_loads_all_modalities() {
    let database_url = std::env::var("SDKWORK_DATABASE_URL").expect("SDKWORK_DATABASE_URL");
    let pool = connect_pool(&database_url).await;
    let store = PostgresFrontendConfigStore::new(pool.clone());

    let loaded = store
        .load_frontend_config(LoadFrontendConfigQuery {
            subject: FrontendConfigSubject {
                tenant_id: 100_001,
                organization_id: 0,
                operator_id: 0,
                operator_type: 0,
            },
        })
        .await
        .expect("load seeded frontend config");

    assert_eq!(loaded.revision, 1);
    assert_eq!(loaded.channels.len(), 7);
    for code in [
        "llm",
        "video",
        "image",
        "audio",
        "music",
        "sound_effect",
        "embedding",
    ] {
        let channel = loaded
            .channels
            .iter()
            .find(|channel| channel.channel_code == code)
            .unwrap_or_else(|| panic!("seeded channel {code} is missing"));
        assert!(
            !channel.models.is_empty(),
            "seeded channel {code} exposes no initialization model"
        );
        let default_catalog_key = channel
            .default_catalog_key
            .as_deref()
            .unwrap_or_else(|| panic!("seeded channel {code} has no default model"));
        assert!(
            channel
                .models
                .iter()
                .any(|model| model.catalog_key == default_catalog_key),
            "seeded channel {code} default is not exposed"
        );
        assert!(
            !channel.fields.is_empty(),
            "seeded channel {code} exposes no field"
        );
    }

    let model_count: usize = loaded
        .channels
        .iter()
        .map(|channel| channel.models.len())
        .sum();
    let field_count: usize = loaded
        .channels
        .iter()
        .map(|channel| channel.fields.len())
        .sum();
    assert_eq!(model_count, 21, "seeded initialization model count");
    assert_eq!(field_count, 28, "seeded field count");

    // The seeded revision hash must describe the document the store publishes.
    let channels = loaded
        .channels
        .into_iter()
        .map(|channel| {
            let channel_code = channel.channel_code.clone();
            FrontendChannelCommand {
                uuid: format!("hash-check-channel-{channel_code}"),
                channel_code: channel.channel_code,
                display_name: channel.display_name,
                display_name_i18n: channel.display_name_i18n,
                description: channel.description,
                enabled: channel.enabled,
                sort_order: channel.sort_order,
                default_catalog_key: channel.default_catalog_key,
                capability_codes: channel.capability_codes,
                config: channel.config,
                schema_version: channel.schema_version,
                models: channel
                    .models
                    .into_iter()
                    .map(|model| {
                        let catalog_key = model.catalog_key.clone();
                        FrontendChannelModelCommand {
                            uuid: format!("hash-check-model-{channel_code}-{catalog_key}"),
                            catalog_key: model.catalog_key,
                            model_id: model.model_id,
                            visible: model.visible,
                            featured: model.featured,
                            sort_order: model.sort_order,
                            badge: model.badge,
                            config: model.config,
                        }
                    })
                    .collect(),
                fields: channel
                    .fields
                    .into_iter()
                    .map(|field| {
                        let field_key = field.field_key.clone();
                        FrontendChannelFieldCommand {
                            uuid: format!("hash-check-field-{channel_code}-{field_key}"),
                            field_key: field.field_key,
                            visible: field.visible,
                            sort_order: field.sort_order,
                            config: field.config,
                        }
                    })
                    .collect(),
            }
        })
        .collect();

    clean_tenant(&pool, HASH_TENANT_ID).await;
    let hash_subject = FrontendConfigSubject {
        tenant_id: HASH_TENANT_ID,
        organization_id: TEST_ORGANIZATION_ID,
        operator_id: 1,
        operator_type: 1,
    };
    let replaced = store
        .replace_frontend_config(ReplaceFrontendConfigCommand {
            subject: hash_subject,
            channels,
            source: "hash-check".to_owned(),
            request_id: "hash-check".to_owned(),
            snapshot_uuid: "hash-check".to_owned(),
            requested_at: "2026-11-17T12:00:00Z".to_owned(),
        })
        .await
        .expect("replace seeded document for the hash check");

    let stored_hash: String = sqlx::query_scalar(
        "SELECT content_sha256 FROM ai_model_frontend_revision \
         WHERE tenant_id = $1 AND organization_id = $2 AND revision = $3",
    )
    .bind(HASH_TENANT_ID)
    .bind(TEST_ORGANIZATION_ID)
    .bind(replaced.revision)
    .fetch_one(&pool)
    .await
    .expect("read the published revision hash");

    let seed_sql =
        include_str!("../../../database/seeds/common/002_ai_model_frontend_defaults.sql");
    let marker = "'seed-fmc-revision-1', 100001, 0, 1, 1, 0, 1, '";
    let hash_start = seed_sql.find(marker).expect("seed revision row") + marker.len();
    let seed_hash = &seed_sql[hash_start..hash_start + 64];
    assert_eq!(
        stored_hash, seed_hash,
        "seed audit hash must match the store's canonical document encoding"
    );

    clean_tenant(&pool, HASH_TENANT_ID).await;
}
