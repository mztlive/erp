//! 管理页面的岗位模板目录，复用开发演示的基础权限定义。

use super::PREDEFINED_ROLES;
use crate::Result;
use crate::entity::role_template::{BuiltinRoleTemplate, FinancePosition};

/// 取得部署岗位模板，包括细分财务岗位及可选综合财务。
/// # 参数
/// 无；模板不依赖数据库或演示账号。
/// # 返回
/// 服务端版本的完整岗位权限及上岗条件。
/// # 错误
/// 静态模板权限非法时返回错误。
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
    use crate::entity::role_template::BuiltinRoleState;
    use crate::entity::{Permission, PermissionSet, Role, RoleData};

    /// 财务任务真实要求的权限齐全，三个岗位的资金、审批和责任配置互相隔离。
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
        for id in ["role-finance", "role-finance-director", "role-cashier", "role-invoice-clerk"] {
            assert!(has(id, "finance_ledger:read"));
        }
    }

    /// 模板选择在写入前整批拒绝非法、重复或空选择。
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

    /// 管理层不改派或干预审批，技术岗位不自动获得业务审批资格。
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

    /// 既有角色即使改名、停用或删除也不被模板恢复。
    #[test]
    fn existing_identity_is_preserved_in_every_state() {
        assert_eq!(BuiltinRoleState::from_role(None), BuiltinRoleState::Missing);
        let mut role = Role::new("role-sales".into(), RoleData::new("自定义销售")).unwrap();
        assert_eq!(BuiltinRoleState::from_role(Some(&role)), BuiltinRoleState::Existing);
        role.disabled = true;
        assert_eq!(BuiltinRoleState::from_role(Some(&role)), BuiltinRoleState::Disabled);
        role.base.deleted_at = 1;
        assert_eq!(BuiltinRoleState::from_role(Some(&role)), BuiltinRoleState::Deleted);
    }
}
