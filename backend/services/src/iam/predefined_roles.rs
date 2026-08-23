//! 业务预定义角色种子。
//!
//! 应用启动时按固定角色 ID 幂等写入：
//! - 数据库中尚不存在（含软删除记录）时创建角色与推荐权限；
//! - 已存在且权限仍等于已知旧种子时，整集升级到当前推荐权限；
//! - 其余已存在角色只**追加**当前种子中尚未覆盖的权限，不删除管理员额外授予的权限，
//!   也不覆盖名称、启停状态与 system 标记。
//!
//! 角色集合与默认权限对齐第一期部门职责（`docs/erp-phase-1.md` §11）、
//! 工作台角色入口（`docs/ui-workspaces/w01-today-workspace.md` §2.1）以及二期销售领导审批轨。
//! `role-root` 由 [`super::ensure_root_role`] 单独维护，不在本清单中。

use entities::{Permission, RoleData};

use super::SharedRbacService;
use crate::errors::Result;

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
        description:
            "商品与供应商主数据、采购二次确认、采购单、履约、销售变更履约确认、采购退货与供应商结算协同。",
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
        description:
            "同步监控、集成错误、对账差异与来源注册；不替代业务部门改写商业事实，不含账号/角色超级权限。",
        permissions: SYSADMIN_PERMISSIONS,
    },
];

/// 销售推荐权限。
const SALES_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    "approval_instance:read",
    "approval_instance:decide",
    "approval_instance:cancel",
    "integration_task:process",
    "integration_task:complete",
    "legacy_import_confirmation:list",
    "legacy_import_confirmation:detail",
    "legacy_import_confirmation:complete",
    "file_asset:list",
    "file_asset:create",
    "file_asset:detail",
    "file_asset:update",
    "document_attachment:list",
    "document_attachment:create",
    "business_document:list",
    "business_document:detail",
    "document_relation:list",
    "document_participant:list",
    "party:list",
    "party:detail",
    "party:create",
    "party:update",
    "party_revision:list",
    "party_contact:*",
    "party_address:*",
    "party_tax_profile:*",
    "customer:list",
    "customer:detail",
    "customer:create",
    "customer:update",
    "customer_sensitive:reveal",
    "customer_assignment:list",
    "customer_assignment:create",
    "contract:*",
    "sales_order:*",
    "sales_change_order:list",
    "sales_change_order:create",
    "sales_change_order:detail",
    "sales_change_order:submit",
    "sales_change_order:delete",
    "customer_acceptance:*",
    "sellable_sku:list",
    "receivable_account:list",
    "receivable_account:detail",
    "customer_receipt:list",
    "customer_receipt:detail",
    "invoice:list",
    "invoice:detail",
    "master_mapping_task:list",
    "master_mapping_task:resolve",
];

/// 销售领导推荐权限。
const SALES_LEADER_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    // 直接指派的审批任务依赖 manage 参与权可见（与采购/财务等审批角色一致）；
    // 无 manage 时既非创建人又非参与人，任务会被参与权过滤掉。
    "work_item:manage",
    "approval_instance:read",
    "approval_instance:decide",
    "approval_instance:cancel",
    "file_asset:list",
    "file_asset:detail",
    "document_attachment:list",
    "business_document:list",
    "business_document:detail",
    "customer:list",
    "customer:detail",
    "customer_scope:detail",
    "contract:list",
    "contract:detail",
    "sales_order:list",
    "sales_order:detail",
    "sales_change_order:list",
    "sales_change_order:detail",
    "sales_change_order:approve",
    "sales_change_order:reject",
    "receivable_account:list",
    "receivable_account:detail",
    "customer_receipt:list",
    "customer_receipt:detail",
    "invoice:list",
    "invoice:detail",
    "cost_entry:list",
    "cost_entry:detail",
    "cost_allocation:list",
];

