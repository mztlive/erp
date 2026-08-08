//! 业务预定义角色种子。
//!
//! 应用启动时按固定角色 ID 幂等写入：数据库中尚不存在（含软删除记录）时创建角色与权限；
//! 已存在则**不覆盖**名称、状态与 Casbin policy，避免冲掉管理员的后续配置。
//!
//! 角色集合与默认权限对齐第一期部门职责（`docs/erp-phase-1.md` §11）、
//! 工作台角色入口（`docs/ui-workspaces/w01-today-workspace.md` §2.1）以及二期销售领导审批轨。
//! `role-root` 由 [`super::ensure_root_role`] 单独维护，不在本清单中。

use entities::{Permission, RoleData};

use super::SharedRbacService;
use crate::errors::Result;

/// 单条预定义角色的静态定义。
#[derive(Debug, Clone, Copy)]
pub(super) struct PredefinedRoleDef {
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
pub(super) const PREDEFINED_ROLES: &[PredefinedRoleDef] = &[
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
        description: "商品与供应商主数据、采购二次确认、采购单、履约与供应商目录/API 协同。",
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
        description: "经营质量、盈亏、履约与票款汇总只读，不参与单据流转写操作。",
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
    "work_item:*",
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
    "product:list",
    "product_revision:list",
    "sku:list",
    "sku_revision:list",
    "product_category:list",
    "product_brand:list",
    "unit_of_measure:list",
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
    "work_item:*",
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
    "sales_order_review:list",
    "sales_order_review:approve",
    "sales_order_review:reject",
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
    "work_item:*",
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
    // 确认与采购
    "contract:detail",
    "sales_order:list",
    "sales_order:detail",
    "procurement_confirmation:*",
    "purchase_order:*",
    "purchase_change_order:*",
    // 履约作业
    "purchase_receipt:*",
    "delivery:*",
    "electronic_delivery:*",
    "service_fulfillment:*",
    // 供应商目录 / API / 订单
    "supplier_catalog_product:*",
    "supplier_catalog_sku:list",
    "supplier_product_mapping:*",
    "supplier_offering:*",
    "supplier_catalog_cost:detail",
    "supplier_catalog_intake_batch:list",
    "supplier_api_connection:list",
    "supplier_api_connection:create",
    "supplier_api_connection:detail",
    "supplier_api_connection:update",
    "supplier_api_connection:health_check",
    "supplier_api_capability:list",
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
];

/// 运营推荐权限。
const OPERATIONS_PERMISSIONS: &[&str] = &[
    "work_item:*",
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
    "voucher_category_profile:*",
    "sales_order:list",
    "sales_order:detail",
    "sales_order_review:list",
    "sales_order_review:approve",
    "sales_order_review:reject",
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
    "supplier_catalog_product:list",
    "supplier_catalog_product:detail",
    "supplier_product_mapping:list",
    "supplier_fulfillment_order:list",
    "supplier_fulfillment_order:detail",
];

/// 仓储推荐权限。
const WAREHOUSE_PERMISSIONS: &[&str] = &[
    "work_item:*",
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
];

/// 财务推荐权限。
const FINANCE_PERMISSIONS: &[&str] = &[
    "work_item:*",
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
    // 上下文只读
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
    "purchase_order:approve",
    "purchase_order:reject",
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

/// 管理层只读推荐权限。
const MANAGEMENT_PERMISSIONS: &[&str] = &[
    "work_item:list",
    "work_item:detail",
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
    "work_item:*",
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
    "supplier_api_connection:health_check",
    "supplier_api_capability:list",
    "bulk_selection_snapshot:*",
    "bulk_selection_item:list",
    "background_job:*",
    "background_job_item:list",
];

/// 确保全部业务预定义角色已写入数据库。
///
/// 对每个固定角色 ID：若不存在则创建角色实体与 Casbin 权限；已有角色仅在权限
/// 与已知旧种子完全相等时升级。管理员增删过权限的角色不得被启动过程覆盖。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回值
/// 全部角色检查或创建完成后返回 `Ok(())`。
///
/// # 错误
/// 角色校验、MongoDB 写入或 Casbin policy 事务失败时返回错误。
pub async fn ensure_predefined_roles(rbac: &SharedRbacService) -> Result<()> {
    for role in PREDEFINED_ROLES {
        seed_one(rbac, role).await?;
    }
    upgrade_procurement_contract_read(rbac).await?;
    upgrade_customer_role_boundaries(rbac).await?;
    Ok(())
}

/// 仅为仍保持旧默认种子的角色收紧客户范围并补齐字段级权限。
async fn upgrade_customer_role_boundaries(rbac: &SharedRbacService) -> Result<()> {
    let sales_desired = parse_permissions(SALES_PERMISSIONS)?;
    let mut sales_previous: Vec<Permission> = sales_desired
        .iter()
        .filter(|permission| {
            !matches!(
                permission.to_string().as_str(),
                "customer:list"
                    | "customer:detail"
                    | "customer:create"
                    | "customer:update"
                    | "customer_sensitive:reveal"
            )
        })
        .cloned()
        .collect();
    sales_previous.push(Permission::parse("customer:*")?);
    sales_previous.push(Permission::parse("party_bank_account:*")?);
    upgrade_exact(rbac, "role-sales", sales_previous, sales_desired).await?;

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
    if rbac
        .upgrade_seeded_role_permissions_if_exact(role_id, previous, desired)
        .await?
    {
        tracing::info!(role_id, "predefined customer permissions upgraded");
    }
    Ok(())
}

/// 为未被管理员改写过的旧采购角色补充合同详情只读权限。
async fn upgrade_procurement_contract_read(rbac: &SharedRbacService) -> Result<()> {
    let desired = parse_permissions(PROCUREMENT_PERMISSIONS)?;
    let previous_without_both =
        remove_permissions(&desired, &["contract:detail", "supplier_catalog_cost:detail"]);
    let previous_without_cost = remove_permissions(&desired, &["supplier_catalog_cost:detail"]);
    let upgraded = rbac
        .upgrade_seeded_role_permissions_if_exact("role-procurement", previous_without_both, desired.clone())
        .await?
        || rbac
            .upgrade_seeded_role_permissions_if_exact("role-procurement", previous_without_cost, desired)
            .await?;
    if upgraded {
        tracing::info!(
            role_id = "role-procurement",
            permissions = "contract:detail,supplier_catalog_cost:detail",
            "predefined role permission upgraded"
        );
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
    use super::{parse_permissions, predefined_role_ids, PREDEFINED_ROLES, SALES_PERMISSIONS};
    use entities::Permission;

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
    fn sales_permissions_include_core_sales_capabilities() {
        let permissions = parse_permissions(SALES_PERMISSIONS).unwrap();
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("sales_order:create").unwrap())));
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("customer:list").unwrap())));
        assert!(permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("contract:create").unwrap())));
        assert!(!permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("party_bank_account:detail").unwrap())));
        assert!(!permissions
            .iter()
            .any(|p| p.covers(&Permission::parse("customer_scope:detail").unwrap())));
        assert!(
            !permissions
                .iter()
                .any(|p| p.covers(&Permission::parse("purchase_order:approve").unwrap())),
            "销售不应默认具备采购财务审核权"
        );
    }

    #[test]
    fn management_permissions_are_read_only() {
        let management = PREDEFINED_ROLES
            .iter()
            .find(|role| role.id == "role-management")
            .unwrap();
        let write_actions = [
            "create", "update", "delete", "approve", "reject", "submit", "post", "claim", "*",
        ];
        for raw in management.permissions {
            let permission = Permission::parse(*raw).unwrap();
            assert!(
                matches!(permission.action(), "list" | "detail"),
                "管理层权限应保持只读，发现: {raw}"
            );
            assert!(
                !write_actions.contains(&permission.action()),
                "管理层禁止写权限: {raw}"
            );
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
