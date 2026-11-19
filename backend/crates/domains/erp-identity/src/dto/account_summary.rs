//! 业务服务使用的账号身份与显示资料，不暴露凭证。
use application_core::AuditActor;
use erp_core::{AccountKind, Error, Result};

use crate::AccountCore;

/// 向业务服务提供账号类型、登录状态与展示资料，不携带凭证。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    /// 跨业务引用账号的稳定 ID。
    pub id: String,
    /// 创建账号时确定的身份类型。
    pub kind: AccountKind,
    /// 账号当前是否允许登录，供业务操作复核身份。
    pub can_login: bool,
    /// 工作项等业务投影使用的显示名。
    pub display_name: String,
    /// 候选选择时用于区分同名人员的登录账号。
    pub login_account: String,
}

impl AccountSummary {
    /// 只有身份和状态数据时构造摘要，显示名与登录账号初始为空。
    pub fn new(id: impl Into<String>, kind: AccountKind, can_login: bool) -> Self {
        Self { id: id.into(), kind, can_login, display_name: String::new(), login_account: String::new() }
    }

    /// 组装业务展示资料时填入显示名，返回更新后的摘要。
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = display_name.into();
        self
    }

    /// 组装候选资料时填入登录账号，返回更新后的摘要。
    pub fn with_login_account(mut self, login_account: impl Into<String>) -> Self {
        self.login_account = login_account.into();
        self
    }

    /// 复核认证主体时，要求账号 ID、类型一致且当前仍可登录。
    pub fn matches_actor(&self, actor: &AuditActor) -> bool {
        self.id == actor.id() && self.kind == actor.kind() && self.can_login
    }

    /// 在需要可登录身份的业务操作前检查账号状态，停用时返回与身份实体一致的业务错误。
    pub fn ensure_can_login(&self) -> Result<()> {
        if self.can_login {
            Ok(())
        } else {
            Err(Error::from("账号已停用，不能参与需要可登录身份的业务操作"))
        }
    }

    /// 筛选后台业务责任候选时，仅接受允许登录的 `Admin` 账号。
    pub fn is_active_backoffice(&self) -> bool {
        self.kind == AccountKind::Admin && self.can_login
    }
}

impl From<&AccountCore> for AccountSummary {
    // 从账号主数据生成业务查询摘要，只提取身份、登录状态与展示字段，不带凭证。
    fn from(account: &AccountCore) -> Self {
        Self::new(account.base.id.clone(), account.kind, account.can_login())
            .with_display_name(account.name.clone())
            .with_login_account(account.secret.account().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证后台与供应商账号采用同一登录状态门槛，停用时均拒绝。
    #[test]
    fn login_eligibility_rejects_inactive_accounts_for_every_kind() {
        for kind in [AccountKind::Admin, AccountKind::Supplier] {
            assert!(AccountSummary::new("enabled", kind, true).ensure_can_login().is_ok());
            assert!(AccountSummary::new("disabled", kind, false).ensure_can_login().is_err());
        }
    }
}
