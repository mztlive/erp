//! 原申请提交事务冻结精确采购生效版本、完整销售版本与允许材料身份。

use erp_core::ids::PurchaseOrderRevisionId;
use erp_finance::entity::payable::{PaymentRequestSalesSource, PaymentRequestSourceSnapshot};
use erp_procurement::dto::purchase_order::{PurchaseOrderLineView, TotalsView};
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderRevision};
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;
use serde_json::{Value, json};

use crate::purchase_center::source_sales::source_sales;
use crate::{Error, Result};

/// 冻结已证明的采购生效版本；后续草稿和当前合同不会覆盖审批材料。
///
/// # 参数
/// * `db` / `executor` - 原申请提交事务。
/// * `order` / `revision` - 已授权的采购单及其精确生效版本。
/// # 返回
/// 返回完整采购成本、销售成交金额与允许文件引用。
/// # 错误
/// 版本不属于采购、销售关系不符、主体缺失或读取失败时拒绝。
pub async fn freeze_payment_request_source(
    db: &Database,
    order: &PurchaseOrder,
    revision: &PurchaseOrderRevision,
    executor: &mut dyn Executor,
) -> Result<PaymentRequestSourceSnapshot> {
    if revision.purchase_order_id.as_ref() != order.base.id
        || order.stable.current_revision_id.as_deref() != Some(revision.base.id.as_str())
        || revision.payment_subject.sales_order_id != order.sales_order_id
    {
        return Err(Error::ConflictError("付款申请采购版本与来源单不匹配".into()));
    }
    let lines = db
        .purchase_order()
        .list_revision_lines(&PurchaseOrderRevisionId::new(&revision.base.id), executor)
        .await?;
    let lines = lines.iter().map(PurchaseOrderLineView::from_revision).collect::<Vec<_>>();
    let sales_preview = source_sales(db, order, &lines, executor).await?;
    let sales = db
        .sales_order_revisions()
        .find_by_id(&sales_preview.revision_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("付款申请来源销售版本不存在".into()))?;
    if sales.base.id != revision.payment_subject.sales_order_revision_id.as_ref()
        || sales.settlement_party_id != revision.payment_subject.settlement_party_id
        || sales.sales_order_id != order.sales_order_id
        || sales.settlement_party_snapshot.as_ref().is_none_or(|snapshot| {
            snapshot.settlement_party_name != revision.payment_subject.settlement_party_name
        })
    {
        return Err(Error::ConflictError("付款申请采购主体与精确销售来源不一致".into()));
    }
    let sales_order = db
        .sales_orders()
        .find_by_id(order.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("付款申请来源销售单不存在".into()))?;
    if sales_order.stable.current_revision_id.as_deref() != Some(sales.base.id.as_str()) {
        return Err(Error::ConflictError("付款申请来源销售版本已变化，请先完成正式采购变更".into()));
    }
    let allowed_attachment_ids =
        sales_preview.materials.iter().map(|file| file.file_asset_id.clone()).collect();
    let sales_source = PaymentRequestSalesSource {
        sales_order_id: sales.sales_order_id.to_string(),
        sales_revision_id: sales.base.id,
        customer_id: sales_order.customer_id.to_string(),
        payer_party_id: revision.payment_subject.settlement_party_id.to_string(),
        payer_party_name: revision.payment_subject.settlement_party_name.clone(),
        preview: serde_json::to_value(sales_preview).map_err(|error| Error::Internal(error.to_string()))?,
    };
    Ok(PaymentRequestSourceSnapshot {
        purchase_preview: purchase_preview(order, revision, &lines),
        sales_sources: vec![sales_source],
        allowed_attachment_ids,
    })
}

fn purchase_preview(
    order: &PurchaseOrder,
    revision: &PurchaseOrderRevision,
    lines: &[PurchaseOrderLineView],
) -> Value {
    json!({
        "purchase_order_id": order.base.id, "purchase_no": order.purchase_no,
        "revision_id": revision.base.id, "revision_no": revision.revision.revision_no,
        "supplier_id": order.supplier_id, "supplier_name": revision.supplier_snapshot.supplier_name,
        "purchase_type": order.purchase_type, "fulfillment_responsibility": order.fulfillment_responsibility,
        "payment_subject": revision.payment_subject,
        "payment_term_snapshot": revision.payment_term_snapshot,
        "totals": TotalsView::from_revision(revision), "lines": lines,
    })
}
