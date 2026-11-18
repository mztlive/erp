//! 仓库经办人资格所需的身份事实消费端口。

use async_trait::async_trait;
use persistence_core::Executor;

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
    /// 给经办资格错误提供职责对应的中文标签：入库为「入库」，仓发为「仓发」。
    pub fn label(self) -> &'static str {
        match self {
            Self::Inbound => "入库",
            Self::Outbound => "仓发",
        }
    }

    /// 从身份事实读取指定职责的许可，入库看 `inbound_eligible`，仓发看 `outbound_eligible`，避免用另一项资格代替。
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
    /// 用原业务授权事务或读取阶段的执行器查询候选账号及职责事实；不存在或停用返回 `None`，供服务提示重新选择，其他身份查询失败返回原错误。
    async fn handler_identity(
        &self,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<HandlerIdentityFact>>;

    /// 在原授权读取事务列举全公司管理端经办候选及权限事实，不按组织限缩；身份枚举或权限判断失败直接返回。
    async fn admin_handler_identities(&self, executor: &mut dyn Executor)
    -> Result<Vec<HandlerIdentityFact>>;
}

/// Fail-closed identity port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedIdentityFactPort;

#[async_trait]
impl IdentityFactPort for FailClosedIdentityFactPort {
    async fn handler_identity(
        &self,
        _account_id: &str,
        _executor: &mut dyn Executor,
    ) -> Result<Option<HandlerIdentityFact>> {
        Err(Error::Internal("身份端口未接线".to_string()))
    }

    async fn admin_handler_identities(
        &self,
        _executor: &mut dyn Executor,
    ) -> Result<Vec<HandlerIdentityFact>> {
        Err(Error::Internal("身份端口未接线".to_string()))
    }
}
