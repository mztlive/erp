//! 采购依据更新记录的拥有仓储。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Repository, Result};

use crate::entity::purchase_order::{PurchaseSourceUpdate, PurchaseSourceUpdateStatus};

/// 采购依据更新记录集合名。
pub const PURCHASE_SOURCE_UPDATES: &str = "purchase_source_updates";

/// 获取采购依据更新记录仓储。
///
/// # 参数
/// * `db` - 业务数据库
///
/// # 返回
/// 返回使用调用方 Executor 的仓储。
///
/// # 错误
/// 无。
pub fn purchase_source_updates(db: &Database) -> Repository<'_, PurchaseSourceUpdate> {
    Repository::new(db, PURCHASE_SOURCE_UPDATES)
}

/// 按创建顺序列出销售单下全部依据更新记录。
///
/// # 参数
/// * `db` - 业务数据库
/// * `sales_order_id` - 来源销售单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回开放与已处理记录，按创建时间与主键升序。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn list_by_sales_order(
    db: &Database,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseSourceUpdate>> {
    purchase_source_updates(db)
        .find_many_sorted(
            doc! { "sales_order_id": sales_order_id },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
}

/// 列出某张销售变更单触发的依据更新记录。
///
/// # 参数
/// * `db` - 业务数据库
/// * `sales_change_order_id` - 销售变更单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回该变更单触发的记录，按创建时间与主键升序。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn list_by_trigger_change(
    db: &Database,
    sales_change_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseSourceUpdate>> {
    purchase_source_updates(db)
        .find_many_sorted(
            doc! { "trigger_sales_change_order_id": sales_change_order_id },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
}

/// 读取采购当前的开放依据更新记录。
///
/// # 参数
/// * `db` - 业务数据库
/// * `purchase_order_id` - 采购单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 存在开放记录时返回该记录。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn find_open_for_purchase(
    db: &Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseSourceUpdate>> {
    purchase_source_updates(db)
        .find_one(doc! { "purchase_order_id": purchase_order_id, "status": PurchaseSourceUpdateStatus::Open.as_str() }, executor)
        .await
}

/// 读取采购最近一条依据更新记录。
///
/// # 参数
/// * `db` - 业务数据库
/// * `purchase_order_id` - 采购单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回创建时间最晚的记录；没有记录时返回 `None`。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn latest_for_purchase(
    db: &Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseSourceUpdate>> {
    Ok(purchase_source_updates(db)
        .find_many_sorted(
            doc! { "purchase_order_id": purchase_order_id },
            doc! { "created_at": -1, "id": -1 },
            executor,
        )
        .await?
        .into_iter()
        .next())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use mongodb::bson::{deserialize_from_document, serialize_to_document};

    use crate::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseSourceImpact,
        PurchaseSourceResolution, PurchaseSourceUpdate, PurchaseType, SourceImpactKind, SourceImpactReason,
        SourceUpdateOpening,
    };

    fn record() -> PurchaseSourceUpdate {
        let order = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "NET-30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("wh-1")),
            },
            "buyer",
            crate::entity::test_support::payment_term,
        )
        .unwrap();
        let impact = PurchaseSourceImpact {
            purchase_order_id: "po-1".into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 2,
                item_name: "礼盒".into(),
                kind: SourceImpactKind::QuantityReduced {
                    baseline_quantity: Quantity::from_str("10.000000").unwrap(),
                    current_quantity: Quantity::from_str("8").unwrap(),
                    excess_quantity: Quantity::from_str("2").unwrap(),
                },
            }],
        };
        PurchaseSourceUpdate::open(
            "psu-1".into(),
            SourceUpdateOpening {
                order: &order,
                impact: &impact,
                sales_order_revision_id: "rev-2",
                trigger_sales_change_order_id: Some("sco-1"),
            },
        )
    }

    /// 记录可经 BSON 存取，数量、处理方式与说明保持原值。
    #[test]
    fn record_round_trips_through_bson() {
        let mut update = record();
        update.maintain("供应商不接受减量", "buyer", Instant::from_unix_secs(2)).unwrap();

        let document = serialize_to_document(&update).unwrap();
        let back: PurchaseSourceUpdate = deserialize_from_document(document).unwrap();

        assert_eq!(back.reasons, update.reasons);
        assert_eq!(back.resolution, Some(PurchaseSourceResolution::Maintained));
        assert_eq!(back.maintain_note.as_deref(), Some("供应商不接受减量"));
    }

    /// 缺少等待提交字段的记录按未等待提交读取。
    #[test]
    fn missing_awaiting_submission_defaults_to_false() {
        let mut document = serialize_to_document(&record()).unwrap();
        document.remove("awaiting_submission");

        let back: PurchaseSourceUpdate = deserialize_from_document(document).unwrap();

        assert!(!back.awaiting_submission);
    }
}
