//! 已授权采购的真实开放履约任务联查，冻结版本或来源不符时不作为当前影响。

use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_fulfillment::entity::fulfillment::delivery::DeliveryState;
use erp_fulfillment::entity::fulfillment::electronic_delivery::ElectronicDeliveryState;
use erp_fulfillment::entity::fulfillment::purchase_receipt::PurchaseReceiptState;
use erp_fulfillment::entity::fulfillment::service_fulfillment::ServiceFulfillmentState;
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_workflow::{WorkItem, WorkItemExt};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;
use serde::Deserialize;

use super::query::aggregate;
use crate::{Error, Result};

#[derive(Deserialize)]
struct ReceiptId {
    id: String,
}

/// 精确采购责任键及真正关联入库单定位当前开放任务，最多一千个。
///
/// # 参数
/// * `db` - 数据库。
/// * `order` - 已授权的采购单。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回责任键、对象和当前草稿同时匹配的开放履约任务。
///
/// # 错误
/// 草稿入库单或开放任务超过一千个时返回 `ValidationError`。
/// 采购单缺责任人、履约责任键不合法或仓储读取失败时返回对应错误。
pub(in crate::supplier_portal) async fn actual_open_tasks(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    let receipts=aggregate::<ReceiptId>(db.purchase_receipts().collection().clone_with_type::<Document>(),vec![doc! {"$match":{"purchase_order_id":&order.base.id,"status":"DRAFT","deleted_at":NOT_DELETED_TIMESTAMP_BSON}},doc! {"$project":{"_id":0,"id":1}},doc! {"$limit":1001}],executor).await?;
    if receipts.len() > 1000 {
        return Err(Error::ValidationError("采购未完成入库作业超过影响读取上限".into()));
    }
    let receipt_ids = receipts.into_iter().map(|receipt| receipt.id).collect::<Vec<_>>();
    let tasks=aggregate::<WorkItem>(db.work_items().collection().clone_with_type::<Document>(),vec![doc! {"$match":{"work_item_type":"FULFILLMENT_OPERATION","status":"OPEN","deleted_at":NOT_DELETED_TIMESTAMP_BSON,"$or":[{"responsibility_key":format!("purchase_order:{}",order.base.id)},{"business_object_type":"purchase_receipt","business_object_id":{"$in":receipt_ids}}]}},doc! {"$sort":{"created_at":1,"id":1}},doc! {"$limit":1001}],executor).await?;
    if tasks.len() > 1000 {
        return Err(Error::ValidationError("采购未完成履约任务超过影响读取上限".into()));
    }
    let facts = current_operation_facts(db, &tasks, executor).await?;
    let owner = order.current_owner_user_id()?;
    let mut current = Vec::new();
    for task in tasks {
        task.fulfillment_responsibility_key()?;
        if facts
            .get(&(task.business_object_type.clone(), task.business_object_id.clone()))
            .is_some_and(|fact| current_operation(&task, &order.base.id, owner, fact))
        {
            current.push(task);
        }
    }
    Ok(current)
}

/// 不含交付对象、地址或商业金额的当前草稿事实。
struct OperationFact {
    order_id: String,
    version: u64,
    warehouse_id: Option<String>,
}

/// 同一页任务按对象类型批量读取草稿，避免逐任务读取。
async fn current_operation_facts(
    db: &Database,
    tasks: &[WorkItem],
    executor: &mut dyn Executor,
) -> Result<HashMap<(String, String), OperationFact>> {
    let ids = |kind: &str| {
        tasks
            .iter()
            .filter(|task| task.business_object_type == kind)
            .map(|task| task.business_object_id.clone())
            .collect::<Vec<_>>()
    };
    let mut facts = HashMap::new();
    for value in db.deliveries().list_active_by_ids(&ids("delivery"), executor).await? {
        if value.status == DeliveryState::Draft
            && let Some(order) = value.purchase_order_id
        {
            facts.insert(
                ("delivery".into(), value.base.id),
                OperationFact {
                    order_id: order.to_string(),
                    version: value.base.version,
                    warehouse_id: None,
                },
            );
        }
    }
    for value in db.electronic_deliveries().list_active_by_ids(&ids("electronic_delivery"), executor).await? {
        if value.status == ElectronicDeliveryState::Draft {
            facts.insert(
                ("electronic_delivery".into(), value.base.id),
                OperationFact {
                    order_id: value.purchase_order_id.to_string(),
                    version: value.base.version,
                    warehouse_id: None,
                },
            );
        }
    }
    for value in db.service_fulfillments().list_active_by_ids(&ids("service_fulfillment"), executor).await? {
        if value.status == ServiceFulfillmentState::Draft {
            facts.insert(
                ("service_fulfillment".into(), value.base.id),
                OperationFact {
                    order_id: value.purchase_order_id.to_string(),
                    version: value.base.version,
                    warehouse_id: None,
                },
            );
        }
    }
    facts.extend(receipt_operation_facts(db, &ids("purchase_receipt"), executor).await?);
    Ok(facts)
}

