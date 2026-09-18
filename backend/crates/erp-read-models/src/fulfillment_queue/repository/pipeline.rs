//! 履约责任队列聚合管道：WorkItem 权限范围与四类履约草稿关联。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_customer::CustomerExt;
use erp_fulfillment::repository::FulfillmentExt;
use erp_party::PartyExt;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_supplier::SupplierExt;
use erp_warehouse::WarehouseExt;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Error, Result};

use super::page::{append_optional_filters, page_facet};
use super::{FulfillmentQueueFilter, prepayment};

const PURCHASE_RECEIPTS: &str = <Database as FulfillmentExt>::PURCHASE_RECEIPTS;
const DELIVERIES: &str = <Database as FulfillmentExt>::DELIVERIES;
const ELECTRONIC_DELIVERIES: &str = <Database as FulfillmentExt>::ELECTRONIC_DELIVERIES;
const SERVICE_FULFILLMENTS: &str = <Database as FulfillmentExt>::SERVICE_FULFILLMENTS;
const PURCHASE_ORDERS: &str = <Database as PurchaseOrderExt>::PURCHASE_ORDERS;
const SALES_ORDERS: &str = <Database as SalesOrderExt>::SALES_ORDERS;
const WAREHOUSES: &str = <Database as WarehouseExt>::WAREHOUSES;

/// 构造履约责任队列聚合管道。
///
/// # 参数
/// * `filter` - 已由 Service 校验的筛选和分页
///
/// # 返回
/// 返回从开放 WorkItem 出发、筛选后一次性分页的聚合阶段。
///
/// # 错误
/// 分页偏移超出 MongoDB 可表示范围时返回错误。
pub(super) fn fulfillment_queue_pipeline(filter: &FulfillmentQueueFilter) -> Result<Vec<Document>> {
    let offset = i64::try_from(filter.offset)
        .map_err(|_| Error::EntityMetadataOutOfRange("fulfillment_queue_offset"))?;
    let page_size = i64::from(filter.page_size);
    let mut pipeline = vec![
        doc! { "$match": base_match(filter) },
        purchase_receipt_lookup(),
        delivery_lookup(),
        electronic_delivery_lookup(),
        service_fulfillment_lookup(),
        doc! {
            "$set": {
                "operation": {
                    "$arrayElemAt": [
                        {
                            "$concatArrays": [
                                "$_purchase_receipt",
                                "$_delivery",
                                "$_electronic_delivery",
                                "$_service_fulfillment",
                            ]
                        },
                        0,
                    ]
                }
            }
        },
        doc! {
            "$match": {
                "operation": { "$type": "object" },
                "$expr": {
                    "$and": [
                        { "$eq": ["$reason_code", "$operation.expected_reason_code"] },
                        { "$eq": ["$owner_role", "$operation.expected_owner_role"] },
                        { "$eq": ["$responsibility_key", "$operation.expected_responsibility_key"] },
                        { "$eq": ["$subject_version", { "$toString": "$operation.edit_version" }] },
                    ]
                }
            }
        },
        purchase_order_lookup(),
        doc! {
            "$set": {
                "_purchase_order": { "$arrayElemAt": ["$_purchase_orders", 0] },
                "_source_sales_order_id": {
                    "$ifNull": [
                        "$operation.sales_order_id",
                        { "$arrayElemAt": ["$_purchase_orders.sales_order_id", 0] },
                    ]
                }
            }
        },
        prepayment::revision_lookup(),
        prepayment::payments_lookup(),
        prepayment::facts(),
        sales_order_lookup(),
        warehouse_lookup(),
        doc! {
            "$set": {
                "_sales_order": { "$arrayElemAt": ["$_sales_orders", 0] },
                "_warehouse": { "$arrayElemAt": ["$_warehouses", 0] },
                "_expected_owner_organization_id": {
                    "$cond": [
                        { "$in": ["$operation.operation_type", ["RECEIPT", "WAREHOUSE_SHIP"]] },
                        "$operation.warehouse_id",
                        { "$arrayElemAt": ["$_sales_orders.settlement_party_id", 0] },
                    ]
                },
                "gate_state": prepayment::state(),
                "_priority_rank": {
                    "$switch": {
                        "branches": [
                            { "case": { "$eq": ["$priority", "urgent"] }, "then": 4 },
                            { "case": { "$eq": ["$priority", "high"] }, "then": 3 },
                            { "case": { "$eq": ["$priority", "normal"] }, "then": 2 },
                        ],
                        "default": 1,
                    }
                }
            }
        },
        doc! {
            "$match": {
                "$expr": { "$eq": ["$owner_organization_id", "$_expected_owner_organization_id"] }
            }
        },
    ];
    if filter.query.is_some() {
        pipeline.push(counterparty_lookup(
            <Database as CustomerExt>::CUSTOMER_ACCOUNTS,
            "$_sales_order.customer_id",
            "_customer_names",
        ));
        pipeline.push(counterparty_lookup(
            <Database as SupplierExt>::SUPPLIER_ACCOUNTS,
            "$_purchase_order.supplier_id",
            "_supplier_names",
        ));
    }
    append_optional_filters(&mut pipeline, filter);
    pipeline.push(page_facet(offset, page_size));
    Ok(pipeline)
}

