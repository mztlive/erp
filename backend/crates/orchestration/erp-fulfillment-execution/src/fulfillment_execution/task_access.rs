//! 客户验收办理按完整执行权限及指定任务人统一授权。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::fulfillment;
use erp_workflow::{WorkItem, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

pub(super) enum TaskPurpose {
    CustomerAcceptance,
    Fulfillment,
}

/// 身份资格和当前任务分配共用调用方已经读取的可信事实。
/// # 参数
/// db、identity 为原业务设施；actor、task、executor 为当前办理上下文。
/// # 返回
/// 同角色完整执行资格与任务分配均成立时成功。
/// # 错误
/// 账号、角色或任务分配不足时拒绝；设施错误透传。
pub(super) async fn require_assignee(
    _db: &Database,
    identity: &Arc<AuthorizationService>,
    actor: &AuditActor,
    task: &WorkItem,
    purpose: TaskPurpose,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = match (purpose, task.work_item_type, task.business_object_type.as_str()) {
        (TaskPurpose::CustomerAcceptance, WorkItemType::CustomerAcceptanceRegistration, "sales_order") => {
            fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST
        },
        (TaskPurpose::Fulfillment, WorkItemType::FulfillmentOperation, "purchase_receipt") => {
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST
        },
        (TaskPurpose::Fulfillment, WorkItemType::FulfillmentOperation, "delivery") => {
            fulfillment::DELIVERY_DELIVERY_LIST
        },
        (TaskPurpose::Fulfillment, WorkItemType::FulfillmentOperation, "electronic_delivery") => {
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST
        },
        (TaskPurpose::Fulfillment, WorkItemType::FulfillmentOperation, "service_fulfillment") => {
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST
        },
        _ => return Err(crate::Error::Internal("履约任务缺少固定办理入口".into())),
    };
    Ok(identity
        .require(
            actor,
            &permission,
            AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() },
            executor,
        )
        .await?)
}
