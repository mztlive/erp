//! Consumer port for account display names required by contract lists.

use std::collections::HashMap;

use async_trait::async_trait;

use crate::error::{Error, Result};

/// Port contract uses to resolve owner display names without depending on `erp-identity`.
#[async_trait]
pub trait AccountNamePort: Send + Sync {
    /// Return display names keyed by account id. Missing accounts are omitted.
    ///
    /// # Parameters
    /// * `account_ids` - owner account ids collected from assignment facts
    ///
    /// # Errors
    /// Adapter query failures.
    async fn names_by_ids(&self, account_ids: &[String]) -> Result<HashMap<String, String>>;
}

/// Empty account-name lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyAccountNames;

#[async_trait]
impl AccountNamePort for EmptyAccountNames {
    async fn names_by_ids(&self, _account_ids: &[String]) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }
}

/// Fail-closed account-name port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAccountNamePort;

#[async_trait]
impl AccountNamePort for FailClosedAccountNamePort {
    async fn names_by_ids(&self, _account_ids: &[String]) -> Result<HashMap<String, String>> {
        Err(Error::Internal("账号端口未接线".to_string()))
    }
}
