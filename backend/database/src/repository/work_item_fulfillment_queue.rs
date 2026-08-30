//! W09 履约责任队列的 MongoDB 页面投影。
//!
//! 查询从当前个人开放的 `FULFILLMENT_OPERATION` WorkItem 出发，以责任事实
//! 作为权限范围，再关联四类履约草稿、来源采购/销售单和仓库。聚合在服务端完成
//! 筛选、指标和分页；客户端不得逐页拉取四个单据列表后自行拼接。

use entities::work_item::{WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use serde::Deserialize;

use super::extensions::{FulfillmentExt, PurchaseOrderExt, SalesOrderExt, WarehouseExt};
use super::Repository;
use crate::executor::Executor;
use crate::{Error, Result};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;

const PURCHASE_RECEIPTS: &str = <Database as FulfillmentExt>::PURCHASE_RECEIPTS;
const DELIVERIES: &str = <Database as FulfillmentExt>::DELIVERIES;
const ELECTRONIC_DELIVERIES: &str = <Database as FulfillmentExt>::ELECTRONIC_DELIVERIES;
const SERVICE_FULFILLMENTS: &str = <Database as FulfillmentExt>::SERVICE_FULFILLMENTS;
const PURCHASE_ORDERS: &str = <Database as PurchaseOrderExt>::PURCHASE_ORDERS;
const SALES_ORDERS: &str = <Database as SalesOrderExt>::SALES_ORDERS;
const WAREHOUSES: &str = <Database as WarehouseExt>::WAREHOUSES;

/// 履约责任队列的仓储筛选；所有字符串均已由 Service 白名单化或规范化。
#[derive(Debug, Clone)]
pub struct FulfillmentQueueFilter {
    /// 当前已认证个人责任人。
    pub owner_user_id: String,
    /// 服务端允许且调用方请求的作业类型稳定代码。
    pub operation_types: Vec<String>,
    /// 精确履约对象；用于工作台单任务聚焦。
    pub operation_id: Option<String>,
    /// 来源销售单。
    pub sales_order_id: Option<String>,
    /// 来源采购单。
    pub purchase_order_id: Option<String>,
    /// 履约仓库。
    pub warehouse_id: Option<String>,
    /// 权限范围内的单号/摘要字面量检索。
    pub query: Option<String>,
    /// 作业日期下界（包含，Unix 秒）。
    pub due_from: Option<i64>,
    /// 作业日期上界（不包含，Unix 秒）。
    pub due_before: Option<i64>,
    /// `SATISFIED`、`BLOCKED` 或空。
    pub gate: Option<String>,
    /// 已检查的分页偏移。
    pub offset: u64,
    /// 单页条数。
    pub page_size: u32,
}

/// 履约责任队列当前页的一行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FulfillmentQueueItemRow {
    pub work_item_id: String,
    pub task_version: u64,
    pub subject_version: String,
    pub owner_role: String,
    pub owner_organization_id: String,
    pub priority: WorkItemPriority,
    pub reason_code: String,
    pub impact_summary: String,
    pub work_item_created_at: u64,
    pub operation_id: String,
    pub operation_type: String,
    pub business_object_type: String,
    pub summary: String,
    pub edit_version: u64,
    pub due_at: i64,
    pub sales_order_id: Option<String>,
    pub sales_order_no: Option<String>,
    pub purchase_order_id: Option<String>,
    pub purchase_order_no: Option<String>,
    pub warehouse_id: Option<String>,
    pub warehouse_label: Option<String>,
    pub sales_order_line_id: Option<String>,
    pub purchase_line_sales_allocation_id: Option<String>,
    pub quantity: Option<String>,
    pub result: Option<String>,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    pub gate_state: String,
}

/// 作业类型跨页计数。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FulfillmentQueueMetricRow {
    pub operation_type: String,
    pub count: i64,
}

/// 当前权限队列内可用的仓库筛选项。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct FulfillmentQueueWarehouseRow {
    pub id: String,
    pub label: String,
}

/// 履约责任队列仓储结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FulfillmentQueueRepositoryPage {
    pub items: Vec<FulfillmentQueueItemRow>,
    pub total: i64,
    pub metrics: Vec<FulfillmentQueueMetricRow>,
    pub warehouses: Vec<FulfillmentQueueWarehouseRow>,
}

#[derive(Debug, Deserialize)]
struct CountRow {
    count: i64,
}

