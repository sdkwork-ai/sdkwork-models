use std::collections::HashSet;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use sdkwork_cloudrouter_http::TrustedRequestSubject;
use sdkwork_utils_rust::datetime::current_timestamp_string;
use sdkwork_utils_rust::SdkWorkResultCode;
use sdkwork_web_core::WebRequestContext;

use crate::api::request_id::generate_server_request_id;
use crate::api::response::{finish_success, problem_for};
use crate::application::EntityUuidGenerator;
use crate::ports::{
    FrontendChannel, FrontendChannelCommand, FrontendChannelField, FrontendChannelFieldCommand,
    FrontendChannelModel, FrontendChannelModelCommand, FrontendConfig, FrontendConfigStore,
    FrontendConfigSubject, LoadFrontendConfigQuery, ReplaceFrontendConfigCommand,
};

const MAX_CHANNELS: usize = 64;
const MAX_CHANNEL_MODELS: usize = 2_000;
const MAX_CHANNEL_FIELDS: usize = 64;
const MAX_CHANNEL_CODE_LEN: usize = 64;
const MAX_DISPLAY_NAME_LEN: usize = 128;
const MAX_DESCRIPTION_LEN: usize = 512;
const MAX_CATALOG_KEY_LEN: usize = 256;
const MAX_FIELD_KEY_LEN: usize = 64;
const MAX_BADGE_LEN: usize = 64;
const MAX_CAPABILITY_CODES: usize = 64;
const MAX_CAPABILITY_CODE_LEN: usize = 64;
const MAX_SCHEMA_VERSION_LEN: usize = 32;
const MAX_LOCALE_LEN: usize = 32;
const MAX_SOURCE_LEN: usize = 64;

#[derive(Clone)]
struct FrontendConfigState {
    store: Arc<dyn FrontendConfigStore + Send + Sync>,
    entity_uuid_generator: Option<Arc<dyn EntityUuidGenerator + Send + Sync>>,
}

pub fn backend_frontend_config_router_with_store(
    store: Arc<dyn FrontendConfigStore + Send + Sync>,
    entity_uuid_generator: Arc<dyn EntityUuidGenerator + Send + Sync>,
) -> Router {
    Router::new()
        .route(
            "/backend/v3/api/ai/frontend_config",
            get(fetch_frontend_config).put(replace_frontend_config),
        )
        .with_state(FrontendConfigState {
            store,
            entity_uuid_generator: Some(entity_uuid_generator),
        })
}

