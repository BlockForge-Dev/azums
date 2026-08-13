pub mod memory;
pub mod sqlite;

pub fn seed_from_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| {
            u64::from_str_radix(value.trim_start_matches("0x"), 16)
                .ok()
                .or_else(|| value.parse::<u64>().ok())
        })
        .unwrap_or(default)
}
