//! Consumer port for Party identity facts required by customer commands.

use async_trait::async_trait;
use erp_core::ids::PartyId;

use crate::error::{Error, Result};

/// Minimal Party identity snapshot used to hydrate customer views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyIdentityFact {
    /// Party stable id.
    pub party_id: String,
    /// Party business number.
    pub party_no: String,
    /// Current legal name when a current revision exists.
    pub legal_name: Option<String>,
    /// Current short name when a current revision exists.
    pub short_name: Option<String>,
}

/// Port customer uses to read Party existence and identity facts.
#[async_trait]
pub trait PartyFactPort: Send + Sync {
    /// Reject when the Party does not exist.
    ///
    /// # Parameters
    /// * `party_id` - Party stable id
    ///
    /// # Errors
    /// Missing Party maps to `NotFound`.
    async fn ensure_exists(&self, party_id: &PartyId) -> Result<()>;

    /// Return identity facts for the given Party ids.
    ///
    /// Missing Parties are omitted; callers treat gaps as silent degradation.
    async fn identities_by_ids(&self, party_ids: &[PartyId]) -> Result<Vec<PartyIdentityFact>>;

    /// Return Party ids whose current legal name or short name matches `keyword`.
    async fn matching_ids_by_name(&self, keyword: &str) -> Result<Vec<String>>;
}

/// Fail-closed Party fact port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedPartyFactPort;

#[async_trait]
impl PartyFactPort for FailClosedPartyFactPort {
    async fn ensure_exists(&self, _party_id: &PartyId) -> Result<()> {
        Err(Error::Internal("主体端口未接线".to_string()))
    }

    async fn identities_by_ids(&self, _party_ids: &[PartyId]) -> Result<Vec<PartyIdentityFact>> {
        Err(Error::Internal("主体端口未接线".to_string()))
    }

    async fn matching_ids_by_name(&self, _keyword: &str) -> Result<Vec<String>> {
        Err(Error::Internal("主体端口未接线".to_string()))
    }
}
