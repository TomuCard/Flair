//! The [`Provider`] trait implemented by every result source.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::query::Query;
use crate::result::{ProviderKind, ResultItem};

/// Cheap, clonable cancellation flag. A new keystroke cancels the previous query.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// A source of results (apps, files, calculator, web, system…).
pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;

    /// Must be fast; long loops must check `cancel` regularly and return early.
    fn query(&self, q: &Query, cancel: &CancellationToken) -> Vec<ResultItem>;

    /// Background indexing or refresh; must not block the caller.
    fn refresh(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_shared_between_clones() {
        let token = CancellationToken::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(clone.is_cancelled());
    }
}
