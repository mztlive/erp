//! 采购创建依据与按销售当前版本剩余数量建单。
//!
//! 创建依据由销售单当前版本的 `GOODS_SERVICE` 行、当前采购覆盖数量和供应商
//! 当前合格供给共同形成。依据精确到销售当前版本、供应商、采购类型、付款条件与
//! 履约责任；一次依据命令只创建一张采购单，并在同一事务内提交审批。
//!
//! 供给、当前修订、可供投影与供应商结算事实由
//! `database::PurchaseOrderExt::load_creation_basis_facts` 一次批量加载；拆单
//! 维度、稳定身份、产品类型映射、履约选项、成本选择、最大可创建数量与请求行
//! 规范化由 `entities::purchase_order::creation_basis` 领域值对象承担。本模块
//! 只负责当前指针解析、任务归属与 RBAC、合格性筛选（条款有效期、AVAILABLE、
//! 零库存、每供应商稳定选一条）、事务编排与 View 映射。

mod create;
mod mapping;
mod query;

pub(super) use create::{
    persist_basis_draft, procurement_quantity_changed, validate_requested_quantities, CreateBasisCommand,
    VerifiedBasisInput,
};
pub(super) use query::{
    basis_groups_and_facts, basis_groups_for_order, load_effective_sales_order, stock_basis_groups_for_order,
};

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;

    use super::create::{ensure_expected_delivery_within_sales_due, parse_basis_sales_order_id};
    use super::mapping::business_date_of;

    fn production_source() -> String {
        fn production_part(source: &str) -> &str {
            source.split("#[cfg(test)]").next().expect("生产代码必须存在")
        }
        [
            production_part(include_str!("creation_basis.rs")),
            production_part(include_str!("creation_basis/query.rs")),
            production_part(include_str!("creation_basis/mapping.rs")),
            production_part(include_str!("creation_basis/create.rs")),
        ]
        .concat()
    }

    /// 上海零点对应前一日 UTC 时仍还原业务自然日。
    #[test]
    fn business_date_uses_shanghai_timezone() {
        let unix_secs = chrono::DateTime::parse_from_rfc3339("2026-08-23T00:00:00+08:00")
            .expect("测试时间合法")
            .timestamp();

        let date = business_date_of(Instant::from_unix_secs(unix_secs)).expect("业务日期合法");

        assert_eq!(date.to_string(), "2026-08-23");
    }

    /// 新依据 ID 只接受销售单加 SHA-256，不兼容旧供应商拼接形式。
    #[test]
    fn basis_id_parser_rejects_legacy_shape() {
        let digest = entities::purchase_order::digest_parts(["scope".to_string()]);
        assert_eq!(
            parse_basis_sales_order_id(&format!("so-1:{digest}"))
                .unwrap()
                .to_string(),
            "so-1"
        );
        assert!(parse_basis_sales_order_id("so-1:supplier-1").is_err());
    }

    /// 采购预计交付日可以早于或等于销售承诺期限，但不得晚于该期限。
    #[test]
    fn expected_delivery_must_not_exceed_sales_due() {
        let sales_due = erp_core::common::time::BusinessDate::from_str("2026-09-10").unwrap();
        let earlier = erp_core::common::time::BusinessDate::from_str("2026-09-09").unwrap();
        let later = erp_core::common::time::BusinessDate::from_str("2026-09-11").unwrap();

        assert!(ensure_expected_delivery_within_sales_due(earlier, sales_due).is_ok());
        assert!(ensure_expected_delivery_within_sales_due(sales_due, sales_due).is_ok());
        assert!(ensure_expected_delivery_within_sales_due(later, sales_due).is_err());
    }

    /// 验证采购依据创建的操作人授权提交栅栏。
    ///
    /// 服务层必须冻结账号与权限快照，并用同一 policy revision CAS 提交业务事务。
    #[test]
    fn create_from_basis_binds_actor_authorization_to_commit() {
        let production = production_source();

        assert!(production.contains("authorize_actor_permission(actor, CREATE_PERMISSION)"));
        assert!(production.contains("ensure_purchase_order_actor_account"));
        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
        assert!(production.contains("ensure_initial_purchase_order_owner"));
        assert!(production.contains("ensure_fulfillment_owner_eligible"));
        assert!(production.contains("submit_created_draft_in_session"));
    }

    /// 创建依据路径必须使用批量事实加载，旧逐行供给与名称查询已删除。
    #[test]
    fn creation_basis_uses_batch_facts_loader() {
        let production = production_source();

        assert!(
            production.contains("load_creation_basis_facts"),
            "必须使用批量事实加载"
        );
        assert!(
            production.contains("basis_groups_and_facts"),
            "事务内必须复用同一批事实"
        );
        assert!(
            !production.contains("list_active_offerings_by_sku("),
            "逐 SKU 供给查询已删除"
        );
        assert!(
            !production.contains("cached_settlement_terms"),
            "逐供应商付款条件缓存已删除"
        );
        assert!(
            !production.contains("resolve_supplier_name"),
            "逐供应商名称查询已删除"
        );
        assert!(
            !production.contains("fn normalize_requested_lines"),
            "请求行规范化已下沉实体"
        );
        assert!(
            !production.contains("fn basis_id_for") && !production.contains("fn basis_scope_key"),
            "依据身份规则已下沉实体"
        );
    }
}
