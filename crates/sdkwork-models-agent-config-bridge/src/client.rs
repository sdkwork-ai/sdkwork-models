//! HTTP client for the sdkwork-agents Config SPI app-api surface.
//!
//! Request bodies mirror the agents app-api DTOs exactly (camelCase, int64
//! settings transported as strings, `deny_unknown_fields` compatible).

use serde::{Deserialize, Serialize};
use std::fmt;

/// Apply request body for `POST /app/v3/api/ai/model_configurations/apply`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyModelConfigurationRequest {
    pub configuration_id: String,
    pub engine_id: String,
    pub vendor_code: String,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub default_model_id: String,
    pub supported_model_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_provider_ids: Vec<String>,
    /// The agents Config SPI transports int64 settings as JSON strings.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_int64_string")]
    pub input_context_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_int64_string")]
    pub output_context_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_int64_string")]
    pub tool_call_rounds: Option<i64>,
    #[serde(default)]
    pub supports_multimodal: bool,
}

impl ApplyModelConfigurationRequest {
    /// Builds the apply request from a client-local engine configuration.
    pub fn from_engine_config(
        configuration_id: impl Into<String>,
        engine_id: impl Into<String>,
        config: &sdkwork_models_user_config_repository_sqlx::UserModelEngineConfig,
        api_key: Option<String>,
    ) -> Self {
        Self {
            configuration_id: configuration_id.into(),
            engine_id: engine_id.into(),
            vendor_code: config.vendor_code.clone(),
            base_url: config.base_url.clone(),
            api_key,
            default_model_id: config.default_model_id.clone(),
            supported_model_ids: config.supported_model_ids.clone(),
            supported_provider_ids: config.supported_provider_ids.clone(),
            input_context_tokens: config.input_context_tokens,
            output_context_tokens: config.output_context_tokens,
            tool_call_rounds: config.tool_call_rounds,
            supports_multimodal: config.supports_multimodal,
        }
    }
}

/// Apply request body for `POST /app/v3/api/ai/model_selections/apply`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelectionApplyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configuration_id: Option<String>,
    pub engine_id: String,
    pub model_id: String,
}

/// Applied configuration result extracted from the agents envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedModelConfiguration {
    pub profile_id: String,
    pub api_key_configured: bool,
}

/// Applied selection result extracted from the agents envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelectionApplyResponse {
    pub profile_id: String,
    pub model_id: String,
}

/// Errors raised while pushing a configuration to the agents Config SPI.
#[derive(Debug)]
pub enum ApplyModelConfigurationError {
    /// The agents surface rejected the request with a non-2xx status.
    Response(u16, String),
    /// The agents envelope reported a non-zero result code.
    Envelope(i64, String),
    /// The response body could not be interpreted.
    Parse(String),
    /// Transport failure.
    Transport(reqwest::Error),
}

impl fmt::Display for ApplyModelConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Response(status, body) => {
                write!(formatter, "agents Config SPI rejected the push (HTTP {status}): {body}")
            }
            Self::Envelope(code, body) => {
                write!(formatter, "agents Config SPI returned code {code}: {body}")
            }
            Self::Parse(message) => write!(formatter, "agents Config SPI response could not be parsed: {message}"),
            Self::Transport(error) => write!(formatter, "agents Config SPI push failed: {error}"),
        }
    }
}

impl std::error::Error for ApplyModelConfigurationError {}

impl From<reqwest::Error> for ApplyModelConfigurationError {
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error)
    }
}

/// Agents app-api response envelope (`{ code, data, traceId }`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkWorkApiEnvelope<T> {
    code: i64,
    data: Option<T>,
}

/// `{ data: { item: { ... } } }` resource payload.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceEnvelope<T> {
    item: T,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppliedItem {
    profile_id: String,
    api_key_configured: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedItem {
    profile_id: String,
    model_id: String,
}

/// HTTP client for the agents app-api Config SPI surface.
///
/// identity:service background-engine-config — this bridge runs in the models
/// engine bootstrap and pushes configuration, not an end-user request; the token
/// is supplied explicitly by the composition root via `with_auth_token` and is
/// never read from a process `*_ACCESS_TOKEN` / `*_AUTH_TOKEN` value here.
#[derive(Debug, Clone)]
pub struct ModelConfigBridgeClient {
    http: reqwest::Client,
    /// Agents app-api origin, e.g. `http://127.0.0.1:8080`.
    base_url: String,
    /// Optional bearer credential for the agents app-api surface.
    auth_token: Option<String>,
}

impl ModelConfigBridgeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            auth_token: None,
        }
    }

    /// Attaches `Authorization: Bearer <token>` to every push.
    pub fn with_auth_token(mut self, auth_token: impl Into<String>) -> Self {
        self.auth_token = Some(auth_token.into());
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The agents app-api origin to dial for one request.
    ///
    /// `APP_SDK_INTEGRATION_SPEC.md` §5.2.2: a backend calling another SDKWork
    /// surface resolves through the shared registry component, so a healthy
    /// registry instance wins and the configured origin is the degradation tier.
    fn resolved_base_url(&self) -> String {
        sdkwork_utils_rust::service_base_url::registry_base_url_or("sdkwork-agents", &self.base_url)
    }

    /// Pushes an engine model configuration (materializes the provider CLI
    /// config on the agents side).
    pub async fn apply_configuration(
        &self,
        request: &ApplyModelConfigurationRequest,
    ) -> Result<AppliedModelConfiguration, ApplyModelConfigurationError> {
        let mut builder = self
            .http
            .post(format!(
                "{}/app/v3/api/ai/model_configurations/apply",
                self.resolved_base_url()
            ))
            .json(request);
        if let Some(token) = &self.auth_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(ApplyModelConfigurationError::Response(status.as_u16(), body));
        }
        let envelope: SdkWorkApiEnvelope<ResourceEnvelope<AppliedItem>> =
            serde_json::from_str(&body).map_err(|error| {
                ApplyModelConfigurationError::Parse(format!("{error}: {body}"))
            })?;
        if envelope.code != 0 {
            return Err(ApplyModelConfigurationError::Envelope(envelope.code, body));
        }
        let item = envelope.data.ok_or_else(|| {
            ApplyModelConfigurationError::Parse("envelope carries no data item".to_string())
        })?;
        Ok(AppliedModelConfiguration {
            profile_id: item.item.profile_id,
            api_key_configured: item.item.api_key_configured,
        })
    }

    /// Pushes an engine model selection (switches the provider's model).
    pub async fn apply_selection(
        &self,
        request: &ModelSelectionApplyRequest,
    ) -> Result<ModelSelectionApplyResponse, ApplyModelConfigurationError> {
        let mut builder = self
            .http
            .post(format!(
                "{}/app/v3/api/ai/model_selections/apply",
                self.resolved_base_url()
            ))
            .json(request);
        if let Some(token) = &self.auth_token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(ApplyModelConfigurationError::Response(status.as_u16(), body));
        }
        let envelope: SdkWorkApiEnvelope<ResourceEnvelope<SelectedItem>> =
            serde_json::from_str(&body).map_err(|error| {
                ApplyModelConfigurationError::Parse(format!("{error}: {body}"))
            })?;
        if envelope.code != 0 {
            return Err(ApplyModelConfigurationError::Envelope(envelope.code, body));
        }
        let item = envelope.data.ok_or_else(|| {
            ApplyModelConfigurationError::Parse("envelope carries no data item".to_string())
        })?;
        Ok(ModelSelectionApplyResponse {
            profile_id: item.item.profile_id,
            model_id: item.item.model_id,
        })
    }
}

