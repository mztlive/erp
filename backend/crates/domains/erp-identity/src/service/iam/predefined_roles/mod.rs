//! 内建岗位模板的共享权限定义与启动身份基础。
//!
//! 管理页面显式生成岗位角色；启动仅初始化组织基础与目录资格。
//! `role-root` 由 [`super::ensure_root_role`] 单独维护，不在模板清单中。

use super::SharedRbacService;
use crate::error::Result;
use crate::service::organization::ensure_home_department;
use crate::service::person_directory::sync_role_grants;

pub(crate) mod permission_tables;
mod templates;
use permission_tables::{
    FINANCE_PERMISSIONS, MANAGEMENT_PERMISSIONS, OPERATIONS_PERMISSIONS, PROCUREMENT_PERMISSIONS,
    SALES_LEADER_PERMISSIONS, SALES_PERMISSIONS, SYSADMIN_PERMISSIONS, WAREHOUSE_PERMISSIONS,
};
pub use templates::builtin_role_templates;

/// 启动时仅准备组织基础与目录资格，不创建或补权业务角色。
/// # 参数
/// `rbac` 为当前身份授权服务。
/// # 返回
/// 基础数据检查完成。
/// # 错误
/// 组织或目录持久化失败时返回错误。
pub async fn ensure_identity_foundation(rbac: &SharedRbacService) -> Result<()> {
    ensure_home_department(rbac.database().clone()).await?;
    sync_role_grants(rbac).await
}

/// 单条预定义角色的静态定义。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PredefinedRoleDef {
    /// 稳定角色 ID（写入 Mongo `roles.id` 与 Casbin `role:{id}`）。
    pub id: &'static str,
    /// 展示名称。
    pub name: &'static str,
    /// 角色说明，帮助管理员理解岗位边界。
    pub description: &'static str,
    /// 推荐权限，格式均为 `resource:action`；支持 `resource:*` 动作通配。
    pub permissions: &'static [&'static str],
}

/// 全部业务预定义角色（不含超级管理员 `role-root`）。
pub(crate) const PREDEFINED_ROLES: &[PredefinedRoleDef] = &[
    PredefinedRoleDef {
        id: "role-sales",
        name: "销售",
        description: "客户、合同、销售单与客户验收；催回款只读；负责本人客户相关单据。",
        permissions: SALES_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-sales-leader",
        name: "销售领导",
        description: "卡券销售领导审批、低毛利/变更审批与团队销售单据只读协同。",
        permissions: SALES_LEADER_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-procurement",
        name: "采购",
        description: "商品与供应商主数据、采购二次确认、采购单、履约、销售变更履约确认、采购退货与供应商结算协同。",
        permissions: PROCUREMENT_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-operations",
        name: "运营",
        description: "卡券类目资料、映射差异、运营审批、执行投影与商品发布协同。",
        permissions: OPERATIONS_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-warehouse",
        name: "仓储",
        description: "入库、仓发、库存查询与库存调整。",
        permissions: WAREHOUSE_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-finance",
        name: "财务",
        description: "采购财务审核、客户/供应商票款、发票核销、成本费用与退款冲正。",
        permissions: FINANCE_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-management",
        name: "管理层",
        description: "经营质量、盈亏、履约与票款汇总只读；仅可在授权范围管理待办责任，不改写业务事实。",
        permissions: MANAGEMENT_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-sysadmin",
        name: "系统管理员",
        description: "同步监控、集成错误、对账差异与来源注册；不替代业务部门改写商业事实，不含账号/角色超级权限。",
        permissions: SYSADMIN_PERMISSIONS,
    },
];

#[cfg(test)]
mod tests {
    use super::{PREDEFINED_ROLES, PROCUREMENT_PERMISSIONS, SALES_PERMISSIONS, builtin_role_templates};
    use crate::Result;
    use crate::entity::rbac::{Permission, PermissionSet};

