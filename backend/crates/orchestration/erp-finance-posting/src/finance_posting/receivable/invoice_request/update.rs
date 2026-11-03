//! 原申请资料保存；不启动审批、不重新占用开票额度。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_finance::dto::receivable::UpdateInvoiceRequest;
use erp_finance::service::receivable::mapping::ensure_expected_version;
use erp_read_models::finance::receivable::invoice_request::InvoiceRequestView;

use super::super::ReceivableProcess;
use super::authorization::{MongoAccess, RequestAction, authorize};
use super::*;

impl ReceivableProcess {
    /// 保存原申请人的草稿或已驳回申请，保留当前状态及审批历史。
    ///
    /// # 参数
    /// * `id` - 原开票申请身份。
    /// * `req` - 乐观锁版本及完整开票资料。
    /// * `actor` - 已认证的原申请人。
    /// # 返回
    /// 返回保存后的开票申请详情。
    /// # 错误
    /// 账号或权限失效、非原申请人、版本冲突、状态或资料无效时不写入。
    pub async fn update_invoice_request(
        &self,
        id: &str,
        req: UpdateInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceRequestView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let response_actor = actor.clone();
        let actor = actor.clone();
        let request_id = id.to_owned();
        let revision = rbac.current_policy_revision().await?;
        rbac.clone()
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move {
                    authorize(
                        &MongoAccess { db: &db, rbac: &rbac },
                        &actor,
                        &request_id,
                        RequestAction::Submit,
                        executor,
                    )
                    .await?;
                    let mut request = load(&db, &request_id, executor).await?;
                    ensure_expected_version(request.base.version, req.expected_version)?;
                    request.update(req.data)?;
                    db.sales_invoice_requests().update(&mut request, executor).await?;
                    let audit = actor.clone().resource_log(
                        "sales_invoice_request.update",
                        "sales_invoice_request",
                        request_id,
                    )?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        super::command::committed_result(
            self.authorized_invoice_request_result(id, &response_actor, RequestAction::Submit).await,
        )
    }
}
