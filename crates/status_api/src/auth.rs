use crate::error::StatusApiError;
use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::HeaderMap;
use execution_core::{OperatorPrincipal, OperatorRole, TenantId};
use platform_auth::{
    constant_time_eq, env_bool, env_var_opt, extract_bearer_token, header_opt, operator_role_name,
    parse_kv_map, parse_operator_role_label, parse_principal_role_map, parse_principal_tenant_map,
    principal_tenant_allowed, resolve_principal_binding,
};
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Debug, Clone)]
pub struct RequestIdentity {
    pub tenant_id: TenantId,
    pub principal: OperatorPrincipal,
    pub request_id: Option<String>,
}

#[async_trait]
impl<S> FromRequestParts<S> for RequestIdentity
where
    S: Send + Sync,
{
    type Rejection = StatusApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let tenant_id = header_required(parts, "x-tenant-id")?;
        let principal_id = header_required(parts, "x-principal-id")?;
        let role = parts
            .headers
            .get("x-principal-role")
            .and_then(|v| v.to_str().ok())
            .map(parse_role)
            .transpose()?
            .unwrap_or(OperatorRole::Viewer);
        let request_id = header_optional(parts, "x-request-id");

        Ok(Self {
            tenant_id: TenantId::from(tenant_id),
            principal: OperatorPrincipal { principal_id, role },
            request_id,
        })
    }
}

pub trait StatusAuthorizer: Send + Sync {
    fn can_view_tenant(&self, principal: &OperatorPrincipal, tenant_id: &TenantId) -> bool;
    fn can_replay(&self, principal: &OperatorPrincipal, tenant_id: &TenantId) -> bool;
    fn can_manage_reconciliation(
        &self,
        principal: &OperatorPrincipal,
        tenant_id: &TenantId,
    ) -> bool;
    fn can_manage_exception_case(
        &self,
        principal: &OperatorPrincipal,
        tenant_id: &TenantId,
    ) -> bool;
    fn can_resolve_exception_case(
        &self,
        principal: &OperatorPrincipal,
        tenant_id: &TenantId,
    ) -> bool;
}

#[derive(Debug, Clone, Default)]
pub struct RoleBasedStatusAuthorizer;

impl StatusAuthorizer for RoleBasedStatusAuthorizer {
    fn can_view_tenant(&self, _principal: &OperatorPrincipal, _tenant_id: &TenantId) -> bool {
        true
    }

    fn can_replay(&self, principal: &OperatorPrincipal, _tenant_id: &TenantId) -> bool {
        matches!(principal.role, OperatorRole::Admin)
    }

    fn can_manage_reconciliation(
        &self,
        principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        matches!(principal.role, OperatorRole::Operator | OperatorRole::Admin)
    }

    fn can_manage_exception_case(
        &self,
        principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        matches!(principal.role, OperatorRole::Operator | OperatorRole::Admin)
    }

    fn can_resolve_exception_case(
        &self,
        principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        matches!(principal.role, OperatorRole::Admin)
    }
}

#[derive(Debug, Clone)]
pub struct StatusAuthConfig {
    pub global_bearer_token: Option<String>,
    pub tenant_bearer_tokens: HashMap<String, String>,
    pub principal_roles: HashMap<String, OperatorRole>,
    pub principal_tenants: HashMap<String, HashSet<String>>,
    pub require_bearer_auth: bool,
    pub require_principal_role_binding: bool,
    pub require_principal_tenant_binding: bool,
    pub redact_failure_provider_details_for_viewer: bool,
    pub redact_callback_error_details_for_viewer: bool,
}

impl Default for StatusAuthConfig {
    fn default() -> Self {
        Self {
            global_bearer_token: None,
            tenant_bearer_tokens: HashMap::new(),
            principal_roles: HashMap::new(),
            principal_tenants: HashMap::new(),
            require_bearer_auth: true,
            require_principal_role_binding: true,
            require_principal_tenant_binding: true,
            redact_failure_provider_details_for_viewer: true,
            redact_callback_error_details_for_viewer: true,
        }
    }
}

impl StatusAuthConfig {
    pub fn from_env() -> Self {
        let cfg = Self {
            global_bearer_token: env_var_opt("STATUS_API_BEARER_TOKEN"),
            tenant_bearer_tokens: parse_kv_map(env_var_opt("STATUS_API_TENANT_TOKENS").as_deref()),
            principal_roles: parse_principal_role_map(
                env_var_opt("STATUS_API_PRINCIPAL_ROLE_BINDINGS").as_deref(),
            ),
            principal_tenants: parse_principal_tenant_map(
                env_var_opt("STATUS_API_PRINCIPAL_TENANT_BINDINGS").as_deref(),
            ),
            require_bearer_auth: env_bool("STATUS_API_REQUIRE_BEARER_AUTH", true),
            require_principal_role_binding: env_bool(
                "STATUS_API_REQUIRE_PRINCIPAL_ROLE_BINDING",
                true,
            ),
            require_principal_tenant_binding: env_bool(
                "STATUS_API_REQUIRE_PRINCIPAL_TENANT_BINDING",
                true,
            ),
            redact_failure_provider_details_for_viewer: env_bool(
                "STATUS_API_REDACT_FAILURE_PROVIDER_DETAILS_FOR_VIEWER",
                true,
            ),
            redact_callback_error_details_for_viewer: env_bool(
                "STATUS_API_REDACT_CALLBACK_ERROR_DETAILS_FOR_VIEWER",
                true,
            ),
        };

        cfg
    }

