use crate::domain::{DomainError, DomainResult, GatewayApiKey};
use crate::ports::PricingCatalog;

pub trait ApiKeySecretHasher {
    fn hash_secret(&self, secret: &str) -> DomainResult<String>;

    /// Every hash that should be probed during authentication, most likely
    /// first.
    ///
    /// A key-ring implementation returns the active generation first and the
    /// retired generations after it, so rotating the pepper does not invalidate
    /// API keys issued under the retired one. The default keeps single-pepper
    /// behavior.
    fn candidate_hashes(&self, secret: &str) -> DomainResult<Vec<String>> {
        Ok(vec![self.hash_secret(secret)?])
    }
}

pub struct ApiKeyAuthenticator<'a, C, H>
where
    C: PricingCatalog,
    H: ApiKeySecretHasher + ?Sized,
{
    catalog: &'a C,
    hasher: &'a H,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthenticateApiKeyQuery<'a> {
    pub credential_secret: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedApiKeyContext {
    pub api_key_id: i64,
    pub tenant_id: i64,
    pub organization_id: i64,
    pub user_id: i64,
    pub api_key_name_snapshot: String,
    pub group_id: i64,
    pub group_code: String,
    pub pricing_plan_code: String,
}

impl<'a, C, H> ApiKeyAuthenticator<'a, C, H>
where
    C: PricingCatalog,
    H: ApiKeySecretHasher + ?Sized,
{
    pub fn new(catalog: &'a C, hasher: &'a H) -> Self {
        Self { catalog, hasher }
    }

    pub fn authenticate(
        &self,
        query: AuthenticateApiKeyQuery<'_>,
    ) -> DomainResult<AuthenticatedApiKeyContext> {
        let key_hashes = self.hasher.candidate_hashes(query.credential_secret)?;
        let api_key = self.find_api_key(&key_hashes)?;
        let group = self
            .catalog
            .find_upstream_account_group(api_key.default_account_group_id)
            .ok_or_else(|| DomainError::new("account group is not available"))?;

        Ok(AuthenticatedApiKeyContext {
            api_key_id: api_key.id,
            tenant_id: api_key.tenant_id,
            organization_id: api_key.organization_id,
            user_id: api_key.user_id,
            api_key_name_snapshot: api_key.display_name(),
            group_id: group.id,
            group_code: group.code,
            pricing_plan_code: group.pricing_plan_code,
        })
    }

    fn find_api_key(&self, key_hashes: &[String]) -> DomainResult<GatewayApiKey> {
        key_hashes
            .iter()
            .find_map(|key_hash| self.catalog.find_api_key_by_hash(key_hash))
            .ok_or_else(|| DomainError::new("api key credential is invalid"))
    }
}
