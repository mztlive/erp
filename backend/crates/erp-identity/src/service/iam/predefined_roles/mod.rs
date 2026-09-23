//! 业务预定义角色种子。
//!
//! 应用启动时按固定角色 ID 幂等写入：
//! - 数据库中尚不存在（含软删除记录）时创建角色与推荐权限；
//! - 已存在且权限仍等于已知旧种子时，整集升级到当前推荐权限；
//! - 其余已存在角色只追加旧业务种子权限；查询目录权限仅在首次创建角色时初始化，
//!   也不覆盖名称、启停状态与 system 标记。
//!
//! 角色集合与默认权限对齐第一期部门职责（`docs/erp-phase-1.md` §11）、
//! 工作台角色入口（W01 今日工作台）以及二期销售领导审批轨。
//! `role-root` 由 [`super::ensure_root_role`] 单独维护，不在本清单中。

use super::SharedRbacService;

pub(crate) mod permission_tables;
mod upgrades;

pub(crate) use permission_tables::{
    FINANCE_PERMISSIONS, MANAGEMENT_PERMISSIONS, OPERATIONS_PERMISSIONS, PROCUREMENT_PERMISSIONS,
    SALES_LEADER_PERMISSIONS, SALES_PERMISSIONS, SYSADMIN_PERMISSIONS, WAREHOUSE_PERMISSIONS,
};
#[cfg(test)]
pub(crate) use upgrades::{
    APPROVAL_HTTP_ACTION_PERMISSIONS, PROCUREMENT_DUTY_GAP_PERMISSIONS, approval_http_legacy_snapshot,
    finance_legacy_permission_snapshots, integration_task_legacy_snapshot,
    legacy_workflow_permission_snapshot, low_margin_confirmation_legacy_snapshot,
    procurement_legacy_permission_snapshots, sales_legacy_permission_snapshots,
};
pub(crate) use upgrades::{
    remove_permissions, upgrade_approval_http_permissions, upgrade_customer_role_boundaries, upgrade_exact,
    upgrade_finance_role_party_read_permissions, upgrade_import_confirmation_permissions,
    upgrade_integration_task_permissions, upgrade_procurement_role_permissions,
    upgrade_sales_role_permissions, upgrade_sellable_sku_reader_permissions, upgrade_workflow_permissions,
};

use crate::entity::{Permission, RoleData};
use crate::error::Result;

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
        description: "客户、合同、销售单与客户验收；催回款只读；负责本人客户与协作客户相关单据。",
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

/// 确保全部业务预定义角色已写入数据库，并补齐当前种子中缺失的权限。
///
/// 对每个固定角色 ID：若不存在则创建角色实体与 Casbin 权限；已有角色先按已知旧
/// 种子做精确升级，再把当前推荐权限中尚未覆盖的项追加进 policy。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回值
/// 全部角色检查、创建或补齐完成后返回 `Ok(())`。
///
/// # 错误
/// 角色校验、MongoDB 写入或 Casbin policy 事务失败时返回错误。
///
/// # 业务约束
/// 管理员额外授予的权限、角色名称与启停状态不会被启动过程删除或覆盖。
pub async fn ensure_predefined_roles(rbac: &SharedRbacService) -> Result<()> {
    for role in PREDEFINED_ROLES {
        seed_one(rbac, role).await?;
    }
    upgrade_workflow_permissions(rbac).await?;
    upgrade_retired_purchase_review_permission(rbac).await?;
    upgrade_sales_role_permissions(rbac).await?;
    upgrade_finance_role_party_read_permissions(rbac).await?;
    upgrade_customer_role_boundaries(rbac).await?;
    upgrade_procurement_role_permissions(rbac).await?;
    upgrade_sellable_sku_reader_permissions(rbac).await?;
    upgrade_import_confirmation_permissions(rbac).await?;
    upgrade_integration_task_permissions(rbac).await?;
    upgrade_supplier_connection_governance_permissions(rbac).await?;
    upgrade_approval_http_permissions(rbac).await?;
    ensure_missing_permissions(rbac).await?;
    super::predefined_data_scopes::ensure_predefined_role_data_scopes(rbac).await?;
    crate::service::organization::ensure_home_department(rbac.database().clone()).await?;
    crate::service::person_directory::sync_role_grants(rbac).await?;
    Ok(())
}

