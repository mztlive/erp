//! Consumer ports for customer existence, numbers and assignment visibility.

use std::collections::HashMap;

use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{CustomerAccountId, PartyId};

use crate::error::{Error, Result};

/// Minimum customer facts required to archive or list contracts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerAccountFact {
    /// Customer role stable id.
    pub id: String,
    /// Customer number shown on contract lists.
    pub customer_no: String,
    /// Settlement-party default when upload omits `settlement_party_id`.
    pub party_id: PartyId,
    /// Whether the customer may receive new archives.
    pub is_active: bool,
}

/// Port contract uses to read customer identity without depending on `erp-customer`.
#[async_trait]
pub trait CustomerFactsPort: Send + Sync {
    /// Load one undeleted customer role.
    ///
    /// # Parameters
    /// * `customer_id` - customer role id
    ///
    /// # Returns
    /// `None` when the customer does not exist or is deleted.
    ///
    /// # Errors
    /// Adapter query failures.
    async fn find_by_id(&self, customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>>;

    /// Load undeleted customer roles by id. Missing ids are omitted.
    ///
    /// # Parameters
    /// * `customer_ids` - customer role ids
    ///
    /// # Errors
    /// Adapter query failures.
    async fn find_by_ids(&self, customer_ids: &[CustomerAccountId]) -> Result<Vec<CustomerAccountFact>>;
}

/// Port contract uses to resolve assigned-scope visibility and owner names.
#[async_trait]
pub trait CustomerAssignmentFactsPort: Send + Sync {
    /// Return customer ids currently assigned to `user_id` as OWNER or COLLABORATOR.
    ///
    /// # Parameters
    /// * `user_id` - current login user
    /// * `as_of` - business day used for assignment validity
    ///
    /// # Returns
    /// Empty vector when the user has no effective assignment.
    ///
    /// # Errors
    /// Adapter query failures.
    async fn assigned_customer_ids(&self, user_id: &str, as_of: BusinessDate) -> Result<Vec<String>>;

    /// Return current OWNER user ids keyed by customer id.
    ///
    /// # Parameters
    /// * `customer_ids` - customers on the current list page
    /// * `as_of` - business day used for assignment validity
    ///
    /// # Returns
    /// Customers without an OWNER assignment are omitted.
    ///
    /// # Errors
    /// Adapter query failures.
    async fn owner_user_ids_by_customer(
        &self,
        customer_ids: &[String],
        as_of: BusinessDate,
    ) -> Result<HashMap<String, String>>;
}

/// Empty customer lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyCustomers;

#[async_trait]
impl CustomerFactsPort for EmptyCustomers {
    async fn find_by_id(&self, _customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>> {
        Ok(None)
    }

    async fn find_by_ids(&self, _customer_ids: &[CustomerAccountId]) -> Result<Vec<CustomerAccountFact>> {
        Ok(Vec::new())
    }
}

/// Empty assignment lookup used by isolated unit tests.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyAssignments;

#[async_trait]
impl CustomerAssignmentFactsPort for EmptyAssignments {
    async fn assigned_customer_ids(&self, _user_id: &str, _as_of: BusinessDate) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn owner_user_ids_by_customer(
        &self,
        _customer_ids: &[String],
        _as_of: BusinessDate,
    ) -> Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }
}

/// Fail-closed customer facts used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedCustomerFactsPort;

#[async_trait]
impl CustomerFactsPort for FailClosedCustomerFactsPort {
    async fn find_by_id(&self, _customer_id: &CustomerAccountId) -> Result<Option<CustomerAccountFact>> {
        Err(Error::Internal("客户端口未接线".to_string()))
    }

    async fn find_by_ids(&self, _customer_ids: &[CustomerAccountId]) -> Result<Vec<CustomerAccountFact>> {
        Err(Error::Internal("客户端口未接线".to_string()))
    }
}

/// Fail-closed assignment facts used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAssignmentFactsPort;

#[async_trait]
impl CustomerAssignmentFactsPort for FailClosedAssignmentFactsPort {
    async fn assigned_customer_ids(&self, _user_id: &str, _as_of: BusinessDate) -> Result<Vec<String>> {
        Err(Error::Internal("客户归属端口未接线".to_string()))
    }

    async fn owner_user_ids_by_customer(
        &self,
        _customer_ids: &[String],
        _as_of: BusinessDate,
    ) -> Result<HashMap<String, String>> {
        Err(Error::Internal("客户归属端口未接线".to_string()))
    }
}