/// Serializes `Option<i64>` as an optional JSON string (the agents Config SPI
/// transports int64 settings as strings).
mod optional_int64_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => serializer.serialize_str(&value.to_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<String>::deserialize(deserializer)?;
        match value {
            Some(value) => value
                .parse::<i64>()
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_request_serializes_int64_settings_as_strings() {
        let config = sdkwork_models_user_config_repository_sqlx::UserModelEngineConfig {
            engine_id: "codex".to_string(),
            channel_code: "team-relay".to_string(),
            vendor_code: "openai".to_string(),
            base_url: "https://relay.example.com/v1".to_string(),
            default_model_id: "gpt-5.6-sol".to_string(),
            supported_model_ids: vec!["gpt-5.6-sol".to_string()],
            supported_provider_ids: vec!["codex".to_string()],
            input_context_tokens: Some(1_050_000),
            output_context_tokens: Some(128_000),
            tool_call_rounds: Some(32),
            supports_multimodal: true,
            api_key_configured: true,
            applied_at: "2026-08-03T00:00:00Z".to_string(),
        };
        let request = ApplyModelConfigurationRequest::from_engine_config(
            "team-relay",
            "codex",
            &config,
            Some("secret-value".to_string()),
        );
        let json = serde_json::to_value(&request).expect("serialize");
        assert_eq!(json["configurationId"], "team-relay");
        assert_eq!(json["engineId"], "codex");
        assert_eq!(json["vendorCode"], "openai");
        assert_eq!(json["baseUrl"], "https://relay.example.com/v1");
        assert_eq!(json["apiKey"], "secret-value");
        assert_eq!(json["defaultModelId"], "gpt-5.6-sol");
        assert_eq!(json["inputContextTokens"], "1050000");
        assert_eq!(json["outputContextTokens"], "128000");
        assert_eq!(json["toolCallRounds"], "32");
        assert_eq!(json["supportsMultimodal"], true);
    }

    #[test]
    fn apply_request_omits_absent_api_key_and_tokens() {
        let config = sdkwork_models_user_config_repository_sqlx::UserModelEngineConfig {
            engine_id: "codex".to_string(),
            channel_code: "official".to_string(),
            vendor_code: "openai".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            default_model_id: "gpt-5.6-sol".to_string(),
            supported_model_ids: vec!["gpt-5.6-sol".to_string()],
            supported_provider_ids: Vec::new(),
            input_context_tokens: None,
            output_context_tokens: None,
            tool_call_rounds: None,
            supports_multimodal: false,
            api_key_configured: false,
            applied_at: "2026-08-03T00:00:00Z".to_string(),
        };
        let request = ApplyModelConfigurationRequest::from_engine_config(
            "official",
            "codex",
            &config,
            None,
        );
        let json = serde_json::to_value(&request).expect("serialize");
        assert!(json.get("apiKey").is_none());
        assert!(json.get("inputContextTokens").is_none());
        assert!(json.get("outputContextTokens").is_none());
        assert!(json.get("toolCallRounds").is_none());
        assert!(json.get("supportedProviderIds").is_none(), "empty provider ids are omitted");
    }

    #[test]
    fn selection_request_serializes_camel_case() {
        let request = ModelSelectionApplyRequest {
            configuration_id: Some("team-relay".to_string()),
            engine_id: "codex".to_string(),
            model_id: "gpt-5.6-reasoning".to_string(),
        };
        let json = serde_json::to_value(&request).expect("serialize");
        assert_eq!(json["configurationId"], "team-relay");
        assert_eq!(json["engineId"], "codex");
        assert_eq!(json["modelId"], "gpt-5.6-reasoning");
    }
}
