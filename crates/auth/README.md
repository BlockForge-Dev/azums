# platform_auth

Shared authentication and authorization helpers used by ingress/status surfaces.

This crate currently centralizes:

- Header helpers (`header_opt`, `extract_bearer_token`)
- Constant-time token comparison (`constant_time_eq`)
- Auth-related env parsing (`env_var_opt`, `env_bool`)
- Binding map parsing (`parse_kv_map`, `parse_principal_tenant_map`, `parse_principal_role_map`)
- Operator-role label conversion helpers
