//! 已通过账号状态与功能权限检查的内部账号候选。

use std::cmp::Ordering;

use crate::entity::AccountCore;

/// 供责任交接等内部调用使用的最小账号信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountPermissionCandidate {
    /// 账号稳定 ID。
    pub user_id: String,
    /// 可读姓名。
    pub display_name: String,
    /// 登录账号，用于区分同名人员。
    pub account: String,
}

impl AccountPermissionCandidate {
    /// 从已通过资格检查的账号提取候选展示字段。
    ///
    /// # 参数
    /// * `account` - 已校验状态与所需功能权限的账号。
    ///
    /// # 返回
    /// 返回不含密码及联系方式的候选信息。
    ///
    /// # 错误
    /// 不返回错误；本转换不代替调用方资格检查。
    pub fn from_account(account: &AccountCore) -> Self {
        Self {
            user_id: account.base.id.clone(),
            display_name: account.name.clone(),
            account: account.secret.account().to_string(),
        }
    }
}

impl Ord for AccountPermissionCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.display_name
            .cmp(&other.display_name)
            .then_with(|| self.account.cmp(&other.account))
            .then_with(|| self.user_id.cmp(&other.user_id))
    }
}

impl PartialOrd for AccountPermissionCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
