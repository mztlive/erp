//! 原回款完整编辑视图与未知提交确认；读取不改变单据状态。

use application_core::AuditActor;
use erp_finance::repository::ReceivableExt;
use erp_permissions::receivable;
use erp_read_models::finance::document_files::{FinancialFileView, financial_document_files};
use erp_read_models::finance::dto::CustomerReceiptView;
use erp_read_models::finance::receivable::ReceivableReadService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::ReceivableProcess;
use super::receipt_access::ensure_receipt_edit_authorized;
use crate::{Error, FinancePostingPorts, Result};

impl ReceivableProcess {
    /// 原登记人按草稿编辑资格读取回款凭证。
    /// # 参数
    /// 回款主键与当前认证操作人。
    /// # 返回
    /// 本单凭证展示清单。
    /// # 错误
    /// 当前编辑资格、原登记人或文件读取校验失败时拒绝。
    pub async fn customer_receipt_draft_files(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<FinancialFileView>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    ensure_receipt_edit_authorized(&rbac, &actor, &id, executor).await?;
                    let receipt = db
                        .customer_receipts()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户回款单不存在".into()))?;
                    Ok(financial_document_files(&db, &id, &receipt.receipt_no, executor).await?)
                })
            })
            .await
    }

    /// 读取原登记人的完整回款字段，供草稿编辑与未知提交确认。
    ///
    /// # 参数
    /// * `id` - 原回款主键
    /// * `actor` - 当前已认证原登记人
    ///
    /// # 返回
    /// 返回真实状态、金额、版本、审批与原拟核销分配；只有 Draft 状态可编辑。
    ///
    /// # 错误
    /// 当前账号失效、无提交或完整来源资格、非原登记人或读取失败时拒绝。
    pub async fn customer_receipt_draft(&self, id: &str, actor: &AuditActor) -> Result<CustomerReceiptView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    ensure_receipt_edit_authorized(&rbac, &actor, &id, executor).await?;
                    receipt_view_with_actions(&db, &rbac, &actor, &id, executor).await
                })
            })
            .await
    }
}

/// 原单读资格已在调用方执行器证明；撤回动作仍须当前 cancel 静态资格。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 授权源。
/// * `actor` - 当前已认证操作人。
/// * `id` - 回款单 ID。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回回款视图；当前账号没有撤回静态资格时去掉 `CANCEL`。
///
/// # 错误
/// 视图或撤回资格读取失败时返回错误。
pub(super) async fn receipt_view_with_actions(
    db: &Database,
    rbac: &FinancePostingPorts,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<CustomerReceiptView> {
    let mut view =
        ReceivableReadService::new(db.clone()).customer_receipt_view_with_executor(id, executor).await?;

    let authorization = &rbac.authorization;
    if view.approval.allowed_actions.iter().any(|action| action == "CANCEL")
        && !authorization
            .has_permission(actor, &receivable::CUSTOMER_RECEIPT_CANCEL_APPROVAL, executor)
            .await?
    {
        view.approval.allowed_actions.retain(|action| action != "CANCEL");
    }
    Ok(view)
}
