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

/// API 启动时通过共享身份授权服务 `rbac` 为尚无组织节点的部署建立默认部门，再同步人员目录资格。
/// 为直接绑定启用销售或采购角色的账号补齐缺失资格，保留既有记录，不创建业务角色或追加角色权限。
/// 角色绑定以 Casbin 为准；组织初始化或目录同步失败时返回错误。
pub async fn ensure_identity_foundation(rbac: &SharedRbacService) -> Result<()> {
    ensure_home_department(rbac.database().clone()).await?;
    sync_role_grants(rbac).await
}

/// 构建岗位模板时使用的静态角色定义，集中维护岗位身份、展示信息和基础权限。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PredefinedRoleDef {
    /// 稳定角色 ID；生成时写入 MongoDB 的 `roles.id`，并用于构造 Casbin 主体 `role:{id}`。
    pub id: &'static str,
    /// 岗位目录展示及新角色命名所用的默认名称。
    pub name: &'static str,
    /// 模板构建时使用的默认职责说明，帮助管理员判断岗位边界。
    pub description: &'static str,
    /// 构建模板时解析的推荐权限，格式为 `resource:action`，支持 `resource:*` 动作通配。
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
        description: "查看经营、履约、票款及待办责任，不修改业务或审批状态。",
        permissions: MANAGEMENT_PERMISSIONS,
    },
    PredefinedRoleDef {
        id: "role-sysadmin",
        name: "系统管理员",
        description: "处理同步与集成异常、技术配置和组织治理；审批流程管理单独授权。",
        permissions: SYSADMIN_PERMISSIONS,
    },
];

#[cfg(test)]
mod tests {
    use super::{PREDEFINED_ROLES, PROCUREMENT_PERMISSIONS, SALES_PERMISSIONS, builtin_role_templates};
    use crate::Result;
    use crate::entity::rbac::{Permission, PermissionSet};

    // 岗位职责测试用此函数解析静态权限表，以规范权限值比较动作和通配覆盖关系。
    fn parse_permissions(raw: &[&str]) -> Result<Vec<Permission>> {
        Ok(raw.iter().map(Permission::parse).collect::<std::result::Result<_, _>>()?)
    }

    // 确认生成采购岗位时授予供应商门户账号、目录、申请处理及供应商维护权限，内部账号管理另行授权。
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

    // 确认岗位模板均排除审批抄送权限；抄送接收资格独立于模板授权。
    #[test]
    fn copy_receiving_is_not_a_role_permission() {
        for template in builtin_role_templates().unwrap() {
            assert!(template.permissions.iter().all(|permission| permission.resource() != "approval_copy"));
        }
    }

    // 检查最终模板的有效权限覆盖，确保整账读取仅授予综合财务及三个细分财务岗位。
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

    // 核对选用与维护公司的岗位分工：销售、采购可查阅已有公司，系统管理员还可创建和更新。
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

    // 检查基础岗位 ID 的唯一性及 `role-` 前缀，确保独立初始化的 `role-root` 留在模板之外。
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

    // 检查预设展示文本符合角色实体限制：名称须非空且最多 32 字符，职责说明最多 256 字符。
    #[test]
    fn predefined_role_names_fit_entity_limits() {
        for role in PREDEFINED_ROLES {
            assert!(!role.name.is_empty());
            assert!(role.name.chars().count() <= 32, "{} 名称过长", role.id);
            assert!(role.description.chars().count() <= 256, "{} 描述过长", role.id);
        }
    }

    // 检查综合财务的最终权限，防止模板通过直接授权或通配重新引入已退役的采购审核旁路。
    #[test]
    fn finance_template_excludes_retired_purchase_review_permission() {
        let catalog = builtin_role_templates().unwrap();
        let finance = catalog.iter().find(|template| template.id == "role-finance").unwrap();
        let permissions = PermissionSet::new(finance.permissions.clone());
        assert!(!permissions.covers_one(&Permission::parse("purchase_order:review").unwrap()));
    }

    // 确认综合财务可查阅结算主体、主体修订及销售变更；创建或更新主体修订、创建销售变更均不随岗授予。
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

    // 确认仓储审批时可查询采购变更列表和详情，模板排除创建、提交及通配权限。
    #[test]
    fn warehouse_can_read_purchase_changes_for_approval() {
        let permissions = super::WAREHOUSE_PERMISSIONS;
        assert!(permissions.contains(&"purchase_change_order:list"));
        assert!(permissions.contains(&"purchase_change_order:detail"));
        assert!(!permissions.contains(&"purchase_change_order:*"));
        assert!(!permissions.contains(&"purchase_change_order:create"));
        assert!(!permissions.contains(&"purchase_change_order:submit"));
    }

    // 检查每张基础权限表非空，且每项均可解析出非空的资源和动作。
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

    // 检查供应商接口治理分工：采购维护业务资料并确认需求，系统管理员负责技术接入，运营不承担所列接入管理动作。
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

    // 检查 W18 历史导入责任确认的列表、详情和完成权限，仅授予销售、采购、运营、仓储、财务。
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

    // 检查 W29 集成任务的处理和完成权限，仅授予销售、采购、运营、财务、系统管理员。
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

    // 按权限覆盖关系检查采购变更的列表和详情资格，并防止基础权限表单独登记采购变更审批动作。
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

    // 检查库存调整任务所需的列表和详情资格仅由仓储、财务、管理层岗位模板覆盖。
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

    // 核对销售变更审批动作的显式授予名单：销售领导、采购、运营。
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

    // 确认采购岗位在核算供应成本时具备查看供应商供货成本详情的权限。
    #[test]
    fn procurement_permissions_include_cost_visibility() {
        let permissions = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        let cost = PermissionSet::new([Permission::parse("supplier_offering_cost:detail").unwrap()]);
        assert!(permissions.covers(&cost));
    }

    // 确认采购岗位维护供应商资料时具备更新及查看敏感信息原文的权限。
    #[test]
    fn procurement_permissions_include_supplier_write_and_sensitive_reveal() {
        let permissions = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        for required in ["supplier:update", "supplier_sensitive:reveal"] {
            let required = PermissionSet::new([Permission::parse(required).unwrap()]);
            assert!(permissions.covers(&required));
        }
    }

    // 检查采购具备销售变更、采购退货、库存查询和供应商结算协同权限。
    // 供应商付款执行、结算单创建与确认、库存调整创建另行授权；集成异常、对账差异与导入批次仅开放详情。
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

    // 检查询价分工：销售发起并处理销售侧结果，采购处理内部报价与反馈；双方的这些动作分别授权。
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

    // 检查销售开单、撤销审批、签约及查看客户、可售 SKU 和采购责任所需的权限。
    // 排除商品主数据列表、结算账户详情和客户范围详情。
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

    // 检查管理层模板以列表、详情和读取为限，仅为查看他人待办保留 `work_item:manage`。
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

    // 检查最终模板逐项授权工作项、审批流程及审批实例动作，避免通配放大工作流权限。
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

    // 检查最终模板排除退役的审批恢复和诊断动作，系统管理员的流程及审批实例权限须另行授予。
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

    // 检查系统管理员的技术岗位权限边界：账号增删改、角色分配或维护及全局通配均不随岗授予。
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

    // 检查商品详情和列表资格按岗位独立配置，并确保采购继续由通配覆盖详情。
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
