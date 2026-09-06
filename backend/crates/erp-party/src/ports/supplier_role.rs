//! Consumer port for supplier-role facts needed by party subresource guards.

use async_trait::async_trait;
use erp_core::ids::PartyId;

use crate::error::{Error, Result};

/// Port party uses to read whether a party currently carries a supplier role.
///
/// Party never depends on `erp-supplier` types. Composition-root adapters query
/// the supplier-account fact and return only this boolean.
#[async_trait]
pub trait SupplierRolePort: Send + Sync {
    /// Return whether `party_id` currently has a supplier role.
    ///
    /// # Parameters
    /// * `party_id` - stable party id
    ///
    /// # Errors
    /// Adapter or storage failures.
    async fn party_has_supplier_role(&self, party_id: &PartyId) -> Result<bool>;
}

/// Fail-closed supplier-role port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedSupplierRolePort;

#[async_trait]
impl SupplierRolePort for FailClosedSupplierRolePort {
    async fn party_has_supplier_role(&self, _party_id: &PartyId) -> Result<bool> {
        Err(Error::Internal("供应商角色端口未接线".to_string()))
    }
}
