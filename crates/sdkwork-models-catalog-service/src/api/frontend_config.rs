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
    #[serde(default)]
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
            let visible: Vec<FrontendChannel> = channels
                .into_iter()
                .filter(|channel| channel.enabled)
                .map(|mut channel| {
                    channel.models.retain(|model| model.visible);
                    channel.fields.retain(|field| field.visible);
                    channel
                })
                .collect();
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
        channels.push(FrontendChannelCommand {
            uuid: generator.generate_entity_uuid()?,
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

/// The channels a tenant sees before it authors its own configuration.
fn default_frontend_channels() -> Vec<FrontendChannel> {
    [
        (
            "llm",
            "LLM",
            "大语言模型",
            vec!["chat", "responses", "completion"],
            10,
        ),
        ("video", "Video", "视频生成", vec!["video_generation"], 20),
        (
            "image",
            "Image",
            "图片生成",
            vec!["image_generation", "image_editing"],
            30,
        ),
        (
            "audio",
            "Audio",
            "音频",
            vec![
                "audio_input",
                "audio_output",
                "speech_to_text",
                "text_to_speech",
            ],
            40,
        ),
        ("music", "Music", "音乐生成", vec!["music_generation"], 50),
        (
            "sound_effect",
            "Sound Effect",
            "音效生成",
            vec!["sound_effect_generation"],
            60,
        ),
        (
            "embedding",
            "Embedding",
            "向量嵌入",
            vec!["embedding", "rerank"],
            70,
        ),
    ]
    .into_iter()
    .map(
        |(code, english, chinese, capabilities, sort_order)| FrontendChannel {
            channel_code: code.to_owned(),
            display_name: english.to_owned(),
            display_name_i18n: json!({ "en-US": english, "zh-CN": chinese }),
            description: None,
            enabled: true,
            sort_order,
            default_catalog_key: None,
            capability_codes: capabilities.into_iter().map(str::to_owned).collect(),
            config: json!({ "pageSize": 100 }),
            schema_version: "1.0.0".to_owned(),
            revision: 0,
            models: Vec::new(),
            fields: default_frontend_fields(),
        },
    )
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
    use super::{default_frontend_channels, if_none_match_matches};

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
        for expected in ["llm", "video", "image", "audio", "music"] {
            assert!(
                codes.iter().any(|code| code == expected),
                "missing {expected}"
            );
        }
    }
}
