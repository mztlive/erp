//! 客户资料字段与按钮只使用同次读取冻结的功能资格。
use erp_identity::PermissionSet;
use erp_party::SensitiveFieldKind;
use erp_permissions::{Permission, contract, customer, party, payable, receivable, sales_order};

use super::sensitive::field_permissions;

pub(super) const PERMISSIONS: &[Permission] = &[
    party::CONTACT_DETAIL,
    party::CONTACT_REVEAL,
    party::ADDRESS_DETAIL,
    party::ADDRESS_REVEAL,
    party::TAX_PROFILE_DETAIL,
    party::BANK_ACCOUNT_DETAIL,
    payable::PAYMENT_RECIPIENT_REVEAL,
    customer::UPDATE,
    contract::CREATE,
    sales_order::CREATE,
    receivable::ACCOUNT_DETAIL,
    customer::ASSIGNMENT_CREATE,
];

pub(super) struct ProfileDisplay(pub PermissionSet);
impl ProfileDisplay {
    /// 检查固定字段或按钮的功能资格。
    /// # 参数
    /// permission 为本批已请求的权限。
    /// # 返回
    /// 当前资格覆盖该权限时为 true。
    /// # 错误
    /// 不执行 I/O。
    pub(super) fn visible(&self, permission: &Permission) -> bool {
        self.0.covers_one(permission)
    }
    /// 按当前字段资格保留或移除从属事实集合。
    /// # 参数
    /// permission 为字段详情权限，rows 为本次快照的事实。
    /// # 返回
    /// 有资格时保留原序列，否则返回空集合。
    /// # 错误
    /// 不执行 I/O。
    pub(super) fn filter<T>(&self, permission: &Permission, rows: Vec<T>) -> Vec<T> {
        if self.visible(permission) { rows } else { Vec::new() }
    }

    /// 揭示入口同时要求字段详情和揭示资格。
    /// # 参数
    /// kind 为实际敏感事实类型。
    /// # 返回
    /// 两项资格分别成立时为 true。
    /// # 错误
    /// 不执行 I/O。
    pub(super) fn reveal(&self, kind: SensitiveFieldKind) -> bool {
        field_permissions(kind).iter().all(|permission| self.visible(permission))
    }
    /// 按字段资格及客户状态生成既有按钮列表。
    /// # 参数
    /// active 为客户当前启用状态。
    /// # 返回
    /// 保持既有顺序的按钮编码，不签发对象授权。
    /// # 错误
    /// 不执行 I/O。
    pub(super) fn actions(&self, active: bool) -> Vec<String> {
        let candidates = [
            ("EDIT_CUSTOMER", customer::UPDATE, false),
            ("UPLOAD_CONTRACT_PDF", contract::CREATE, true),
            ("CREATE_SALES_ORDER", sales_order::CREATE, true),
            ("OPEN_RECEIVABLE", receivable::ACCOUNT_DETAIL, false),
            ("MANAGE_ASSIGNMENTS", customer::ASSIGNMENT_CREATE, false),
        ];
        let mut actions: Vec<String> = candidates
            .into_iter()
            .filter(|(_, permission, needs_active)| (!needs_active || active) && self.visible(permission))
            .map(|(name, _, _)| name.into())
            .collect();
        for (kind, action) in [
            (SensitiveFieldKind::ContactMobile, "REVEAL_CONTACT"),
            (SensitiveFieldKind::Address, "REVEAL_ADDRESS"),
            (SensitiveFieldKind::BankAccountNumber, "REVEAL_BANK_ACCOUNT"),
        ] {
            if self.reveal(kind) {
                actions.push(action.into());
            }
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reveal_needs_both_field_permissions_and_inactive_customer_keeps_only_valid_actions() {
        let view_only = ProfileDisplay(PermissionSet::new([party::CONTACT_DETAIL]));
        assert!(view_only.visible(&party::CONTACT_DETAIL));
        assert!(!view_only.reveal(SensitiveFieldKind::ContactMobile));
        assert!(view_only.actions(true).is_empty());
        assert_eq!(view_only.filter(&party::CONTACT_DETAIL, vec!["contact"]), ["contact"]);
        assert!(view_only.filter(&party::ADDRESS_DETAIL, vec!["address"]).is_empty());
        assert!(view_only.filter(&party::TAX_PROFILE_DETAIL, vec!["tax"]).is_empty());
        assert!(view_only.filter(&party::BANK_ACCOUNT_DETAIL, vec!["bank"]).is_empty());
        let reveal_only = ProfileDisplay(PermissionSet::new([party::CONTACT_REVEAL]));
        assert!(!reveal_only.reveal(SensitiveFieldKind::ContactMobile));
        let granted = ProfileDisplay(PermissionSet::new([
            party::CONTACT_DETAIL,
            party::CONTACT_REVEAL,
            contract::CREATE,
            sales_order::CREATE,
            customer::UPDATE,
        ]));
        assert_eq!(
            granted.actions(true),
            ["EDIT_CUSTOMER", "UPLOAD_CONTRACT_PDF", "CREATE_SALES_ORDER", "REVEAL_CONTACT"]
        );
        assert_eq!(granted.actions(false), ["EDIT_CUSTOMER", "REVEAL_CONTACT"]);
    }
}
