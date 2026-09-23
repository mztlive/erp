//! Consumer port for account login facts required by customer commands.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{Error, Result};

/// Port customer uses to validate sales accounts and resolve display names.
#[async_trait]
pub trait AccountFactPort: Send + Sync {
    /// Reject when the account does not exist or cannot log in.
    ///
    /// # Parameters
    /// * `user_id` - account id
    ///
    /// # Errors
    /// Missing account maps to `NotFound`; disabled account maps to `BusinessLogicError`.
    async fn ensure_can_login(&self, user_id: &str) -> Result<()>;

    /// Return display names keyed by account id. Missing accounts are omitted.
    async fn names_by_ids(&self, account_ids: &[String]) -> Result<HashMap<String, String>>;
}

/// Fail-closed account fact port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAccountFactPort;

#[async_trait]
impl AccountFactPort for FailClosedAccountFactPort {
    async fn ensure_can_login(&self, _user_id: &str) -> Result<()> {
        Err(Error::Internal("账号端口未接线".to_string()))
    }

    async fn names_by_ids(&self, _account_ids: &[String]) -> Result<HashMap<String, String>> {
        Err(Error::Internal("账号端口未接线".to_string()))
    }
}