fn base_match(filter: &FulfillmentQueueFilter) -> Document {
    let mut matched = doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "status": WorkItemStatus::Open.as_str(),
        "work_item_type": WorkItemType::FulfillmentOperation.as_str(),
        "owner_user_id": &filter.owner_user_id,
        "$or": operation_contracts(&filter.operation_types),
    };
    if let Some(operation_id) = &filter.operation_id {
        matched.insert("business_object_id", operation_id);
    }
    matched
}

fn operation_contracts(operation_types: &[String]) -> Vec<Document> {
    operation_types
        .iter()
        .filter_map(|operation_type| match operation_type.as_str() {
            "RECEIPT" => Some(doc! {
                "business_object_type": "purchase_receipt",
                "reason_code": "PURCHASE_RECEIPT_READY",
            }),
            "WAREHOUSE_SHIP" => Some(doc! {
                "business_object_type": "delivery",
                "reason_code": "WAREHOUSE_DELIVERY_READY",
            }),
            "SUPPLIER_DIRECT" => Some(doc! {
                "business_object_type": "delivery",
                "reason_code": "SUPPLIER_DIRECT_DELIVERY_READY",
            }),
            "ELECTRONIC" => Some(doc! {
                "business_object_type": "electronic_delivery",
                "reason_code": "ELECTRONIC_DELIVERY_READY",
            }),
            "SERVICE" => Some(doc! {
                "business_object_type": "service_fulfillment",
                "reason_code": "SERVICE_FULFILLMENT_READY",
            }),
            _ => None,
        })
        .collect()
}

fn purchase_receipt_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": PURCHASE_RECEIPTS,
            "let": { "object_id": "$business_object_id", "object_type": "$business_object_type" },
            "pipeline": [
                {
                    "$match": {
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                        "status": "DRAFT",
                        "$expr": {
                            "$and": [
                                { "$eq": ["$$object_type", "purchase_receipt"] },
                                { "$eq": ["$id", "$$object_id"] },
                            ]
                        }
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "operation_id": "$id",
                        "operation_type": { "$literal": "RECEIPT" },
                        "business_object_type": { "$literal": "purchase_receipt" },
                        "summary": "$receipt_no",
                        "edit_version": "$version",
                        "due_at": "$created_at",
                        "purchase_order_id": 1,
                        "warehouse_id": 1,
                        "expected_reason_code": { "$literal": "PURCHASE_RECEIPT_READY" },
                        "expected_owner_role": { "$literal": "warehouse_inbound_handler" },
                        "expected_responsibility_key": {
                            "$concat": ["warehouse:", { "$toString": "$warehouse_id" }, ":receipt"]
                        },
                    }
                },
            ],
            "as": "_purchase_receipt",
        }
    }
}

