//! 文件只沿申请允许清单及精确来源读取，完整银行账号另经当前任务资格揭示。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_finance::dto::payable::RevealPaymentRecipientRequest;
use erp_finance::entity::payable::{
    PaymentRequestAccountSnapshot, PurchasePaymentRequest, PurchasePaymentRequestStatus,
};
use erp_identity::AccessTarget;
use erp_party::PartyExt;
use erp_party::repository::prelude::*;
use erp_permissions::{payable as payable_permissions, purchase_payment_request as payment_permissions};
use erp_support::repository::FileAssetExt;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use erp_workflow::repository::prelude::*;
use erp_workflow::{ApprovalIntegrationExt, DocumentType, WorkItemExt};
use persistence_core::{Executor, Transactional};

use super::{PaymentRequestAttachmentView, PurchasePaymentRequestReadService};
use crate::ports::work_item_authorization::{AuthorizedTaskFact, WorkItemAuthorizationReadPort};
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
    /// 确定采购付款申请的适用条件时，普通详情沿全部来源资格证明本申请材料关系与治理状态。
    ///
    /// # 参数
    /// * `actor` / `id` / `asset_id` - 当前身份、申请与允许文件身份。
    ///
    /// # 返回
    /// 返回冻结元数据与当前关系版本，用于下载前后复验。
    ///
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
                Box::pin(async move { this.material_read(&actor, &id, &asset_id, executor).await })
            })
            .await
    }

    // 处理采购付款申请时，详情资格先于申请和文件读取，全部补齐后验证原查询。
    async fn material_read(
        &self,
        actor: &AuditActor,
        id: &str,
        asset_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<PaymentRequestMaterialReference> {
        let scope = self.source_scope(actor, &payment_permissions::DETAIL, executor).await?;
        let request = self.load(id, executor).await?;
        if !self.readable(&request, &scope, executor).await? {
            return Err(Error::NotFound("付款申请材料不存在或无权下载".into()));
        }

        let material = self.material(&request, asset_id, executor).await?;
        scope.deliver(executor).await?;
        Ok(material)
    }

    /// 确定采购付款申请的适用条件时，当前审批和执行任务只可下载本申请冻结允许材料。
    ///
    /// # 参数
    /// * `actor` / `task_id` / `asset_id` / `auth` - 身份、任务、文件与正式任务授权器。
    ///
    /// # 返回
    /// 返回与当前任务和申请提交绑定的文件证明。
    ///
    /// # 错误
    /// 任务失效、关系越界、文件改变或安全隔离时拒绝。
    pub async fn require_task_material(
        &self,
        actor: &AuditActor,
        task_id: &str,
        asset_id: &str,
        auth: Arc<dyn WorkItemAuthorizationReadPort>,
    ) -> Result<PaymentRequestMaterialReference> {
        let this = self.clone();
        let id = task_id.to_owned();
        let asset_id = asset_id.to_owned();
        let actor_for_tx = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (expected, access) = auth.authorize(&id, &actor_for_tx, executor).await?;
                    let request = this.task_request(&actor_for_tx, &id, &expected, executor).await?;
                    let material = this.material(&request, &asset_id, executor).await?;

                    this.rbac.validate(&access, executor).await?;
                    Ok::<_, Error>(material)
                })
            })
            .await
    }

    /// 检查采购付款申请资格时，主动揭示先重验敏感动作、当前任务与申请冻结账户版本。
    ///
    /// # 参数
    /// * `actor` / `id` / `input` / `auth` - 当前账号、申请、预期任务账户与任务授权器。
    ///
    /// # 返回
    /// 返回已证明的银行账户快照；调用方须经主体敏感服务解密并记录审计。
    ///
    /// # 错误
    /// 缺少敏感权限、任务或账户版本变化、账户失效或归属不符时拒绝。
    pub async fn authorize_reveal(
        &self,
        actor: &AuditActor,
        id: &str,
        input: &RevealPaymentRecipientRequest,
        auth: Arc<dyn WorkItemAuthorizationReadPort>,
    ) -> Result<PaymentRequestAccountSnapshot> {
        let this = self.clone();
        let actor_for_tx = actor.clone();
        let id = id.to_owned();
        let input_for_tx = input.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (expected, access) =
                        auth.authorize(input_for_tx.work_item_id.as_ref(), &actor_for_tx, executor).await?;
                    let snapshot =
                        this.reveal_snapshot(&actor_for_tx, &id, &input_for_tx, &expected, executor).await?;
                    this.rbac.validate(&access, executor).await?;
                    Ok(snapshot)
                })
            })
            .await
    }

    // 读取采购付款申请时，在原任务读取阶段核对敏感资格、精确任务版本与真实冻结账户。
    async fn reveal_snapshot(
        &self,
        actor: &AuditActor,
        id: &str,
        input: &RevealPaymentRecipientRequest,
        fact: &AuthorizedTaskFact,
        executor: &mut dyn Executor,
    ) -> Result<PaymentRequestAccountSnapshot> {
        self.rbac
            .require(actor, &payable_permissions::PAYMENT_RECIPIENT_REVEAL, AccessTarget::Operation, executor)
            .await?;

        let request = self.task_request(actor, input.work_item_id.as_ref(), fact, executor).await?;
        let task = self
            .db
            .work_items()
            .find_by_id(input.work_item_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("付款申请任务不存在".into()))?;
        if request.base.id != id || task.base.version.to_string() != input.expected_task_version {
            return Err(Error::ConflictError("付款申请任务版本已变化".into()));
        }

        let snapshot =
            request.payee_account.ok_or_else(|| Error::ConflictError("付款申请未冻结收款账户".into()))?;
        self.validate_reveal_account(&snapshot, input, executor).await?;
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
                .ok_or_else(|| {
                    Error::ConflictError("付款申请的审批资料与本次提交内容不一致，请核对提交资料".into())
                })?;
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::ids::WorkItemId;
    use erp_identity::testing::unavailable_authorization;
    use erp_workflow::WorkItemType;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn material_authorization_failure_stops_before_request_and_file_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("payment_request_material_qualification");
        let service = PurchasePaymentRequestReadService::new(db, unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        let result = service.material_read(&actor, "request", "file", &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }

    #[tokio::test]
    async fn reveal_requires_snapshot_before_identity_or_request_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1")
            .await
            .unwrap()
            .database("payment_request_reveal_guard");
        let service = PurchasePaymentRequestReadService::new(db, unavailable_authorization());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let input = RevealPaymentRecipientRequest {
            work_item_id: WorkItemId::new("task"),
            expected_task_version: "1".into(),
            expected_bank_account_id: "bank-account".into(),
            expected_bank_account_version: 1,
        };
        let fact = AuthorizedTaskFact {
            work_item_type: WorkItemType::SupplierPaymentExecution,
            business_object_type: "purchase_payment_request".into(),
            business_object_id: "request".into(),
            subject_version: "1".into(),
            allowed_actions: vec![],
        };

        let result = service.reveal_snapshot(&actor, "request", &input, &fact, &mut NoTransaction).await;
        assert!(
            matches!(result, Err(Error::ValidationError(reason)) if reason == "写入授权必须使用快照事务")
        );

        let result =
            service.reveal_snapshot(&actor, "request", &input, &fact, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }
}