/// 入库任务按实际采购来源及仓库责任键批量读取。
async fn receipt_operation_facts(
    db: &Database,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<(String, String), OperationFact>> {
    let mut facts = HashMap::new();
    for value in db.purchase_receipts().list_active_by_ids(ids, executor).await? {
        if value.status == PurchaseReceiptState::Draft {
            facts.insert(
                ("purchase_receipt".into(), value.base.id),
                OperationFact {
                    order_id: value.purchase_order_id.to_string(),
                    version: value.base.version,
                    warehouse_id: Some(value.warehouse_id.to_string()),
                },
            );
        }
    }
    Ok(facts)
}

/// 当前草稿版本、采购来源、责任键和真实个人责任必须同时匹配。
fn current_operation(task: &WorkItem, order_id: &str, owner: &str, fact: &OperationFact) -> bool {
    if fact.order_id != order_id || !task.matches_subject_version(&fact.version.to_string()) {
        return false;
    }
    match fact.warehouse_id.as_deref() {
        Some(warehouse) => {
            task.responsibility_key() == Some(format!("warehouse:{warehouse}:receipt").as_str())
        },
        None => {
            task.owner_user_id.as_deref() == Some(owner)
                && task.responsibility_key() == Some(format!("purchase_order:{order_id}").as_str())
        },
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemPriority};
    use erp_workflow::{WorkItemData, WorkItemType};

    use super::*;

    /// 构造具有真实当前个人责任和冻结版本的履约工作项。
    fn task(kind: &str, key: &str, role: &str, reason: &str) -> WorkItem {
        WorkItem::new_with_responsibility_key(
            erp_core::ids::WorkItemId::new("task1"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: kind.into(),
                business_object_id: "operation1".into(),
                subject_version: "3".into(),
                owner_role: role.into(),
                owner_organization_id: "org1".into(),
                owner_user_id: "buyer1".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some(reason.into()),
                impact_summary: None,
            },
            key,
        )
        .unwrap()
    }

    #[test]
    fn task_impact_requires_exact_purchase_current_version_and_current_owner() {
        let task = task(
            "electronic_delivery",
            "purchase_order:purchase1",
            "purchase_order_owner",
            "ELECTRONIC_DELIVERY_READY",
        );
        task.fulfillment_responsibility_key().unwrap();
        let mut fact = OperationFact { order_id: "purchase1".into(), version: 3, warehouse_id: None };
        assert!(current_operation(&task, "purchase1", "buyer1", &fact));
        assert!(!current_operation(&task, "purchase1", "other-buyer", &fact));
        assert!(!current_operation(&task, "other-purchase", "buyer1", &fact));
        fact.version = 4;
        assert!(!current_operation(&task, "purchase1", "buyer1", &fact));
    }

    #[test]
    fn warehouse_task_impact_follows_receipt_actual_purchase_and_warehouse() {
        let task = task(
            "purchase_receipt",
            "warehouse:warehouse1:receipt",
            "warehouse_inbound_handler",
            "PURCHASE_RECEIPT_READY",
        );
        task.fulfillment_responsibility_key().unwrap();
        let mut fact = OperationFact {
            order_id: "purchase1".into(),
            version: 3,
            warehouse_id: Some("warehouse1".into()),
        };
        assert!(current_operation(&task, "purchase1", "current-purchase-buyer", &fact));
        fact.warehouse_id = Some("other-warehouse".into());
        assert!(!current_operation(&task, "purchase1", "current-purchase-buyer", &fact));
    }
}