    pub fn authenticate(
        &self,
        identity: &RequestIdentity,
        headers: &HeaderMap,
    ) -> Result<(), StatusApiError> {
        self.authenticate_bearer(identity.tenant_id.as_str(), headers)?;
        self.enforce_principal_role(identity)?;
        self.enforce_principal_tenant(identity)?;
        Ok(())
    }

    pub fn should_redact_failure_provider_details(&self, role: OperatorRole) -> bool {
        self.redact_failure_provider_details_for_viewer && matches!(role, OperatorRole::Viewer)
    }

    pub fn should_redact_callback_error_details(&self, role: OperatorRole) -> bool {
        self.redact_callback_error_details_for_viewer && matches!(role, OperatorRole::Viewer)
    }

    fn authenticate_bearer(
        &self,
        tenant_id: &str,
        headers: &HeaderMap,
    ) -> Result<(), StatusApiError> {
        if !self.require_bearer_auth {
            return Ok(());
        }

        let token = extract_bearer_token(headers)
            .ok_or_else(|| StatusApiError::Unauthorized("missing bearer token".to_owned()))?;

        if let Some(expected) = self.tenant_bearer_tokens.get(tenant_id) {
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            if env_bool("STATUS_API_DEBUG_AUTH_HEADER_ENABLED", false) {
                tracing::warn!(
                    tenant_id = tenant_id,
                    authorization = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    host = headers
                        .get("host")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_for = headers
                        .get("x-forwarded-for")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_proto = headers
                        .get("x-forwarded-proto")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    "status bearer auth mismatch against tenant token"
                );
            }
            return Err(StatusApiError::Unauthorized(debug_invalid_bearer_message(
                headers,
            )));
        }

        if let Some(expected) = self.global_bearer_token.as_ref() {
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            if env_bool("STATUS_API_DEBUG_AUTH_HEADER_ENABLED", false) {
                tracing::warn!(
                    tenant_id = tenant_id,
                    authorization = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    host = headers
                        .get("host")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_for = headers
                        .get("x-forwarded-for")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_proto = headers
                        .get("x-forwarded-proto")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    "status bearer auth mismatch against global token"
                );
            }
            return Err(StatusApiError::Unauthorized(debug_invalid_bearer_message(
                headers,
            )));
        }

        Err(StatusApiError::Unauthorized(
            "no status-api bearer token configured for tenant".to_owned(),
        ))
    }

    fn enforce_principal_role(&self, identity: &RequestIdentity) -> Result<(), StatusApiError> {
        let principal_id = identity.principal.principal_id.as_str();
        let actual_role = identity.principal.role;

        match resolve_principal_binding(&self.principal_roles, principal_id) {
            Some(expected_role) if *expected_role == actual_role => Ok(()),
            Some(expected_role) => Err(StatusApiError::Forbidden(format!(
                "principal `{}` role mismatch: expected `{}` got `{}`",
                principal_id,
                operator_role_name(*expected_role),
                operator_role_name(actual_role)
            ))),
            None if self.require_principal_role_binding => Err(StatusApiError::Forbidden(format!(
                "principal `{}` is not mapped to any role",
                principal_id
            ))),
            None => Ok(()),
        }
    }

    fn enforce_principal_tenant(&self, identity: &RequestIdentity) -> Result<(), StatusApiError> {
        let principal_id = identity.principal.principal_id.as_str();
        let tenant_id = identity.tenant_id.as_str();

        match resolve_principal_binding(&self.principal_tenants, principal_id) {
            Some(tenants) if principal_tenant_allowed(tenants, tenant_id) => Ok(()),
            Some(_) => Err(StatusApiError::Forbidden(format!(
                "principal `{}` is not allowed for tenant `{}`",
                principal_id, tenant_id
            ))),
            None if self.require_principal_tenant_binding => Err(StatusApiError::Forbidden(
                format!("principal `{}` is not bound to any tenant", principal_id),
            )),
            None => Ok(()),
        }
    }
}

fn debug_invalid_bearer_message(headers: &HeaderMap) -> String {
    if !env_bool("STATUS_API_DEBUG_AUTH_HEADER_ENABLED", false) {
        return "invalid bearer token".to_owned();
    }

    format!(
        "invalid bearer token; authorization={}; host={}; x-forwarded-for={}; x-forwarded-proto={}",
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>"),
        headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>"),
        headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>"),
        headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>")
    )
}

fn header_required(parts: &Parts, name: &str) -> Result<String, StatusApiError> {
    header_opt(&parts.headers, name)
        .ok_or_else(|| StatusApiError::Unauthorized(format!("missing required header `{name}`")))
}

fn header_optional(parts: &Parts, name: &str) -> Option<String> {
    header_opt(&parts.headers, name)
}

fn parse_role(value: &str) -> Result<OperatorRole, StatusApiError> {
    parse_operator_role_label(value.trim()).ok_or_else(|| {
        StatusApiError::Unauthorized(format!("invalid x-principal-role `{}`", Printable(value)))
    })
}

struct Printable<'a>(&'a str);

impl fmt::Display for Printable<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
