//! 采购责任范围重读与责任人 CAS 的真实工作流适配。

use super::map_service;
use crate::errors::Error;
use async_trait::async_trait;
use erp_core::ids::PurchaseOrderId;
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderStatus};
use erp_procurement::repository::PurchaseOrderExt;
use erp_workflow::entity::work_item::WorkItem;
use erp_workflow::{Error as WorkflowError, Result as WorkflowResult, WorkItemExt};
use mongodb::Database;
use persistence_core::Executor;

#[async_trait]
trait PurchaseResponsibilityPort: Sync {
    async fn load_order(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<PurchaseOrder>>;
    async fn open_tasks(&self, key: &str, executor: &mut dyn Executor) -> WorkflowResult<Vec<WorkItem>>;
    async fn persist_order(
        &self,
        order: &mut PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()>;
}
struct MongoPurchaseResponsibility<'a> {
    db: &'a Database,
}
#[async_trait]
impl PurchaseResponsibilityPort for MongoPurchaseResponsibility<'_> {
    async fn load_order(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<Option<PurchaseOrder>> {
        self.db
            .purchase_orders()
            .find_by_id(&PurchaseOrderId::new(id.to_string()), executor)
            .await
            .map_err(WorkflowError::from)
    }
    async fn open_tasks(&self, key: &str, executor: &mut dyn Executor) -> WorkflowResult<Vec<WorkItem>> {
        self.db
            .work_items()
            .list_open_fulfillment_by_responsibility_key(key, executor)
            .await
            .map_err(WorkflowError::from)
    }
    async fn persist_order(
        &self,
        order: &mut PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> WorkflowResult<()> {
        self.db
            .purchase_orders()
            .update(order, executor)
            .await
            .map_err(WorkflowError::from)?;
        Ok(())
    }
}
pub(super) async fn purchase_order_fulfillment_scope(
    db: &Database,
    selected: &WorkItem,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> WorkflowResult<(String, Vec<WorkItem>)> {
    scope(
        &MongoPurchaseResponsibility { db },
        selected,
        purchase_order_id,
        executor,
    )
    .await
}
pub(super) async fn reassign_purchase_order_owner(
    db: &Database,
    purchase_order_id: &str,
    target_user_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> WorkflowResult<()> {
    reassign(
        &MongoPurchaseResponsibility { db },
        purchase_order_id,
        target_user_id,
        actor_id,
        executor,
    )
    .await
}
async fn scope(
    port: &impl PurchaseResponsibilityPort,
    selected: &WorkItem,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> WorkflowResult<(String, Vec<WorkItem>)> {
    let responsibility_key = format!("purchase_order:{purchase_order_id}");
    let order = port
        .load_order(purchase_order_id, executor)
        .await?
        .ok_or_else(|| WorkflowError::BusinessLogicError("履约任务引用的采购单不存在".to_string()))?;
    if matches!(
        order.stable.status,
        PurchaseOrderStatus::Completed | PurchaseOrderStatus::Voided
    ) {
        return Err(WorkflowError::BusinessLogicError(
            "已完成或已作废采购单不能变更责任人".to_string(),
        ));
    }
    let original_owner = order
        .current_owner_user_id()
        .map_err(|error| map_service(Error::from(error)))?
        .to_string();
    if selected.owner_user_id.as_deref() != Some(original_owner.as_str()) {
        return Err(WorkflowError::ConflictError(
            "采购单责任人与当前履约任务责任不一致，请刷新责任事实后重试".to_string(),
        ));
    }
    let tasks = port.open_tasks(&responsibility_key, executor).await?;
    if tasks.is_empty() || !tasks.iter().any(|task| task.base.id == selected.base.id) {
        return Err(WorkflowError::ConflictError(
            "采购单开放履约任务已变化，请刷新后重试".to_string(),
        ));
    }
    Ok((original_owner, tasks))
}
async fn reassign(
    port: &impl PurchaseResponsibilityPort,
    purchase_order_id: &str,
    target_user_id: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> WorkflowResult<()> {
    let mut order = port
        .load_order(purchase_order_id, executor)
        .await?
        .ok_or_else(|| WorkflowError::BusinessLogicError("履约任务引用的采购单不存在".to_string()))?;
    order
        .reassign_owner(target_user_id.to_string(), actor_id.to_string())
        .map_err(|error| map_service(Error::from(error)))?;
    port.persist_order(&mut order, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId, WorkItemId};
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrderData, PurchaseType,
    };
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};
    use erp_workflow::WorkItemType;
    use std::sync::Mutex;

    struct Marker(u64);
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        pointer: usize,
        calls: Mutex<Vec<&'static str>>,
        fail: Option<usize>,
        order: Option<PurchaseOrder>,
        tasks: Vec<WorkItem>,
        written: Mutex<Option<PurchaseOrder>>,
    }
    impl Recorder {
        fn new(executor: &mut Marker) -> Self {
            assert_eq!(executor.0, 171);
            Self {
                pointer: executor as *mut Marker as usize,
                calls: Mutex::new(Vec::new()),
                fail: None,
                order: Some(order()),
                tasks: vec![item("selected"), item("other")],
                written: Mutex::new(None),
            }
        }
        fn record(&self, name: &'static str, executor: &mut dyn Executor) -> WorkflowResult<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            let mut calls = self.calls.lock().unwrap();
            let index = calls.len();
            calls.push(name);
            if self.fail == Some(index) {
                return Err(WorkflowError::ConflictError(format!("provider {index}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl PurchaseResponsibilityPort for Recorder {
        async fn load_order(
            &self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> WorkflowResult<Option<PurchaseOrder>> {
            assert_eq!(id, "po-1");
            self.record("order", executor)?;
            Ok(self.order.clone())
        }
        async fn open_tasks(&self, key: &str, executor: &mut dyn Executor) -> WorkflowResult<Vec<WorkItem>> {
            assert_eq!(key, "purchase_order:po-1");
            self.record("tasks", executor)?;
            Ok(self.tasks.clone())
        }
        async fn persist_order(
            &self,
            order: &mut PurchaseOrder,
            executor: &mut dyn Executor,
        ) -> WorkflowResult<()> {
            self.record("CAS", executor)?;
            *self.written.lock().unwrap() = Some(order.clone());
            Ok(())
        }
    }
    fn order() -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "org-procurement".to_string(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
                creation_basis_id: "basis-1".into(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer-1".into(),
                target_warehouse_id: Some(WarehouseId::new("wh-1")),
            },
            "creator",
            |code| {
                Ok(PaymentTermFact {
                    canonical_code: code.into(),
                    prepay_gate: false,
                    prepay_minimum_ratio: None,
                    days_after_delivery: Some(30),
                    calendar_due: None,
                })
            },
        )
        .unwrap()
    }
    fn item(id: &str) -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new(id),
            WorkItemData {
                work_item_type: WorkItemType::BusinessException,
                business_object_type: "purchase_order".into(),
                business_object_id: "po-1".into(),
                subject_version: "1".into(),
                owner_role: "operations".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "buyer-1".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap()
    }
    #[tokio::test]
    async fn purchase_responsibility_uses_original_executor_and_real_entity_mutation() {
        let mut ex = Marker(171);
        let port = Recorder::new(&mut ex);
        let (owner, tasks) = scope(&port, &item("selected"), "po-1", &mut ex).await.unwrap();
        assert_eq!(owner, "buyer-1");
        assert_eq!(tasks, port.tasks);
        reassign(&port, "po-1", " buyer-2 ", "actor-2", &mut ex)
            .await
            .unwrap();
        assert_eq!(*port.calls.lock().unwrap(), ["order", "tasks", "order", "CAS"]);
        let written = port.written.lock().unwrap();
        let changed = written.as_ref().unwrap();
        assert_eq!(changed.owner_user_id.as_deref(), Some("buyer-2"));
        assert_eq!(changed.stable.updated_by, "actor-2");
        assert_eq!(changed.base, port.order.as_ref().unwrap().base);
    }
    #[tokio::test]
    async fn purchase_responsibility_stops_at_every_provider_failure() {
        for fail in 0..2 {
            let mut ex = Marker(171);
            let mut port = Recorder::new(&mut ex);
            port.fail = Some(fail);
            assert!(
                matches!(scope(&port, &item("selected"), "po-1", &mut ex).await, Err(WorkflowError::ConflictError(message)) if message == format!("provider {fail}"))
            );
            assert_eq!(*port.calls.lock().unwrap(), ["order", "tasks"][..=fail]);
            let mut port = Recorder::new(&mut ex);
            port.fail = Some(fail);
            assert!(
                matches!(reassign(&port, "po-1", "buyer-2", "actor", &mut ex).await, Err(WorkflowError::ConflictError(message)) if message == format!("provider {fail}"))
            );
            assert_eq!(*port.calls.lock().unwrap(), ["order", "CAS"][..=fail]);
            assert!(port.written.lock().unwrap().is_none());
        }
    }
    #[tokio::test]
    async fn purchase_responsibility_preserves_guard_order_and_missing_scope() {
        for status in [PurchaseOrderStatus::Completed, PurchaseOrderStatus::Voided] {
            let mut ex = Marker(171);
            let mut port = Recorder::new(&mut ex);
            port.order.as_mut().unwrap().stable.status = status;
            port.order.as_mut().unwrap().owner_user_id = None;
            assert!(
                matches!(scope(&port, &item("selected"), "po-1", &mut ex).await, Err(WorkflowError::BusinessLogicError(message)) if message == "已完成或已作废采购单不能变更责任人")
            );
            assert_eq!(*port.calls.lock().unwrap(), ["order"]);
        }
        for case in 0..5 {
            let mut ex = Marker(171);
            let mut port = Recorder::new(&mut ex);
            match case {
                0 => port.order = None,
                1 => port.order.as_mut().unwrap().owner_user_id = None,
                2 => port.order.as_mut().unwrap().owner_user_id = Some("other".into()),
                3 => port.tasks.clear(),
                _ => port.tasks = vec![item("not-selected")],
            }
            assert!(scope(&port, &item("selected"), "po-1", &mut ex).await.is_err());
            assert_eq!(port.calls.lock().unwrap().len(), if case < 3 { 1 } else { 2 });
        }
        let mut ex = Marker(171);
        let port = Recorder::new(&mut ex);
        assert!(reassign(&port, "po-1", " ", "actor", &mut ex).await.is_err());
        assert_eq!(*port.calls.lock().unwrap(), ["order"]);
        assert!(port.written.lock().unwrap().is_none());
    }
}
