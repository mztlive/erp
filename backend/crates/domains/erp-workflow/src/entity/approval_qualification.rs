//! 审批人的账号与同角色读写资格；不包含具体单据访问权。
use erp_identity::AccountSummary;

/// 资格失效原因由用例分别映射为输入错误或运行受阻。
pub(crate) enum ApprovalQualification {
    Inactive,
    MissingPermissions,
    Eligible(AccountSummary),
}

impl ApprovalQualification {
    // 根据 identity 已核验的账号和同角色完整权限形成审批资格。
    // 保留账号失效、权限不足与合格账号三种结果。
    //
    // 基础设施错误由取证用例传播。
    //
    // account 为有效账号摘要；same_role 表示同一有效角色授予读取与决定权限。
    pub(crate) fn from_account(account: Option<AccountSummary>, same_role: bool) -> Self {
        match account {
            None => Self::Inactive,
            Some(account) if same_role => Self::Eligible(account),
            Some(_) => Self::MissingPermissions,
        }
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;

    use super::*;

    #[test]
    fn qualification_preserves_denial_reason_and_verified_display_snapshot() {
        let account = AccountSummary::new("approver", AccountKind::Admin, true)
            .with_display_name("审批人")
            .with_login_account("login");
        assert!(matches!(ApprovalQualification::from_account(None, true), ApprovalQualification::Inactive));
        assert!(matches!(
            ApprovalQualification::from_account(Some(account.clone()), false),
            ApprovalQualification::MissingPermissions
        ));
        let ApprovalQualification::Eligible(actual) =
            ApprovalQualification::from_account(Some(account.clone()), true)
        else {
            panic!("同角色完整资格应保留账号摘要");
        };
        assert_eq!(actual, account);
    }
}