    /// 解析静态权限表，供岗位职责断言使用。
    fn parse_permissions(raw: &[&str]) -> Result<Vec<Permission>> {
        Ok(raw.iter().map(Permission::parse).collect::<std::result::Result<_, _>>()?)
    }

    /// 显式采购模板授予门户职责和供应商维护权限，不授予内部账号管理。
    #[test]
    fn portal_permissions_are_explicit_procurement_template_grants() {
        let catalog = builtin_role_templates().unwrap();
        let procurement = catalog.iter().find(|role| role.id == "role-procurement").unwrap();
        let permissions = PermissionSet::new(procurement.permissions.clone());
        for code in [
            "supplier_portal_account:list",
            "supplier_portal_account:create",
            "supplier_portal_account:update",
            "supplier_portal_catalog:list",
            "supplier_portal_catalog:update",
            "supplier_portal_request:list",
            "supplier_portal_request:detail",
            "supplier_portal_request:review",
            "supplier:update",
        ] {
            assert!(permissions.covers_one(&Permission::parse(code).unwrap()), "{code}");
        }
        assert!(procurement.permissions.iter().all(|permission| permission.resource() != "account"));
    }

    /// 抄送接收资格不通过岗位模板授予。
    #[test]
    fn copy_receiving_is_not_a_role_permission() {
        for template in builtin_role_templates().unwrap() {
            assert!(template.permissions.iter().all(|permission| permission.resource() != "approval_copy"));
        }
    }

    /// 整账读取仅授予综合财务及三个细分财务模板。
    #[test]
    fn ledger_read_is_only_granted_to_finance_templates() {
        let ledger = Permission::parse("finance_ledger:read").unwrap();
        for template in builtin_role_templates().unwrap() {
            let has_ledger = PermissionSet::new(template.permissions).covers_one(&ledger);
            assert_eq!(
                has_ledger,
                matches!(
                    template.id.as_str(),
                    "role-finance" | "role-finance-director" | "role-cashier" | "role-invoice-clerk"
                ),
                "{} 的整账读取资格不符合岗位职责",
                template.id
            );
        }
    }

    /// 采购和销售选择已有公司，系统管理员维护公司。
    #[test]
    fn company_permissions_follow_role_duties() {
        let procurement = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        let sysadmin = PermissionSet::new(parse_permissions(super::SYSADMIN_PERMISSIONS).unwrap());
        let sales = PermissionSet::new(parse_permissions(SALES_PERMISSIONS).unwrap());
        for action in ["list", "detail", "create", "update"] {
            let permission = Permission::parse(format!("company:{action}")).unwrap();
            assert!(sysadmin.covers_one(&permission));
            assert_eq!(procurement.covers_one(&permission), matches!(action, "list" | "detail"));
            assert_eq!(sales.covers_one(&permission), matches!(action, "list" | "detail"));
        }
    }

    /// 岗位身份保持唯一，且不包含独立维护的超级管理员。
    #[test]
    fn predefined_role_ids_are_unique_and_stable() {
        let ids: Vec<_> = PREDEFINED_ROLES.iter().map(|role| role.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "预定义角色 ID 必须唯一");
        assert!(ids.iter().all(|id| id.starts_with("role-")));
        assert!(!ids.contains(&"role-root"), "root 由 ensure_root_role 单独维护")
    }

    #[test]
    fn predefined_role_names_fit_entity_limits() {
        for role in PREDEFINED_ROLES {
            assert!(!role.name.is_empty());
            assert!(role.name.chars().count() <= 32, "{} 名称过长", role.id);
            assert!(role.description.chars().count() <= 256, "{} 描述过长", role.id);
        }
    }

    /// 财务模板不授予已退役的采购审核旁路权限。
    #[test]
    fn finance_template_excludes_retired_purchase_review_permission() {
        let catalog = builtin_role_templates().unwrap();
        let finance = catalog.iter().find(|template| template.id == "role-finance").unwrap();
        let permissions = PermissionSet::new(finance.permissions.clone());
        assert!(!permissions.covers_one(&Permission::parse("purchase_order:review").unwrap()));
    }

