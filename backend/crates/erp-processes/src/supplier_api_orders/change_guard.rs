use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_fulfillment::entity::fulfillment::{DeliveryState, DeliveryType};
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::{FulfillmentResponsibility, PurchaseOrder};
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, active_for_purchase};
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::order_tracking::TrackingState;
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::repository::SupplierFulfillmentExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::audit::persist_log;
use crate::fulfillment_execution::task::supersede::close_api_delivery_task;
use crate::{Error, Result};

/// 在采购变更根事务内替代未发单或已经可信取消的旧 API 执行。
/// # 参数
/// 旧采购单、新正式修订身份、已授权变更执行人与同一事务。
/// # 返回
/// 返回是否存在应在新采购版本重新安排的 API 执行。
/// # 错误
/// 已发单但未确认取消、结果未知、已发货或任一 CAS 失败时拒绝。
pub async fn supersede_purchase_executions(
    db: &Database,
    purchase: &PurchaseOrder,
    replacement_revision: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let mut existing = active_for_purchase(db, &purchase.base.id, executor).await?;
    for execution in &mut existing {
        prepare_replacement(db, execution, replacement_revision, executor).await?;
    }
    for execution in &mut existing {
        if purchase.fulfillment_responsibility == FulfillmentResponsibility::SupplierDirect {
            retire_api_delivery_draft(db, purchase, execution, actor.id(), executor).await?;
        }
        stop_tracking(db, execution, executor).await?;
        db.supplier_api_order_executions().update(execution, executor).await?;
    }
    if !existing.is_empty() {
        let audit = actor.clone().resource_log(
            "purchase_order.api_execution_supersede",
            "purchase_order",
            purchase.base.id.clone(),
        )?;
        persist_log(db, &audit, executor).await?;
    }
    Ok(!existing.is_empty())
}

async fn prepare_replacement(
    db: &Database,
    execution: &mut ApiOrderExecution,
    replacement_revision: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(mut order) = db.supplier_fulfillment_orders().find_by_id(&execution.base.id, executor).await?
    else {
        return execution.stop_for_purchase_change(replacement_revision).map_err(Error::from);
    };
    if order.ensure_unsubmitted().is_ok() && execution.ensure_unsent_for_purchase_change().is_ok() {
        execution.stop_for_purchase_change(replacement_revision)?;
    } else {
        confirm_cancelled_replacement(db, execution, replacement_revision, &order, executor).await?;
    }
    db.supplier_fulfillment_orders().update(&mut order, executor).await?;
    Ok(())
}

async fn confirm_cancelled_replacement(
    db: &Database,
    execution: &mut ApiOrderExecution,
    replacement_revision: &str,
    order: &SupplierFulfillmentOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let tracking =
        db.supplier_order_tracking().find_by_id(&execution.base.id, executor).await?.ok_or_else(|| {
            Error::BusinessLogicError("原供应商订单缺少已核实的取消证据，请先处理原单".into())
        })?;
    let id = tracking
        .applied_observation_id
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("原供应商订单取消尚未查询并应用，请稍后核对".into()))?;
    let observation = db
        .supplier_order_observations()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("原供应商订单取消观察不存在，请核对".into()))?;
    execution.stop_cancelled_for_purchase_change(replacement_revision, order, &tracking, &observation)?;
    Ok(())
}

/// 仅退役当前执行确定生成、从未发货的 API 直发草稿；保留文档与行记录。
async fn retire_api_delivery_draft(
    db: &Database,
    purchase: &PurchaseOrder,
    execution: &ApiOrderExecution,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let id = format!("{}-delivery", execution.base.id);
    let mut delivery = db
        .deliveries()
        .find_by_id_including_deleted(&id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("原 API 执行缺少自动直发草稿，请核对原执行".into()))?;
    if delivery.delivery_type != DeliveryType::SupplierDirect
        || delivery.purchase_order_id.as_ref().map(|id| id.as_ref()) != Some(purchase.base.id.as_str())
        || delivery.status != DeliveryState::Draft
    {
        return Err(Error::ConflictError("原 API 直发草稿已有履约或来源不符，不能随采购变更退役".into()));
    }
    if delivery.base.deleted_at != 0 {
        return Err(Error::ConflictError("原 API 草稿已被其他操作退役，请核对执行关系后重试".into()));
    }
    close_api_delivery_task(db, &delivery, actor_id, executor).await?;
    db.deliveries().soft_delete(&mut delivery, executor).await?;
    Ok(())
}

async fn stop_tracking(
    db: &Database,
    execution: &ApiOrderExecution,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let Some(mut tracking) = db.supplier_order_tracking().find_by_id(&execution.base.id, executor).await? {
        tracking.state = TrackingState::Complete;
        tracking.lease_id = None;
        tracking.lease_until = None;
        tracking.last_error = Some("采购变更已替代安全停止的原执行，已停止原单跟踪".into());
        db.supplier_order_tracking().update(&mut tracking, executor).await?;
    }
    Ok(())
}
