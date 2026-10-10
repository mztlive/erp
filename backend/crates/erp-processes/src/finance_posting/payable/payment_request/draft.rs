//! 草稿不占用应付金额；保存时仍逐单证明申请资格。
use erp_audit::AuditActorLogs;
use erp_finance::dto::payable::payment_request::SavePurchasePaymentRequest;
use persistence_core::Transactional;

use super::*;
use crate::audit::persist_log;

impl PurchasePaymentRequestProcess {
    /// 创建当前采购经办人的付款申请草稿。
    /// # 参数
    /// * `req` - 逐应付来源、金额和付款依据。
    /// * `actor` - 当前已认证申请人。
    /// # 返回
    /// 返回不占用额度的申请草稿。
    /// # 错误
    /// 来源无资格、金额或收款账户不合法时返回错误。
    pub async fn create(
        &self,
        req: SavePurchasePaymentRequest,
        actor: &AuditActor,
    ) -> Result<PurchasePaymentRequest> {
        if req.expected_version.is_some() {
            return Err(Error::ValidationError("新申请不能指定已有版本".into()));
        }
        let mut request = PurchasePaymentRequest::new(id_generator::next_id(), req.data, actor.id())?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    require_sources(&db, &rbac, &request, &actor, "create", executor).await?;
                    let basis =
                        source::prepare_submission(&db, &rbac, &request, &actor, false, executor).await?;
                    request.set_draft_basis(basis)?;
                    db.purchase_payment_requests().create(&request, executor).await?;
                    let audit = actor.resource_log(
                        "purchase_payment_request.create",
                        "purchase_payment_request",
                        request.base.id.clone(),
                    )?;
                    persist_log(&db, &audit, executor).await?;
                    Ok(request)
                })
            })
            .await
    }

    /// 保存草稿、驳回或撤回后的申请资料。
    /// # 参数
    /// * `id` - 申请身份。
    /// * `req` - 当前版本与完整表单。
    /// * `actor` - 原申请人。
    /// # 返回
    /// 返回新版本申请；草稿不占用额度。
    /// # 错误
    /// 非本人、版本变化、已经批准或来源不合法时拒绝。
    pub async fn update(
        &self,
        id: &str,
        req: SavePurchasePaymentRequest,
        actor: &AuditActor,
    ) -> Result<PurchasePaymentRequest> {
        let version = req.expected_version.ok_or_else(|| Error::ValidationError("请提供申请版本".into()))?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut request = load(&db, &id, executor).await?;
                    ensure_creator(&request, &actor)?;
                    ensure_version(&request, version)?;
                    request.update(req.data)?;
                    require_sources(&db, &rbac, &request, &actor, "update", executor).await?;
                    let basis =
                        source::prepare_submission(&db, &rbac, &request, &actor, false, executor).await?;
                    request.set_draft_basis(basis)?;
                    db.purchase_payment_requests().update(&mut request, executor).await?;
                    let audit = actor.resource_log(
                        "purchase_payment_request.update",
                        "purchase_payment_request",
                        id,
                    )?;
                    persist_log(&db, &audit, executor).await?;
                    Ok(request)
                })
            })
            .await
    }
}