pub fn app_frontend_config_router_with_store(
    store: Arc<dyn FrontendConfigStore + Send + Sync>,
) -> Router {
    Router::new()
        .route(
            "/app/v3/api/ai/frontend_config",
            get(fetch_app_frontend_config),
        )
        .with_state(FrontendConfigState {
            store,
            entity_uuid_generator: None,
        })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrontendConfigResponse {
    revision: i64,
    channels: Vec<FrontendChannelResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrontendChannelResponse {
    channel_code: String,
    display_name: String,
    display_name_i18n: Value,
    description: Option<String>,
    enabled: bool,
    sort_order: i64,
    default_catalog_key: Option<String>,
    capability_codes: Vec<String>,
    config: Value,
    schema_version: String,
    revision: i64,
    models: Vec<FrontendChannelModelResponse>,
    fields: Vec<FrontendChannelFieldResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrontendChannelModelResponse {
    catalog_key: String,
    /// Model ids are int64 and cross the HTTP boundary as decimal strings
    /// (API_SPEC section 13.6); a JavaScript client must never round them.
    #[serde(with = "sdkwork_utils_rust::serde_int64::option")]
    model_id: Option<i64>,
    visible: bool,
    featured: bool,
    sort_order: i64,
    badge: Option<String>,
    config: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrontendChannelFieldResponse {
    field_key: String,
    visible: bool,
    sort_order: i64,
    config: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendConfigReplaceRequest {
    #[serde(default)]
    channels: Vec<FrontendChannelRequest>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendChannelRequest {
    channel_code: String,
    display_name: String,
    #[serde(default)]
    display_name_i18n: Value,
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    sort_order: i64,
    #[serde(default)]
    default_catalog_key: Option<String>,
    #[serde(default)]
    capability_codes: Vec<String>,
    #[serde(default = "default_config")]
    config: Value,
    #[serde(default = "default_schema_version")]
    schema_version: String,
    #[serde(default)]
    models: Vec<FrontendChannelModelRequest>,
    #[serde(default)]
    fields: Vec<FrontendChannelFieldRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendChannelModelRequest {
    catalog_key: String,
    #[serde(default, with = "sdkwork_utils_rust::serde_int64::option")]
    model_id: Option<i64>,
    #[serde(default = "default_enabled")]
    visible: bool,
    #[serde(default)]
    featured: bool,
    #[serde(default)]
    sort_order: i64,
    #[serde(default)]
    badge: Option<String>,
    #[serde(default = "default_config")]
    config: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrontendChannelFieldRequest {
    field_key: String,
    #[serde(default = "default_enabled")]
    visible: bool,
    #[serde(default)]
    sort_order: i64,
    #[serde(default = "default_config")]
    config: Value,
}

fn default_enabled() -> bool {
    true
}

fn default_config() -> Value {
    json!({})
}

fn default_schema_version() -> String {
    "1.0.0".to_owned()
}

async fn fetch_frontend_config(
    ctx: WebRequestContext,
    State(state): State<FrontendConfigState>,
    trusted: TrustedRequestSubject,
    _headers: HeaderMap,
) -> Response {
    let subject = map_subject(trusted);
    match state
        .store
        .load_frontend_config(LoadFrontendConfigQuery { subject })
        .await
    {
        Ok(config) => {
            let config = if config.channels.is_empty() {
                FrontendConfig {
                    revision: 0,
                    channels: default_frontend_channels(),
                }
            } else {
                config
            };
            finish_success(&ctx, to_response(config))
        }
        Err(error) => problem_for(&ctx, SdkWorkResultCode::InternalError, error.to_string()),
    }
}

async fn fetch_app_frontend_config(
    ctx: WebRequestContext,
    State(state): State<FrontendConfigState>,
    trusted: Option<TrustedRequestSubject>,
    headers: HeaderMap,
) -> Response {
    let subject = trusted.map(map_subject).unwrap_or(FrontendConfigSubject {
        tenant_id: 0,
        organization_id: 0,
        operator_id: 0,
        operator_type: 0,
    });
    match state
        .store
        .load_frontend_config(LoadFrontendConfigQuery { subject })
        .await
    {
        Ok(config) => {
            let channels = if config.channels.is_empty() {
                default_frontend_channels()
            } else {
                config.channels
            };
            let visible = app_visible_channels(channels);
            let response = to_response(FrontendConfig {
                revision: config.revision,
                channels: visible,
            });
            let etag = format!("\"fmc-{}\"", response.revision);
            if if_none_match_matches(&headers, &etag) {
                let mut not_modified = StatusCode::NOT_MODIFIED.into_response();
                if let Ok(value) = HeaderValue::from_str(&etag) {
                    not_modified.headers_mut().insert(header::ETAG, value);
                }
                not_modified
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
                return not_modified;
            }
            let mut response = finish_success(&ctx, response);
            if let Ok(value) = HeaderValue::from_str(&etag) {
                response.headers_mut().insert(header::ETAG, value);
            }
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            response
        }
        Err(error) => problem_for(&ctx, SdkWorkResultCode::InternalError, error.to_string()),
    }
}

/// What the app surface may see: enabled channels, visible models and fields,
/// and a default that names one of the visible models.
fn app_visible_channels(channels: Vec<FrontendChannel>) -> Vec<FrontendChannel> {
    channels
        .into_iter()
        .filter(|channel| channel.enabled)
        .map(|mut channel| {
            channel.models.retain(|model| model.visible);
            channel.fields.retain(|field| field.visible);
            // Legacy documents may still point at a model that is no longer
            // visible; the app surface must never receive a default it cannot
            // resolve.
            if let Some(default_catalog_key) = channel.default_catalog_key.as_deref() {
                if !channel
                    .models
                    .iter()
                    .any(|model| model.catalog_key == default_catalog_key)
                {
                    channel.default_catalog_key = channel
                        .models
                        .first()
                        .map(|model| model.catalog_key.clone());
                }
            }
            channel
        })
        .collect()
}

/// Whether an `If-None-Match` header names the current entity tag. Handles the
/// `*` wildcard and comma-separated weak or strong validators.
fn if_none_match_matches(headers: &HeaderMap, etag: &str) -> bool {
    let Some(value) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    value
        .split(',')
        .map(|candidate| candidate.trim())
        .any(|candidate| candidate == "*" || candidate.trim_start_matches("W/") == etag)
}

async fn replace_frontend_config(
    ctx: WebRequestContext,
    State(state): State<FrontendConfigState>,
    trusted: TrustedRequestSubject,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request = match serde_json::from_slice::<FrontendConfigReplaceRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return problem_for(
                &ctx,
                SdkWorkResultCode::ValidationError,
                format!("invalid frontend config payload: {error}"),
            )
        }
    };
    if let Err(message) = validate_request(&request) {
        return problem_for(&ctx, SdkWorkResultCode::ValidationError, message);
    }
    let subject = map_subject(trusted);
    let command = match build_replace_command(&state, subject, request) {
        Ok(command) => command,
        Err(error) => {
            return problem_for(&ctx, SdkWorkResultCode::InternalError, error.to_string())
        }
    };
    match state.store.replace_frontend_config(command).await {
        Ok(config) => finish_success(&ctx, to_response(config)),
        Err(error) => problem_for(&ctx, SdkWorkResultCode::InternalError, error.to_string()),
    }
}

fn build_replace_command(
    state: &FrontendConfigState,
    subject: FrontendConfigSubject,
    request: FrontendConfigReplaceRequest,
) -> crate::domain::DomainResult<ReplaceFrontendConfigCommand> {
    let Some(generator) = state.entity_uuid_generator.as_ref() else {
        return Err(crate::domain::DomainError::new(
            "frontend config replace requires an entity UUID generator",
        ));
    };
    let mut channels = Vec::with_capacity(request.channels.len());
    for channel in request.channels {
        let mut models = Vec::with_capacity(channel.models.len());
        for model in channel.models {
            models.push(FrontendChannelModelCommand {
                uuid: generator.generate_entity_uuid()?,
                catalog_key: model.catalog_key,
                model_id: model.model_id,
                visible: model.visible,
                featured: model.featured,
                sort_order: model.sort_order,
                badge: model.badge,
                config: model.config,
            });
        }
        let mut fields = Vec::with_capacity(channel.fields.len());
        for field in channel.fields {
            fields.push(FrontendChannelFieldCommand {
                uuid: generator.generate_entity_uuid()?,
                field_key: field.field_key,
                visible: field.visible,
                sort_order: field.sort_order,
                config: field.config,
            });
        }
        let display_name_i18n = if channel.display_name_i18n.is_null() {
            json!({})
        } else {
            channel.display_name_i18n
        };
        channels.push(FrontendChannelCommand {
            uuid: generator.generate_entity_uuid()?,
            channel_code: channel.channel_code,
            display_name: channel.display_name,
            display_name_i18n,
            description: channel.description,
            enabled: channel.enabled,
            sort_order: channel.sort_order,
            default_catalog_key: channel.default_catalog_key,
            capability_codes: channel.capability_codes,
            config: channel.config,
            schema_version: channel.schema_version,
            models,
            fields,
        });
    }
    Ok(ReplaceFrontendConfigCommand {
        subject,
        channels,
        source: request.source.unwrap_or_else(|| "admin".to_owned()),
        request_id: generate_server_request_id().unwrap_or_else(|_| String::new()),
        snapshot_uuid: generator.generate_entity_uuid()?,
        requested_at: current_timestamp_string(),
    })
}

fn validate_request(request: &FrontendConfigReplaceRequest) -> Result<(), String> {
    if let Some(source) = request.source.as_deref() {
        validate_text("source", source, MAX_SOURCE_LEN, true)?;
    }
    if request.channels.len() > MAX_CHANNELS {
        return Err(format!(
            "at most {MAX_CHANNELS} frontend channels are supported"
        ));
    }
    let mut channel_codes = HashSet::with_capacity(request.channels.len());
    for channel in &request.channels {
        validate_channel_code(&channel.channel_code)?;
        if !channel_codes.insert(channel.channel_code.as_str()) {
            return Err(format!(
                "duplicate frontend channel code: {}",
                channel.channel_code
            ));
        }
        validate_text(
            "channel displayName",
            &channel.display_name,
            MAX_DISPLAY_NAME_LEN,
            false,
        )?;
        if !channel.display_name_i18n.is_null() {
            let Some(labels) = channel.display_name_i18n.as_object() else {
                return Err(format!(
                    "channel {} displayNameI18n must be a JSON object",
                    channel.channel_code
                ));
            };
            for (locale, value) in labels {
                validate_text(
                    "channel displayNameI18n locale",
                    locale,
                    MAX_LOCALE_LEN,
                    false,
                )?;
                let Some(text) = value.as_str() else {
                    return Err(format!(
                        "channel {} displayNameI18n values must be strings",
                        channel.channel_code
                    ));
                };
                validate_text(
                    "channel displayNameI18n value",
                    text,
                    MAX_DISPLAY_NAME_LEN,
                    false,
                )?;
            }
        }
        validate_text(
            "channel schemaVersion",
            &channel.schema_version,
            MAX_SCHEMA_VERSION_LEN,
            false,
        )?;
        validate_sort_order("channel sortOrder", channel.sort_order)?;
        if let Some(description) = &channel.description {
            validate_text(
                "channel description",
                description,
                MAX_DESCRIPTION_LEN,
                true,
            )?;
        }
        if channel.capability_codes.len() > MAX_CAPABILITY_CODES {
            return Err(format!(
                "channel {} supports at most {MAX_CAPABILITY_CODES} capability codes",
                channel.channel_code
            ));
        }
        for code in &channel.capability_codes {
            validate_text(
                "channel capability code",
                code,
                MAX_CAPABILITY_CODE_LEN,
                false,
            )?;
        }
        if !channel.config.is_object() {
            return Err(format!(
                "channel {} config must be a JSON object",
                channel.channel_code
            ));
        }
        if let Some(default_catalog_key) = &channel.default_catalog_key {
            validate_text(
                "channel defaultCatalogKey",
                default_catalog_key,
                MAX_CATALOG_KEY_LEN,
                false,
            )?;
        }
        if channel.models.len() > MAX_CHANNEL_MODELS {
            return Err(format!(
                "channel {} exposes at most {MAX_CHANNEL_MODELS} models",
                channel.channel_code
            ));
        }
        let mut catalog_keys = HashSet::with_capacity(channel.models.len());
        for model in &channel.models {
            validate_text(
                "model catalogKey",
                &model.catalog_key,
                MAX_CATALOG_KEY_LEN,
                false,
            )?;
            if !catalog_keys.insert(model.catalog_key.as_str()) {
                return Err(format!(
                    "channel {} repeats catalog key {}",
                    channel.channel_code, model.catalog_key
                ));
            }
            if let Some(badge) = &model.badge {
                validate_text("model badge", badge, MAX_BADGE_LEN, true)?;
            }
            if let Some(model_id) = model.model_id {
                if model_id <= 0 {
                    return Err(format!(
                        "channel {} model {} modelId must be a positive int64",
                        channel.channel_code, model.catalog_key
                    ));
                }
            }
            validate_sort_order("model sortOrder", model.sort_order)?;
            if !model.config.is_object() {
                return Err(format!(
                    "channel {} model {} config must be a JSON object",
                    channel.channel_code, model.catalog_key
                ));
            }
        }
        if channel.fields.len() > MAX_CHANNEL_FIELDS {
            return Err(format!(
                "channel {} exposes at most {MAX_CHANNEL_FIELDS} fields",
                channel.channel_code
            ));
        }
        let mut field_keys = HashSet::with_capacity(channel.fields.len());
        for field in &channel.fields {
            validate_text("field key", &field.field_key, MAX_FIELD_KEY_LEN, false)?;
            validate_sort_order("field sortOrder", field.sort_order)?;
            if !field_keys.insert(field.field_key.as_str()) {
                return Err(format!(
                    "channel {} repeats field key {}",
                    channel.channel_code, field.field_key
                ));
            }
            if !field.config.is_object() {
                return Err(format!(
                    "channel {} field {} config must be a JSON object",
                    channel.channel_code, field.field_key
                ));
            }
        }
        if let Some(default_catalog_key) = channel.default_catalog_key.as_deref() {
            match channel
                .models
                .iter()
                .find(|model| model.catalog_key.as_str() == default_catalog_key)
            {
                Some(model) if model.visible => {}
                Some(_) => {
                    return Err(format!(
                        "channel {} defaultCatalogKey {} must name a visible exposed model",
                        channel.channel_code, default_catalog_key
                    ));
                }
                None => {
                    return Err(format!(
                        "channel {} defaultCatalogKey {} must be one of its exposed models",
                        channel.channel_code, default_catalog_key
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The sort-order columns are PostgreSQL `INTEGER`; reject values the database
/// cannot store instead of surfacing a 500 from the insert.
fn validate_sort_order(field: &str, value: i64) -> Result<(), String> {
    if value < i64::from(i32::MIN) || value > i64::from(i32::MAX) {
        return Err(format!("{field} must fit in a 32-bit integer"));
    }
    Ok(())
}

fn validate_channel_code(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_CHANNEL_CODE_LEN {
        return Err(format!(
            "channelCode must be 1-{MAX_CHANNEL_CODE_LEN} characters"
        ));
    }
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return Err("channelCode must not be empty".to_owned());
    };
    if !first.is_ascii_lowercase() {
        return Err("channelCode must start with a lowercase letter".to_owned());
    }
    if !characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(
            "channelCode may contain only lowercase letters, digits, and underscores".to_owned(),
        );
    }
    Ok(())
}

fn validate_text(
    field: &str,
    value: &str,
    max_len: usize,
    allow_empty: bool,
) -> Result<(), String> {
    if !allow_empty && value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > max_len {
        return Err(format!("{field} must be at most {max_len} characters"));
    }
    Ok(())
}

fn map_subject(trusted: TrustedRequestSubject) -> FrontendConfigSubject {
    FrontendConfigSubject {
        tenant_id: trusted.tenant_id,
        organization_id: trusted.organization_id,
        operator_id: trusted.operator_id,
        operator_type: trusted.operator_type,
    }
}

fn to_response(config: FrontendConfig) -> FrontendConfigResponse {
    FrontendConfigResponse {
        revision: config.revision,
        channels: config
            .channels
            .into_iter()
            .map(to_channel_response)
            .collect(),
    }
}

fn to_channel_response(channel: FrontendChannel) -> FrontendChannelResponse {
    FrontendChannelResponse {
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
        revision: channel.revision,
        models: channel.models.into_iter().map(to_model_response).collect(),
        fields: channel.fields.into_iter().map(to_field_response).collect(),
    }
}

fn to_model_response(model: FrontendChannelModel) -> FrontendChannelModelResponse {
    FrontendChannelModelResponse {
        catalog_key: model.catalog_key,
        model_id: model.model_id,
        visible: model.visible,
        featured: model.featured,
        sort_order: model.sort_order,
        badge: model.badge,
        config: model.config,
    }
}

fn to_field_response(field: FrontendChannelField) -> FrontendChannelFieldResponse {
    FrontendChannelFieldResponse {
        field_key: field.field_key,
        visible: field.visible,
        sort_order: field.sort_order,
        config: field.config,
    }
}

/// One product surface and the catalog models it exposes before a tenant
/// authors its own configuration.
///
/// The seeded database rows in `database/seeds/common/002_ai_model_frontend_defaults.sql`
/// mirror this table, so a fresh database, a saved revision, and the fallback
/// document a tenant sees before it saves all describe the same initialization
/// models. The first catalog key is the surface's default model.
struct DefaultFrontendChannel {
    channel_code: &'static str,
    display_name: &'static str,
    display_name_zh_cn: &'static str,
    capability_codes: &'static [&'static str],
    sort_order: i64,
    catalog_keys: &'static [&'static str],
}

const DEFAULT_FRONTEND_CHANNELS: &[DefaultFrontendChannel] = &[
    DefaultFrontendChannel {
        channel_code: "llm",
        display_name: "LLM",
        display_name_zh_cn: "大语言模型",
        capability_codes: &["chat", "responses", "completion"],
        sort_order: 10,
        catalog_keys: &[
            "deepseek/deepseek-flash",
            "deepseek/deepseek-v4-pro",
            "anthropic/claude-opus-5",
        ],
    },
    DefaultFrontendChannel {
        channel_code: "video",
        display_name: "Video",
        display_name_zh_cn: "视频生成",
        capability_codes: &["video_generation"],
        sort_order: 20,
        catalog_keys: &["luma_ai/ray-3.2", "minimax/MiniMax-H3", "vidu/viduq3-pro"],
    },
    DefaultFrontendChannel {
        channel_code: "image",
        display_name: "Image",
        display_name_zh_cn: "图片生成",
        capability_codes: &["image_generation", "image_editing"],
        sort_order: 30,
        catalog_keys: &[
            "google/gemini-3-pro-image",
            "openai/gpt-image-2",
            "kuaishou/kling-image-3.0",
        ],
    },
    DefaultFrontendChannel {
        channel_code: "audio",
        display_name: "Audio",
        display_name_zh_cn: "音频",
        capability_codes: &[
            "audio_input",
            "audio_output",
            "speech_to_text",
            "text_to_speech",
        ],
        sort_order: 40,
        catalog_keys: &[
            "openai/gpt-realtime-2.1",
            "google/gemini-2.5-pro-preview-tts",
            "openai/tts-1-hd",
        ],
    },
    DefaultFrontendChannel {
        channel_code: "music",
        display_name: "Music",
        display_name_zh_cn: "音乐生成",
        capability_codes: &["music_generation"],
        sort_order: 50,
        catalog_keys: &[
            "mureka/mureka-v9.5",
            "mureka/mureka-o2",
            "elevenlabs/music_v2_5",
        ],
    },
    DefaultFrontendChannel {
        channel_code: "sound_effect",
        display_name: "Sound Effect",
        display_name_zh_cn: "音效生成",
        capability_codes: &["sound_effect_generation"],
        sort_order: 60,
        catalog_keys: &[
            "vidu/audio1.0-timing2audio",
            "kuaishou/kling-sound-t2a",
            "vidu/audio1.0-text2audio",
        ],
    },
    DefaultFrontendChannel {
        channel_code: "embedding",
        display_name: "Embedding",
        display_name_zh_cn: "向量嵌入",
        capability_codes: &["embedding", "rerank"],
        sort_order: 70,
        catalog_keys: &[
            "openai/text-embedding-3-large",
            "alibaba/text-embedding-v4",
            "zhipu/embedding-3",
        ],
    },
];

/// The channels a tenant sees before it authors its own configuration.
fn default_frontend_channels() -> Vec<FrontendChannel> {
    DEFAULT_FRONTEND_CHANNELS
        .iter()
        .map(|channel| FrontendChannel {
            channel_code: channel.channel_code.to_owned(),
            display_name: channel.display_name.to_owned(),
            display_name_i18n: json!({
                "en-US": channel.display_name,
                "zh-CN": channel.display_name_zh_cn,
            }),
            description: None,
            enabled: true,
            sort_order: channel.sort_order,
            default_catalog_key: channel.catalog_keys.first().map(|key| (*key).to_owned()),
            capability_codes: channel
                .capability_codes
                .iter()
                .map(|code| (*code).to_owned())
                .collect(),
            config: json!({ "pageSize": 100 }),
            schema_version: "1.0.0".to_owned(),
            revision: 0,
            models: default_frontend_channel_models(channel.catalog_keys),
            fields: default_frontend_fields(),
        })
        .collect()
}

/// The initialization models exposed by one surface. The first entry is the
/// default model and is marked featured so the admin page can highlight it.
fn default_frontend_channel_models(catalog_keys: &[&str]) -> Vec<FrontendChannelModel> {
    catalog_keys
        .iter()
        .enumerate()
        .map(|(index, catalog_key)| FrontendChannelModel {
            catalog_key: (*catalog_key).to_owned(),
            model_id: None,
            visible: true,
            featured: index == 0,
            sort_order: (index as i64 + 1) * 10,
            badge: None,
            config: json!({}),
        })
        .collect()
}

fn default_frontend_fields() -> Vec<FrontendChannelField> {
    ["price", "context", "modalities", "capabilities"]
        .into_iter()
        .enumerate()
        .map(|(index, key)| FrontendChannelField {
            field_key: key.to_owned(),
            visible: true,
            sort_order: (index as i64 + 1) * 10,
            config: json!({}),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::body::to_bytes;
    use axum::extract::State;
    use sdkwork_cloudrouter_http::TrustedRequestSubject;
    use sdkwork_models_contract_service::{
        DomainResult, EntityUuidGenerator, FrontendConfigFuture,
    };
    use sdkwork_web_core::{
        ServerRequestId, WebApiSurface, WebAuthMode, WebRequestContext, WebTransportFacts,
    };
    use serde_json::json;

    use super::*;

    struct EmptyFrontendConfigStore;

    impl FrontendConfigStore for EmptyFrontendConfigStore {
        fn load_frontend_config<'a>(
            &'a self,
            _query: LoadFrontendConfigQuery,
        ) -> FrontendConfigFuture<'a, FrontendConfig> {
            Box::pin(async {
                Ok(FrontendConfig {
                    revision: 0,
                    channels: Vec::new(),
                })
            })
        }

        fn replace_frontend_config<'a>(
            &'a self,
            _command: ReplaceFrontendConfigCommand,
        ) -> FrontendConfigFuture<'a, FrontendConfig> {
            Box::pin(async {
                Ok(FrontendConfig {
                    revision: 1,
                    channels: Vec::new(),
                })
            })
        }
    }

    struct StaticFrontendConfigStore(FrontendConfig);

    impl FrontendConfigStore for StaticFrontendConfigStore {
        fn load_frontend_config<'a>(
            &'a self,
            _query: LoadFrontendConfigQuery,
        ) -> FrontendConfigFuture<'a, FrontendConfig> {
            let config = self.0.clone();
            Box::pin(async move { Ok(config) })
        }

        fn replace_frontend_config<'a>(
            &'a self,
            _command: ReplaceFrontendConfigCommand,
        ) -> FrontendConfigFuture<'a, FrontendConfig> {
            let config = self.0.clone();
            Box::pin(async move { Ok(config) })
        }
    }

    #[derive(Default)]
    struct CapturingFrontendConfigStore {
        commands: Mutex<Vec<ReplaceFrontendConfigCommand>>,
    }

    impl FrontendConfigStore for CapturingFrontendConfigStore {
        fn load_frontend_config<'a>(
            &'a self,
            _query: LoadFrontendConfigQuery,
        ) -> FrontendConfigFuture<'a, FrontendConfig> {
            Box::pin(async {
                Ok(FrontendConfig {
                    revision: 0,
                    channels: Vec::new(),
                })
            })
        }

        fn replace_frontend_config<'a>(
            &'a self,
            command: ReplaceFrontendConfigCommand,
        ) -> FrontendConfigFuture<'a, FrontendConfig> {
            self.commands
                .lock()
                .expect("capture replace commands")
                .push(command);
            Box::pin(async {
                Ok(FrontendConfig {
                    revision: 1,
                    channels: Vec::new(),
                })
            })
        }
    }

    struct FixedUuidGenerator;

    impl EntityUuidGenerator for FixedUuidGenerator {
        fn generate_entity_uuid(&self) -> DomainResult<String> {
            Ok("test-entity-uuid".to_owned())
        }
    }

    fn trusted_subject() -> TrustedRequestSubject {
        TrustedRequestSubject {
            tenant_id: 100_001,
            organization_id: 0,
            user_id: 1,
            operator_id: 1,
            operator_type: 1,
        }
    }

    fn test_context() -> WebRequestContext {
        WebRequestContext {
            request_id: ServerRequestId("test-request".to_owned()),
            api_surface: WebApiSurface::BackendApi,
            auth_mode: WebAuthMode::DualToken,
            transport: WebTransportFacts {
                path: "/backend/v3/api/ai/frontend_config".to_owned(),
                method: "GET".to_owned(),
                auth_token_present: true,
                access_token_present: true,
                api_key_present: false,
                ingress_token_present: false,
                oauth_bearer_present: false,
                agent_token_present: false,
            },
            principal: None,
            locale: None,
            client_kind: None,
            operation: None,
            trace_id: None,
            idempotency_key: None,
        }
    }

    #[tokio::test]
    async fn admin_read_serves_the_initialization_fallback_without_tenant_rows() {
        let state = FrontendConfigState {
            store: Arc::new(EmptyFrontendConfigStore),
            entity_uuid_generator: None,
        };
        let response = fetch_frontend_config(
            test_context(),
            State(state),
            trusted_subject(),
            HeaderMap::new(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read response body");
        let value: Value = serde_json::from_slice(&body).expect("parse response body");
        assert_eq!(value["code"], 0);
        let channels = value["data"]["channels"]
            .as_array()
            .expect("channels array");
        assert_eq!(channels.len(), 7);
        for channel in channels {
            assert_eq!(
                channel["models"].as_array().map(Vec::len),
                Some(3),
                "the fallback document must expose initialization models"
            );
            assert!(
                channel["defaultCatalogKey"].is_string(),
                "the fallback document must name a default model"
            );
        }
    }

    #[tokio::test]
    async fn app_read_emits_an_etag_and_honors_if_none_match() {
        let state = FrontendConfigState {
            store: Arc::new(StaticFrontendConfigStore(FrontendConfig {
                revision: 7,
                channels: default_frontend_channels(),
            })),
            entity_uuid_generator: None,
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::IF_NONE_MATCH,
            HeaderValue::from_static("\"fmc-7\""),
        );
        let not_modified =
            fetch_app_frontend_config(test_context(), State(state.clone()), None, headers).await;
        assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            not_modified
                .headers()
                .get(axum::http::header::ETAG)
                .and_then(|value| value.to_str().ok()),
            Some("\"fmc-7\"")
        );

        let response =
            fetch_app_frontend_config(test_context(), State(state), None, HeaderMap::new()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::ETAG)
                .and_then(|value| value.to_str().ok()),
            Some("\"fmc-7\"")
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read app response body");
        let value: Value = serde_json::from_slice(&body).expect("parse app response body");
        assert_eq!(value["code"], 0);
        assert_eq!(value["data"]["channels"].as_array().map(Vec::len), Some(7));
    }

    #[tokio::test]
    async fn replace_handler_builds_the_command_from_the_wire_document() {
        let store = Arc::new(CapturingFrontendConfigStore::default());
        let state = FrontendConfigState {
            store: store.clone(),
            entity_uuid_generator: Some(Arc::new(FixedUuidGenerator)),
        };
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "channels": [{
                    "channelCode": "llm",
                    "displayName": "LLM",
                    "enabled": true,
                    "sortOrder": 10,
                    "defaultCatalogKey": "vendor/model",
                    "capabilityCodes": ["chat"],
                    "schemaVersion": "1.0.0",
                    "models": [{
                        "catalogKey": "vendor/model",
                        "modelId": "42",
                        "visible": true,
                        "featured": true,
                        "sortOrder": 10
                    }],
                    "fields": [{ "fieldKey": "price", "visible": true, "sortOrder": 10 }]
                }],
                "source": "test"
            }))
            .expect("serialize replace request"),
        );
        let response = replace_frontend_config(
            test_context(),
            State(state),
            trusted_subject(),
            HeaderMap::new(),
            body,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);

        let commands = store.commands.lock().expect("capture replace commands");
        assert_eq!(commands.len(), 1);
        let command = &commands[0];
        assert_eq!(command.source, "test");
        assert_eq!(command.channels.len(), 1);
        let channel = &command.channels[0];
        assert_eq!(channel.channel_code, "llm");
        assert_eq!(
            channel.display_name_i18n,
            json!({}),
            "an omitted displayNameI18n must persist as an empty object"
        );
        assert_eq!(channel.default_catalog_key.as_deref(), Some("vendor/model"));
        assert_eq!(channel.models.len(), 1);
        assert_eq!(channel.models[0].model_id, Some(42));
        assert!(channel.models[0].featured);
        assert_eq!(channel.fields[0].field_key, "price");
    }

    #[tokio::test]
    async fn replace_handler_rejects_a_dangling_default_with_a_problem_response() {
        let state = FrontendConfigState {
            store: Arc::new(EmptyFrontendConfigStore),
            entity_uuid_generator: None,
        };
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "channels": [{
                    "channelCode": "llm",
                    "displayName": "LLM",
                    "defaultCatalogKey": "vendor/missing",
                    "models": [{ "catalogKey": "vendor/model" }]
                }]
            }))
            .expect("serialize invalid replace request"),
        );
        let response = replace_frontend_config(
            test_context(),
            State(state),
            trusted_subject(),
            HeaderMap::new(),
            body,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("application/problem+json")
        );
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read problem body");
        let value: Value = serde_json::from_slice(&body).expect("parse problem body");
        assert!(value["code"].is_number());
        assert!(value["traceId"].is_string());
    }

    #[test]
    fn if_none_match_accepts_the_exact_weak_and_wildcard_validators() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::IF_NONE_MATCH,
            axum::http::HeaderValue::from_static("\"fmc-3\""),
        );
        assert!(if_none_match_matches(&headers, "\"fmc-3\""));
        assert!(!if_none_match_matches(&headers, "\"fmc-4\""));

        headers.insert(
            axum::http::header::IF_NONE_MATCH,
            axum::http::HeaderValue::from_static("W/\"fmc-3\", \"fmc-2\""),
        );
        assert!(if_none_match_matches(&headers, "\"fmc-3\""));

        headers.insert(
            axum::http::header::IF_NONE_MATCH,
            axum::http::HeaderValue::from_static("*"),
        );
        assert!(if_none_match_matches(&headers, "\"fmc-9\""));

        headers.remove(axum::http::header::IF_NONE_MATCH);
        assert!(!if_none_match_matches(&headers, "\"fmc-3\""));
    }

    #[test]
    fn default_channels_cover_the_product_surfaces() {
        let codes: Vec<String> = default_frontend_channels()
            .into_iter()
            .map(|channel| channel.channel_code)
            .collect();
        for expected in [
            "llm",
            "video",
            "image",
            "audio",
            "music",
            "sound_effect",
            "embedding",
        ] {
            assert!(
                codes.iter().any(|code| code == expected),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn default_channels_expose_initialization_models() {
        for channel in default_frontend_channels() {
            assert!(
                !channel.models.is_empty(),
                "channel {} must expose initialization models",
                channel.channel_code
            );
            assert_eq!(
                channel.default_catalog_key.as_deref(),
                Some(channel.models[0].catalog_key.as_str()),
                "channel {} default model must be its first exposed model",
                channel.channel_code
            );
            assert!(
                channel.models.iter().all(|model| model.visible),
                "channel {} initialization models must be visible",
                channel.channel_code
            );
            assert!(
                channel.models.iter().filter(|model| model.featured).count() == 1,
                "channel {} must feature exactly one initialization model",
                channel.channel_code
            );
        }
    }

    #[test]
    fn database_seed_matches_the_fallback_initialization_document() {
        let sql =
            include_str!("../../../../database/seeds/common/002_ai_model_frontend_defaults.sql");
        for channel in default_frontend_channels() {
            let channel_code = channel.channel_code.as_str();
            assert!(
                sql.contains(&format!("'seed-fmc-channel-{channel_code}'")),
                "seed is missing the {channel_code} channel row"
            );
            let default_catalog_key = channel
                .default_catalog_key
                .as_deref()
                .expect("default catalog key");
            let capability_codes =
                serde_json::to_string(&channel.capability_codes).expect("capability codes json");
            assert!(
                sql.contains(&format!(
                    "'{default_catalog_key}', '{capability_codes}'::jsonb"
                )),
                "seed default model or capability codes drifted for {channel_code}"
            );
            let display_name_i18n =
                serde_json::to_string(&channel.display_name_i18n).expect("display name i18n json");
            assert!(
                sql.contains(&format!("'{channel_code}', '{}',", channel.display_name))
                    && sql.contains(&format!("'{display_name_i18n}'::jsonb")),
                "seed display name or i18n drifted for {channel_code}"
            );
            for (index, model) in channel.models.iter().enumerate() {
                let sort_order = (index as i64 + 1) * 10;
                let featured = if model.featured { "TRUE" } else { "FALSE" };
                assert!(
                    sql.contains(&format!(
                        "'seed-fmc-model-{channel_code}-{}', '{channel_code}', '{}', {featured}, {sort_order})",
                        index + 1,
                        model.catalog_key
                    )),
                    "seed row for {} of {channel_code} drifted",
                    model.catalog_key
                );
                assert!(
                    sql.contains(&format!("'{}'", model.catalog_key)),
                    "seed is missing the {} catalog key",
                    model.catalog_key
                );
            }
            for field in &channel.fields {
                assert!(
                    sql.contains(&format!(
                        ", '{channel_code}', '{}', {})",
                        field.field_key, field.sort_order
                    )),
                    "seed field {} of {channel_code} drifted",
                    field.field_key
                );
            }
        }
    }

    #[test]
    fn app_read_drops_hidden_rows_and_repairs_a_dangling_default() {
        let mut llm = default_frontend_channels()
            .into_iter()
            .find(|channel| channel.channel_code == "llm")
            .expect("llm defaults");
        llm.models[1].visible = false;
        llm.default_catalog_key = Some(llm.models[1].catalog_key.clone());
        llm.fields[0].visible = false;
        let mut disabled = default_frontend_channels()
            .into_iter()
            .find(|channel| channel.channel_code == "video")
            .expect("video defaults");
        disabled.enabled = false;

        let visible = app_visible_channels(vec![llm, disabled]);
        assert_eq!(visible.len(), 1);
        let llm = &visible[0];
        assert!(llm.models.iter().all(|model| model.visible));
        assert!(llm.fields.iter().all(|field| field.visible));
        assert_eq!(
            llm.default_catalog_key.as_deref(),
            Some("deepseek/deepseek-flash"),
            "a hidden default must be replaced by the first visible model"
        );
    }

    #[test]
    fn replace_rejects_a_default_model_outside_the_exposed_models() {
        let request: FrontendConfigReplaceRequest = serde_json::from_value(json!({
            "channels": [{
                "channelCode": "llm",
                "displayName": "LLM",
                "defaultCatalogKey": "vendor/missing",
                "models": [{ "catalogKey": "vendor/model" }]
            }]
        }))
        .expect("deserialize replace request");
        let error = validate_request(&request).expect_err("dangling default must be rejected");
        assert!(
            error.contains("defaultCatalogKey"),
            "unexpected error: {error}"
        );

        let valid: FrontendConfigReplaceRequest = serde_json::from_value(json!({
            "channels": [{
                "channelCode": "llm",
                "displayName": "LLM",
                "defaultCatalogKey": "vendor/model",
                "models": [{ "catalogKey": "vendor/model" }]
            }]
        }))
        .expect("deserialize replace request");
        validate_request(&valid).expect("default model inside the exposed set must be accepted");

        let invisible_default: FrontendConfigReplaceRequest = serde_json::from_value(json!({
            "channels": [{
                "channelCode": "llm",
                "displayName": "LLM",
                "defaultCatalogKey": "vendor/model",
                "models": [{ "catalogKey": "vendor/model", "visible": false }]
            }]
        }))
        .expect("deserialize replace request");
        validate_request(&invisible_default)
            .expect_err("an invisible default model must be rejected");
    }

    #[test]
    fn replace_rejects_invalid_wire_shapes() {
        let huge_sort: FrontendConfigReplaceRequest = serde_json::from_value(json!({
            "channels": [{
                "channelCode": "llm",
                "displayName": "LLM",
                "sortOrder": i64::MAX
            }]
        }))
        .expect("deserialize replace request");
        validate_request(&huge_sort).expect_err("out-of-range sort order must be rejected");

        let bad_i18n: FrontendConfigReplaceRequest = serde_json::from_value(json!({
            "channels": [{
                "channelCode": "llm",
                "displayName": "LLM",
                "displayNameI18n": "oops"
            }]
        }))
        .expect("deserialize replace request");
        validate_request(&bad_i18n).expect_err("non-object displayNameI18n must be rejected");

        let bad_model_id: FrontendConfigReplaceRequest = serde_json::from_value(json!({
            "channels": [{
                "channelCode": "llm",
                "displayName": "LLM",
                "models": [{ "catalogKey": "vendor/model", "modelId": "0" }]
            }]
        }))
        .expect("deserialize replace request");
        validate_request(&bad_model_id).expect_err("non-positive model id must be rejected");
    }

    #[test]
    fn channel_model_id_crosses_the_wire_as_a_decimal_string() {
        let response = FrontendChannelModelResponse {
            catalog_key: "deepseek/deepseek-flash".to_owned(),
            model_id: Some(42),
            visible: true,
            featured: true,
            sort_order: 10,
            badge: None,
            config: json!({}),
        };
        let value = serde_json::to_value(&response).expect("serialize model response");
        assert_eq!(value["modelId"], json!("42"));

        let request: FrontendChannelModelRequest = serde_json::from_value(json!({
            "catalogKey": "deepseek/deepseek-flash",
            "modelId": "42",
        }))
        .expect("deserialize model request");
        assert_eq!(request.model_id, Some(42));

        let without_id: FrontendChannelModelRequest = serde_json::from_value(json!({
            "catalogKey": "deepseek/deepseek-flash",
        }))
        .expect("deserialize model request without id");
        assert_eq!(without_id.model_id, None);
    }
}
