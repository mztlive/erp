//! 采购详情的来源依据提示（第一期 §6.3“销售变更对采购的影响”第 10 条）。
//!
//! 读取依据更新记录；生效实物采购同时读取当前净履约和退货办理范围。

use erp_procurement::entity::purchase_order::{
    PurchaseOrder, PurchaseSourceResolution, PurchaseSourceUpdate, PurchaseSourceUpdateStatus, PurchaseType,
};
use erp_procurement::repository::purchase_source_update::latest_for_purchase;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::purchase_center::direct_return::direct_return_plan;
use crate::purchase_center::dto::PurchaseSourceUpdateView;

/// 读取采购的来源依据提示。
///
/// # 参数
/// * `db` - MongoDB 数据库句柄
/// * `order` - 已授权采购单
/// * `executor` - 调用方执行器
///
/// # 返回
/// 采购引用旧销售版本或存在未处理记录时返回提示；否则返回 `None`。
///
/// # 错误
/// 仓储读取失败时返回错误。
pub(super) async fn purchase_source_update_view(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseSourceUpdateView>> {
    let sales = db.sales_orders().find_by_id(order.sales_order_id.as_ref(), executor).await?;
    let current_revision = sales.and_then(|sales| sales.stable.current_revision_id);
    let latest = latest_for_purchase(db, &order.base.id, executor).await?;
    let outdated =
        current_revision.as_deref().is_some_and(|current| current != order.sales_order_revision_id.as_ref());
    let open = latest.as_ref().is_some_and(|update| update.status == PurchaseSourceUpdateStatus::Open);
    if !outdated && !open {
        return Ok(None);
    }
    let ids = [Some(order.sales_order_revision_id.to_string()), current_revision.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let revisions = db.sales_order_revisions().find_revisions_by_ids(&ids, executor).await?;
    let revision_no = |id: &str| {
        revisions.iter().find(|revision| revision.base.id == id).map(|revision| revision.revision.revision_no)
    };
    let mut view = source_update_view(
        latest.as_ref(),
        outdated,
        revision_no(order.sales_order_revision_id.as_ref()),
        current_revision.as_deref().and_then(revision_no),
    );
    if open && order.stable.current_revision_id.is_some() && order.purchase_type == PurchaseType::Physical {
        view.return_handling = Some(direct_return_plan(db, order, executor).await?);
    }
    Ok(Some(view))
}

/// 由最近一条记录与版本信息构造提示视图。
fn source_update_view(
    latest: Option<&PurchaseSourceUpdate>,
    outdated: bool,
    purchase_sales_revision_no: Option<u32>,
    current_sales_revision_no: Option<u32>,
) -> PurchaseSourceUpdateView {
    let open = latest.filter(|update| update.status == PurchaseSourceUpdateStatus::Open);
    let resolved = latest.filter(|update| update.status == PurchaseSourceUpdateStatus::Resolved);
    let last_resolution = resolved.and_then(|update| update.resolution);
    let awaiting_submission = open.is_some_and(|update| update.awaiting_submission);
    PurchaseSourceUpdateView {
        return_handling: None,
        affected: open.is_some() && !awaiting_submission,
        awaiting_submission,
        record_id: open.map(|update| update.base.id.clone()),
        record_version: open.map(|update| update.base.version),
        blocks_payment: open.is_some_and(|update| update.blocks_payment),
        reasons: open.map(|update| update.reasons.clone()).unwrap_or_default(),
        purchase_sales_revision_no,
        current_sales_revision_no,
        sales_revision_outdated: outdated,
        last_resolution,
        last_resolution_label: last_resolution
            .map(|resolution| PurchaseSourceResolution::label(resolution).to_string()),
        last_maintain_note: resolved.and_then(|update| update.maintain_note.clone()),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::Instant;
    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseSourceImpact,
        PurchaseSourceResolution, PurchaseSourceUpdate, PurchaseType, SourceImpactKind, SourceImpactReason,
        SourceUpdateOpening,
    };

    use super::source_update_view;

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
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("wh-1")),
            },
            "buyer",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        let impact = PurchaseSourceImpact {
            purchase_order_id: "po-1".into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 1,
                item_name: "礼盒".into(),
                kind: SourceImpactKind::Zeroed { baseline_quantity: Quantity::from_str("5").unwrap() },
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

    /// 未处理记录提供原因、版本与付款暂停。
    #[test]
    fn open_record_is_affected_with_version_for_maintain() {
        let open = record();

        let view = source_update_view(Some(&open), true, Some(1), Some(2));

        assert!(view.affected);
        assert_eq!(view.record_id.as_deref(), Some("psu-1"));
        assert_eq!(view.record_version, Some(open.base.version));
        assert!(view.blocks_payment);
        assert_eq!(view.reasons.len(), 1);
        assert_eq!(view.last_resolution, None);
    }

    /// 草稿已修改待提交时不再标记受影响，只提示提交审批。
    #[test]
    fn awaiting_record_prompts_submission() {
        let mut awaiting = record();
        awaiting.await_submission("rev-2").unwrap();

        let view = source_update_view(Some(&awaiting), false, Some(2), Some(2));

        assert!(!view.affected);
        assert!(view.awaiting_submission);
        assert!(!view.blocks_payment);
        assert_eq!(view.record_id.as_deref(), Some("psu-1"));
    }

    /// 已登记维持时只展示处理结果与旧版本提示。
    #[test]
    fn maintained_record_is_shown_as_last_resolution() {
        let mut maintained = record();
        maintained.maintain("供应商不接受减量", "buyer", Instant::from_unix_secs(1)).unwrap();

        let view = source_update_view(Some(&maintained), true, Some(1), Some(2));

        assert!(!view.affected);
        assert!(view.sales_revision_outdated);
        assert_eq!(view.last_resolution, Some(PurchaseSourceResolution::Maintained));
        assert_eq!(view.last_resolution_label.as_deref(), Some("已登记维持原采购"));
        assert_eq!(view.last_maintain_note.as_deref(), Some("供应商不接受减量"));
    }
}