fn delivery_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": DELIVERIES,
            "let": { "object_id": "$business_object_id", "object_type": "$business_object_type" },
            "pipeline": [
                {
                    "$match": {
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                        "status": "DRAFT",
                        "$expr": {
                            "$and": [
                                { "$eq": ["$$object_type", "delivery"] },
                                { "$eq": ["$id", "$$object_id"] },
                            ]
                        }
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "operation_id": "$id",
                        "operation_type": "$delivery_type",
                        "business_object_type": { "$literal": "delivery" },
                        "summary": "$delivery_no",
                        "edit_version": "$version",
                        "due_at": "$created_at",
                        "purchase_order_id": 1,
                        "sales_order_id": 1,
                        "warehouse_id": 1,
                        "carrier": 1,
                        "tracking_no": 1,
                        "expected_reason_code": {
                            "$cond": [
                                { "$eq": ["$delivery_type", "WAREHOUSE_SHIP"] },
                                "WAREHOUSE_DELIVERY_READY",
                                "SUPPLIER_DIRECT_DELIVERY_READY",
                            ]
                        },
                        "expected_owner_role": {
                            "$cond": [
                                { "$eq": ["$delivery_type", "WAREHOUSE_SHIP"] },
                                "warehouse_outbound_handler",
                                "purchase_order_owner",
                            ]
                        },
                        "expected_responsibility_key": {
                            "$cond": [
                                { "$eq": ["$delivery_type", "WAREHOUSE_SHIP"] },
                                {
                                    "$concat": [
                                        "warehouse:",
                                        { "$toString": "$warehouse_id" },
                                        ":warehouse_ship",
                                    ]
                                },
                                { "$concat": ["purchase_order:", { "$toString": "$purchase_order_id" }] },
                            ]
                        },
                    }
                },
            ],
            "as": "_delivery",
        }
    }
}

fn electronic_delivery_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": ELECTRONIC_DELIVERIES,
            "let": { "object_id": "$business_object_id", "object_type": "$business_object_type" },
            "pipeline": [
                {
                    "$match": {
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                        "status": "DRAFT",
                        "$expr": {
                            "$and": [
                                { "$eq": ["$$object_type", "electronic_delivery"] },
                                { "$eq": ["$id", "$$object_id"] },
                            ]
                        }
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "operation_id": "$id",
                        "operation_type": { "$literal": "ELECTRONIC" },
                        "business_object_type": { "$literal": "electronic_delivery" },
                        "summary": "$fulfillment_no",
                        "edit_version": "$version",
                        "due_at": "$occurred_at",
                        "purchase_order_id": 1,
                        "sales_order_line_id": 1,
                        "purchase_line_sales_allocation_id": 1,
                        "quantity": { "$toString": "$quantity" },
                        "result": 1,
                        "expected_reason_code": { "$literal": "ELECTRONIC_DELIVERY_READY" },
                        "expected_owner_role": { "$literal": "purchase_order_owner" },
                        "expected_responsibility_key": {
                            "$concat": ["purchase_order:", { "$toString": "$purchase_order_id" }]
                        },
                    }
                },
            ],
            "as": "_electronic_delivery",
        }
    }
}

fn service_fulfillment_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": SERVICE_FULFILLMENTS,
            "let": { "object_id": "$business_object_id", "object_type": "$business_object_type" },
            "pipeline": [
                {
                    "$match": {
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                        "status": "DRAFT",
                        "$expr": {
                            "$and": [
                                { "$eq": ["$$object_type", "service_fulfillment"] },
                                { "$eq": ["$id", "$$object_id"] },
                            ]
                        }
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "operation_id": "$id",
                        "operation_type": { "$literal": "SERVICE" },
                        "business_object_type": { "$literal": "service_fulfillment" },
                        "summary": "$fulfillment_no",
                        "edit_version": "$version",
                        "due_at": "$occurred_at",
                        "purchase_order_id": 1,
                        "sales_order_line_id": 1,
                        "purchase_line_sales_allocation_id": 1,
                        "quantity": { "$toString": "$quantity" },
                        "result": 1,
                        "expected_reason_code": { "$literal": "SERVICE_FULFILLMENT_READY" },
                        "expected_owner_role": { "$literal": "purchase_order_owner" },
                        "expected_responsibility_key": {
                            "$concat": ["purchase_order:", { "$toString": "$purchase_order_id" }]
                        },
                    }
                },
            ],
            "as": "_service_fulfillment",
        }
    }
}

fn purchase_order_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": PURCHASE_ORDERS,
            "let": { "purchase_order_id": "$operation.purchase_order_id" },
            "pipeline": [
                {
                    "$match": {
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                        "$expr": { "$eq": ["$id", "$$purchase_order_id"] },
                    }
                },
                { "$project": { "_id": 0, "id": 1, "purchase_no": 1, "supplier_id": 1, "sales_order_id": 1, "current_revision_id": 1 } },
            ],
            "as": "_purchase_orders",
        }
    }
}

fn sales_order_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": SALES_ORDERS,
            "let": { "sales_order_id": "$_source_sales_order_id" },
            "pipeline": [
                {
                    "$match": {
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                        "$expr": { "$eq": ["$id", "$$sales_order_id"] },
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "id": 1,
                        "order_no": 1,
                        "customer_id": 1,
                        "settlement_party_id": 1,
                    }
                },
            ],
            "as": "_sales_orders",
        }
    }
}

