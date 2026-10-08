//! 退货、退款和资金冲正的跨域事务、审批与审计组合入口。
use crate::audit::persist_log;

mod adapter;
mod cancel_approval;
mod cancel_write;
pub(crate) mod command_recovery;
mod customer_refund;
mod draft_update;
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
use erp_audit::AuditActorLogs;
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
    ///
    /// # 参数
    /// * `db` - 组合根数据库。
    ///
    /// # 返回
    /// 返回命令入口。对象读取端口为失败关闭实现。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        let rbac = crate::adapters::identity::shared_rbac_service(db.clone());
        Self { db, rbac, object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort) }
    }
    /// 注入组合根已配置的对象读取授权能力。
    ///
    /// 消耗 `self`。
    ///
    /// # 参数
    /// * `object_read` - 组合根已配置的对象读取端口。
    ///
    /// # 返回
    /// 返回换入该端口后的命令入口。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_object_read(
        mut self,
        object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        self.object_read = object_read;
        self
    }
    /// 注入组合根持有的共享 RBAC，保持所有命令使用同一授权提供方。
    ///
    /// 消耗 `self`。
    ///
    /// # 参数
    /// * `rbac` - 组合根持有的共享 RBAC。
    ///
    /// # 返回
    /// 返回换入该 RBAC 后的命令入口。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = rbac;
        self
    }
    /// 用当前数据库构造退货退款领域服务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库的 `ReturnsService`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn domain(&self) -> ReturnsService {
        ReturnsService::new(self.db.clone())
    }

    /// 用当前数据库构造退货退款读模型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回绑定当前数据库的 `ReturnsReadService`。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn reads(&self) -> ReturnsReadService {
        ReturnsReadService::new(self.db.clone())
    }
}

/// 在审批运行时的外层事务内执行客户退款或供应商退款的最终过账。
///
/// 只接受 `DocumentType::CustomerRefund` 与 `DocumentType::SupplierRefund`。回款冲正和付款冲正不在此分派。
///
/// # 参数
/// * `db` - 数据库实例
/// * `document_type` - 退款单据类型
/// * `business_object_id` - 业务对象 ID
/// * `actor` - 已认证操作人
/// * `executor` - 审批运行时持有的执行器
///
/// # 返回
/// 领域过账、业务审计与全部关联事实写入成功时返回 `Ok(())`。
///
/// # 错误
/// 单据类型不是客户退款或供应商退款时返回 `BusinessLogicError`。领域状态、额度或持久化失败时返回对应错误。
pub async fn finalize_approved_return(
    db: &Database,
    document_type: DocumentType,
    business_object_id: &str,
    actor: &application_core::AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let actor_id = actor.id();
    match document_type {
        DocumentType::CustomerRefund => {
            customer_refund::apply_customer_refund_final_post(
                db,
                business_object_id,
                actor_id,
                actor,
                executor,
            )
            .await
        },
        DocumentType::SupplierRefund => {
            supplier_refund::apply_supplier_refund_final_post(
                db,
                business_object_id,
                actor_id,
                actor,
                executor,
            )
            .await
        },
        other => Err(Error::BusinessLogicError(format!("单据类型 {} 不属于退款冲正最终动作", other.label()))),
    }
}

/// 在审批运行时持有的事务内撤回资金纠错单审批。
///
/// # 参数
/// * `db` - 数据库。
/// * `document_type` - 客户退款、供应商退款、回款冲正或付款冲正。
/// * `id` - 单据主键。
/// * `action` - 合同强类型领域动作。
/// * `actor` - 已认证操作人。
/// * `executor` - 审批运行时持有的执行器。
///
/// # 返回
/// 成功时无返回值。单据已按动作写回，并追加撤回审计。
///
/// # 错误
/// 单据不存在、类型与动作不匹配、状态迁移或 CAS 写入失败时返回错误。
pub async fn cancel_approval(
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
    persist_log(db, &audit, executor).await?;
    Ok(())
}
