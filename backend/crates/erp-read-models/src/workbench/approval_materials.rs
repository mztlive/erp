//! 提交事务冻结本单据直接关联的审批材料，不追溯客户或供应商其他附件。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::ids::{BusinessDocumentId, FileAssetId, SalesChangeOrderId, SalesOrderId};
use erp_returns::repository::ReturnsExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use erp_support::repository::FileAssetExt;
use erp_support::repository::prelude::*;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use erp_workflow::entity::approval_integration::display_snapshot::{
    ApprovalBriefSection, ApprovalMaterialFile,
};
use erp_workflow::{DocumentRegistryExt, DocumentType, WorkflowAuthorizationPort};
use mongodb::Database;
use persistence_core::Executor;

use super::{authorize_material_transfer, capture_approval_display};
use crate::sales_center::materials::{
    change_evidence, change_submission_contract, material_from_asset, require_contract_pdf,
    submission_contract, validate_materials,
};
use crate::{Error, Result};

mod related_sales;

/// 在原提交事务内冻结显示与附件版本，历史快照不补取当前材料。
///
/// # 参数
/// * `db` / `executor` - 原业务提交事务。
/// * `auth` - 在同一事务中重验提交账号和文件预览资格的权限端口。
/// * `snapshot` - 已匹配本次提交的强类型快照。
/// # 返回
/// 返回本单直接附件、锁定销售合同与采购来源销售材料的不可变快照。
/// # 错误
/// 单据类型不匹配、附件资产缺失或材料超限时中止提交。
pub async fn freeze_approval_materials(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    mut snapshot: ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<ApprovalSubjectSnapshot> {
    let (mut materials, actor) = direct_materials(db, auth, &snapshot, executor).await?;
    if let Some(contract) = sales_contract_material(db, auth, &actor, &snapshot, executor).await? {
        materials.push(contract);
    }
    if snapshot.display.is_none() {
        snapshot.display = Some(
            capture_approval_display(db, snapshot.document_type, &snapshot.business_object_id, executor)
                .await?,
        );
    }
    if snapshot.document_type == DocumentType::PurchaseOrder {
        let (source_sales, files) = related_sales::capture(db, &snapshot, executor).await?;
        if let Some(display) = &mut snapshot.display {
            display.source_sales = source_sales;
            display.validate()?;
        }
        materials.extend(files);
    }
    if snapshot.document_type == DocumentType::SalesChangeOrder {
        materials.extend(sales_change_evidence(db, &mut snapshot, executor).await?);
    }
    snapshot.with_material_files(unique_materials(materials)).map_err(Error::from)
}

/// 相同冻结文件跨合同和来源版本去重；不同文件版本继续交给领域快照拒绝。
fn unique_materials(mut materials: Vec<ApprovalMaterialFile>) -> Vec<ApprovalMaterialFile> {
    materials.sort_by(|left, right| left.file_asset_id.as_ref().cmp(right.file_asset_id.as_ref()));
    materials.dedup();
    materials
}

/// 本单直接附件保持原上传者或通用文件预览资格，不以关联记录替代转授授权。
async fn direct_materials(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<(Vec<ApprovalMaterialFile>, AuditActor)> {
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
    Ok((files.iter().map(material_from_asset).collect(), actor))
}

/// 销售提交只转授本次锁定且提交者当前可读的唯一合同正文，不遍历合同其他附件。
async fn sales_contract_material(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalMaterialFile>> {
    let contract = match snapshot.document_type {
        DocumentType::SalesOrder | DocumentType::VoucherSalesOrder => {
            let submission = db
                .sales_order_submissions()
                .find_by_order_and_no(
                    &SalesOrderId::new(&snapshot.business_object_id),
                    snapshot.subject_version,
                    executor,
                )
                .await?
                .ok_or_else(|| Error::ConflictError("销售审批精确提交不存在".into()))?;
            submission_contract(db, &submission, executor).await?
        },
        DocumentType::SalesChangeOrder => {
            let submission = db
                .sales_change_submissions()
                .find_by_order_and_no(
                    &SalesChangeOrderId::new(&snapshot.business_object_id),
                    snapshot.subject_version,
                    executor,
                )
                .await?
                .ok_or_else(|| Error::ConflictError("销售变更审批精确提交不存在".into()))?;
            change_submission_contract(db, &submission, executor).await?
        },
        _ => None,
    };
    let Some(contract) = contract else { return Ok(None) };
    if !auth.approval_contract_readable(actor, contract.contract_id.as_ref(), executor).await? {
        return Err(Error::Forbidden("无权将锁定合同作为销售审批材料".into()));
    }
    let file = db
        .file_assets()
        .find_by_id(contract.contract_pdf_file_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售审批锁定合同 PDF 不存在".into()))?;
    require_contract_pdf(&file)?;
    let material = material_from_asset(&file);
    validate_materials(db, std::slice::from_ref(&material), executor).await?;
    Ok(Some(material))
}

/// 销售变更审批继承基准版本的冻结凭证，历史材料关闭时保留清晰限制说明。
async fn sales_change_evidence(
    db: &Database,
    snapshot: &mut ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<Vec<ApprovalMaterialFile>> {
    let submission = db
        .sales_change_submissions()
        .find_by_order_and_no(
            &SalesChangeOrderId::new(&snapshot.business_object_id),
            snapshot.subject_version,
            executor,
        )
        .await?
        .ok_or_else(|| Error::ConflictError("销售变更审批精确提交不存在".into()))?;
    match change_evidence(db, &submission, executor).await {
        Ok(files) => Ok(files),
        Err(error) => {
            related_sales::material_unavailable(error)?;
            if let Some(display) = &mut snapshot.display {
                display.source.extra_sections.push(ApprovalBriefSection::new(
                    "原销售凭证".into(),
                    "暂不可读取，请联系销售核对资料后再审批".into(),
                ));
                display.validate()?;
            }
            Ok(Vec::new())
        },
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str) -> ApprovalMaterialFile {
        ApprovalMaterialFile {
            file_asset_id: FileAssetId::new(id),
            file_name: "合同.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 1,
            asset_version: 1,
            content_hmac: "a".repeat(64),
        }
    }

    #[test]
    fn duplicate_contract_reference_does_not_consume_an_extra_material_slot() {
        let mut files = (0..100).map(|index| file(&format!("file-{index}"))).collect::<Vec<_>>();
        files.push(file("file-0"));
        assert_eq!(unique_materials(files).len(), 100);
        let mut changed = file("file-0");
        changed.asset_version = 2;
        let variants = unique_materials(vec![file("file-0"), changed]);
        assert_eq!(variants.len(), 2);
    }
}