fn warehouse_lookup() -> Document {
    doc! {
        "$lookup": {
            "from": WAREHOUSES,
            "let": { "warehouse_id": "$operation.warehouse_id" },
            "pipeline": [
                {
                    "$match": {
                        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                        "$expr": { "$eq": ["$id", "$$warehouse_id"] },
                    }
                },
                { "$project": { "_id": 0, "id": 1, "warehouse_code": 1 } },
            ],
            "as": "_warehouses",
        }
    }
}

/// 读取来源单据的当前往来方名称；缺失或删除主数据不制造名称命中。
fn counterparty_lookup(accounts: &str, account_id: &str, output: &str) -> Document {
    doc! { "$lookup": {
        "from": accounts, "let": { "account_id": account_id }, "as": output,
        "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$eq": ["$id", "$$account_id"] } } },
            { "$lookup": { "from": <Database as PartyExt>::PARTIES, "localField": "party_id", "foreignField": "id", "as": "party" } },
            { "$unwind": "$party" },
            { "$match": { "party.deleted_at": NOT_DELETED_TIMESTAMP_BSON } },
            { "$lookup": { "from": <Database as PartyExt>::PARTY_REVISIONS, "localField": "party.current_revision_id", "foreignField": "id", "as": "revision" } },
            { "$unwind": "$revision" },
            { "$match": { "revision.deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$expr": { "$eq": ["$revision.party_id", "$party.id"] } } },
            { "$project": { "_id": 0, "legal_name": "$revision.legal_name", "short_name": "$revision.short_name" } },
        ]
    } }
}

#[cfg(test)]
mod tests {
    use super::super::FulfillmentQueueFilter;
    use super::fulfillment_queue_pipeline;

    #[test]
    fn pipeline_starts_from_owned_open_work_items_and_returns_one_facet() {
        let pipeline = fulfillment_queue_pipeline(
            &FulfillmentQueueFilter::new("user-1".to_string())
                .with_operation_types(vec![
                    "RECEIPT".to_string(),
                    "WAREHOUSE_SHIP".to_string(),
                    "SUPPLIER_DIRECT".to_string(),
                    "ELECTRONIC".to_string(),
                    "SERVICE".to_string(),
                ])
                .with_scope(None, Some("sales-1".to_string()), None, Some("warehouse-1".to_string()))
                .with_conditions(
                    Some("SO.1".to_string()),
                    Some(1_700_000_000),
                    Some(1_800_000_000),
                    Some("SATISFIED".to_string()),
                )
                .with_paging(20, 20),
        )
        .expect("测试分页应有效");
        let rendered = format!("{pipeline:?}");

        assert!(rendered.contains("FULFILLMENT_OPERATION"));
        assert!(rendered.contains("user-1"));
        assert!(rendered.contains("purchase_receipts"));
        assert!(rendered.contains("electronic_deliveries"));
        assert!(rendered.contains("service_fulfillments"));
        assert!(rendered.contains("purchase_orders"));
        assert!(rendered.contains("sales_orders"));
        assert!(rendered.contains("warehouses"));
        assert!(rendered.contains("$facet"));
        assert!(rendered.contains("metrics"));
        assert!(rendered.contains("warehouses"));
        assert!(rendered.contains("SO\\\\.1"), "检索词必须按字面量转义");
        let customer_lookup = pipeline
            .iter()
            .position(|stage| {
                stage
                    .get_document("$lookup")
                    .ok()
                    .is_some_and(|lookup| lookup.get_str("as").ok() == Some("_customer_names"))
            })
            .unwrap();
        let facet = pipeline.iter().position(|stage| stage.contains_key("$facet")).unwrap();
        assert!(customer_lookup < facet, "名称过滤必须先于分页和计数");
        assert!(rendered.contains("party.current_revision_id"));
        assert!(rendered.contains("revision.party_id"));
    }

    #[test]
    fn unknown_operation_type_fails_closed_in_initial_match() {
        let pipeline = fulfillment_queue_pipeline(
            &FulfillmentQueueFilter::new("user-1".to_string())
                .with_operation_types(vec!["UNKNOWN".to_string()]),
        )
        .expect("测试分页应有效");
        let rendered = format!("{pipeline:?}");
        assert!(rendered.contains("Array([])"));
    }
}