/// 为已存在的预定义角色追加当前种子中尚未覆盖的权限。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回值
/// 全部角色检查或补齐完成后返回 `Ok(())`。
///
/// # 错误
/// 权限解析或 Casbin policy 写入失败时返回错误。
///
/// # 业务约束
/// 只追加缺失权限；已有更宽通配覆盖目标时视为已具备，不再写入重复细项。
async fn ensure_missing_permissions(rbac: &SharedRbacService) -> Result<()> {
    for role in PREDEFINED_ROLES {
        // 已存在角色的目录授权由管理员显式维护；缺失无法区分从未授予与人工撤销。
        let desired = parse_permissions(role.permissions)?
            .into_iter()
            .filter(|permission| {
                !matches!(
                    (permission.resource(), permission.action()),
                    ("sales_person" | "procurement_person" | "business_person" | "settlement_party" | "warehouse", "list")
                        | ("person_query_qualification", "manage")
                )
            })
            .collect();
        if rbac.ensure_missing_seeded_role_permissions(role.id, desired).await? {
            tracing::info!(
                role_id = role.id,
                role_name = role.name,
                "predefined role missing permissions appended"
            );
        }
    }
    Ok(())
}

/// 从仍保持上一版默认权限快照的财务角色移除已退役采购审核旁路权限。
async fn upgrade_retired_purchase_review_permission(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(FINANCE_PERMISSIONS)?;
    let mut previous = desired.clone();
    previous.push(Permission::parse("purchase_order:review")?);
    upgrade_exact(rbac, "role-finance", previous, desired).await
}

/// 将仍保持上一版默认种子的采购/系统管理员权限收口到 W20 固定职责。
async fn upgrade_supplier_connection_governance_permissions(rbac: &SharedRbacService) -> Result<()> {
    let procurement_desired = parse_permissions(PROCUREMENT_PERMISSIONS)?;
    let mut procurement_previous = remove_permissions(
        &procurement_desired,
        &["supplier_api_connection:update_business_profile", "supplier_api_capability:confirm_requirement"],
    );
    procurement_previous.push(Permission::parse("supplier_api_connection:health_check")?);
    upgrade_exact(rbac, "role-procurement", procurement_previous, procurement_desired).await?;

    let sysadmin_desired = parse_permissions(SYSADMIN_PERMISSIONS)?;
    let sysadmin_previous = remove_permissions(
        &sysadmin_desired,
        &[
            "supplier_api_connection:bind_endpoint_reference",
            "supplier_api_connection:manage_credential_reference",
            "supplier_api_connection:enable",
            "supplier_api_connection:disable",
            "supplier_api_connection:catalog_sync",
            "supplier_api_connection:view_reference_metadata",
            "supplier_api_capability:update",
        ],
    );
    upgrade_exact(rbac, "role-sysadmin", sysadmin_previous, sysadmin_desired).await
}

/// 从目标权限还原低毛利确认上线前的销售领导精确快照。
async fn seed_one(rbac: &SharedRbacService, role: &PredefinedRoleDef) -> Result<()> {
    let permissions = parse_permissions(role.permissions)?;
    // 可分配、可后续由管理员调整；仅 `role-root` 使用 system=true 的强保护边界。
    let data = RoleData::new(role.name.to_string()).with_description(role.description.to_string());
    let created = rbac.seed_role_if_absent(role.id, data, permissions).await?;
    if created {
        tracing::info!(role_id = role.id, role_name = role.name, "predefined role seeded");
    }
    Ok(())
}

