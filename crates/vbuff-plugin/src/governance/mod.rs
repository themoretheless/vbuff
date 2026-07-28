//! Auditable plugin distribution and test contracts.
//!
//! These modules validate evidence produced by an external plugin runner. They
//! do not start subprocesses, open sockets, or claim OS sandbox enforcement.

mod action_bundle;
mod marketplace;
mod network;
mod supervisor;
mod test_harness;

pub use action_bundle::{ActionBundle, SignedActionBundle};
pub use marketplace::{MarketplaceCategory, MarketplaceExample, MarketplaceMetadata};
pub use network::{AuthorizedFetch, NetworkFetchPolicy};
pub use supervisor::{PluginFailureReport, PluginRuntimeState, PluginSupervisor};
pub use test_harness::{PluginTestCase, PluginTestObservation, PluginTestVerdict};

use crate::{PluginError, Result};

pub(super) fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

pub(super) fn invalid<T>(message: &str) -> Result<T> {
    Err(PluginError::InvalidBundle(message.into()))
}

#[cfg(test)]
mod tests;