    /// 财务模板只读结算主体修订和销售变更。
    #[test]
    fn finance_template_reads_parties_and_sales_changes_without_writes() {
        let catalog = builtin_role_templates().unwrap();
        let finance = catalog.iter().find(|template| template.id == "role-finance").unwrap();
        let permissions = PermissionSet::new(finance.permissions.clone());
        for code in [
            "party:list",
            "party:detail",
            "party_revision:list",
            "sales_change_order:list",
            "sales_change_order:detail",
        ] {
            assert!(permissions.covers_one(&Permission::parse(code).unwrap()), "{code}");
        }

        for code in ["party_revision:create", "party_revision:update", "sales_change_order:create"] {
            assert!(!permissions.covers_one(&Permission::parse(code).unwrap()), "{code}");
        }
    }

    /// 仓储审批采购变更需要读取对象，但不得获得创建或提交变更的职责。
    #[test]
    fn warehouse_can_read_purchase_changes_for_approval() {
        let permissions = super::WAREHOUSE_PERMISSIONS;
        assert!(permissions.contains(&"purchase_change_order:list"));
        assert!(permissions.contains(&"purchase_change_order:detail"));
        assert!(!permissions.contains(&"purchase_change_order:*"));
        assert!(!permissions.contains(&"purchase_change_order:create"));
        assert!(!permissions.contains(&"purchase_change_order:submit"));
    }

    #[test]
    fn all_predefined_permissions_are_parseable() {
        for role in PREDEFINED_ROLES {
            let permissions = parse_permissions(role.permissions).expect(role.id);
            assert!(!permissions.is_empty(), "{} 至少应配置一条权限", role.id);
            for permission in &permissions {
                assert!(!permission.resource().is_empty());
                assert!(!permission.action().is_empty());
            }
        }
    }

    #[test]
    fn supplier_connection_governance_permissions_follow_fixed_responsibility_roles() {
        let procurement = PREDEFINED_ROLES.iter().find(|role| role.id == "role-procurement").unwrap();
        assert!(procurement.permissions.contains(&"supplier_api_connection:update_business_profile"));
        assert!(procurement.permissions.contains(&"supplier_api_capability:confirm_requirement"));
        assert!(!procurement.permissions.contains(&"supplier_api_connection:health_check"));

        let sysadmin = PREDEFINED_ROLES.iter().find(|role| role.id == "role-sysadmin").unwrap();
        for permission in [
            "supplier_api_connection:bind_endpoint_reference",
            "supplier_api_connection:manage_credential_reference",
            "supplier_api_connection:health_check",
            "supplier_api_connection:enable",
            "supplier_api_connection:disable",
            "supplier_api_connection:catalog_sync",
            "supplier_api_connection:view_reference_metadata",
            "supplier_api_capability:update",
        ] {
            assert!(sysadmin.permissions.contains(&permission), "缺少 {permission}");
        }
        assert!(!sysadmin.permissions.contains(&"supplier_api_capability:confirm_requirement"));

        let operations = PREDEFINED_ROLES.iter().find(|role| role.id == "role-operations").unwrap();
        for permission in [
            "supplier_api_connection:bind_endpoint_reference",
            "supplier_api_connection:manage_credential_reference",
            "supplier_api_connection:enable",
            "supplier_api_connection:disable",
            "supplier_api_connection:catalog_sync",
            "supplier_api_capability:update",
        ] {
            assert!(!operations.permissions.contains(&permission), "运营不应取得 {permission}");
        }
    }

    #[test]
    fn import_confirmation_permissions_match_fixed_responsibility_roles() {
        let responsibility_roles =
            ["role-sales", "role-procurement", "role-operations", "role-warehouse", "role-finance"];
        let required = [
            "legacy_import_confirmation:list",
            "legacy_import_confirmation:detail",
            "legacy_import_confirmation:complete",
        ];

        for role in PREDEFINED_ROLES {
            for permission in required {
                assert_eq!(
                    role.permissions.contains(&permission),
                    responsibility_roles.contains(&role.id),
                    "{} 的 W18 责任确认权限不符合固定角色注册表: {permission}",
                    role.id
                );
            }
        }
    }

