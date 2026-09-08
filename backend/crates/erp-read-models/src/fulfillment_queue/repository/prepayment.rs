//! 先款门槛查询投影：冻结采购版本 + 正式付款核销净额，筛选及分页前计算。
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_finance::repository::PayableExt;
use erp_procurement::repository::PurchaseOrderExt;
use mongodb::{
    bson::{doc, Document},
    Database,
};

pub(super) fn revision_lookup() -> Document {
    doc! { "$lookup": {
        "from": <Database as PurchaseOrderExt>::PURCHASE_ORDER_REVISIONS,
        "let": { "revision_id": "$_purchase_order.current_revision_id" },
        "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$eq": ["$id", "$$revision_id"] } } },
            { "$project": { "_id": 0, "payment_term_snapshot": 1, "gross_amount": 1 } },
        ],
        "as": "_gate_revisions",
    } }
}

/// 按来源采购单分录汇总 APPLY − REVERSE，与正式履约校验使用同一事实链。
/// 返回聚合阶段；没有付款事实时由外层补 Decimal 零。
pub(super) fn payments_lookup() -> Document {
    doc! { "$lookup": {
        "from": <Database as PayableExt>::PAYABLE_ENTRIES,
        "let": { "purchase_order_id": "$operation.purchase_order_id" },
        "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$eq": ["$source_document_id", "$$purchase_order_id"] } } },
            allocation_lookup(),
            { "$unwind": "$_payments" },
            { "$group": { "_id": null, "paid": { "$sum": "$_payments.paid" } } },
        ],
        "as": "_gate_payments",
    } }
}

fn allocation_lookup() -> Document {
    doc! { "$lookup": {
        "from": <Database as PayableExt>::PAYMENT_ALLOCATIONS,
        "let": { "entry_id": "$id" },
        "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$eq": ["$payable_entry_id", "$$entry_id"] } } },
            { "$group": { "_id": null, "paid": { "$sum": { "$cond": [
                { "$eq": ["$allocation_action", "apply"] }, "$allocated_amount",
                { "$multiply": ["$allocated_amount", -1] },
            ] } } } },
        ],
        "as": "_payments",
    } }
}

pub(super) fn facts() -> Document {
    doc! { "$set": {
        "_gate_revision": { "$arrayElemAt": ["$_gate_revisions", 0] },
        "_gate_paid": { "$ifNull": [{ "$arrayElemAt": ["$_gate_payments.paid", 0] }, { "$toDecimal": "0" }] },
    } }
}

/// 缺失生效版本时失败关闭；自有库存仓发不受供应商付款条件约束。
/// 门槛金额及比例均来自冻结快照，不能按付款条件名称猜测。
pub(super) fn state() -> Document {
    doc! { "$switch": {
        "branches": [
            { "case": { "$eq": ["$operation.operation_type", "WAREHOUSE_SHIP"] }, "then": "SATISFIED" },
            { "case": { "$ne": [{ "$type": "$_gate_revision" }, "object"] }, "then": "BLOCKED" },
            { "case": { "$ne": ["$_gate_revision.payment_term_snapshot.prepay_gate", true] }, "then": "NOT_APPLICABLE" },
            { "case": { "$and": [
                { "$eq": [{ "$ifNull": ["$_gate_revision.payment_term_snapshot.prepay_minimum_amount", null] }, null] },
                { "$eq": [{ "$ifNull": ["$_gate_revision.payment_term_snapshot.prepay_minimum_ratio", null] }, null] },
            ] }, "then": "BLOCKED" },
            { "case": { "$lt": ["$_gate_paid", required_amount()] }, "then": "BLOCKED" },
        ],
        "default": "SATISFIED",
    } }
}

/// 金额、比例须同时满足，取较高门槛。
/// Decimal `$round` 与正式命令 `round_to_cent` 同为银行家舍入。
pub(super) fn required_amount() -> Document {
    doc! { "$max": [
        { "$ifNull": ["$_gate_revision.payment_term_snapshot.prepay_minimum_amount", { "$toDecimal": "0" }] },
        { "$round": [
            { "$multiply": ["$_gate_revision.gross_amount", { "$ifNull": ["$_gate_revision.payment_term_snapshot.prepay_minimum_ratio", { "$toDecimal": "0" }] }] },
            2,
        ] },
    ] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_uses_frozen_thresholds_and_reverse_payment_facts() {
        let sources = format!("{:?} {:?}", revision_lookup(), payments_lookup());
        assert!(sources.contains("current_revision_id"));
        assert!(sources.contains("payable_entry_id"));
        assert!(sources.contains("allocation_action"));
        assert!(sources.contains("-1"));
        let gate = state().to_string();
        assert!(gate.contains("prepay_minimum_amount"));
        assert!(gate.contains("prepay_minimum_ratio"));
        assert!(gate.contains("BLOCKED"));
        assert!(!gate.contains("payment_term_code"));
    }

    #[test]
    fn ratio_projection_uses_decimal_bankers_rounding() {
        let amount = required_amount().to_string();
        assert!(amount.contains("$round"));
        assert!(amount.contains("$toDecimal"));
        assert!(!amount.contains("$floor"));
    }
}