/// 将静态权限字符串解析为领域权限集合。
///
/// # 参数
/// * `raw` - `resource:action` 字符串切片
///
/// # 返回值
/// 返回解析后的权限列表。
///
/// # 错误
/// 任一字符串不符合权限格式时返回错误。
fn parse_permissions(raw: &[&str]) -> Result<Vec<Permission>> {
    raw.iter().map(|permission| Permission::parse(*permission).map_err(Into::into)).collect()
}

/// 返回预定义角色 ID 列表（测试与文档校验用）。
///
/// # 返回值
/// 按定义顺序返回角色 ID。
#[cfg(test)]
pub(super) fn predefined_role_ids() -> Vec<&'static str> {
    PREDEFINED_ROLES.iter().map(|role| role.id).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        APPROVAL_HTTP_ACTION_PERMISSIONS, FINANCE_PERMISSIONS, PREDEFINED_ROLES, PROCUREMENT_PERMISSIONS,
        SALES_LEADER_PERMISSIONS, SALES_PERMISSIONS, approval_http_legacy_snapshot,
        integration_task_legacy_snapshot, legacy_workflow_permission_snapshot,
        low_margin_confirmation_legacy_snapshot, parse_permissions, predefined_role_ids,
        procurement_legacy_permission_snapshots, sales_legacy_permission_snapshots,
    };
    use crate::entity::rbac::{Permission, PermissionSet};

    /// 采购选择已有公司，系统管理员维护公司；启动补权限保留额外人工授权。
    #[test]
    fn company_permissions_follow_role_duties_and_append_without_replacing() {
        let procurement = PermissionSet::new(parse_permissions(PROCUREMENT_PERMISSIONS).unwrap());
        let sysadmin = PermissionSet::new(parse_permissions(super::SYSADMIN_PERMISSIONS).unwrap());
        for action in ["list", "detail", "create", "update"] {
            let permission = Permission::parse(format!("company:{action}")).unwrap();
            assert!(sysadmin.covers_one(&permission));
            assert_eq!(procurement.covers_one(&permission), matches!(action, "list" | "detail"));
        }
        let mut previous = super::remove_permissions(
            &parse_permissions(PROCUREMENT_PERMISSIONS).unwrap(),
            &["company:list", "company:detail"],
        );
        let custom = Permission::parse("custom_resource:read").unwrap();
        previous.push(custom.clone());
        let old = PermissionSet::new(previous);
        let merged = old.with_missing(&procurement).unwrap();
        assert!(merged.covers(&procurement));
        assert!(merged.covers_one(&custom));
        assert!(merged.with_missing(&procurement).is_none());
    }

    #[test]
    fn predefined_role_ids_are_unique_and_stable() {
        let ids = predefined_role_ids();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "预定义角色 ID 必须唯一");
        assert!(ids.iter().all(|id| id.starts_with("role-")));
        assert!(!ids.contains(&"role-root"), "root 由 ensure_root_role 单独维护，含公司级 DataScope")
    }

    #[test]
    fn predefined_role_names_fit_entity_limits() {
        for role in PREDEFINED_ROLES {
            assert!(!role.name.is_empty());
            assert!(role.name.chars().count() <= 32, "{} 名称过长", role.id);
            assert!(role.description.chars().count() <= 256, "{} 描述过长", role.id);
        }
    }

    #[test]
    fn finance_seed_excludes_retired_purchase_review_permission() {
        let desired = parse_permissions(FINANCE_PERMISSIONS).unwrap();
        let mut previous = desired.clone();
        previous.push(Permission::parse("purchase_order:review").unwrap());

        assert!(!desired.iter().any(|permission| permission.to_string() == "purchase_order:review"));
        assert!(previous.iter().any(|permission| permission.to_string() == "purchase_order:review"));
    }

    /// 财务选择结算主体需要读取名称，历史默认快照升级不包含主体修改权限。
    #[test]
    fn finance_party_read_upgrade_covers_previous_defaults() {
        let desired = parse_permissions(FINANCE_PERMISSIONS).unwrap();
        assert!(FINANCE_PERMISSIONS.contains(&"party_revision:list"));
        assert!(FINANCE_PERMISSIONS.contains(&"sales_change_order:detail"));
        assert!(FINANCE_PERMISSIONS.contains(&"sales_change_order:list"));
        assert!(!FINANCE_PERMISSIONS.contains(&"sales_change_order:create"));
        assert!(!FINANCE_PERMISSIONS.contains(&"party_revision:*"));
        let snapshots = super::finance_legacy_permission_snapshots(&desired).unwrap();
        assert!(snapshots.contains(&super::remove_permissions(&desired, &["party_revision:list"])));
        assert!(snapshots.contains(&super::remove_permissions(
            &desired,
            &["party:list", "party:detail", "party_revision:list"]
        )));
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
    fn integration_task_upgrade_snapshot_only_removes_two_permissions() {
        for role in PREDEFINED_ROLES.iter().filter(|role| {
            matches!(
                role.id,
                "role-sales" | "role-procurement" | "role-operations" | "role-finance" | "role-sysadmin"
            )
        }) {
            let desired = parse_permissions(role.permissions).unwrap();
            let previous = integration_task_legacy_snapshot(&desired);
            assert_eq!(previous.len() + 2, desired.len());
            assert!(previous.iter().all(|permission| permission.resource() != "integration_task"));
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

    #[test]
    fn low_margin_permissions_are_owned_only_by_sales_leader_and_upgrade_is_exact() {
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

        let desired = parse_permissions(SALES_LEADER_PERMISSIONS).unwrap();
        let previous = low_margin_confirmation_legacy_snapshot(&desired);
        assert_eq!(previous.len() + 1, desired.len());
        for permission in required {
            assert!(
                previous.iter().all(|candidate| candidate.to_string() != permission),
                "旧快照仍包含 {permission}"
            );
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
    fn procurement_legacy_snapshots_cover_current_database_seed() {
        let desired = parse_permissions(PROCUREMENT_PERMISSIONS).unwrap();
        let snapshots = procurement_legacy_permission_snapshots(&desired).unwrap();
        let duty_gap_count = super::PROCUREMENT_DUTY_GAP_PERMISSIONS.len();
        assert_eq!(snapshots.len(), 4);
        assert_eq!(snapshots[0].len(), desired.len() - duty_gap_count - 1);
        assert_eq!(snapshots[1].len(), desired.len() - duty_gap_count - 3);
        assert_eq!(snapshots[2].len(), desired.len() - duty_gap_count);
        assert_eq!(snapshots[3].len(), desired.len() - duty_gap_count);
        assert!(
            snapshots[2].iter().any(|permission| permission.to_string() == "supplier_catalog_cost:detail")
        );
        assert!(!snapshots[2].iter().any(|permission| permission.to_string() == "supplier_sensitive:reveal"));
        assert!(snapshots.iter().flatten().all(|permission| {
            super::PROCUREMENT_DUTY_GAP_PERMISSIONS.iter().all(|gap| permission.to_string() != *gap)
        }));
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

    #[test]
    fn startup_appends_missing_seed_permissions_without_dropping_custom_grants() {
        let desired = PermissionSet::new(parse_permissions(SALES_PERMISSIONS).unwrap());
        let extra = Permission::parse("custom:extra").unwrap();
        let current = PermissionSet::new(
            desired
                .as_slice()
                .iter()
                .filter(|permission| permission.to_string() != "sellable_sku:list")
                .cloned()
                .chain(std::iter::once(extra.clone())),
        );
        let merged = current.with_missing(&desired).expect("应补齐可售 SKU 查询权限");

        assert!(merged.covers(&desired));
        assert!(merged.covers_one(&extra));
        assert!(desired.with_missing(&desired).is_none());
    }

    #[test]
    fn sales_legacy_snapshots_cover_catalog_and_customer_boundary_seeds() {
        let desired = parse_permissions(SALES_PERMISSIONS).unwrap();
        let snapshots = sales_legacy_permission_snapshots(&desired).unwrap();
        // 快照[0]为选品上线前：desired 去掉 13 个选品权限。
        assert_eq!(snapshots[0].len(), desired.len() - 13);
        // 快照[1]为可售池上线前：[0] 去掉 sellable_sku:list 并补 7 个商品只读权限。
        assert_eq!(snapshots[1].len(), desired.len() - 7);
        // 快照[2]为客户边界收紧前：[1] 去掉 5 个客户细分权限并补 customer:* 与 party_bank_account:*。
        assert_eq!(snapshots[2].len(), desired.len() - 10);
        assert!(snapshots[1].iter().any(|permission| permission.to_string() == "sku:list"));
        assert!(snapshots[2].iter().any(|permission| permission.to_string() == "party_bank_account:*"));
        // 可售池上线前的快照（[1] 起）不含 sellable_sku:list；[0] 为选品上线前仍保留它。
        assert!(
            snapshots[1..].iter().flatten().all(|permission| permission.to_string() != "sellable_sku:list")
        );
        assert!(
            snapshots
                .iter()
                .flatten()
                .all(|permission| { !permission.to_string().starts_with("sales_selection_") })
        );
    }

    #[test]
    fn management_permissions_are_read_only() {
        let management = PREDEFINED_ROLES.iter().find(|role| role.id == "role-management").unwrap();
        for raw in management.permissions {
            let permission = Permission::parse(*raw).unwrap();
            if matches!(
                raw,
                &"work_item:manage"
                    | &"work_item:reassign"
                    | &"approval_process:read"
                    | &"approval_instance:read"
                    | &"approval_instance:cancel"
                    | &"approval_instance:resume"
                    | &"approval_instance:cancel_blocked"
                    | &"background_job:cancel"
            ) {
                continue;
            }
            assert!(
                matches!(permission.action(), "list" | "detail"),
                "管理层除任务责任管理外应保持只读，发现: {raw}"
            );
        }
    }

    #[test]
    fn workflow_permissions_are_explicit_and_legacy_snapshots_are_recognizable() {
        for role in PREDEFINED_ROLES {
            assert!(!role.permissions.contains(&"work_item:*"), "{} 不得继续授予任务通配权限", role.id);
            let desired = parse_permissions(role.permissions).unwrap();
            let previous = legacy_workflow_permission_snapshot(role.id, &desired).unwrap();
            if role.id == "role-management" {
                assert!(previous.iter().all(|permission| permission.to_string() != "work_item:manage"));
            } else {
                assert!(previous.iter().any(|permission| permission.to_string() == "work_item:*"));
            }
            if matches!(role.id, "role-sales-leader" | "role-operations") {
                assert!(
                    previous.iter().any(|permission| permission.to_string() == "sales_change_order:approve")
                );
            }
        }
    }

    #[test]
    fn approval_http_permissions_replace_recover_diagnose_and_team_actions() {
        assert_eq!(APPROVAL_HTTP_ACTION_PERMISSIONS.len(), 11);
        for role in PREDEFINED_ROLES {
            for forbidden in ["approval_instance:recover", "approval_instance:diagnose"] {
                assert!(!role.permissions.contains(&forbidden), "{} 不得继续授予 {forbidden}", role.id);
            }
            let desired = parse_permissions(role.permissions).unwrap();
            let previous = approval_http_legacy_snapshot(role.id, &desired).unwrap();
            assert!(
                previous
                    .iter()
                    .all(|permission| !APPROVAL_HTTP_ACTION_PERMISSIONS
                        .contains(&permission.to_string().as_str())),
                "{} 旧快照不得包含新动作权限",
                role.id
            );
            if role.id == "role-sysadmin" {
                for required in APPROVAL_HTTP_ACTION_PERMISSIONS {
                    assert!(role.permissions.contains(required), "系统管理员缺少 {required}");
                }
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
