//! Version information embedded at build time (see `build.rs`).

/// Full version: `0.4.0-main.57` (main channel), `0.4.0` (stable), `0.4.0-dev` (local).
pub const VERSION: &str = env!("FLAIR_BUILD_VERSION");
/// `main`, `stable` or `dev`.
pub const CHANNEL: &str = env!("FLAIR_BUILD_CHANNEL");
pub const COMMIT: &str = env!("FLAIR_BUILD_COMMIT");
const EPOCH: &str = env!("FLAIR_BUILD_EPOCH");

/// Build date, `2026-11-26`.
pub fn date() -> String {
    flair_core::time::UtcDateTime::from_unix(EPOCH.parse().unwrap_or(0)).date()
}

/// True for local builds: they never install updates.
pub fn is_dev() -> bool {
    CHANNEL == "dev"
}
