//! 发单资金保障与正式实付门槛分别执行；供应商协议要求不能被采购配置降级。
use erp_party::SensitiveDataCodec;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage};
use erp_supply::entity::execution_budget::ApprovedExecutionBudget;
use erp_supply::ports::connector::funds::OrderFundingRequirement;
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::Executor;

use super::SupplierApiOrderProcess;
use super::snapshot::ExecutionSnapshot;
use crate::fulfillment_execution::ensure_prepay_gate;
use crate::{Error, Result, supplier_deposits};

/// 从服务器保存的加密快照读取执行组批准预算。
/// # 参数
/// `execution` 必须由调用方按当前权限读取，`codec` 为应用敏感资料加密器。
/// # 返回
/// 与原执行组及采购修订精确绑定的商品、运费和总预算。
/// # 错误
/// 快照缺失、历史记录无预算或来源版本不符时拒绝，不能采用客户端金额。
pub fn execution_budget(
    execution: &ApiOrderExecution,
    codec: &SensitiveDataCodec,
) -> Result<ApprovedExecutionBudget> {
    let encrypted = execution
        .request_encrypted
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("执行组尚未冻结批准预算".into()))?;
    let snapshot: ExecutionSnapshot = serde_json::from_str(&codec.decrypt(encrypted)?)
        .map_err(|_| Error::Internal("执行组批准预算无法读取".into()))?;
    let budget = snapshot
        .approved_budget
        .ok_or_else(|| Error::BusinessLogicError("原执行未保存可核实的费用分配，请先核查采购依据".into()))?;
    if budget.execution_id != execution.base.id
        || budget.purchase_revision_id != execution.purchase_order_revision_id
        || budget.approved_total != snapshot.approved_total
    {
        return Err(Error::ConflictError("执行组批准预算与原采购版本不一致".into()));
    }
    Ok(budget)
}

impl SupplierApiOrderProcess {
    pub(super) async fn ensure_preflight_funds(
        &self,
        execution: &ApiOrderExecution,
        purchase: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, executor)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        match connector.funding_requirement() {
            OrderFundingRequirement::PostedPaymentGate => {
                ensure_prepay_gate(&self.db, executor, purchase).await
            },
            OrderFundingRequirement::VerifiedSupplierDeposit => Ok(()),
        }
    }

    pub(super) async fn ensure_execution_funds(
        &self,
        execution: &ApiOrderExecution,
        purchase: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, executor)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        match connector.funding_requirement() {
            OrderFundingRequirement::PostedPaymentGate => {
                ensure_prepay_gate(&self.db, executor, purchase).await
            },
            OrderFundingRequirement::VerifiedSupplierDeposit => {
                if !matches!(
                    execution.stage,
                    ApiOrderStage::Create
                        | ApiOrderStage::ConfirmOrder
                        | ApiOrderStage::CreationUnknown
                        | ApiOrderStage::ConfirmationUnknown
                ) {
                    return Ok(());
                }
                let snapshot = self.decode(execution)?;
                supplier_deposits::ensure_execution_funds(
                    &self.db,
                    execution,
                    snapshot.approved_total,
                    executor,
                )
                .await
            },
        }
    }
    pub(super) async fn mark_funds_sent(
        &self,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, executor)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if connector.funding_requirement() == OrderFundingRequirement::VerifiedSupplierDeposit {
            let snapshot = self.decode(execution)?;
            supplier_deposits::mark_execution_funds_sent(
                &self.db,
                execution,
                snapshot.approved_total,
                executor,
            )
            .await?;
        }
        Ok(())
    }
}
