//! 仓库经办人资格所需的身份事实消费端口。

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
    /// 返回资格错误里使用的中文操作标签。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 入库返回「入库」，仓发返回「仓发」。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn label(self) -> &'static str {
        match self {
            Self::Inbound => "入库",
            Self::Outbound => "仓发",
        }
    }

    /// 判断身份事实是否具备该履约责任的资格。
    ///
    /// # 参数
    /// * `fact` - 经办人身份事实。
    ///
    /// # 返回
    /// 入库看 `inbound_eligible`，仓发看 `outbound_eligible`；具备时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
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
    /// 返回一名经办人候选的身份事实。
    ///
    /// 账号不存在或已停用时返回 `None`。服务层把这种情况映射为含「账号不存在或已停用」的业务错误。
    ///
    /// # 参数
    /// * `account_id` - 候选账号 ID。
    ///
    /// # 返回
    /// 找到可用账号时返回身份事实；缺失或停用时返回 `None`。
    ///
    /// # 错误
    /// 除账号缺失以外的身份查询失败时返回错误。
    async fn handler_identity(&self, account_id: &str) -> Result<Option<HandlerIdentityFact>>;

    /// 返回全公司管理端经办人候选，不按组织过滤。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回身份事实列表。
    ///
    /// # 错误
    /// 身份列举或权限判断失败时返回错误。
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
