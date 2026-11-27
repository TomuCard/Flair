//! Core of Flair: shared types and logic with no UI or OS dependency.
//!
//! - [`query`]: user query and prefix parsing;
//! - [`result`]: result items and their actions;
//! - [`rank`]: fuzzy matching and ranking bonuses; [`frecency`]: launch history;
//! - [`provider`]: the [`Provider`](provider::Provider) trait every result source implements;
//! - [`settings`]: the single settings registry and the TOML configuration;
//! - [`theme`]: 3 colors → full set of contrast-checked UI tokens;
//! - [`i18n`]: user interface translations (Fluent).

pub mod archive;
pub mod frecency;
pub mod history;
pub mod i18n;
pub mod lexicon;
pub mod provider;
pub mod query;
pub mod rank;
pub mod result;
pub mod settings;
pub mod theme;
pub mod time;
pub mod update;

pub use provider::{CancellationToken, Provider};
pub use query::Query;
pub use result::{Action, IconSource, LaunchTarget, ProviderKind, ResultItem, SystemCommand};