/// 采购推荐权限。
const PROCUREMENT_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    "approval_instance:read",
    "approval_instance:decide",
    "approval_instance:cancel",
    "integration_task:process",
    "integration_task:complete",
    // W29 责任任务对象读取；不授 list，治理菜单仍只对系统管理员开放。
    "integration_error_task:detail",
    "reconciliation_difference:detail",
    "legacy_import_confirmation:list",
    "legacy_import_confirmation:detail",
    "legacy_import_confirmation:complete",
    // W18 确认任务的业务对象读取权；不授 batch:list，避免打开导入治理菜单。
    "legacy_import_batch:detail",
    "file_asset:list",
    "file_asset:create",
    "file_asset:detail",
    "file_asset:preview",
    "file_asset:update",
    "document_attachment:list",
    "document_attachment:create",
    "business_document:list",
    "business_document:detail",
    "document_relation:list",
    "document_participant:list",
    // 商品主数据
    "product_category:*",
    "product_brand:*",
    "unit_of_measure:*",
    "sku_attribute:*",
    "sku_attribute_value:*",
    "product:*",
    "product_revision:list",
    "sku:list",
    "sku_revision:list",
    "sellable_sku:list",
    "voucher_category_profile:list",
    "warehouse:list",
    "warehouse:create",
    "warehouse:update",
    "warehouse_revision:list",
    "warehouse_sku_policy:*",
    // 供应商
    "party:list",
    "party:detail",
    "party:create",
    "party:update",
    "party_revision:list",
    "party_contact:*",
    "party_address:*",
    "party_tax_profile:*",
    "party_bank_account:*",
    "supplier:*",
    "supplier_sensitive:reveal",
    // 确认与采购
    "contract:detail",
    "sales_order:list",
    "sales_order:detail",
    "purchase_order:*",
    "purchase_change_order:*",
    // 销售变更履约影响确认（财务复核仍由 work_item 责任角色隔离）
    "sales_change_order:list",
    "sales_change_order:detail",
    "sales_change_order:approve",
    "sales_change_order:reject",
    // 采购退货（财务保留票款冲正，不替代采购建单）
    "purchase_return_order:list",
    "purchase_return_order:create",
    "purchase_return_order:detail",
    // 履约作业
    "purchase_receipt:*",
    "delivery:*",
    "electronic_delivery:*",
    "service_fulfillment:*",
    // 库存只读：核验入库结果、预占与变更影响
    "stock_balance:list",
    "stock_balance:detail",
    "stock_movement:list",
    "stock_reservation:list",
    // 供应商供给 / API / 订单
    "supplier_offering:*",
    "supplier_offering_availability:*",
    "supplier_offering_cost:detail",
    "supplier_api_connection:list",
    "supplier_api_connection:create",
    "supplier_api_connection:detail",
    "supplier_api_connection:update",
    "supplier_api_connection:update_business_profile",
    "supplier_api_capability:list",
    "supplier_api_capability:confirm_requirement",
    "supplier_fulfillment_order:*",
    "supplier_refund_fact:post",
    "product_publication:*",
    "product_publication_revision:*",
    "product_publication_delivery:*",
    // 应付只读
    "payable_account:list",
    "payable_account:detail",
    "supplier_payment:list",
    "supplier_payment:detail",
    // 供应商结算协同：查看与补证；正式结论由结算经办人在服务层二次校验
    "supplier_settlement_statement:list",
    "supplier_settlement_statement:detail",
    "supplier_settlement_item:list",
    "supplier_settlement_difference:list",
    "supplier_settlement_difference:update",
];

/// 运营推荐权限。
const OPERATIONS_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    "approval_instance:read",
    "approval_instance:decide",
    "approval_instance:cancel",
    "integration_task:process",
    "integration_task:complete",
    "legacy_import_confirmation:list",
    "legacy_import_confirmation:detail",
    "legacy_import_confirmation:complete",
    "file_asset:list",
    "file_asset:create",
    "file_asset:detail",
    "file_asset:update",
    "document_attachment:list",
    "document_attachment:create",
    "business_document:list",
    "business_document:detail",
    "product_category:list",
    "product:list",
    "product_revision:list",
    "sku:list",
    "sku_revision:list",
    "sellable_sku:list",
    "voucher_category_profile:*",
    "sales_order:list",
    "sales_order:detail",
    "sales_change_order:list",
    "sales_change_order:detail",
    "sales_change_order:approve",
    "sales_change_order:reject",
    "master_mapping_task:*",
    "mall_sales_sync_job:list",
    "mall_sales_sync_job:detail",
    "mall_sales_order_snapshot:list",
    "mall_sales_reconciliation_job:list",
    "mall_sales_reconciliation_item:list",
    "mall_sales_reconciliation_item:resolve",
    "sales_order_projection:*",
    "sales_order_projection_revision:*",
    "sales_order_projection_delivery:*",
    "product_publication:*",
    "product_publication_revision:*",
    "product_publication_delivery:*",
    "mall_order:list",
    "mall_order:detail",
    "mall_order_fact:list",
    "mall_card_instance:list",
    "mall_card_instance:detail",
    "mall_balance_snapshot:list",
    "mall_refund:list",
    "mall_balance_restoration:list",
    "mall_after_sales_request:list",
    "supplier_offering:list",
    "supplier_fulfillment_order:list",
    "supplier_fulfillment_order:detail",
];

