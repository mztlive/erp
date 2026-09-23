//! 履约责任队列分页切片、可选筛选与行投影。

use mongodb::bson::{Document, doc};

use super::{FulfillmentQueueFilter, prepayment};

/// 当前页切片、跨页计数、作业类型指标。
///
/// # 参数
/// * `offset` - 已检查的分页偏移
/// * `page_size` - 单页条数
///
/// # 返回
/// 返回 `$facet` 阶段。
///
/// # 错误
/// 无。
pub(super) fn page_facet(offset: i64, page_size: i64) -> Document {
    doc! {
        "$facet": {
            "items": [
                { "$sort": { "operation.due_at": 1, "_priority_rank": -1, "id": 1 } },
                { "$skip": offset },
                { "$limit": page_size },
                { "$project": item_projection() },
            ],
            "total": [{ "$count": "count" }],
            "metrics": [
                { "$group": { "_id": "$operation.operation_type", "count": { "$sum": 1 } } },
                { "$project": { "_id": 0, "operation_type": "$_id", "count": 1 } },
                { "$sort": { "operation_type": 1 } },
            ],
        }
    }
}

/// 追加来源单据、仓库、作业日期、先款门槛和字面量检索。
///
/// # 参数
/// * `pipeline` - 已关联来源单据的聚合管道
/// * `filter` - 已由 Service 校验的筛选
///
/// # 返回
/// 无。按调用方提供的条件追加 `$match`。
///
/// # 错误
/// 无。
pub(super) fn append_optional_filters(pipeline: &mut Vec<Document>, filter: &FulfillmentQueueFilter) {
    let mut matched = Document::new();
    if let Some(sales_order_id) = &filter.sales_order_id {
        matched.insert("_source_sales_order_id", sales_order_id);
    }
    if let Some(purchase_order_id) = &filter.purchase_order_id {
        matched.insert("operation.purchase_order_id", purchase_order_id);
    }
    if let Some(warehouse_id) = &filter.warehouse_id {
        matched.insert("operation.warehouse_id", warehouse_id);
    }
    if let Some(due_from) = filter.due_from {
        matched.insert("operation.due_at", doc! { "$gte": due_from });
    }
    if let Some(due_before) = filter.due_before {
        match matched.get_document_mut("operation.due_at") {
            Ok(range) => {
                range.insert("$lt", due_before);
            },
            Err(_) => {
                matched.insert("operation.due_at", doc! { "$lt": due_before });
            },
        }
    }
    if let Some(gate) = &filter.gate {
        matched.insert("gate_state", gate);
    }
    if !matched.is_empty() {
        pipeline.push(doc! { "$match": matched });
    }
    if let Some(query) = filter.query.as_deref() {
        let literal = regex::escape(query);
        pipeline.push(doc! {
            "$match": {
                "$or": [
                    { "operation.summary": { "$regex": &literal, "$options": "i" } },
                    { "operation.operation_id": { "$regex": &literal, "$options": "i" } },
                    { "_purchase_order.purchase_no": { "$regex": &literal, "$options": "i" } },
                    { "_sales_order.order_no": { "$regex": &literal, "$options": "i" } },
                    { "_customer_names.legal_name": { "$regex": &literal, "$options": "i" } },
                    { "_customer_names.short_name": { "$regex": &literal, "$options": "i" } },
                    { "_supplier_names.legal_name": { "$regex": &literal, "$options": "i" } },
                    { "_supplier_names.short_name": { "$regex": &literal, "$options": "i" } },
                ]
            }
        });
    }
}

fn item_projection() -> Document {
    doc! {
        "_id": 0,
        "work_item_id": "$id",
        "task_version": "$version",
        "subject_version": 1,
        "owner_role": 1,
        "owner_organization_id": 1,
        "priority": 1,
        "reason_code": { "$ifNull": ["$reason_code", ""] },
        "impact_summary": { "$ifNull": ["$impact_summary", ""] },
        "work_item_created_at": "$created_at",
        "operation_id": "$operation.operation_id",
        "operation_type": "$operation.operation_type",
        "business_object_type": "$operation.business_object_type",
        "summary": "$operation.summary",
        "edit_version": "$operation.edit_version",
        "due_at": "$operation.due_at",
        "sales_order_id": "$_source_sales_order_id",
        "sales_order_no": "$_sales_order.order_no",
        "purchase_order_id": "$operation.purchase_order_id",
        "purchase_order_no": "$_purchase_order.purchase_no",
        "warehouse_id": "$operation.warehouse_id",
        "warehouse_label": "$_warehouse.warehouse_code",
        "sales_order_line_id": "$operation.sales_order_line_id",
        "purchase_line_sales_allocation_id": "$operation.purchase_line_sales_allocation_id",
        "quantity": "$operation.quantity",
        "result": "$operation.result",
        "carrier": "$operation.carrier",
        "tracking_no": "$operation.tracking_no",
        "gate_state": 1,
        "gate_required_amount": { "$toString": prepayment::required_amount() },
        "gate_effective_paid_amount": { "$toString": "$_gate_paid" },
    }
}
