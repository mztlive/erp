//! 管理页面的岗位模板目录，复用开发演示的基础权限定义。

use super::PREDEFINED_ROLES;
use crate::Result;
use crate::entity::role_template::{BuiltinRoleTemplate, FinancePosition};

/// 在管理端预览或显式生成岗位前，构建含权限及上岗条件的服务端目录。
/// 从综合财务派生财务总监、出纳和开票岗，保留综合财务供按需选择。
/// 构建仅使用静态定义，无需数据库或演示账号；权限解析失败时返回错误。
pub fn builtin_role_templates() -> Result<Vec<BuiltinRoleTemplate>> {
    let mut templates = Vec::new();
    for role in PREDEFINED_ROLES {
        let template = BuiltinRoleTemplate::new(role.id, role.name, role.description, role.permissions)?;
        if role.id == "role-finance" {
            for kind in [FinancePosition::Director, FinancePosition::Cashier, FinancePosition::Invoice] {
                templates.push(template.finance(kind)?);
            }
        }
        templates.push(template);
    }
    Ok(templates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoleId;
    use crate::entity::role_template::BuiltinRoleState;
    use crate::entity::{Permission, PermissionSet, Role, RoleData};

    // 检查出纳、开票和财务总监具备各自任务所需权限，资金、开票、审批与责任配置按岗位分离。
    // 采购可提交付款申请；细分财务中仅出纳可付款、财务总监可关闭，综合及细分财务均可读取整账。
    #[test]
    fn finance_positions_cover_execution_and_separate_duties() {
        let catalog = builtin_role_templates().unwrap();
        let permissions = |id: &str| {
            PermissionSet::new(catalog.iter().find(|item| item.id == id).unwrap().permissions.clone())
        };
        let has = |id: &str, code: &str| permissions(id).covers_one(&Permission::parse(code).unwrap());

        for code in [
            "payable_account:list",
            "payable_account:detail",
            "party_bank_account:reveal",
            "supplier_payment:list",
            "supplier_payment:detail",
            "supplier_payment:commit",
            "customer_receipt:submit",
            "customer_refund:submit",
            "supplier_refund:submit",
            "receipt_reversal:submit",
            "payment_reversal:submit",
        ] {
            assert!(has("role-cashier", code), "{code}");
        }

        for code in [
            "receivable_account:list",
            "receivable_account:detail",
            "invoice:create",
            "invoice:post",
            "sales_invoice_request:list",
            "sales_invoice_request:detail",
            "purchase_invoice_allocation:create",
        ] {
            assert!(has("role-invoice-clerk", code), "{code}");
        }

        assert!(has("role-finance-director", "approval_instance:decide"));
        assert!(has("role-finance-director", "finance_responsibility:manage"));
        for id in ["role-cashier", "role-invoice-clerk"] {
            assert!(!has(id, "approval_instance:decide"));
            assert!(!has(id, "finance_responsibility:manage"));
        }

        assert!(!has("role-finance-director", "supplier_payment:commit"));
        assert!(!has("role-finance-director", "customer_receipt:create"));
        assert!(!has("role-cashier", "invoice:post"));
        assert!(!has("role-invoice-clerk", "supplier_payment:commit"));

        assert!(has("role-cashier", "purchase_payment_request:pay"));
        assert!(has("role-finance-director", "purchase_payment_request:close"));
        assert!(!has("role-finance-director", "purchase_payment_request:pay"));
        assert!(!has("role-cashier", "purchase_payment_request:close"));
        assert!(!has("role-invoice-clerk", "purchase_payment_request:pay"));
        assert!(!has("role-invoice-clerk", "purchase_payment_request:close"));
        for id in ["role-finance-director", "role-cashier", "role-invoice-clerk"] {
            assert!(!has(id, "purchase_payment_request:create"));
        }
        assert!(has("role-procurement", "purchase_payment_request:submit"));
        assert!(!has("role-procurement", "purchase_payment_request:pay"));

        for id in ["role-finance", "role-finance-director", "role-cashier", "role-invoice-clerk"] {
            assert!(has(id, "finance_ledger:read"));
        }
    }

    // 检查生成前的目录数量与推荐边界；选择保留请求顺序，空选、未知或重复岗位整批拒绝。
    // 超级管理员不在候选目录中，所有模板均排除全资源通配。
    #[test]
    fn selection_is_bounded_and_rejects_unknown_or_duplicate_templates() {
        let catalog = builtin_role_templates().unwrap();
        assert_eq!(catalog.len(), 11);
        assert_eq!(catalog.iter().filter(|item| item.recommended).count(), 10);

        assert!(BuiltinRoleTemplate::select(&catalog, &[]).is_err());
        assert!(BuiltinRoleTemplate::select(&catalog, &["role-root".into()]).is_err());
        assert!(BuiltinRoleTemplate::select(&catalog, &["role-sales".into(), "role-sales".into()]).is_err());

        let selected =
            BuiltinRoleTemplate::select(&catalog, &["role-sales".into(), "role-cashier".into()]).unwrap();
        assert_eq!(selected[0].id, "role-sales");
        assert_eq!(selected[1].id, "role-cashier");

        for template in catalog {
            assert!(!template.permissions.iter().any(|permission| permission.resource() == "*"));
        }
    }

    // 检查有别于待办查看的改派、审批干预及后台任务取消权限不随管理层岗位授予。
    // 系统管理员的审批决定及流程发布权限须另行授予。
    #[test]
    fn management_and_technical_templates_keep_job_boundaries() {
        let catalog = builtin_role_templates().unwrap();
        for (id, denied) in [
            (
                "role-management",
                &[
                    "work_item:reassign",
                    "approval_instance:resume",
                    "approval_instance:cancel",
                    "background_job:cancel",
                ][..],
            ),
            ("role-sysadmin", &["approval_instance:decide", "approval_process:publish"][..]),
        ] {
            let template = catalog.iter().find(|template| template.id == id).unwrap();
            let permissions = PermissionSet::new(template.permissions.clone());
            for code in denied {
                assert!(!permissions.covers_one(&Permission::parse(code).unwrap()));
            }
        }
    }

    // 检查生成前的角色状态判定：仅缺失可新建，改名、停用或软删除的角色仍保留既有身份。
    #[test]
    fn existing_identity_is_preserved_in_every_state() {
        assert_eq!(BuiltinRoleState::from_role(None), BuiltinRoleState::Missing);
        let mut role = Role::new(RoleId::parse("role-sales").unwrap(), RoleData::new("自定义销售")).unwrap();
        assert_eq!(BuiltinRoleState::from_role(Some(&role)), BuiltinRoleState::Existing);
        role.disabled = true;
        assert_eq!(BuiltinRoleState::from_role(Some(&role)), BuiltinRoleState::Disabled);
        role.base.deleted_at = 1;
        assert_eq!(BuiltinRoleState::from_role(Some(&role)), BuiltinRoleState::Deleted);
    }
}
