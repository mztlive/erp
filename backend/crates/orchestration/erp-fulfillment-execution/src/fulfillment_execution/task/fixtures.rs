//! 离线任务编排测试共享的业务对象，不包含生产步骤替身。
use erp_core::ids::{DeliveryId, PurchaseOrderId, SalesOrderId, WorkItemId};
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryData, DeliveryType};
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use mongodb::ClientSession;
use persistence_core::Executor;

pub(super) struct TestExecutor {
    pub(super) marker: u64,
}

impl Executor for TestExecutor {
    fn session(&mut self) -> Option<&mut ClientSession> {
        self.marker += 1;
        None
    }
}

pub(super) fn task() -> WorkItem {
    WorkItem::new_with_responsibility_key(
        WorkItemId::new("task-1"),
        WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: "delivery".to_string(),
            business_object_id: "delivery-1".to_string(),
            subject_version: "1".to_string(),
            owner_role: "purchase_order_owner".to_string(),
            owner_organization_id: "party-1".to_string(),
            owner_user_id: "buyer-1".to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("SUPPLIER_DIRECT_DELIVERY_READY".to_string()),
            impact_summary: None,
        },
        "purchase_order:po-1",
    )
    .unwrap()
}
pub(super) fn delivery() -> Delivery {
    Delivery::new(
        DeliveryId::new("delivery-1"),
        DeliveryData {
            delivery_no: "DN20260907-000001".to_string(),
            delivery_type: DeliveryType::SupplierDirect,
            sales_order_id: SalesOrderId::new("sales-1"),
            purchase_order_id: Some(PurchaseOrderId::new("po-1")),
            warehouse_id: None,
            tracking_entries: Vec::new(),
            address_snapshot_encrypted: None,
            address_snapshot_fingerprint: None,
        },
    )
    .unwrap()
}
