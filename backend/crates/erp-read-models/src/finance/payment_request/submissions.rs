//! 历史原审核依据只能按不可变提交身份读取，不能以当前申请资料代替。

use application_core::AuditActor;
use erp_finance::entity::payable::PurchasePaymentRequest;
use erp_finance::repository::PurchasePaymentRequestSnapshotExt;
use persistence_core::{Executor, Transactional};

use super::previews::frozen_previews;
use super::{
    PaymentRequestMaterialReference, PurchasePaymentRequestReadService, PurchasePaymentRequestSubmissionView,
};
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    /// 返回指定提交版本的完整原审核材料，重新提交不得改变旧结果。
    ///
    /// # 参数
    /// * `actor` / `id` / `version` - 当前身份、申请及精确提交版本。
    /// # 返回
    /// 返回完整原始金额、来源预览、脱敏账户和冻结附件。
    /// # 错误
    /// 任一历史采购来源不可读、版本缺失或冻结关系不一致时拒绝。
    pub async fn submission(
        &self,
        actor: &AuditActor,
        id: &str,
        version: u32,
    ) -> Result<PurchasePaymentRequestSubmissionView> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request = this.authorized_submission(&actor, &id, version, executor).await?;
                    let (purchase_previews, sales_previews) =
                        frozen_previews(&request.lines, &request.payer_party_id)?;
                    let attachments = this.attachments(&request, executor).await?;
                    Ok(PurchasePaymentRequestSubmissionView {
                        request,
                        purchase_previews,
                        sales_previews,
                        attachments,
                    })
                })
            })
            .await
    }

    /// 历史下载沿历史逐采购资格及精确提交冻结文件版本复验。
    ///
    /// # 参数
    /// * `actor` / `id` / `version` / `asset_id` - 当前身份、申请、提交版本和文件。
    /// # 返回
    /// 返回历史文件下载前后可比较的关系证明。
    /// # 错误
    /// 历史资格失效、文件不在原清单或治理/内容版本改变时拒绝。
    pub async fn require_submission_material(
        &self,
        actor: &AuditActor,
        id: &str,
        version: u32,
        asset_id: &str,
    ) -> Result<PaymentRequestMaterialReference> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        let asset_id = asset_id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request = this.authorized_submission(&actor, &id, version, executor).await?;
                    this.material(&request, &asset_id, executor).await
                })
            })
            .await
    }

    async fn authorized_submission(
        &self,
        actor: &AuditActor,
        id: &str,
        version: u32,
        executor: &mut dyn Executor,
    ) -> Result<PurchasePaymentRequest> {
        let request = self
            .db
            .purchase_payment_request_snapshots()
            .find_for_version(id, version, executor)
            .await?
            .filter(|snapshot| {
                snapshot.request_id == id
                    && snapshot.request.base.id == id
                    && snapshot.request.submission_version == version
                    && snapshot.request.approval_instance_id.as_deref()
                        == Some(snapshot.approval_instance_id.as_str())
            })
            .map(|snapshot| snapshot.request)
            .ok_or_else(|| Error::NotFound("付款申请历史提交不存在或无权查看".into()))?;
        let scope = self.source_scope(actor, "detail", executor).await?;
        if !self.readable(&request, &scope, executor).await? {
            return Err(Error::NotFound("付款申请历史提交不存在或无权查看".into()));
        }
        Ok(request)
    }
}
