#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedProviderEndpoints {
    pub primary: String,
    pub urls: Vec<String>,
    pub fallbacks: Vec<String>,
}

pub fn parse_provider_urls(raw: &str) -> Vec<String> {
    let urls = raw
        .split(|ch| matches!(ch, ',' | ';' | '\n' | '\r'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    dedupe_provider_urls(urls)
}

pub fn dedupe_provider_urls(urls: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for url in urls {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|existing| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_owned());
    }
    out
}

pub fn preferred_provider_urls(preferred: Option<&str>, candidates: &[String]) -> Vec<String> {
    let mut urls = Vec::new();
    if let Some(preferred) = preferred {
        urls.extend(parse_provider_urls(preferred));
    }
    urls.extend(candidates.iter().cloned());
    dedupe_provider_urls(urls)
}

pub fn primary_provider_url(candidates: &[String], default_url: &str) -> String {
    candidates
        .iter()
        .find_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        })
        .unwrap_or_else(|| default_url.to_owned())
}

pub fn resolve_provider_urls(
    explicit: Option<&str>,
    env_primary_key: &str,
    env_list_key: &str,
    env_fallbacks_key: &str,
    env_single_key: &str,
    default_url: &str,
) -> OrderedProviderEndpoints {
    if let Some(raw) = explicit {
        let urls = parse_provider_urls(raw);
        if !urls.is_empty() {
            let primary = primary_provider_url(&urls, default_url);
            let fallbacks = urls.iter().skip(1).cloned().collect::<Vec<_>>();
            return OrderedProviderEndpoints {
                primary,
                urls,
                fallbacks,
            };
        }
    }

    let mut urls = Vec::new();
    if let Ok(primary) = std::env::var(env_primary_key) {
        urls.extend(parse_provider_urls(&primary));
    }
    if let Ok(list) = std::env::var(env_list_key) {
        urls.extend(parse_provider_urls(&list));
    }
    if let Ok(fallbacks) = std::env::var(env_fallbacks_key) {
        urls.extend(parse_provider_urls(&fallbacks));
    }
    if urls.is_empty() {
        if let Ok(single) = std::env::var(env_single_key) {
            urls.extend(parse_provider_urls(&single));
        }
    }
    if urls.is_empty() {
        urls.push(default_url.to_owned());
    }
    let urls = dedupe_provider_urls(urls);
    let primary = primary_provider_url(&urls, default_url);
    let fallbacks = urls.iter().skip(1).cloned().collect::<Vec<_>>();
    OrderedProviderEndpoints {
        primary,
        urls,
        fallbacks,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dedupe_provider_urls, parse_provider_urls, preferred_provider_urls, primary_provider_url,
        resolve_provider_urls,
    };

    #[test]
    fn parse_provider_urls_supports_mixed_delimiters() {
        let parsed = parse_provider_urls("https://a, https://b;https://c\nhttps://d\rhttps://e");
        assert_eq!(
            parsed,
            vec![
                "https://a".to_owned(),
                "https://b".to_owned(),
                "https://c".to_owned(),
                "https://d".to_owned(),
                "https://e".to_owned()
            ]
        );
    }

    #[test]
    fn dedupe_provider_urls_preserves_first_order() {
        let deduped = dedupe_provider_urls(vec![
            "https://a".to_owned(),
            "https://b".to_owned(),
            "https://a".to_owned(),
            "https://c".to_owned(),
            "https://b".to_owned(),
        ]);
        assert_eq!(
            deduped,
            vec![
                "https://a".to_owned(),
                "https://b".to_owned(),
                "https://c".to_owned()
            ]
        );
    }

    #[test]
    fn preferred_provider_urls_promotes_current_provider() {
        let urls = preferred_provider_urls(
            Some("https://fallback"),
            &["https://primary".to_owned(), "https://fallback".to_owned()],
        );
        assert_eq!(
            urls,
            vec!["https://fallback".to_owned(), "https://primary".to_owned()]
        );
    }

    #[test]
    fn primary_provider_url_uses_default_when_empty() {
        assert_eq!(
            primary_provider_url(&[], "https://default"),
            "https://default".to_owned()
        );
    }

    #[test]
    fn resolve_provider_urls_prefers_explicit_over_env() {
        std::env::set_var("TEST_RPC_PRIMARY_URL", "https://env-primary");
        let resolved = resolve_provider_urls(
            Some("https://explicit-primary,https://explicit-fallback"),
            "TEST_RPC_PRIMARY_URL",
            "TEST_RPC_URLS",
            "TEST_RPC_FALLBACK_URLS",
            "TEST_RPC_URL",
            "https://default",
        );
        std::env::remove_var("TEST_RPC_PRIMARY_URL");
        std::env::remove_var("TEST_RPC_URLS");
        std::env::remove_var("TEST_RPC_FALLBACK_URLS");
        std::env::remove_var("TEST_RPC_URL");

        assert_eq!(resolved.primary, "https://explicit-primary");
        assert_eq!(
            resolved.urls,
            vec![
                "https://explicit-primary".to_owned(),
                "https://explicit-fallback".to_owned()
            ]
        );
        assert_eq!(resolved.fallbacks, vec!["https://explicit-fallback".to_owned()]);
    }
}
