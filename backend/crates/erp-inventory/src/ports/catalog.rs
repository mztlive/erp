//! Consumer port for SKU identity and revision display facts.

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Minimal SKU identity used to hydrate inventory list/detail views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuFact {
    /// Stable SKU id.
    pub id: String,
    /// SKU number.
    pub sku_no: String,
    /// Current revision id used to resolve name and specification.
    pub current_revision_id: Option<String>,
}

/// Minimal SKU revision display fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuRevisionFact {
    /// Revision id.
    pub id: String,
    /// Current SKU name.
    pub name: String,
    /// Optional specification summary.
    pub specification: Option<String>,
}

/// Port inventory uses to read SKU identity without depending on `erp-catalog`.
#[async_trait]
pub trait CatalogFactsPort: Send + Sync {
    /// Return SKU facts keyed by id.
    ///
    /// # Parameters
    /// * `ids` - SKU ids
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Adapter query failures.
    async fn skus_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuFact>>;

    /// Return SKU revision facts keyed by id.
    ///
    /// # Parameters
    /// * `ids` - SKU revision ids
    /// * `executor` - data-access executor chosen by the caller
    ///
    /// # Errors
    /// Adapter query failures.
    async fn sku_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuRevisionFact>>;
}

/// Fail-closed catalog facts port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedCatalogFacts;

#[async_trait]
impl CatalogFactsPort for FailClosedCatalogFacts {
    async fn skus_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuFact>> {
        Err(Error::Internal("商品事实端口未接线".to_string()))
    }

    async fn sku_revisions_by_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuRevisionFact>> {
        Err(Error::Internal("商品事实端口未接线".to_string()))
    }
}
