//! 文件只沿申请允许清单及精确来源读取，完整银行账号另经当前任务资格揭示。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_finance::dto::payable::RevealPaymentRecipientRequest;
use erp_finance::entity::payable::{
    PaymentRequestAccountSnapshot, PurchasePaymentRequest, PurchasePaymentRequestStatus,
};
use erp_party::PartyExt;
use erp_party::repository::prelude::*;
use erp_support::repository::FileAssetExt;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use erp_workflow::repository::prelude::*;
use erp_workflow::{ApprovalIntegrationExt, DocumentType, WorkItemExt};
use persistence_core::{Executor, Transactional};

use super::{PaymentRequestAttachmentView, PurchasePaymentRequestReadService};
use crate::approval_runtime::has_document_permission;
use crate::ports::work_item_authorization::WorkItemAuthorizationReadPort;
use crate::sales_center::materials::{material_from_asset, validate_materials};
use crate::{Error, Result};

/// 服务器文件读取证明，冻结文件版本不作为可转授客户端令牌。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentRequestMaterialReference {
    pub file: ApprovalMaterialFile,
    pub request_id: String,
    relation_version: String,
}

impl PurchasePaymentRequestReadService {
    /// 普通详情沿全部来源资格证明本申请材料关系与治理状态。
    ///
    /// # 参数
    /// * `actor` / `id` / `asset_id` - 当前身份、申请与允许文件身份。
    /// # 返回
    /// 返回冻结元数据与当前关系版本，用于下载前后复验。
    /// # 错误
    /// 任一来源不可读、文件不在清单、文件变更或治理关闭时拒绝。
    pub async fn require_material(
        &self,
        actor: &AuditActor,
        id: &str,
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
                    let request = this.load(&id, executor).await?;
                    let scope = this.source_scope(&actor, "detail", executor).await?;
                    if !this.readable(&request, &scope, executor).await? {
                        return Err(Error::NotFound("付款申请材料不存在或无权下载".into()));
                    }
                    this.material(&request, &asset_id, executor).await
                })
            })
            .await
    }

    /// 当前审批和执行任务只可下载本申请冻结允许材料。
    ///
    /// # 参数
    /// * `actor` / `task_id` / `asset_id` / `auth` - 身份、任务、文件与正式任务授权器。
    /// # 返回
    /// 返回与当前任务和申请提交绑定的文件证明。
    /// # 错误
    /// 任务失效、关系越界、文件改变或安全隔离时拒绝。
    pub async fn require_task_material(
        &self,
        actor: &AuditActor,
        task_id: &str,
        asset_id: &str,
        auth: &impl WorkItemAuthorizationReadPort,
    ) -> Result<PaymentRequestMaterialReference> {
        let fact = auth.authorize(task_id, actor).await?;
        let this = self.clone();
        let id = task_id.to_owned();
        let asset_id = asset_id.to_owned();
        let expected = fact.clone();
        let actor_for_tx = actor.clone();
        let reference = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let request = this.task_request(&actor_for_tx, &id, &expected, executor).await?;
                    this.material(&request, &asset_id, executor).await
                })
            })
            .await?;
        if auth.authorize(task_id, actor).await? != fact {
            return Err(Error::ConflictError("付款申请任务资格已变化".into()));
        }
        Ok(reference)
    }

    /// 主动揭示先重验敏感动作、当前任务与申请冻结账户版本。
    ///
    /// # 参数
    /// * `actor` / `id` / `input` / `auth` - 当前账号、申请、预期任务账户与任务授权器。
    /// # 返回
    /// 返回已证明的银行账户快照；调用方须经主体敏感服务解密并记录审计。
    /// # 错误
    /// 缺少敏感权限、任务或账户版本变化、账户失效或归属不符时拒绝。
    pub async fn authorize_reveal(
        &self,
        actor: &AuditActor,
        id: &str,
        input: &RevealPaymentRecipientRequest,
        auth: &impl WorkItemAuthorizationReadPort,
    ) -> Result<PaymentRequestAccountSnapshot> {
        let fact = auth.authorize(input.work_item_id.as_ref(), actor).await?;
        let this = self.clone();
        let actor_for_tx = actor.clone();
        let id = id.to_owned();
        let input_for_tx = input.clone();
        let expected = fact.clone();
        let snapshot = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if !has_document_permission(
                        &this.db,
                        &this.rbac,
                        &actor_for_tx,
                        "party_bank_account",
                        "reveal",
                        executor,
                    )
                    .await?
                    {
                        return Err(Error::Forbidden("无权揭示完整收款账号".into()));
                    }
                    let request = this
                        .task_request(&actor_for_tx, input_for_tx.work_item_id.as_ref(), &expected, executor)
                        .await?;
                    let task = this
                        .db
                        .work_items()
                        .find_by_id(input_for_tx.work_item_id.as_ref(), executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("付款申请任务不存在".into()))?;
                    if request.base.id != id
                        || task.base.version.to_string() != input_for_tx.expected_task_version
                    {
                        return Err(Error::ConflictError("付款申请任务版本已变化".into()));
                    }
                    let snapshot = request
                        .payee_account
                        .ok_or_else(|| Error::ConflictError("付款申请未冻结收款账户".into()))?;
                    this.validate_reveal_account(&snapshot, &input_for_tx, executor).await?;
                    Ok(snapshot)
                })
            })
            .await?;
        if auth.authorize(input.work_item_id.as_ref(), actor).await? != fact {
            return Err(Error::ConflictError("付款申请任务资格已变化".into()));
        }
        Ok(snapshot)
    }

    async fn validate_reveal_account(
        &self,
        snapshot: &PaymentRequestAccountSnapshot,
        input: &RevealPaymentRecipientRequest,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .party_bank_accounts()
            .find_by_id(&snapshot.account_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("付款申请收款账户不存在".into()))?;
        let current = self
            .db
            .party_bank_accounts()
            .list_current_on(&account.party_id, BusinessDate::today(), executor)
            .await?;
        if snapshot.account_id != input.expected_bank_account_id
            || snapshot.version != input.expected_bank_account_version
            || account.base.version != snapshot.version
            || account.party_id.as_ref() != snapshot.supplier_party_id
            || !current.iter().any(|candidate| candidate.base.id == account.base.id)
        {
            return Err(Error::ConflictError("付款申请收款账户已变化，请关闭余款后重新申请".into()));
        }
        Ok(())
    }

    pub(super) async fn material(
        &self,
        request: &PurchasePaymentRequest,
        asset_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PaymentRequestMaterialReference> {
        let mut files = self.allowed_files(request, executor).await?;
        if matches!(
            request.status,
            PurchasePaymentRequestStatus::Approved
                | PurchasePaymentRequestStatus::PartiallyPaid
                | PurchasePaymentRequestStatus::Paid
                | PurchasePaymentRequestStatus::Closed
        ) {
            files.extend(self.payment_receipts(request, executor).await?);
        }
        let file = files
            .into_iter()
            .find(|file| file.file_asset_id.as_ref() == asset_id)
            .ok_or_else(|| Error::NotFound("付款申请材料不存在或无权下载".into()))?;
        validate_materials(&self.db, std::slice::from_ref(&file), executor).await?;
        Ok(PaymentRequestMaterialReference {
            file,
            request_id: request.base.id.clone(),
            relation_version: format!(
                "{}:{}:{}",
                request.base.id, request.base.version, request.submission_version
            ),
        })
    }

    pub(super) async fn attachments(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PaymentRequestAttachmentView>> {
        Ok(self
            .allowed_files(request, executor)
            .await?
            .into_iter()
            .map(|file| PaymentRequestAttachmentView {
                file_asset_id: file.file_asset_id.to_string(),
                file_name: file.file_name,
                content_type: file.content_type,
                byte_size: file.byte_size,
            })
            .collect())
    }

    async fn allowed_files(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalMaterialFile>> {
        let ids = request
            .data
            .attachment_ids
            .iter()
            .chain(request.lines.iter().flat_map(|line| &line.source_snapshot.allowed_attachment_ids))
            .collect::<BTreeSet<_>>();
        let is_editable = matches!(
            request.status,
            PurchasePaymentRequestStatus::Draft
                | PurchasePaymentRequestStatus::Rejected
                | PurchasePaymentRequestStatus::Withdrawn
        );
        if let Some(instance) = request.approval_instance_id.as_ref().filter(|_| !is_editable) {
            let snapshot = self
                .db
                .approval_subject_snapshots()
                .find_by_process_instance_id(instance, executor)
                .await?
                .filter(|snapshot| {
                    snapshot.document_type == DocumentType::PurchasePaymentRequest
                        && snapshot.business_object_id == request.base.id
                        && snapshot.subject_version == request.submission_version
                })
                .ok_or_else(|| Error::ConflictError("付款申请冻结材料与当前提交不一致".into()))?;
            return Ok(snapshot
                .material_files
                .into_iter()
                .filter(|file| ids.contains(&file.file_asset_id.to_string()))
                .collect());
        }
        let mut files = Vec::new();
        for id in ids {
            let file = self
                .db
                .file_assets()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("付款申请附件不存在".into()))?;
            files.push(material_from_asset(&file));
        }
        Ok(files)
    }
}