/// 仓储推荐权限。
const WAREHOUSE_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    "approval_instance:read",
    "approval_instance:decide",
    "approval_instance:cancel",
    "legacy_import_confirmation:list",
    "legacy_import_confirmation:detail",
    "legacy_import_confirmation:complete",
    "file_asset:list",
    "file_asset:create",
    "file_asset:detail",
    "document_attachment:list",
    "document_attachment:create",
    "business_document:list",
    "business_document:detail",
    "warehouse:list",
    "warehouse:detail",
    "warehouse_revision:list",
    "warehouse_sku_policy:list",
    "purchase_order:list",
    "purchase_order:detail",
    "purchase_receipt:*",
    "delivery:*",
    "stock_balance:list",
    "stock_balance:detail",
    "stock_movement:list",
    "stock_reservation:list",
    "stock_adjustment:*",
    "product:list",
    "sku:list",
    "sellable_sku:list",
];

/// 财务推荐权限。
const FINANCE_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    "approval_instance:read",
    "approval_instance:decide",
    "approval_instance:cancel",
    "integration_task:process",
    "integration_task:complete",
    "legacy_import_confirmation:list",
    "legacy_import_confirmation:detail",
    "legacy_import_confirmation:complete",
    "file_asset:list",
    "file_asset:create",
    "file_asset:detail",
    "file_asset:update",
    "document_attachment:list",
    "document_attachment:create",
    "business_document:list",
    "business_document:detail",
    "document_relation:list",
    "document_participant:list",
    // 上下文只读（往来主体搜索：登记回款/销项发票需选择结算主体）
    "party:list",
    "party:detail",
    "customer:list",
    "customer:detail",
    "customer_scope:detail",
    "customer_sensitive:reveal",
    "party_bank_account:list",
    "party_bank_account:detail",
    "party_bank_account:create",
    "party_bank_account:update",
    "party_bank_account:reveal",
    "supplier:list",
    "supplier:detail",
    "sales_order:list",
    "sales_order:detail",
    "purchase_order:list",
    "purchase_order:detail",
    "purchase_order:review",
    // 采购变更复核：审批区与变更清单需要读取变更单（决定走 approval_instance:decide）
    "purchase_change_order:list",
    "purchase_change_order:detail",
    // 库存调整复核：审批区与调整单详情需要读取调整单（决定走 approval_instance:decide）
    "stock_adjustment:list",
    "stock_adjustment:detail",
    "contract:list",
    "contract:detail",
    // 客户往来
    "receivable_account:*",
    "receivable_funds_review:*",
    "customer_receipt:*",
    "invoice:*",
    // 供应商往来
    "payable_account:*",
    "supplier_payment:*",
    "purchase_invoice_allocation:*",
    // 成本与结算
    "cost_entry:*",
    "cost_allocation:list",
    "supplier_settlement_statement:*",
    "supplier_settlement_item:list",
    "supplier_settlement_difference:*",
    // 退货退款冲正
    "sales_return_case:*",
    "purchase_return_order:*",
    "customer_refund:*",
    "supplier_refund:*",
    "receipt_reversal:*",
    "payment_reversal:*",
    // 卡券消费台账只读 + 复核相关
    "mall_order:list",
    "mall_order:detail",
    "mall_order_fact:list",
    "mall_card_instance:list",
    "mall_card_instance:detail",
    "mall_balance_snapshot:list",
];

/// 管理层业务只读与待办责任管理推荐权限。
const MANAGEMENT_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    "work_item:manage",
    "work_item:reassign",
    "approval_process:read",
    "approval_instance:read",
    "approval_instance:cancel",
    "approval_instance:resume",
    "approval_instance:cancel_blocked",
    "file_asset:list",
    "file_asset:detail",
    "document_attachment:list",
    "business_document:list",
    "business_document:detail",
    "customer:list",
    "customer:detail",
    "customer_scope:detail",
    "supplier:list",
    "supplier:detail",
    "contract:list",
    "contract:detail",
    "sales_order:list",
    "sales_order:detail",
    "purchase_order:list",
    "purchase_order:detail",
    // 管理层打开变更任务详情页需读取变更单（只读，不参与决定）
    "purchase_change_order:list",
    "purchase_change_order:detail",
    "stock_adjustment:list",
    "stock_adjustment:detail",
    "stock_balance:list",
    "stock_balance:detail",
    "stock_movement:list",
    "receivable_account:list",
    "receivable_account:detail",
    "customer_receipt:list",
    "customer_receipt:detail",
    "invoice:list",
    "invoice:detail",
    "payable_account:list",
    "payable_account:detail",
    "supplier_payment:list",
    "supplier_payment:detail",
    "cost_entry:list",
    "cost_entry:detail",
    "cost_allocation:list",
    "mall_order:list",
    "mall_order:detail",
    "mall_card_instance:list",
    "mall_card_instance:detail",
    "mall_balance_snapshot:list",
    "supplier_settlement_statement:list",
    "supplier_settlement_statement:detail",
];