#[derive(Debug, Deserialize)]
struct FulfillmentQueueFacetRow {
    #[serde(default)]
    items: Vec<FulfillmentQueueItemRow>,
    #[serde(default)]
    total: Vec<CountRow>,
    #[serde(default)]
    metrics: Vec<FulfillmentQueueMetricRow>,
    #[serde(default)]
    warehouses: Vec<FulfillmentQueueWarehouseRow>,
}

impl<'a> Repository<'a, WorkItem> {
    /// 查询当前个人责任范围内的履约页面投影。
    ///
    /// # 参数
    /// * `filter` - 已由 Service 校验的筛选和分页
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页、跨页总数、类型指标和仓库选项。
    ///
    /// # 错误
    /// 聚合管道构造、MongoDB 执行或类型化反序列化失败时返回错误。
    pub async fn search_fulfillment_queue(
        &self,
        filter: &FulfillmentQueueFilter,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentQueueRepositoryPage> {
        let pipeline = fulfillment_queue_pipeline(filter)?;
        let collection = self.collection();
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<FulfillmentQueueFacetRow>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<FulfillmentQueueFacetRow>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        let facet = rows.into_iter().next().unwrap_or(FulfillmentQueueFacetRow {
            items: Vec::new(),
            total: Vec::new(),
            metrics: Vec::new(),
            warehouses: Vec::new(),
        });
        Ok(FulfillmentQueueRepositoryPage {
            items: facet.items,
            total: facet.total.first().map_or(0, |row| row.count),
            metrics: facet.metrics,
            warehouses: facet.warehouses,
        })
    }
}

fn fulfillment_queue_pipeline(filter: &FulfillmentQueueFilter) -> Result<Vec<Document>> {
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
                "gate_state": {
                    "$cond": [
                        { "$eq": ["$operation.operation_type", "WAREHOUSE_SHIP"] },
                        "SATISFIED",
                        "NOT_APPLICABLE",
                    ]
                },
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
    append_optional_filters(&mut pipeline, filter);
    pipeline.push(doc! {
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
            "warehouses": [
                { "$match": { "operation.warehouse_id": { "$type": "string", "$ne": "" } } },
                {
                    "$group": {
                        "_id": "$operation.warehouse_id",
                        "label": { "$first": "$_warehouse.warehouse_code" },
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "id": "$_id",
                        "label": { "$ifNull": ["$label", "$_id"] },
                    }
                },
                { "$sort": { "label": 1, "id": 1 } },
            ],
        }
    });
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
                { "$project": { "_id": 0, "id": 1, "purchase_no": 1, "sales_order_id": 1 } },
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

fn append_optional_filters(pipeline: &mut Vec<Document>, filter: &FulfillmentQueueFilter) {
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
            }
            Err(_) => {
                matched.insert("operation.due_at", doc! { "$lt": due_before });
            }
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
    }
}

#[cfg(test)]
mod tests {
    use super::{fulfillment_queue_pipeline, FulfillmentQueueFilter};

    #[test]
    fn pipeline_starts_from_owned_open_work_items_and_returns_one_facet() {
        let pipeline = fulfillment_queue_pipeline(&FulfillmentQueueFilter {
            owner_user_id: "user-1".to_string(),
            operation_types: vec![
                "RECEIPT".to_string(),
                "WAREHOUSE_SHIP".to_string(),
                "SUPPLIER_DIRECT".to_string(),
                "ELECTRONIC".to_string(),
                "SERVICE".to_string(),
            ],
            operation_id: None,
            sales_order_id: Some("sales-1".to_string()),
            purchase_order_id: None,
            warehouse_id: Some("warehouse-1".to_string()),
            query: Some("SO.1".to_string()),
            due_from: Some(1_700_000_000),
            due_before: Some(1_800_000_000),
            gate: Some("SATISFIED".to_string()),
            offset: 20,
            page_size: 20,
        })
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
    }

    #[test]
    fn unknown_operation_type_fails_closed_in_initial_match() {
        let pipeline = fulfillment_queue_pipeline(&FulfillmentQueueFilter {
            owner_user_id: "user-1".to_string(),
            operation_types: vec!["UNKNOWN".to_string()],
            operation_id: None,
            sales_order_id: None,
            purchase_order_id: None,
            warehouse_id: None,
            query: None,
            due_from: None,
            due_before: None,
            gate: None,
            offset: 0,
            page_size: 20,
        })
        .expect("测试分页应有效");
        let rendered = format!("{pipeline:?}");
        assert!(rendered.contains("Array([])"));
    }
}
