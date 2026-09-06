//! Consumer port for identity facts required by warehouse handler eligibility.

use async_trait::async_trait;

use crate::error::{Error, Result};

/// Warehouse inbound or outbound handler duty used to select eligibility facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerDuty {
    /// 采购到货入库经办人。
    Inbound,
    /// 公司仓发货经办人。
    Outbound,
}

impl HandlerDuty {
    /// Return the original Chinese operation label used in eligibility errors.
    pub fn label(self) -> &'static str {
        match self {
            Self::Inbound => "入库",
            Self::Outbound => "仓发",
        }
    }

    /// Return whether `fact` is eligible for this duty.
    pub fn is_eligible(self, fact: &HandlerIdentityFact) -> bool {
        match self {
            Self::Inbound => fact.inbound_eligible,
            Self::Outbound => fact.outbound_eligible,
        }
    }
}

/// Minimal identity snapshot used to evaluate warehouse fulfillment handlers.
///
/// Composition adapters must compute eligibility from current identity facts:
/// inbound covers `purchase_receipt:list/detail/update/post`; outbound covers
/// `delivery:list/detail/update/post`. Warehouse does not depend on identity
/// or workflow types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerIdentityFact {
    /// Stable account id.
    pub user_id: String,
    /// Account display name.
    pub display_name: String,
    /// Login account.
    pub account: String,
    /// Whether the account may log in (work-item availability).
    pub can_login: bool,
    /// Whether RBAC covers inbound fulfillment execution permissions.
    pub inbound_eligible: bool,
    /// Whether RBAC covers outbound fulfillment execution permissions.
    pub outbound_eligible: bool,
}

/// Port warehouse uses to read handler identity and permission facts.
#[async_trait]
pub trait IdentityFactPort: Send + Sync {
    /// Return identity facts for one handler candidate.
    ///
    /// Missing or disabled accounts return `None`. The service maps that to the
    /// original "账号不存在或已停用" business error.
    ///
    /// # Parameters
    /// * `account_id` - candidate account id
    ///
    /// # Errors
    /// Identity lookup failures other than a missing account.
    async fn handler_identity(&self, account_id: &str) -> Result<Option<HandlerIdentityFact>>;

    /// Return company-wide admin handler candidates (no organization filter).
    ///
    /// # Errors
    /// Identity listing or permission evaluation failures.
    async fn admin_handler_identities(&self) -> Result<Vec<HandlerIdentityFact>>;
}

/// Fail-closed identity port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedIdentityFactPort;

#[async_trait]
impl IdentityFactPort for FailClosedIdentityFactPort {
    async fn handler_identity(&self, _account_id: &str) -> Result<Option<HandlerIdentityFact>> {
        Err(Error::Internal("身份端口未接线".to_string()))
    }

    async fn admin_handler_identities(&self) -> Result<Vec<HandlerIdentityFact>> {
        Err(Error::Internal("身份端口未接线".to_string()))
    }
}
