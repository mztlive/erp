//! 开票申请命令统一使用独立财务回执与一次中文业务事件。

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{
    AuditAction, AuditFact, AuditField, AuditFieldKind, AuditLog, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult,
};
use erp_finance::entity::receivable::SalesInvoiceRequest;
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;
use persistence_core::NoTransaction;

use super::super::ReceivableProcess;
use crate::finance_posting::command_recovery::{recovered_resource, recovered_view};
use crate::{Error, Result};

const FIELDS: &[AuditField] =
    &[AuditField { code: "amount", label: "申请金额", kind: AuditFieldKind::Amount }];
pub(super) const SUBMIT: AuditAction = AuditAction {
    code: "sales_invoice_request.submit",
    resource_type: "sales_invoice_request",
    label: "提交开票申请",
    version: 1,
    allowed_fields: FIELDS,
};
pub(super) const CANCEL: AuditAction = AuditAction {
    code: "sales_invoice_request.cancel",
    resource_type: "sales_invoice_request",
    label: "撤回开票申请",
    version: 1,
    allowed_fields: FIELDS,
};

/// 只投影申请编号与金额，不记录请求体、原因原文或命令机器协议。
/// # 参数
/// 当前操作人、已执行的申请事实、明确动作目录及可选稳定命令身份。
/// # 返回
/// 返回一次结构化中文业务事件。
/// # 错误
/// 动作、身份或安全投影不合法时返回错误。
pub(super) fn event(
    actor: &AuditActor,
    request: &SalesInvoiceRequest,
    action: AuditAction,
    command: Option<&CommandReceipt>,
) -> Result<AuditLog> {
    let context = BusinessEventContext::new(actor.clone(), action)?
        .with_command_id(command.map(|value| value.id().to_string()))?;
    Ok(context.log(BusinessEventContent {
        target_id: request.base.id.clone(),
        target_number: Some(request.request_no.clone()),
        result: BusinessEventResult::Succeeded,
        field_changes: vec![],
        facts: vec![AuditFact {
            field: "amount".to_string(),
            value: AuditValue::Amount { value: request.data.amount },
        }],
    })?)
}

impl ReceivableProcess {
    /// 错误恢复只查证原命令；无法读回原对象时保持首次未知提交错误。
    /// # 参数
    /// * `result` - 首次事务结果。
    /// * `command` - 本次请求的精确命令身份。
    /// # 返回
    /// 返回首次结果对象当前视图。
    /// # 错误
    /// 未保存回执时保留原错误；未知提交恢复失败时保留原未知错误。
    pub(super) async fn finish_invoice_request_command(
        &self,
        result: Result<String>,
        command: &CommandReceipt,
    ) -> Result<InvoiceRequestView> {
        let error = match result {
            Ok(id) => return Ok(self.read.invoice_request_detail(&id).await?),
            Err(error) => error,
        };
        let lookup = FinanceCommandReceiptService::new(self.db.clone())
            .committed_resource_id(command, &mut NoTransaction)
            .await
            .map_err(Error::from);
        let recovered = recovered_resource(error, lookup)?;
        let view = self.read.invoice_request_detail(&recovered.id).await.map_err(Error::from);
        recovered_view(view, recovered.original_unknown)
    }
}
