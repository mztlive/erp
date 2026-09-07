//! Consumer port for purchase-receipt document numbers used by movement views.

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimal purchase-receipt identity used to hydrate movement source document numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptNoFact {
    /// Purchase receipt id.
    pub id: String,
    /// Receipt number.
    pub receipt_no: String,
}

/// Port inventory uses to read receipt numbers without depending on fulfillment.
#[async_trait]
pub trait FulfillmentFactsPort: Send + Sync {
    /// Return receipt numbers keyed by purchase-receipt id.
    ///
    /// # Parameters
    /// * `ids` - purchase receipt ids
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Adapter query failures.
    async fn receipt_nos_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReceiptNoFact>>;
}

/// Fail-closed fulfillment facts port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedFulfillmentFacts;

#[async_trait]
impl FulfillmentFactsPort for FailClosedFulfillmentFacts {
    async fn receipt_nos_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReceiptNoFact>> {
        Err(Error::Internal("履约入库事实端口未接线".to_string()))
    }
}
