//! 预定义角色种子升级逻辑（已知旧种子精确匹配后升级到当前推荐权限）。

use super::permission_tables::{
    FINANCE_PERMISSIONS, MANAGEMENT_PERMISSIONS, OPERATIONS_PERMISSIONS, PROCUREMENT_PERMISSIONS,
    SALES_LEADER_PERMISSIONS, SALES_PERMISSIONS, SYSADMIN_PERMISSIONS, WAREHOUSE_PERMISSIONS,
};
use super::{PREDEFINED_ROLES, SharedRbacService, parse_permissions};
use crate::entity::Permission;
use crate::error::Result;

#[cfg(test)]
pub(crate) fn low_margin_confirmation_legacy_snapshot(desired: &[Permission]) -> Vec<Permission> {
    remove_permissions(desired, &["sales_change_order:approve"])
}

/// 本轮审批 HTTP 动作级权限。
pub(crate) const APPROVAL_HTTP_ACTION_PERMISSIONS: &[&str] = &[
    "approval_process:read",
    "approval_process:create",
    "approval_process:edit",
    "approval_process:publish",
    "approval_process:retire",
    "approval_instance:read",
    "approval_instance:decide",
    "approval_instance:cancel",
    "approval_instance:resume",
    "approval_instance:cancel_blocked",
    "approval_instance:upgrade_binding",
];

/// 将仍保持领取/诊断/恢复种子的角色精确升级到 11 个审批动作权限。
///
/// # 错误
/// 权限解析或 Casbin 写入失败时返回错误。
pub(crate) async fn upgrade_approval_http_permissions(rbac: &SharedRbacService) -> Result<()> {
    upgrade_all_roles_from_snapshot(rbac, approval_http_legacy_snapshot).await
}

/// 从当前目标权限还原删除领取/诊断权限前的精确快照。
///
/// # 错误
/// 旧权限字符串无法解析时返回错误。
pub(crate) fn approval_http_legacy_snapshot(
    role_id: &str,
    desired: &[Permission],
) -> Result<Vec<Permission>> {
    let previous = remove_permissions(desired, APPROVAL_HTTP_ACTION_PERMISSIONS);
    let _ = role_id;
    Ok(previous)
}

/// 将仍保持旧工作流权限种子的角色收紧为显式最小动作。
pub(crate) async fn upgrade_workflow_permissions(rbac: &SharedRbacService) -> Result<()> {
    upgrade_all_roles_from_snapshot(rbac, legacy_workflow_permission_snapshot).await
}

/// 对全部预定义角色执行同一快照升级（审批动作与工作流收紧共用该循环）。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `snapshot` - 由当前推荐权限还原上一版默认种子的纯函数
///
/// # 返回值
/// 全部角色检查或升级完成后返回 `Ok(())`。
///
/// # 错误
/// 权限解析或 Casbin policy 写入失败时返回错误。
async fn upgrade_all_roles_from_snapshot(
    rbac: &SharedRbacService,
    snapshot: fn(&str, &[Permission]) -> Result<Vec<Permission>>,
) -> Result<()> {
    for role in PREDEFINED_ROLES {
        let desired = parse_permissions(role.permissions)?;
        let previous = snapshot(role.id, &desired)?;
        upgrade_exact(rbac, role.id, previous, desired).await?;
    }
    Ok(())
}

/// 对给定角色执行“移除若干权限即得旧种子”的精确升级（各岗位补齐升级共用该循环）。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `roles` - 角色 ID 与静态推荐权限
/// * `removed` - 本轮新增的权限代码；移除后即为上一版默认种子
///
/// # 返回值
/// 全部角色检查或升级完成后返回 `Ok(())`。
///
/// # 错误
/// 权限解析或 Casbin policy 写入失败时返回错误。
async fn upgrade_roles_by_removing(
    rbac: &SharedRbacService,
    roles: &[(&str, &[&str])],
    removed: &[&str],
) -> Result<()> {
    for (role_id, raw) in roles {
        let desired = parse_permissions(raw)?;
        let previous = remove_permissions(&desired, removed);
        upgrade_exact(rbac, role_id, previous, desired).await?;
    }
    Ok(())
}

