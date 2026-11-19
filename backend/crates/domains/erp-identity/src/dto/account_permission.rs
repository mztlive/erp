//! 已通过账号状态与功能权限检查的内部账号候选。

use std::cmp::Ordering;

use crate::entity::AccountCore;

/// 供责任交接等内部选择场景使用的已通过功能资格检查的账号摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountPermissionCandidate {
    /// 用于提交选择的稳定账号 ID。
    pub user_id: String,
    /// 供候选列表展示的人员姓名。
    pub display_name: String,
    /// 用于区分同名候选人的登录账号。
    pub account: String,
}

impl AccountPermissionCandidate {
    /// 资格检查通过后提取候选展示字段，不返回密码或联系方式，也不重新检查资格。
    pub fn from_account(account: &AccountCore) -> Self {
        Self {
            user_id: account.base.id.clone(),
            display_name: account.name.clone(),
            account: account.secret.account().to_string(),
        }
    }
}

impl Ord for AccountPermissionCandidate {
    // 排序时依次比较姓名、登录账号和稳定 ID，确保同名候选的顺序固定。
    fn cmp(&self, other: &Self) -> Ordering {
        self.display_name
            .cmp(&other.display_name)
            .then_with(|| self.account.cmp(&other.account))
            .then_with(|| self.user_id.cmp(&other.user_id))
    }
}

impl PartialOrd for AccountPermissionCandidate {
    // 候选作部分比较时沿用完整排序，任意两项均可比较。
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