    #[test]
    fn integration_task_permissions_match_fixed_w29_roles() {
        let responsibility_roles =
            ["role-sales", "role-procurement", "role-operations", "role-finance", "role-sysadmin"];
        for role in PREDEFINED_ROLES {
            for permission in ["integration_task:process", "integration_task:complete"] {
                assert_eq!(
                    role.permissions.contains(&permission),
                    responsibility_roles.contains(&role.id),
                    "{} 的 W29 强命令权限不符合固定角色注册表: {permission}",
                    role.id
                );
            }
        }
    }

    #[test]
    fn purchase_change_read_follows_review_responsibility_roles() {
        // 采购变更由仓储确认履约影响、财务复核；管理层可打开任务详情。各岗位必须能读取变更单，
        // 决定本身走 approval_instance:decide，无需 approve/reject 资源动作。
        // 采购用通配 `purchase_change_order:*` 覆盖，按解析后的覆盖关系断言。
        let required = ["purchase_change_order:list", "purchase_change_order:detail"];
        let read_roles = ["role-procurement", "role-warehouse", "role-finance", "role-management"];
        for role in PREDEFINED_ROLES {
            let permissions = parse_permissions(role.permissions).unwrap();
            for permission in required {
                let parsed = Permission::parse(permission).unwrap();
                let covered = permissions.iter().any(|seeded| seeded.covers(&parsed));
                assert_eq!(
                    covered,
                    read_roles.contains(&role.id),
                    "{} 的采购变更读取权限不符合固定责任角色注册表: {permission}",
                    role.id
                );
            }
            assert!(
                !role.permissions.contains(&"purchase_change_order:approve"),
                "{} 不应配置采购变更 approve（决定走 approval_instance:decide）",
                role.id
            );
        }
    }

    #[test]
    fn stock_adjustment_read_follows_review_responsibility_roles() {
        // 库存调整由财务复核（仓储可创建并代行）；管理层可打开任务详情。
        // 决定本身走 approval_instance:decide，无需 approve/reject 资源动作。
        let required = ["stock_adjustment:list", "stock_adjustment:detail"];
        let read_roles = ["role-warehouse", "role-finance", "role-management"];
        for role in PREDEFINED_ROLES {
            let permissions = parse_permissions(role.permissions).unwrap();
            for permission in required {
                let parsed = Permission::parse(permission).unwrap();
                let covered = permissions.iter().any(|seeded| seeded.covers(&parsed));
                assert_eq!(
                    covered,
                    read_roles.contains(&role.id),
                    "{} 的库存调整读取权限不符合固定责任角色注册表: {permission}",
                    role.id
                );
            }
        }
    }

    /// 销售变更审批权限仅授予销售领导、采购和运营岗位。
    #[test]
    fn sales_change_approval_permissions_follow_assigned_roles() {
        let required = ["sales_change_order:approve"];
        let holders = ["role-sales-leader", "role-procurement", "role-operations"];
        for role in PREDEFINED_ROLES {
            assert_eq!(
                role.permissions.contains(&"sales_change_order:approve"),
                holders.contains(&role.id),
                "{} 的销售变更审批权与固定角色注册表不一致",
                role.id
            );
        }

        let leader = PREDEFINED_ROLES.iter().find(|role| role.id == "role-sales-leader").unwrap();
        for permission in required {
            assert!(leader.permissions.contains(&permission), "销售领导缺少 {permission}");
        }
    }

    #[test]
    fn procurement_permissions_include_cost_visibility() {
        let permissions = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        let cost = PermissionSet::new([Permission::parse("supplier_offering_cost:detail").unwrap()]);
        assert!(permissions.covers(&cost));
    }