/// 从当前目标权限确定性还原 D03 落地前的工作流权限快照。
pub(crate) fn legacy_workflow_permission_snapshot(
    role_id: &str,
    desired: &[Permission],
) -> Result<Vec<Permission>> {
    let mut previous = desired
        .iter()
        .filter(|permission| permission.resource() != "approval_instance")
        .filter(|permission| {
            !matches!(role_id, "role-sales-leader" | "role-operations")
                || permission.to_string() != "sales_change_order:approve"
        })
        .filter(|permission| {
            permission.resource() != "work_item"
                || (role_id == "role-management" && matches!(permission.action(), "list" | "detail"))
        })
        .cloned()
        .collect::<Vec<_>>();
    if role_id != "role-management" {
        previous.push(Permission::parse("work_item:*")?);
    }
    if matches!(role_id, "role-sales-leader" | "role-operations") {
        previous.push(Permission::parse("sales_change_order:approve")?);
        previous.push(Permission::parse("sales_change_order:reject")?);
    }
    Ok(previous)
}

/// 仅将仍保持历史默认种子的销售角色收紧为公司商品池只读权限。
pub(crate) async fn upgrade_sales_role_permissions(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(SALES_PERMISSIONS)?;
    for previous in sales_legacy_permission_snapshots(&desired)? {
        upgrade_exact(rbac, "role-sales", previous, desired.clone()).await?;
    }
    Ok(())
}

/// 构造销售角色已知历史默认权限快照，管理员自定义权限不在匹配范围内。
pub(crate) fn sales_legacy_permission_snapshots(desired: &[Permission]) -> Result<Vec<Vec<Permission>>> {
    let before_selection = remove_permissions(
        desired,
        &[
            "sales_selection_booklet:list",
            "sales_selection_booklet:get",
            "sales_selection_booklet:create",
            "sales_selection_booklet:maintain",
            "sales_selection_booklet:prepare",
            "sales_selection_booklet:publish",
            "sales_selection_booklet:copy_link",
            "sales_selection_booklet:rotate_link",
            "sales_selection_booklet:close",
            "sales_selection_booklet:revoke",
            "sales_selection_booklet:void",
            "sales_selection_proposal:list",
            "sales_selection_proposal:get",
        ],
    );
    let mut before_sellable_pool = remove_permissions(&before_selection, &["sellable_sku:list"]);
    for permission in [
        "product:list",
        "product_revision:list",
        "sku:list",
        "sku_revision:list",
        "product_category:list",
        "product_brand:list",
        "unit_of_measure:list",
    ] {
        before_sellable_pool.push(Permission::parse(permission)?);
    }

    let mut before_customer_boundary = remove_permissions(
        &before_sellable_pool,
        &[
            "customer:list",
            "customer:detail",
            "customer:create",
            "customer:update",
            "customer_sensitive:reveal",
        ],
    );
    before_customer_boundary.push(Permission::parse("customer:*")?);
    before_customer_boundary.push(Permission::parse("party_bank_account:*")?);
    Ok(vec![before_selection, before_sellable_pool, before_customer_boundary])
}

/// 仅为仍保持历史默认种子的财务角色补齐往来主体只读权限。
///
/// 登记回款/销项发票的往来主体选择器调用 `GET /admin/parties`（`party:list`），
/// 财务角色缺该权限时无法选择结算主体（E2E flow-01 W11 复现）。
pub(crate) async fn upgrade_finance_role_party_read_permissions(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(FINANCE_PERMISSIONS)?;
    for previous in finance_legacy_permission_snapshots(&desired)? {
        upgrade_exact(rbac, "role-finance", previous, desired.clone()).await?;
    }
    Ok(())
}

