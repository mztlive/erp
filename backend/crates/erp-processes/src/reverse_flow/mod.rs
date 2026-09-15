//! 退货、退款和资金冲正的跨域事务、审批与审计组合入口。
mod adapter;
mod cancel_approval;
mod cancel_write;
mod customer_refund;
mod payment_posting;
mod payment_reversal;
mod purchase_return;
mod receipt_reversal;
mod sales_return;
mod start_approval;
mod supplier_refund;

pub use adapter::{
    customer_refund_object_readable, payment_reversal_object_readable, receipt_reversal_object_readable,
    supplier_refund_object_readable,
};
use erp_audit::{AuditActorLogs, AuditExt};
use erp_identity::SharedRbacService;
use erp_read_models::returns_center::ReturnsReadService;
use erp_returns::repository::ReturnsExt;
use erp_returns::service::ReturnsService;
use erp_workflow::entity::document_registry::DocumentType;
use mongodb::Database;
pub use payment_posting::PaymentReversalProcess;
use persistence_core::Executor;
pub use receipt_reversal::ReceiptReversalProcess;

use crate::{Error, Result};

/// 退货、退款和冲正命令的原事务与审批授权组合。
pub struct ReturnsProcess {
    db: Database,
    rbac: SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}
impl ReturnsProcess {
    /// 使用原共享 RBAC 和失败关闭对象读取端口构造命令入口。
    pub fn new(db: Database) -> Self {
        let rbac = crate::adapters::identity::shared_rbac_service(db.clone());
        Self { db, rbac, object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort) }
    }
    /// 注入组合根已配置的对象读取授权能力。
    pub fn with_object_read(
        mut self,
        object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        self.object_read = object_read;
        self
    }
    /// 注入组合根持有的共享 RBAC，保持所有命令使用同一授权提供方。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = rbac;
        self
    }
    pub(super) fn domain(&self) -> ReturnsService {
        ReturnsService::new(self.db.clone())
    }
    pub(super) fn reads(&self) -> ReturnsReadService {
        ReturnsReadService::new(self.db.clone())
    }
}

/// 在审批运行时的外层事务内执行退款或冲正最终动作。
///
/// # 参数
/// * `db` - 数据库实例
/// * `document_type` - 退款或冲正单据类型
/// * `business_object_id` - 业务对象 ID
/// * `actor` - 已认证操作人
/// * `session` - 审批运行时持有的唯一事务会话
///
/// # 返回
/// 领域过账、业务审计与全部关联事实写入成功时返回 `Ok(())`。
///
/// # 错误
/// 单据类型不属于退货退款域，或领域状态/额度/持久化不变量失败时返回错误。
pub async fn finalize_approved_return_in_transaction(
    db: &Database,
    document_type: DocumentType,
    business_object_id: &str,
    actor: &application_core::AuditActor,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let actor_id = actor.id();
    match document_type {
        DocumentType::CustomerRefund => {
            customer_refund::apply_customer_refund_final_post(
                db,
                business_object_id,
                actor_id,
                actor,
                session,
            )
            .await
        },
        DocumentType::SupplierRefund => {
            supplier_refund::apply_supplier_refund_final_post(
                db,
                business_object_id,
                actor_id,
                actor,
                session,
            )
            .await
        },
        other => Err(Error::BusinessLogicError(format!("单据类型 {} 不属于退款冲正最终动作", other.label()))),
    }
}

/// 在审批运行时持有的事务内撤回资金纠错单审批。
///
/// # 错误
/// 单据不存在、类型与动作不匹配、状态迁移或 CAS 写入失败时返回错误。
pub async fn cancel_approval_in_transaction(
    db: &Database,
    document_type: DocumentType,
    id: &str,
    action: erp_workflow::service::approval::policy::ApprovalDomainAction,
    actor: &application_core::AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    match document_type {
        DocumentType::CustomerRefund => {
            let mut refund = db
                .customer_refunds()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("客户退款单不存在".to_string()))?;
            adapter::execute_customer_refund_domain_action(&mut refund, action)?;
            erp_returns::service::ReturnsService::new(db.clone())
                .persist_customer_refund(&mut refund, executor)
                .await?;
        },
        DocumentType::SupplierRefund => {
            let mut refund = db
                .supplier_refunds()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("供应商退款单不存在".to_string()))?;
            adapter::execute_supplier_refund_domain_action(&mut refund, action)?;
            erp_returns::service::ReturnsService::persist_supplier_refund(db, &mut refund, executor).await?;
        },
        DocumentType::ReceiptReversal => {
            let mut reversal = db
                .receipt_reversals()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("回款冲正单不存在".to_string()))?;
            adapter::execute_receipt_reversal_domain_action(&mut reversal, action)?;
            erp_returns::service::ReturnsService::persist_receipt_reversal(db, &mut reversal, executor)
                .await?;
        },
        DocumentType::PaymentReversal => {
            let mut reversal = db
                .payment_reversals()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("付款冲正单不存在".to_string()))?;
            adapter::execute_payment_reversal_domain_action(&mut reversal, action)?;
            erp_returns::service::ReturnsService::persist_payment_reversal(db, &mut reversal, executor)
                .await?;
        },
        other => {
            return Err(Error::ValidationError(format!("{} 不属于退货退款审批动作端口", other.label())));
        },
    }
    let audit =
        actor.clone().resource_log("returns.cancel_approval", document_type.as_str(), id.to_string())?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