    #[test]
    fn procurement_permissions_include_supplier_write_and_sensitive_reveal() {
        let permissions = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        for required in ["supplier:update", "supplier_sensitive:reveal"] {
            let required = PermissionSet::new([Permission::parse(required).unwrap()]);
            assert!(permissions.covers(&required));
        }
    }

    #[test]
    fn procurement_permissions_cover_assigned_duties_without_finance_writes() {
        let permissions = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        for required in [
            "sales_change_order:list",
            "sales_change_order:detail",
            "sales_change_order:approve",
            "sales_change_order:reject",
            "purchase_return_order:list",
            "purchase_return_order:create",
            "purchase_return_order:detail",
            "stock_balance:list",
            "stock_balance:detail",
            "stock_movement:list",
            "stock_reservation:list",
            "supplier_settlement_statement:list",
            "supplier_settlement_statement:detail",
            "supplier_settlement_item:list",
            "supplier_settlement_difference:list",
            "supplier_settlement_difference:update",
            "integration_error_task:detail",
            "reconciliation_difference:detail",
            "legacy_import_batch:detail",
        ] {
            let required = PermissionSet::new([Permission::parse(required).unwrap()]);
            assert!(permissions.covers(&required), "采购缺少 {required:?}");
        }

        for forbidden in [
            "supplier_payment:commit",
            "supplier_settlement_statement:confirm",
            "supplier_settlement_statement:create",
            "integration_error_task:list",
            "reconciliation_difference:list",
            "legacy_import_batch:list",
            "stock_adjustment:create",
        ] {
            let forbidden = Permission::parse(forbidden).unwrap();
            assert!(!permissions.covers_one(&forbidden), "采购不应默认具备 {forbidden}");
        }
    }

    #[test]
    fn inquiry_position_permissions_separate_sales_results_and_internal_procurement() {
        let sales = PermissionSet::new(parse_permissions(SALES_PERMISSIONS).unwrap());
        let procurement = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        for action in ["create", "submit", "accept", "reinquire", "import", "convert_sales", "recover"] {
            assert!(sales.covers_one(&Permission::parse(format!("sales_inquiry:{action}")).unwrap()));
            assert!(!procurement.covers_one(&Permission::parse(format!("sales_inquiry:{action}")).unwrap()));
        }

        for action in ["detail", "save_feedback", "publish", "reassign", "internal_quote", "convert_expenses"]
        {
            let permission = Permission::parse(format!("procurement_inquiry:{action}")).unwrap();
            assert!(procurement.covers_one(&permission));
            assert!(!sales.covers_one(&permission));
        }
    }

    #[test]
    fn sales_permissions_include_core_sales_capabilities() {
        let permissions = parse_permissions(SALES_PERMISSIONS).unwrap();
        assert!(permissions.iter().any(|p| p.covers(&Permission::parse("sales_order:create").unwrap())));
        assert!(
            permissions.iter().any(|p| p.covers(&Permission::parse("sales_order:cancel_approval").unwrap()))
        );
        assert!(permissions.iter().any(|p| p.covers(&Permission::parse("customer:list").unwrap())));
        assert!(permissions.iter().any(|p| p.covers(&Permission::parse("contract:create").unwrap())));
        assert!(permissions.iter().any(|p| p.covers(&Permission::parse("sellable_sku:list").unwrap())));
        assert!(
            permissions
                .iter()
                .any(|p| p.covers(&Permission::parse("procurement_responsibility:list").unwrap()))
        );

        for forbidden in [
            "product:list",
            "product_revision:list",
            "sku:list",
            "sku_revision:list",
            "product_category:list",
            "product_brand:list",
            "unit_of_measure:list",
        ] {
            assert!(!permissions.iter().any(|p| p.covers(&Permission::parse(forbidden).unwrap())));
        }
        assert!(
            !permissions.iter().any(|p| p.covers(&Permission::parse("party_bank_account:detail").unwrap()))
        );
        assert!(!permissions.iter().any(|p| p.covers(&Permission::parse("customer_scope:detail").unwrap())));
    }