/// 系统管理员（技术运维，非超级管理员）推荐权限。
const SYSADMIN_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
    "work_item:manage",
    "work_item:reassign",
    "work_item:close",
    "integration_task:process",
    "integration_task:complete",
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
    "file_asset:list",
    "file_asset:detail",
    "document_attachment:list",
    "business_document:list",
    "business_document:detail",
    "audit_log:list",
    "audit_event:list",
    "source_system:*",
    "external_identity_map:*",
    "mall_sales_sync_job:*",
    "mall_sales_order_snapshot:*",
    "mall_sales_sync_cursor:detail",
    "mall_sales_reconciliation_job:*",
    "mall_sales_reconciliation_item:*",
    "master_mapping_task:list",
    "legacy_import_batch:*",
    "legacy_import_row:list",
    "legacy_import_confirmation:*",
    "inbox_message:*",
    "integration_error_task:*",
    "reconciliation_difference:*",
    "mall_consumption_backfill_job:*",
    "mall_consumption_backfill_item:list",
    "mall_consumption_cutover:*",
    "mall_card_instance:list",
    "mall_card_instance:detail",
    "mall_card_instance:create",
    "mall_balance_snapshot:list",
    "mall_balance_snapshot:create",
    "mall_card_instance_correction:list",
    "mall_order:list",
    "mall_order:detail",
    "mall_order_fact:list",
    "mall_order_fact:submit",
    "supplier_api_connection:list",
    "supplier_api_connection:detail",
    "supplier_api_connection:bind_endpoint_reference",
    "supplier_api_connection:manage_credential_reference",
    "supplier_api_connection:health_check",
    "supplier_api_connection:enable",
    "supplier_api_connection:disable",
    "supplier_api_connection:catalog_sync",
    "supplier_api_connection:view_reference_metadata",
    "supplier_api_capability:list",
    "supplier_api_capability:update",
    "bulk_selection_snapshot:*",
    "bulk_selection_item:list",
    "background_job:*",
    "background_job_item:list",
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
    upgrade_purchase_review_permissions(rbac).await?;
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
        let desired = parse_permissions(role.permissions)?;
        if rbac
            .ensure_missing_seeded_role_permissions(role.id, desired)
            .await?
        {
            tracing::info!(
                role_id = role.id,
                role_name = role.name,
                "predefined role missing permissions appended"
            );
        }
    }
    Ok(())
}

/// 将仍保持 W08 双路由权限种子的财务角色硬切到唯一审核命令权限。
async fn upgrade_purchase_review_permissions(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(FINANCE_PERMISSIONS)?;
    let previous = purchase_review_legacy_snapshot(&desired)?;
    upgrade_exact(rbac, "role-finance", previous, desired).await
}

/// 从 W08 当前权限确定性还原双审核路由时期的财务默认快照。
fn purchase_review_legacy_snapshot(desired: &[Permission]) -> Result<Vec<Permission>> {
    let mut previous = remove_permissions(desired, &["purchase_order:review"]);
    previous.push(Permission::parse("purchase_order:approve")?);
    previous.push(Permission::parse("purchase_order:reject")?);
    Ok(previous)
}

