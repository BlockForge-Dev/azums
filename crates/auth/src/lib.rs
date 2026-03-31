use execution_core::OperatorRole;
use http::{header::AUTHORIZATION, HeaderMap};
use std::collections::{HashMap, HashSet};

pub fn header_opt(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn env_var_opt(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "no" | "n" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

pub fn split_kv(input: &str) -> Option<(&str, &str)> {
    input.split_once('=').or_else(|| input.split_once(':'))
}

pub fn parse_kv_map(raw: Option<&str>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(raw) = raw else {
        return out;
    };

    for part in raw.split(|ch| ch == ';' || ch == ',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = split_kv(trimmed) else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if !key.is_empty() && !value.is_empty() {
            out.insert(key.to_owned(), value.to_owned());
        }
    }

    out
}

pub fn parse_principal_tenant_map(raw: Option<&str>) -> HashMap<String, HashSet<String>> {
    let mut out = HashMap::new();
    let Some(raw) = raw else {
        return out;
    };

    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((principal_id, tenant_list_raw)) = split_kv(trimmed) else {
            continue;
        };
        let principal_id = principal_id.trim();
        if principal_id.is_empty() {
            continue;
        }

        let tenants: HashSet<String> = tenant_list_raw
            .split(|ch| ch == '|' || ch == ',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if !tenants.is_empty() {
            out.insert(principal_id.to_owned(), tenants);
        }
    }

    out
}

pub fn principal_tenant_allowed(allowed_tenants: &HashSet<String>, tenant_id: &str) -> bool {
    if allowed_tenants.contains(tenant_id) || allowed_tenants.contains("*") {
        return true;
    }
    allowed_tenants
        .iter()
        .any(|pattern| wildcard_match(pattern, tenant_id))
}

pub fn principal_binding_matches(pattern: &str, principal_id: &str) -> bool {
    pattern == principal_id || wildcard_match(pattern, principal_id)
}

pub fn resolve_principal_binding<'a, T>(
    bindings: &'a HashMap<String, T>,
    principal_id: &str,
) -> Option<&'a T> {
    if let Some(exact) = bindings.get(principal_id) {
        return Some(exact);
    }

    let mut best: Option<(&T, usize, usize)> = None;
    for (pattern, value) in bindings {
        if !pattern.contains('*') {
            continue;
        }
        if !wildcard_match(pattern, principal_id) {
            continue;
        }
        let specificity = pattern
            .chars()
            .filter(|character| *character != '*')
            .count();
        let pattern_len = pattern.len();
        match best {
            Some((_, best_specificity, best_len))
                if best_specificity > specificity
                    || (best_specificity == specificity && best_len >= pattern_len) =>
            {
                continue;
            }
            _ => {
                best = Some((value, specificity, pattern_len));
            }
        }
    }

    best.map(|(value, _, _)| value)
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return false;
    }

    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');
    let segments = pattern
        .split('*')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    if segments.is_empty() {
        return true;
    }

    let mut cursor = 0usize;

    for (index, segment) in segments.iter().enumerate() {
        if index == 0 && !starts_with_wildcard {
            if !value.starts_with(segment) {
                return false;
            }
            cursor = segment.len();
            continue;
        }

        if index == segments.len() - 1 && !ends_with_wildcard {
            if let Some(found_at) = value[cursor..].rfind(segment) {
                let absolute = cursor + found_at;
                if absolute + segment.len() != value.len() {
                    return false;
                }
                cursor = absolute + segment.len();
                continue;
            }
            return false;
        }

        if let Some(found_at) = value[cursor..].find(segment) {
            cursor += found_at + segment.len();
        } else {
            return false;
        }
    }

    true
}

pub fn parse_operator_role_label(value: &str) -> Option<OperatorRole> {
    match value.trim().to_ascii_lowercase().as_str() {
        "viewer" => Some(OperatorRole::Viewer),
        "operator" => Some(OperatorRole::Operator),
        "admin" => Some(OperatorRole::Admin),
        _ => None,
    }
}

pub fn operator_role_name(role: OperatorRole) -> &'static str {
    match role {
        OperatorRole::Viewer => "viewer",
        OperatorRole::Operator => "operator",
        OperatorRole::Admin => "admin",
    }
}

pub fn parse_principal_role_map(raw: Option<&str>) -> HashMap<String, OperatorRole> {
    let mut out = HashMap::new();
    for (principal_id, role_raw) in parse_kv_map(raw) {
        if let Some(role) = parse_operator_role_label(&role_raw) {
            out.insert(principal_id, role);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        parse_principal_role_map, parse_principal_tenant_map, principal_binding_matches,
        principal_tenant_allowed, resolve_principal_binding,
    };
    use std::collections::HashMap;

    #[test]
    fn principal_tenant_map_parses_multi_tenant() {
        let parsed = parse_principal_tenant_map(Some("svc-a=tenant_a|tenant_b;svc-b:tenant_c"));
        assert_eq!(parsed.len(), 2);
        assert!(parsed
            .get("svc-a")
            .map(|set| set.contains("tenant_a") && set.contains("tenant_b"))
            .unwrap_or(false));
    }

    #[test]
    fn principal_role_map_parses_known_roles() {
        let parsed = parse_principal_role_map(Some("alice:viewer;ops=admin"));
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn principal_tenant_allowed_supports_exact_and_global_wildcard() {
        let parsed = parse_principal_tenant_map(Some("svc-a=tenant_a|*"));
        let set = parsed.get("svc-a").expect("binding exists");
        assert!(principal_tenant_allowed(set, "tenant_a"));
        assert!(principal_tenant_allowed(set, "tenant_ws_123"));
    }

    #[test]
    fn principal_tenant_allowed_supports_prefix_wildcard() {
        let parsed = parse_principal_tenant_map(Some("svc-a=tenant_demo|tenant_ws_*"));
        let set = parsed.get("svc-a").expect("binding exists");
        assert!(principal_tenant_allowed(set, "tenant_demo"));
        assert!(principal_tenant_allowed(set, "tenant_ws_abcdef"));
        assert!(!principal_tenant_allowed(set, "tenant_other"));
    }

    #[test]
    fn principal_binding_match_supports_exact_and_wildcard() {
        assert!(principal_binding_matches("demo-operator", "demo-operator"));
        assert!(principal_binding_matches(
            "workspace-*-admin",
            "workspace-alpha-admin"
        ));
        assert!(!principal_binding_matches(
            "workspace-*-admin",
            "workspace-alpha-viewer"
        ));
    }

    #[test]
    fn resolve_principal_binding_prefers_exact_over_wildcard() {
        let mut map: HashMap<String, &str> = HashMap::new();
        map.insert("workspace-*".to_owned(), "wild");
        map.insert("workspace-123".to_owned(), "exact");
        let selected = resolve_principal_binding(&map, "workspace-123").copied();
        assert_eq!(selected, Some("exact"));
    }

    #[test]
    fn resolve_principal_binding_supports_wildcard() {
        let mut map: HashMap<String, &str> = HashMap::new();
        map.insert("workspace-*".to_owned(), "workspace");
        map.insert("user:*".to_owned(), "user");
        let selected = resolve_principal_binding(&map, "user:abc").copied();
        assert_eq!(selected, Some("user"));
    }
}
