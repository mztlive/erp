//! 业务服务使用的账号身份与显示资料，不暴露凭证。
use application_core::AuditActor;
use erp_core::{AccountKind, Error, Result};

use crate::AccountCore;

/// 业务服务使用的账号身份与显示资料摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSummary {
    /// Stable account id.
    pub id: String,
    /// Frozen account kind.
    pub kind: AccountKind,
    /// Whether the account may log in.
    pub can_login: bool,
    /// Display name used by work-item projections.
    pub display_name: String,
    /// Login account used by candidate pickers.
    pub login_account: String,
}

impl AccountSummary {
    /// 构造账号事实；显示名和登录账号初始为空。
    /// 返回尚未附带显示名和登录账号的事实。
    ///
    /// `can_login`：是否允许登录。
    pub fn new(id: impl Into<String>, kind: AccountKind, can_login: bool) -> Self {
        Self { id: id.into(), kind, can_login, display_name: String::new(), login_account: String::new() }
    }

    /// 附上显示名。
    /// 返回写入显示名后的事实。消耗 `self`。
    ///
    /// `display_name`：工作项投影使用的显示名。
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = display_name.into();
        self
    }

    /// 附上登录账号。
    /// 返回写入登录账号后的事实。消耗 `self`。
    ///
    /// `login_account`：候选人选择使用的登录账号。
    pub fn with_login_account(mut self, login_account: impl Into<String>) -> Self {
        self.login_account = login_account.into();
        self
    }

    /// 当当前账号仍匹配认证身份时返回 `true`。
    /// 稳定 ID、类型一致且允许登录时为 true。
    ///
    /// actor 为认证主体。
    pub fn matches_actor(&self, actor: &AuditActor) -> bool {
        self.id == actor.id() && self.kind == actor.kind() && self.can_login
    }

    /// 校验账号摘要允许承担需要可登录身份的业务操作。
    /// 账号启用时成功。
    ///
    /// 账号停用时返回与身份实体一致的业务错误。
    pub fn ensure_can_login(&self) -> Result<()> {
        if self.can_login {
            Ok(())
        } else {
            Err(Error::from("账号已停用，不能参与需要可登录身份的业务操作"))
        }
    }

    /// 当账号可以承担后台责任时返回 `true`。
    /// 账号类型为 `Admin` 且允许登录时返回 `true`，否则返回 `false`。
    pub fn is_active_backoffice(&self) -> bool {
        self.kind == AccountKind::Admin && self.can_login
    }
}

impl From<&AccountCore> for AccountSummary {
    // 从身份主数据提取账号目录可用的身份、状态和展示信息，不把凭证带入摘要。
    fn from(account: &AccountCore) -> Self {
        Self::new(account.base.id.clone(), account.kind, account.can_login())
            .with_display_name(account.name.clone())
            .with_login_account(account.secret.account().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_eligibility_rejects_inactive_accounts_for_every_kind() {
        for kind in [AccountKind::Admin, AccountKind::Supplier] {
            assert!(AccountSummary::new("enabled", kind, true).ensure_can_login().is_ok());
            assert!(AccountSummary::new("disabled", kind, false).ensure_can_login().is_err());
        }
    }
}