    /// 最终管理层模板只读，仅保留待办查看管理资格。
    #[test]
    fn management_permissions_are_read_only() {
        let catalog = builtin_role_templates().unwrap();
        let management = catalog.iter().find(|template| template.id == "role-management").unwrap();
        for permission in &management.permissions {
            assert!(
                matches!(permission.action(), "list" | "detail" | "read")
                    || (permission.resource() == "work_item" && permission.action() == "manage"),
                "管理层除待办查看管理资格外应保持只读，发现: {permission}"
            );
        }
    }

    /// 最终岗位模板必须使用显式工作流动作。
    #[test]
    fn workflow_permissions_are_explicit() {
        for template in builtin_role_templates().unwrap() {
            assert!(
                template.permissions.iter().all(|permission| {
                    !matches!(permission.resource(), "work_item" | "approval_process" | "approval_instance")
                        || permission.action() != "*"
                }),
                "{} 不得授予工作流通配权限",
                template.id
            );
        }
    }

    /// 最终模板排除退役审批动作，系统管理员不自动获得流程或审批权限。
    #[test]
    fn approval_templates_exclude_retired_actions_and_sysadmin_approval_access() {
        for template in builtin_role_templates().unwrap() {
            let permissions = PermissionSet::new(template.permissions.clone());
            for forbidden in ["approval_instance:recover", "approval_instance:diagnose"] {
                assert!(
                    !permissions.covers_one(&Permission::parse(forbidden).unwrap()),
                    "{} 不得授予 {forbidden}",
                    template.id
                );
            }
            if template.id == "role-sysadmin" {
                assert!(template.permissions.iter().all(|permission| {
                    !matches!(permission.resource(), "approval_process" | "approval_instance")
                }));
            }
        }
    }

    #[test]
    fn sysadmin_does_not_include_account_or_role_admin() {
        let sysadmin = PREDEFINED_ROLES.iter().find(|role| role.id == "role-sysadmin").unwrap();
        let permissions = parse_permissions(sysadmin.permissions).unwrap();
        for forbidden in [
            "admin:create",
            "admin:update",
            "admin:delete",
            "admin:update_role",
            "role:create",
            "role:update",
            "role:delete",
            "*:*",
        ] {
            let required = Permission::parse(forbidden).unwrap();
            assert!(
                !permissions.iter().any(|p| p.covers(&required)),
                "系统管理员预定义角色不得覆盖 {forbidden}"
            );
        }
    }

    #[test]
    fn product_detail_is_read_only_split_without_granting_list() {
        // 详情只读拆分：除采购外的新补角色拥有 detail 且不拥有 list；
        // 运营/仓储保留 list 以便走新高效接口；采购靠通配覆盖。
        let detail_only =
            ["role-sales", "role-sales-leader", "role-finance", "role-management", "role-sysadmin"];
        let detail_with_list = ["role-operations", "role-warehouse"];
        let detail = Permission::parse("product:detail").unwrap();
        let list = Permission::parse("product:list").unwrap();
        for role in PREDEFINED_ROLES {
            let permissions = parse_permissions(role.permissions).unwrap();
            let covers_detail = permissions.iter().any(|seeded| seeded.covers(&detail));
            let covers_list = permissions.iter().any(|seeded| seeded.covers(&list));
            if role.id == "role-procurement" {
                assert!(covers_detail, "采购应靠 product:* 覆盖 detail");
                assert!(!role.permissions.contains(&"product:detail"), "采购不得重复添加 product:detail");
            } else if detail_only.contains(&role.id) {
                assert!(covers_detail, "{} 应拥有 product:detail", role.id);
                assert!(!covers_list, "{} 不应拥有 product:list", role.id);
            } else if detail_with_list.contains(&role.id) {
                assert!(covers_detail, "{} 应拥有 product:detail", role.id);
                assert!(covers_list, "{} 应保留 product:list", role.id);
            }
        }
    }
}
