use std::future::Future;
use std::pin::Pin;

use serde_json::Value;

use crate::DomainResult;

pub type FrontendConfigFuture<'a, T> = Pin<Box<dyn Future<Output = DomainResult<T>> + Send + 'a>>;

/// Tenant scope of one front-end configuration read or replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontendConfigSubject {
    pub tenant_id: i64,
    pub organization_id: i64,
    pub operator_id: i64,
    pub operator_type: i32,
}

/// One model exposed in a front-end channel.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendChannelModel {
    pub catalog_key: String,
    pub model_id: Option<i64>,
    pub visible: bool,
    pub featured: bool,
    pub sort_order: i64,
    pub badge: Option<String>,
    pub config: Value,
}

/// One field shown by a front-end channel.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendChannelField {
    pub field_key: String,
    pub visible: bool,
    pub sort_order: i64,
    pub config: Value,
}

/// One complete front-end channel as served to the admin and app surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendChannel {
    pub channel_code: String,
    pub display_name: String,
    pub display_name_i18n: Value,
    pub description: Option<String>,
    pub enabled: bool,
    pub sort_order: i64,
    pub default_catalog_key: Option<String>,
    pub capability_codes: Vec<String>,
    pub config: Value,
    pub schema_version: String,
    pub revision: i64,
    pub models: Vec<FrontendChannelModel>,
    pub fields: Vec<FrontendChannelField>,
}

/// The revisioned document served by the models backend.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendConfig {
    pub revision: i64,
    pub channels: Vec<FrontendChannel>,
}

/// Query for one tenant's front-end configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadFrontendConfigQuery {
    pub subject: FrontendConfigSubject,
}

/// One channel in a replace request, with server-assigned row UUIDs.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendChannelCommand {
    pub uuid: String,
    pub channel_code: String,
    pub display_name: String,
    pub display_name_i18n: Value,
    pub description: Option<String>,
    pub enabled: bool,
    pub sort_order: i64,
    pub default_catalog_key: Option<String>,
    pub capability_codes: Vec<String>,
    pub config: Value,
    pub schema_version: String,
    pub models: Vec<FrontendChannelModelCommand>,
    pub fields: Vec<FrontendChannelFieldCommand>,
}

/// One model row in a replace request.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendChannelModelCommand {
    pub uuid: String,
    pub catalog_key: String,
    pub model_id: Option<i64>,
    pub visible: bool,
    pub featured: bool,
    pub sort_order: i64,
    pub badge: Option<String>,
    pub config: Value,
}

/// One field row in a replace request.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontendChannelFieldCommand {
    pub uuid: String,
    pub field_key: String,
    pub visible: bool,
    pub sort_order: i64,
    pub config: Value,
}

/// A transactional replacement of one tenant's front-end configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceFrontendConfigCommand {
    pub subject: FrontendConfigSubject,
    pub channels: Vec<FrontendChannelCommand>,
    pub source: String,
    pub request_id: String,
    pub snapshot_uuid: String,
    pub requested_at: String,
}

/// Storage port for the front-end model configuration tables.
pub trait FrontendConfigStore {
    fn load_frontend_config<'a>(
        &'a self,
        query: LoadFrontendConfigQuery,
    ) -> FrontendConfigFuture<'a, FrontendConfig>;

    fn replace_frontend_config<'a>(
        &'a self,
        command: ReplaceFrontendConfigCommand,
    ) -> FrontendConfigFuture<'a, FrontendConfig>;
}
