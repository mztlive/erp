//! 电子交付事实、图片凭证和任务完成在调用方同一事务中确认。
use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::ElectronicDeliveryId;
use erp_fulfillment::dto::{ConfirmElectronicDeliveryRequest, ElectronicDeliveryView};
use erp_fulfillment::entity::fulfillment::{ElectronicDelivery, ElectronicDeliveryData, FulfillmentResult};
use erp_fulfillment::service::FulfillmentService;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::repository::PurchaseOrderExt;
use erp_support::PendingAttachmentBatch;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::FulfillmentProcess;
use super::purchase_context::{ensure_allocation_valid, ensure_po_fulfillable, ensure_prepay_gate};
use crate::{Error, Result};

struct Confirmation {
    request: ConfirmElectronicDeliveryRequest,
    encrypted_recipient: String,
    fingerprint: String,
}

impl FulfillmentProcess {
    /// 确认电子交付；原始对象仅用于加密与指纹，不进入日志或响应。
    ///
    /// # 参数
    /// * `id` - 草稿身份
    /// * `req` - 当前版本、实际交付事实和凭证引用
    /// * `pending` - 本次待登记文件
    /// * `actor` - 当前认证用户
    ///
    /// # 返回
    /// 返回已确认交付事实，不携带审批绑定。
    ///
    /// # 错误
    /// 版本、数量、凭证、采购资格或先款门槛无效时回滚整个事务。
    pub async fn confirm_electronic_delivery(
        &self,
        id: &str,
        mut req: ConfirmElectronicDeliveryRequest,
        pending: Arc<dyn PendingAttachmentBatch>,
        actor: &AuditActor,
    ) -> Result<ElectronicDeliveryView> {
        req.validate()?;
        let mut used = HashSet::new();
        pending.resolve_id(&mut req.evidence_attachment_id, &mut used)?;
        pending.ensure_all_used(&used)?;
        let confirmation = Confirmation {
            encrypted_recipient: self.sensitive_data.encrypt(req.recipient_snapshot.trim())?,
            fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                req.recipient_snapshot.trim(),
                &self.fingerprint_key,
            ),
            request: req,
        };
        persist(&self.db, ElectronicDeliveryId::new(id.to_string()), confirmation, pending, actor.clone())
            .await
    }
}

async fn persist(
    db: &Database,
    id: ElectronicDeliveryId,
    confirmation: Confirmation,
    pending: Arc<dyn PendingAttachmentBatch>,
    actor: AuditActor,
) -> Result<ElectronicDeliveryView> {
    let db = db.clone();
    let client = db.client().clone();
    let record = client
        .with_transaction(move |executor| {
            Box::pin(async move { confirm(&db, &id, confirmation, pending.as_ref(), &actor, executor).await })
        })
        .await?;
    Ok(record.into())
}

async fn confirm(
    db: &Database,
    id: &ElectronicDeliveryId,
    confirmation: Confirmation,
    pending: &dyn PendingAttachmentBatch,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ElectronicDelivery> {
    let domain = FulfillmentService::new(db.clone());
    let mut record = domain.prepare_electronic_confirmation(id, executor).await?;
    if record.base.version != confirmation.request.version {
        return Err(Error::ConflictError("电子交付草稿版本已变化，请刷新后重试".into()));
    }
    let order = purchase_context(db, &record, executor).await?;
    super::service_confirm::ensure_service_evidence_asset(
        db,
        &confirmation.request.evidence_attachment_id,
        pending,
        executor,
    )
    .await?;
    record.apply_confirmation(confirmation.data(&record, actor))?;
    pending.persist(db, executor).await?;
    domain.persist_electronic_confirmation(&mut record, executor).await?;
    finish(db, &record, &order, actor, executor).await?;
    Ok(record)
}

async fn purchase_context(
    db: &Database,
    record: &ElectronicDelivery,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    let order = db
        .purchase_orders()
        .find_by_id(record.purchase_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;
    ensure_po_fulfillable(&order)?;
    ensure_prepay_gate(db, executor, &order).await?;
    ensure_allocation_valid(
        db,
        executor,
        &order,
        &record.purchase_line_sales_allocation_id,
        &record.sales_order_line_id,
    )
    .await?;
    Ok(order)
}

async fn finish(
    db: &Database,
    record: &ElectronicDelivery,
    order: &PurchaseOrder,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    super::task::complete_fulfillment_task(
        db,
        super::task::FulfillmentTaskObject::ElectronicDelivery(record),
        actor.id(),
        executor,
    )
    .await?;
    if record.result != FulfillmentResult::Failure {
        super::customer_acceptance::task::ensure_customer_acceptance_task(
            db,
            &order.sales_order_id,
            super::customer_acceptance::task::CustomerAcceptanceTaskReason::DeliveryAvailable,
            executor,
        )
        .await?;
    }
    let audit = actor.clone().resource_log(
        "electronic_delivery.confirm",
        "electronic_delivery",
        record.base.id.clone(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

impl Confirmation {
    fn data(self, record: &ElectronicDelivery, actor: &AuditActor) -> ElectronicDeliveryData {
        ElectronicDeliveryData {
            fulfillment_no: record.fulfillment_no.clone(),
            sales_order_line_id: record.sales_order_line_id.clone(),
            purchase_order_id: record.purchase_order_id.clone(),
            purchase_line_sales_allocation_id: record.purchase_line_sales_allocation_id.clone(),
            recipient_snapshot: self.encrypted_recipient,
            recipient_snapshot_fingerprint: self.fingerprint,
            quantity: self.request.quantity,
            result: self.request.result,
            evidence_attachment_id: Some(self.request.evidence_attachment_id),
            fact_no: record.fact.fact_no.clone(),
            occurred_at: Instant::from_unix_secs(self.request.occurred_at),
            recorded_at: Instant::now(),
            recorded_by: actor.id().to_string(),
            source_type: record.fact.source_type,
            source_reference: record.fact.source_reference.clone(),
            reason_code: record.fact.reason_code.clone(),
            reason_text: record.fact.reason_text.clone(),
        }
    }
}