/// 本轮补齐前财务默认种子缺少的往来主体只读权限。
pub(crate) const FINANCE_PARTY_READ_GAP_PERMISSIONS: &[&str] =
    &["party:list", "party:detail", "party_revision:list"];

/// 构造可安全识别的历史财务默认权限快照。
pub(crate) fn finance_legacy_permission_snapshots(desired: &[Permission]) -> Result<Vec<Vec<Permission>>> {
    Ok(vec![
        remove_permissions(desired, &["party_revision:list"]),
        remove_permissions(desired, &["sales_change_order:list", "sales_change_order:detail"]),
        remove_permissions(
            desired,
            &["party_revision:list", "sales_change_order:list", "sales_change_order:detail"],
        ),
        remove_permissions(
            desired,
            &[
                "party:list",
                "party:detail",
                "party_revision:list",
                "sales_change_order:list",
                "sales_change_order:detail",
            ],
        ),
        remove_permissions(desired, FINANCE_PARTY_READ_GAP_PERMISSIONS),
    ])
}

/// 仅为仍保持历史默认种子的采购角色补齐当前供应商维护权限。
pub(crate) async fn upgrade_procurement_role_permissions(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(PROCUREMENT_PERMISSIONS)?;
    for previous in procurement_legacy_permission_snapshots(&desired)? {
        upgrade_exact(rbac, "role-procurement", previous, desired.clone()).await?;
    }
    Ok(())
}

/// 本轮补齐前采购默认种子缺少的职责权限。
///
/// 这些权限只追加、不替换既有动作；历史快照必须先剔除它们，才能继续精确
/// 识别上一版默认种子。
pub(crate) const PROCUREMENT_DUTY_GAP_PERMISSIONS: &[&str] = &[
    "integration_error_task:detail",
    "reconciliation_difference:detail",
    "legacy_import_batch:detail",
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
];

/// 构造可安全识别的历史采购默认权限快照。
pub(crate) fn procurement_legacy_permission_snapshots(
    desired: &[Permission],
) -> Result<Vec<Vec<Permission>>> {
    let before_duty_gaps = remove_permissions(desired, PROCUREMENT_DUTY_GAP_PERMISSIONS);
    let before_sellable_pool = remove_permissions(&before_duty_gaps, &["sellable_sku:list"]);
    let before_sensitive_reveal = remove_permissions(
        &before_duty_gaps,
        &["sellable_sku:list", "file_asset:preview", "supplier_sensitive:reveal"],
    );
    let mut catalog_era = remove_permissions(
        &before_duty_gaps,
        &[
            "sellable_sku:list",
            "supplier_sensitive:reveal",
            "file_asset:preview",
            "supplier_offering_availability:*",
            "supplier_offering_cost:detail",
        ],
    );
    for permission in [
        "supplier_catalog_cost:detail",
        "supplier_catalog_intake_batch:list",
        "supplier_catalog_product:*",
        "supplier_catalog_sku:list",
        "supplier_product_mapping:*",
    ] {
        catalog_era.push(Permission::parse(permission)?);
    }
    Ok(vec![before_sellable_pool, before_sensitive_reveal, catalog_era, before_duty_gaps])
}

/// 仅为仍保持旧默认种子的角色收紧客户范围并补齐字段级权限。
pub(crate) async fn upgrade_customer_role_boundaries(rbac: &SharedRbacService) -> Result<()> {
    upgrade_roles_by_removing(
        rbac,
        &[("role-finance", FINANCE_PERMISSIONS)],
        &[
            "customer_scope:detail",
            "customer_sensitive:reveal",
            "party_bank_account:list",
            "party_bank_account:detail",
            "party_bank_account:create",
            "party_bank_account:update",
            "party_bank_account:reveal",
        ],
    )
    .await?;
    upgrade_roles_by_removing(
        rbac,
        &[("role-sales-leader", SALES_LEADER_PERMISSIONS), ("role-management", MANAGEMENT_PERMISSIONS)],
        &["customer_scope:detail"],
    )
    .await
}

