//! 提交事务冻结本单据直接关联的审批材料，不追溯客户或供应商其他附件。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::ids::{BusinessDocumentId, FileAssetId};
use erp_returns::repository::ReturnsExt;
use erp_support::repository::FileAssetExt;
use erp_support::repository::prelude::*;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalMaterialFile;
use erp_workflow::{DocumentRegistryExt, DocumentType, WorkflowAuthorizationPort};
use mongodb::Database;
use persistence_core::Executor;

use super::{authorize_material_transfer, capture_approval_display};
use crate::{Error, Result};

/// 在原提交事务内冻结显示与附件版本，历史快照不补取当前材料。
///
/// # 参数
/// * `db` / `executor` - 原业务提交事务。
/// * `auth` - 在同一事务中重验提交账号和文件预览资格的权限端口。
/// * `snapshot` - 已匹配本次提交的强类型快照。
/// # 返回
/// 返回仅包含本单据直接附件及本次退款凭证的不可变快照。
/// # 错误
/// 单据类型不匹配、附件资产缺失或材料超限时中止提交。
pub async fn freeze_approval_materials(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    mut snapshot: ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let document = db
        .business_documents()
        .find_by_id(&snapshot.business_object_id, executor)
        .await?
        .filter(|row| row.document_type == snapshot.document_type)
        .ok_or_else(|| Error::ConflictError("审批材料缺少匹配的业务单据注册".into()))?;
    let mut ids = db
        .document_attachments()
        .list_by_document(&BusinessDocumentId::new(document.base.id), executor)
        .await?
        .into_iter()
        .map(|row| row.file_asset_id.to_string())
        .collect::<BTreeSet<_>>();
    if let Some(id) = evidence(db, snapshot.document_type, &snapshot.business_object_id, executor).await? {
        ids.insert(id.to_string());
    }
    if ids.len() > 100 {
        return Err(Error::ValidationError("审批材料最多 100 个文件".into()));
    }
    let files =
        db.file_assets().find_by_ids(&ids.iter().map(FileAssetId::new).collect::<Vec<_>>(), executor).await?;
    if files.len() != ids.len() {
        return Err(Error::ConflictError("审批材料引用的文件不存在或已删除".into()));
    }
    let account = auth
        .load_account(&snapshot.payload.submitted_by, executor)
        .await?
        .filter(|account| account.is_active_backoffice())
        .ok_or_else(|| Error::Forbidden("审批材料原提交账号不存在或已失效".into()))?;
    let actor = AuditActor::new(account.id, account.login_account, account.kind);
    authorize_material_transfer(auth, &actor, &files, executor).await?;
    let materials = files
        .into_iter()
        .map(|file| ApprovalMaterialFile {
            file_asset_id: FileAssetId::new(file.base.id),
            file_name: file.file_name,
            content_type: file.content_type,
            byte_size: file.byte_size,
            asset_version: file.base.version,
            content_hmac: file.content_hmac.as_str().to_string(),
        })
        .collect();
    if snapshot.display.is_none() {
        snapshot.display = Some(
            capture_approval_display(db, snapshot.document_type, &snapshot.business_object_id, executor)
                .await?,
        );
    }
    snapshot.with_material_files(materials).map_err(Error::from)
}

/// 退款与冲正只允许各自保存的直接凭证，不扩展到原资金单其他附件。
async fn evidence(
    db: &Database,
    kind: DocumentType,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<FileAssetId>> {
    let ids = [id.to_string()];
    match kind {
        DocumentType::CustomerRefund => Ok(db
            .customer_refunds()
            .list_active_by_ids(&ids, executor)
            .await?
            .pop()
            .and_then(|row| row.evidence_attachment_id)),
        DocumentType::SupplierRefund => Ok(db
            .supplier_refunds()
            .list_active_by_ids(&ids, executor)
            .await?
            .pop()
            .and_then(|row| row.evidence_attachment_id)),
        DocumentType::ReceiptReversal => Ok(db
            .receipt_reversals()
            .list_active_by_ids(&ids, executor)
            .await?
            .pop()
            .and_then(|row| row.evidence_attachment_id)),
        DocumentType::PaymentReversal => Ok(db
            .payment_reversals()
            .list_active_by_ids(&ids, executor)
            .await?
            .pop()
            .and_then(|row| row.evidence_attachment_id)),
        _ => Ok(None),
    }
}
