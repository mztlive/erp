//! Consumer port for warehouse identity and revision display facts.

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimal warehouse identity used to hydrate inventory list/detail views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarehouseFact {
    /// Stable warehouse id.
    pub id: String,
    /// Warehouse code.
    pub warehouse_code: String,
    /// Current revision id used to resolve the display name.
    pub current_revision_id: Option<String>,
}

/// Minimal warehouse revision display fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarehouseRevisionFact {
    /// Revision id.
    pub id: String,
    /// Current warehouse name.
    pub name: String,
}

/// Port inventory uses to read warehouse identity without depending on `erp-warehouse`.
#[async_trait]
pub trait WarehouseFactsPort: Send + Sync {
    /// Return whether a warehouse id exists.
    ///
    /// # Parameters
    /// * `id` - warehouse id
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Adapter query failures.
    async fn warehouse_exists(&self, id: &str, executor: &mut dyn Executor) -> Result<bool>;

    /// Return warehouse facts keyed by id.
    ///
    /// # Parameters
    /// * `ids` - warehouse ids
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Adapter query failures.
    async fn warehouses_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseFact>>;

    /// Return warehouse revision facts keyed by id.
    ///
    /// # Parameters
    /// * `ids` - warehouse revision ids
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Adapter query failures.
    async fn warehouse_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseRevisionFact>>;
}

/// Fail-closed warehouse facts port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedWarehouseFacts;

#[async_trait]
impl WarehouseFactsPort for FailClosedWarehouseFacts {
    async fn warehouse_exists(&self, _id: &str, _executor: &mut dyn Executor) -> Result<bool> {
        Err(Error::Internal("仓库事实端口未接线".to_string()))
    }

    async fn warehouses_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseFact>> {
        Err(Error::Internal("仓库事实端口未接线".to_string()))
    }

    async fn warehouse_revisions_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WarehouseRevisionFact>> {
        Err(Error::Internal("仓库事实端口未接线".to_string()))
    }
}