/// 将仍保持上一版默认种子的采购/系统管理员权限收口到 W20 固定职责。
async fn upgrade_supplier_connection_governance_permissions(rbac: &SharedRbacService) -> Result<()> {
    let procurement_desired = parse_permissions(PROCUREMENT_PERMISSIONS)?;
    let mut procurement_previous = remove_permissions(
        &procurement_desired,
        &[
            "supplier_api_connection:update_business_profile",
            "supplier_api_capability:confirm_requirement",
        ],
    );
    procurement_previous.push(Permission::parse("supplier_api_connection:health_check")?);
    upgrade_exact(
        rbac,
        "role-procurement",
        procurement_previous,
        procurement_desired,
    )
    .await?;

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
#[cfg(test)]
fn low_margin_confirmation_legacy_snapshot(desired: &[Permission]) -> Vec<Permission> {
    remove_permissions(desired, &["sales_change_order:approve"])
}

/// 本轮审批 HTTP 动作级权限。
const APPROVAL_HTTP_ACTION_PERMISSIONS: &[&str] = &[
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
async fn upgrade_approval_http_permissions(rbac: &SharedRbacService) -> Result<()> {
    for role in PREDEFINED_ROLES {
        let desired = parse_permissions(role.permissions)?;
        let previous = approval_http_legacy_snapshot(role.id, &desired)?;
        upgrade_exact(rbac, role.id, previous, desired).await?;
    }
    Ok(())
}

/// 从当前目标权限还原删除领取/诊断权限前的精确快照。
///
/// # 错误
/// 旧权限字符串无法解析时返回错误。
fn approval_http_legacy_snapshot(role_id: &str, desired: &[Permission]) -> Result<Vec<Permission>> {
    let previous = remove_permissions(desired, APPROVAL_HTTP_ACTION_PERMISSIONS);
    let _ = role_id;
    Ok(previous)
}

/// 将仍保持旧工作流权限种子的角色收紧为显式最小动作。
async fn upgrade_workflow_permissions(rbac: &SharedRbacService) -> Result<()> {
    for role in PREDEFINED_ROLES {
        let desired = parse_permissions(role.permissions)?;
        let previous = legacy_workflow_permission_snapshot(role.id, &desired)?;
        upgrade_exact(rbac, role.id, previous, desired).await?;
    }
    Ok(())
}

/// 从当前目标权限确定性还原 D03 落地前的工作流权限快照。
fn legacy_workflow_permission_snapshot(role_id: &str, desired: &[Permission]) -> Result<Vec<Permission>> {
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
async fn upgrade_sales_role_permissions(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(SALES_PERMISSIONS)?;
    for previous in sales_legacy_permission_snapshots(&desired)? {
        upgrade_exact(rbac, "role-sales", previous, desired.clone()).await?;
    }
    Ok(())
}

/// 构造销售角色已知历史默认权限快照，管理员自定义权限不在匹配范围内。
fn sales_legacy_permission_snapshots(desired: &[Permission]) -> Result<Vec<Vec<Permission>>> {
    let mut before_sellable_pool = remove_permissions(desired, &["sellable_sku:list"]);
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
    Ok(vec![before_sellable_pool, before_customer_boundary])
}

/// 仅为仍保持历史默认种子的财务角色补齐往来主体只读权限。
///
/// 登记回款/销项发票的往来主体选择器调用 `GET /admin/parties`（`party:list`），
/// 财务角色缺该权限时无法选择结算主体（E2E flow-01 W11 复现）。
async fn upgrade_finance_role_party_read_permissions(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(FINANCE_PERMISSIONS)?;
    for previous in finance_legacy_permission_snapshots(&desired)? {
        upgrade_exact(rbac, "role-finance", previous, desired.clone()).await?;
    }
    Ok(())
}

/// 本轮补齐前财务默认种子缺少的往来主体只读权限。
const FINANCE_PARTY_READ_GAP_PERMISSIONS: &[&str] = &["party:list", "party:detail"];

/// 构造可安全识别的历史财务默认权限快照。
fn finance_legacy_permission_snapshots(desired: &[Permission]) -> Result<Vec<Vec<Permission>>> {
    Ok(vec![remove_permissions(
        desired,
        FINANCE_PARTY_READ_GAP_PERMISSIONS,
    )])
}

/// 仅为仍保持历史默认种子的采购角色补齐当前供应商维护权限。
async fn upgrade_procurement_role_permissions(rbac: &SharedRbacService) -> Result<()> {
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
const PROCUREMENT_DUTY_GAP_PERMISSIONS: &[&str] = &[
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
fn procurement_legacy_permission_snapshots(desired: &[Permission]) -> Result<Vec<Vec<Permission>>> {
    let before_duty_gaps = remove_permissions(desired, PROCUREMENT_DUTY_GAP_PERMISSIONS);
    let before_sellable_pool = remove_permissions(&before_duty_gaps, &["sellable_sku:list"]);
    let before_sensitive_reveal = remove_permissions(
        &before_duty_gaps,
        &[
            "sellable_sku:list",
            "file_asset:preview",
            "supplier_sensitive:reveal",
        ],
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
    Ok(vec![
        before_sellable_pool,
        before_sensitive_reveal,
        catalog_era,
        before_duty_gaps,
    ])
}

/// 仅为仍保持旧默认种子的角色收紧客户范围并补齐字段级权限。
async fn upgrade_customer_role_boundaries(rbac: &SharedRbacService) -> Result<()> {
    let finance_desired = parse_permissions(FINANCE_PERMISSIONS)?;
    let finance_previous = remove_permissions(
        &finance_desired,
        &[
            "customer_scope:detail",
            "customer_sensitive:reveal",
            "party_bank_account:list",
            "party_bank_account:detail",
            "party_bank_account:create",
            "party_bank_account:update",
            "party_bank_account:reveal",
        ],
    );
    upgrade_exact(rbac, "role-finance", finance_previous, finance_desired).await?;

    for (role_id, raw) in [
        ("role-sales-leader", SALES_LEADER_PERMISSIONS),
        ("role-management", MANAGEMENT_PERMISSIONS),
    ] {
        let desired = parse_permissions(raw)?;
        let previous = remove_permissions(&desired, &["customer_scope:detail"]);
        upgrade_exact(rbac, role_id, previous, desired).await?;
    }
    Ok(())
}

/// 为仍保持旧默认种子的公司商品池读者补齐独立查询权限。
async fn upgrade_sellable_sku_reader_permissions(rbac: &SharedRbacService) -> Result<()> {
    for (role_id, raw) in [
        ("role-operations", OPERATIONS_PERMISSIONS),
        ("role-warehouse", WAREHOUSE_PERMISSIONS),
    ] {
        let desired = parse_permissions(raw)?;
        let previous = remove_permissions(&desired, &["sellable_sku:list"]);
        upgrade_exact(rbac, role_id, previous, desired).await?;
    }
    Ok(())
}

/// 为仍保持旧默认种子的 W18 五类业务责任角色补齐强类型确认权限。
async fn upgrade_import_confirmation_permissions(rbac: &SharedRbacService) -> Result<()> {
    for (role_id, raw) in [
        ("role-sales", SALES_PERMISSIONS),
        ("role-procurement", PROCUREMENT_PERMISSIONS),
        ("role-operations", OPERATIONS_PERMISSIONS),
        ("role-warehouse", WAREHOUSE_PERMISSIONS),
        ("role-finance", FINANCE_PERMISSIONS),
    ] {
        let desired = parse_permissions(raw)?;
        let previous = remove_permissions(
            &desired,
            &[
                "legacy_import_confirmation:list",
                "legacy_import_confirmation:detail",
                "legacy_import_confirmation:complete",
            ],
        );
        upgrade_exact(rbac, role_id, previous, desired).await?;
    }
    Ok(())
}

/// 仅为仍保持上一版默认种子的 W29 固定责任角色补齐两项强命令权限。
async fn upgrade_integration_task_permissions(rbac: &SharedRbacService) -> Result<()> {
    for (role_id, raw) in [
        ("role-sales", SALES_PERMISSIONS),
        ("role-procurement", PROCUREMENT_PERMISSIONS),
        ("role-operations", OPERATIONS_PERMISSIONS),
        ("role-finance", FINANCE_PERMISSIONS),
        ("role-sysadmin", SYSADMIN_PERMISSIONS),
    ] {
        let desired = parse_permissions(raw)?;
        let previous = integration_task_legacy_snapshot(&desired);
        upgrade_exact(rbac, role_id, previous, desired).await?;
    }
    Ok(())
}

/// 从当前目标权限精确移除 W29 两项强命令权限，形成上一版默认种子。
fn integration_task_legacy_snapshot(desired: &[Permission]) -> Vec<Permission> {
    remove_permissions(
        desired,
        &["integration_task:process", "integration_task:complete"],
    )
}

/// 从预定义权限集合中移除指定稳定权限代码。
fn remove_permissions(permissions: &[Permission], removed: &[&str]) -> Vec<Permission> {
    permissions
        .iter()
        .filter(|permission| !removed.contains(&permission.to_string().as_str()))
        .cloned()
        .collect()
}

/// 执行一次仅匹配旧种子的安全权限升级。
async fn upgrade_exact(
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
    if rbac
        .upgrade_seeded_role_permissions_if_exact(role_id, previous, desired)
        .await?
    {
        tracing::info!(role_id, "predefined role permissions upgraded");
    }
    Ok(())
}

/// 幂等写入单条预定义角色。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `role` - 预定义角色静态定义
///
/// # 返回值
/// 已存在并跳过，或新建成功时返回 `Ok(())`。
///
/// # 错误
/// 权限解析失败，或创建角色/policy 失败（并发冲突除外，冲突视为已由其他实例写入）时返回错误。
async fn seed_one(rbac: &SharedRbacService, role: &PredefinedRoleDef) -> Result<()> {
    let permissions = parse_permissions(role.permissions)?;
    let data = RoleData {
        name: role.name.to_string(),
        description: Some(role.description.to_string()),
        // 可分配、可后续由管理员调整；仅 `role-root` 使用 system=true 的强保护边界。
        system: false,
    };
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
    raw.iter()
        .map(|permission| Permission::parse(*permission).map_err(Into::into))
        .collect()
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
        approval_http_legacy_snapshot, integration_task_legacy_snapshot, legacy_workflow_permission_snapshot,
        low_margin_confirmation_legacy_snapshot, parse_permissions, predefined_role_ids,
        procurement_legacy_permission_snapshots, purchase_review_legacy_snapshot,
        sales_legacy_permission_snapshots, APPROVAL_HTTP_ACTION_PERMISSIONS, FINANCE_PERMISSIONS,
        PREDEFINED_ROLES, PROCUREMENT_PERMISSIONS, SALES_LEADER_PERMISSIONS, SALES_PERMISSIONS,
    };
    use entities::{Permission, PermissionSet};

    #[test]
    fn predefined_role_ids_are_unique_and_stable() {
        let ids = predefined_role_ids();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids.len(), sorted.len(), "预定义角色 ID 必须唯一");
        assert!(ids.iter().all(|id| id.starts_with("role-")));
        assert!(!ids.contains(&"role-root"), "root 由 ensure_root_role 单独维护");
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
    fn finance_purchase_review_permission_hard_cuts_legacy_actions() {
        let desired = parse_permissions(FINANCE_PERMISSIONS).unwrap();
        let previous = purchase_review_legacy_snapshot(&desired).unwrap();
        let desired_codes = desired.iter().map(ToString::to_string).collect::<Vec<_>>();
        let previous_codes = previous.iter().map(ToString::to_string).collect::<Vec<_>>();

        assert!(desired_codes.contains(&"purchase_order:review".to_string()));
        assert!(!desired_codes.contains(&"purchase_order:approve".to_string()));
        assert!(!desired_codes.contains(&"purchase_order:reject".to_string()));
        assert!(!previous_codes.contains(&"purchase_order:review".to_string()));
        assert!(previous_codes.contains(&"purchase_order:approve".to_string()));
        assert!(previous_codes.contains(&"purchase_order:reject".to_string()));
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
        let procurement = PREDEFINED_ROLES
            .iter()
            .find(|role| role.id == "role-procurement")
            .unwrap();
        assert!(procurement
            .permissions
            .contains(&"supplier_api_connection:update_business_profile"));
        assert!(procurement
            .permissions
            .contains(&"supplier_api_capability:confirm_requirement"));
        assert!(!procurement
            .permissions
            .contains(&"supplier_api_connection:health_check"));

        let sysadmin = PREDEFINED_ROLES
            .iter()
            .find(|role| role.id == "role-sysadmin")
            .unwrap();
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
        assert!(!sysadmin
            .permissions
            .contains(&"supplier_api_capability:confirm_requirement"));

        let operations = PREDEFINED_ROLES
            .iter()
            .find(|role| role.id == "role-operations")
            .unwrap();
        for permission in [
            "supplier_api_connection:bind_endpoint_reference",
            "supplier_api_connection:manage_credential_reference",
            "supplier_api_connection:enable",
            "supplier_api_connection:disable",
            "supplier_api_connection:catalog_sync",
            "supplier_api_capability:update",
        ] {
            assert!(
                !operations.permissions.contains(&permission),
                "运营不应取得 {permission}"
            );
        }
    }

    #[test]
    fn import_confirmation_permissions_match_fixed_responsibility_roles() {
        let responsibility_roles = [
            "role-sales",
            "role-procurement",
            "role-operations",
            "role-warehouse",
            "role-finance",
        ];
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
        let responsibility_roles = [
            "role-sales",
            "role-procurement",
            "role-operations",
            "role-finance",
            "role-sysadmin",
        ];
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
            assert!(previous
                .iter()
                .all(|permission| permission.resource() != "integration_task"));
        }
    }

    #[test]
    fn purchase_change_read_follows_review_responsibility_roles() {
        // 采购变更由财务复核；管理层可打开任务详情。两者必须能读取变更单，
        // 决定本身走 approval_instance:decide，无需 approve/reject 资源动作。
        // 采购用通配 `purchase_change_order:*` 覆盖，按解析后的覆盖关系断言。
        let required = ["purchase_change_order:list", "purchase_change_order:detail"];
        let read_roles = ["role-procurement", "role-finance", "role-management"];
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
        let leader = PREDEFINED_ROLES
            .iter()
            .find(|role| role.id == "role-sales-leader")
            .unwrap();
        for permission in required {
            assert!(
                leader.permissions.contains(&permission),
                "销售领导缺少 {permission}"
            );
        }

        let desired = parse_permissions(SALES_LEADER_PERMISSIONS).unwrap();
        let previous = low_margin_confirmation_legacy_snapshot(&desired);
        assert_eq!(previous.len() + 1, desired.len());
        for permission in required {
            assert!(
                previous
                    .iter()
                    .all(|candidate| candidate.to_string() != permission),
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
            "supplier_payment:create",
            "supplier_settlement_statement:confirm",
            "supplier_settlement_statement:create",
            "integration_error_task:list",
            "reconciliation_difference:list",
            "legacy_import_batch:list",
            "stock_adjustment:create",
        ] {
            let forbidden = Permission::parse(forbidden).unwrap();
            assert!(
                !permissions.covers_one(&forbidden),
                "采购不应默认具备 {forbidden}"
            );
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
        assert!(snapshots[2]
            .iter()
            .any(|permission| permission.to_string() == "supplier_catalog_cost:detail"));
        assert!(!snapshots[2]
            .iter()
            .any(|permission| permission.to_string() == "supplier_sensitive:reveal"));
        assert!(snapshots
            .iter()
            .flatten()
            .all(|permission| super::PROCUREMENT_DUTY_GAP_PERMISSIONS
                .iter()
                .all(|gap| permission.to_string() != *gap)));
    }

    #[test]
    fn sales_permissions_include_core_sales_capabilities() {
        let permissions = parse_permissions(SALES_PERMISSIONS).unwrap();
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("sales_order:create").unwrap())));
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("sales_order:cancel_approval").unwrap())));
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("customer:list").unwrap())));
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("contract:create").unwrap())));
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("sellable_sku:list").unwrap())));
        for forbidden in [
            "product:list",
            "product_revision:list",
            "sku:list",
            "sku_revision:list",
            "product_category:list",
            "product_brand:list",
            "unit_of_measure:list",
        ] {
            assert!(!permissions
                .iter()
                .any(|p| p.covers(&Permission::parse(forbidden).unwrap())));
        }
        assert!(!permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("party_bank_account:detail").unwrap())));
        assert!(!permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("customer_scope:detail").unwrap())));
        assert!(
            !permissions
                .iter()
                .any(|p| p.covers(&Permission::parse("purchase_order:review").unwrap())),
            "销售不应默认具备采购财务审核权"
        );
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
        assert_eq!(snapshots[0].len(), desired.len() + 6);
        assert_eq!(snapshots[1].len(), desired.len() + 3);
        assert!(snapshots[0]
            .iter()
            .any(|permission| permission.to_string() == "sku:list"));
        assert!(snapshots[1]
            .iter()
            .any(|permission| permission.to_string() == "party_bank_account:*"));
        assert!(snapshots
            .iter()
            .flatten()
            .all(|permission| permission.to_string() != "sellable_sku:list"));
    }

    #[test]
    fn management_permissions_are_read_only() {
        let management = PREDEFINED_ROLES
            .iter()
            .find(|role| role.id == "role-management")
            .unwrap();
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
            assert!(
                !role.permissions.contains(&"work_item:*"),
                "{} 不得继续授予任务通配权限",
                role.id
            );
            let desired = parse_permissions(role.permissions).unwrap();
            let previous = legacy_workflow_permission_snapshot(role.id, &desired).unwrap();
            if role.id == "role-management" {
                assert!(previous
                    .iter()
                    .all(|permission| permission.to_string() != "work_item:manage"));
            } else {
                assert!(previous
                    .iter()
                    .any(|permission| permission.to_string() == "work_item:*"));
            }
            if matches!(role.id, "role-sales-leader" | "role-operations") {
                assert!(previous
                    .iter()
                    .any(|permission| permission.to_string() == "sales_change_order:approve"));
            }
        }
    }

    #[test]
    fn approval_http_permissions_replace_recover_diagnose_and_team_actions() {
        assert_eq!(APPROVAL_HTTP_ACTION_PERMISSIONS.len(), 11);
        for role in PREDEFINED_ROLES {
            for forbidden in ["approval_instance:recover", "approval_instance:diagnose"] {
                assert!(
                    !role.permissions.contains(&forbidden),
                    "{} 不得继续授予 {forbidden}",
                    role.id
                );
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
        let sysadmin = PREDEFINED_ROLES
            .iter()
            .find(|role| role.id == "role-sysadmin")
            .unwrap();
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
}