/// 为仍保持旧默认种子的公司商品池读者补齐独立查询权限。
pub(crate) async fn upgrade_sellable_sku_reader_permissions(rbac: &SharedRbacService) -> Result<()> {
    upgrade_roles_by_removing(
        rbac,
        &[("role-operations", OPERATIONS_PERMISSIONS), ("role-warehouse", WAREHOUSE_PERMISSIONS)],
        &["sellable_sku:list"],
    )
    .await
}

/// 为仍保持旧默认种子的 W18 五类业务责任角色补齐强类型确认权限。
pub(crate) async fn upgrade_import_confirmation_permissions(rbac: &SharedRbacService) -> Result<()> {
    upgrade_roles_by_removing(
        rbac,
        &[
            ("role-sales", SALES_PERMISSIONS),
            ("role-procurement", PROCUREMENT_PERMISSIONS),
            ("role-operations", OPERATIONS_PERMISSIONS),
            ("role-warehouse", WAREHOUSE_PERMISSIONS),
            ("role-finance", FINANCE_PERMISSIONS),
        ],
        &[
            "legacy_import_confirmation:list",
            "legacy_import_confirmation:detail",
            "legacy_import_confirmation:complete",
        ],
    )
    .await
}

/// 仅为仍保持上一版默认种子的 W29 固定责任角色补齐两项强命令权限。
pub(crate) async fn upgrade_integration_task_permissions(rbac: &SharedRbacService) -> Result<()> {
    upgrade_roles_by_removing(
        rbac,
        &[
            ("role-sales", SALES_PERMISSIONS),
            ("role-procurement", PROCUREMENT_PERMISSIONS),
            ("role-operations", OPERATIONS_PERMISSIONS),
            ("role-finance", FINANCE_PERMISSIONS),
            ("role-sysadmin", SYSADMIN_PERMISSIONS),
        ],
        INTEGRATION_TASK_GAP_PERMISSIONS,
    )
    .await
}

/// 从当前目标权限精确移除 W29 两项强命令权限，形成上一版默认种子。
#[cfg(test)]
pub(crate) fn integration_task_legacy_snapshot(desired: &[Permission]) -> Vec<Permission> {
    remove_permissions(desired, INTEGRATION_TASK_GAP_PERMISSIONS)
}

/// 本轮补齐前默认种子缺少的 W29 两项强命令权限。
pub(crate) const INTEGRATION_TASK_GAP_PERMISSIONS: &[&str] =
    &["integration_task:process", "integration_task:complete"];

/// 从预定义权限集合中移除指定稳定权限代码。
pub(crate) fn remove_permissions(permissions: &[Permission], removed: &[&str]) -> Vec<Permission> {
    permissions
        .iter()
        .filter(|permission| !removed.contains(&permission.to_string().as_str()))
        .cloned()
        .collect()
}

/// 执行一次仅匹配旧种子的安全权限升级。
pub(crate) async fn upgrade_exact(
    rbac: &SharedRbacService,
    role_id: &str,
    previous: Vec<Permission>,
    desired: Vec<Permission>,
) -> Result<()> {
    let legacy_workflow_previous = legacy_workflow_permission_snapshot(role_id, &previous)?;
    if legacy_workflow_previous != previous
        && rbac
            .upgrade_seeded_role_permissions_if_exact(role_id, legacy_workflow_previous, desired.clone())
            .await?
    {
        tracing::info!(role_id, "predefined role permissions upgraded");
        return Ok(());
    }
    if rbac.upgrade_seeded_role_permissions_if_exact(role_id, previous, desired).await? {
        tracing::info!(role_id, "predefined role permissions upgraded");
    }
    Ok(())
}
