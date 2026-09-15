//! W26 的唯一正式任务工厂；绑定供应商订单而非集成错误任务。
use erp_core::ids::WorkItemId;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::service::supplier_fulfillment::W26_BUSINESS_OBJECT_TYPE;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};

use crate::Result;
const W26_OWNER_ROLE: &str = "role-procurement";
const W26_OWNER_ORGANIZATION: &str = "company";
/// 在原创建分支与时点执行 WorkItem 构造校验；调用方预先生成身份。
pub(super) fn create(
    work_item_id: WorkItemId,
    order: &SupplierFulfillmentOrder,
    work_item_type: WorkItemType,
    actor_id: &str,
) -> Result<WorkItem> {
    Ok(WorkItem::new(
        work_item_id,
        WorkItemData {
            work_item_type,
            business_object_type: W26_BUSINESS_OBJECT_TYPE.to_string(),
            business_object_id: order.base.id.clone(),
            subject_version: order.base.version.to_string(),
            owner_role: W26_OWNER_ROLE.to_string(),
            owner_organization_id: W26_OWNER_ORGANIZATION.to_string(),
            owner_user_id: actor_id.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some(match work_item_type {
                WorkItemType::IntegrationResultUnknown => "SUPPLIER_RESULT_UNKNOWN".to_string(),
                WorkItemType::BusinessException => "SUPPLIER_BUSINESS_EXCEPTION".to_string(),
                _ => unreachable!("W26 producer only creates registered exception tasks"),
            }),
            impact_summary: Some(format!("供应商订单 {} 需要核实原动作结果", order.fulfillment_order_no)),
        },
    )?)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_fulfillment::*;

    use super::*;
    fn sample_order() -> SupplierFulfillmentOrder {
        SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("order-1"),
            SupplierFulfillmentOrderData {
                fulfillment_order_no: "FO-2026-001".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_id: SupplierApiConnectionId::new("connection-1"),
                split_no: 1,
                fulfillment_status: FulfillmentStatus::Submitting,
                cancel_status: CancelStatus::None,
                refund_status: RefundStatus::None,
                external_order_no: None,
                submitted_at: Some(Instant::from_unix_secs(1_700_000_000)),
                accepted_at: None,
                completed_at: None,
                address_snapshot_encrypted: "encrypted".to_string(),
                address_snapshot_fingerprint: "fingerprint".to_string(),
            },
        )
        .unwrap()
    }
    #[test]
    fn w26_factory_binds_order_current_subject_and_original_personal_responsibility() {
        let mut order = sample_order();
        order.base.version = 7;
        for (kind, reason) in [
            (WorkItemType::IntegrationResultUnknown, "SUPPLIER_RESULT_UNKNOWN"),
            (WorkItemType::BusinessException, "SUPPLIER_BUSINESS_EXCEPTION"),
        ] {
            let task = create(WorkItemId::new("formal-1"), &order, kind, "actor-1").unwrap();
            assert_eq!(task.base.id, "formal-1");
            assert_eq!(task.business_object_type, "SUPPLIER_FULFILLMENT_ORDER");
            assert_eq!(task.business_object_id, "order-1");
            assert_eq!(task.subject_version, "7");
            assert_eq!(task.work_item_type, kind);
            assert_eq!(task.owner_role, "role-procurement");
            assert_eq!(task.owner_organization_id, "company");
            assert_eq!(task.owner_user_id.as_deref(), Some("actor-1"));
            assert_eq!(task.assignment_source, AssignmentSource::SystemRule);
            assert_eq!(task.priority, WorkItemPriority::High);
            assert_eq!(task.due_at, None);
            assert_eq!(task.reason_code.as_deref(), Some(reason));
            assert_eq!(task.impact_summary.as_deref(), Some("供应商订单 FO-2026-001 需要核实原动作结果"));
        }
    }
    #[test]
    fn w26_factory_keeps_work_item_validation_for_empty_personal_owner() {
        assert!(
            create(WorkItemId::new("formal-1"), &sample_order(), WorkItemType::BusinessException, "")
                .is_err()
        );
    }
}
