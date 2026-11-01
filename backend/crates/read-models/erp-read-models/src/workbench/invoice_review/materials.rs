//! 开票审批唯一来源的当前生效销售资料，不改写历史申请快照。

use application_core::AuditActor;
use erp_finance::repository::ReceivableExt;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalRelatedSalesSnapshot;
use erp_workflow::service::approval::execution::ApprovalRuntimeService;
use erp_workflow::service::approval::execution::runtime_service::{
    ApprovalMaterialView, ApprovalMaterialsView,
};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::{request, source};
use crate::sales_center::approval_source_sales::{ApprovalSourceSalesOrder, sources};
use crate::sales_center::materials::revision_materials;
use crate::workbench::approval_materials::related_sales::material_unavailable;
use crate::{Error, Result};

/// 当前生效版本的完整单据和受限附件目录，明确独立于提交快照。
#[derive(Debug, Serialize)]
pub struct InvoiceSalesBasis {
    pub source: ApprovalRelatedSalesSnapshot,
    pub sales_order: ApprovalSourceSalesOrder,
    pub attachments: Vec<ApprovalMaterialView>,
    pub unavailable_reason: Option<String>,
    #[serde(skip)]
    files: Vec<ApprovalMaterialFile>,
}

/// 请求必须命中页面所见版本的精确文件允许清单。
fn reference(
    source_revision: &str,
    files: &[ApprovalMaterialFile],
    revision_id: &str,
    file_id: &str,
) -> Result<ApprovalMaterialFile> {
    if source_revision != revision_id {
        return Err(Error::ConflictError("销售版本已变更，请刷新后重新打开资料".into()));
    }
    files
        .iter()
        .find(|file| file.file_asset_id.as_ref() == file_id)
        .cloned()
        .ok_or_else(|| Error::NotFound("当前销售版本未关联此附件".into()))
}

/// 在冻结提交资料之外补充当前销售依据，业务不可用不影响原资料。
/// # 参数
/// 数据库与已授权实例资料。
/// # 返回
/// 当前开票来源或单独的不可用提示；其他审批类型均为空。
/// # 错误
/// 数据库等基础设施错误继续传播。
pub(crate) async fn optional_sales_basis(
    db: &Database,
    materials: &ApprovalMaterialsView,
) -> Result<(Option<InvoiceSalesBasis>, Option<String>)> {
    if materials.document_type != "sales_invoice_request" {
        return Ok((None, None));
    }
    match sales_basis(db, materials).await {
        Ok(basis) => Ok((Some(basis), None)),
        Err(error) => {
            material_unavailable(error)?;
            Ok((None, Some("当前销售依据暂不可读取，请刷新后重试；仍不可用时请联系销售核对".into())))
        },
    }
}

/// 读取已授权开票实例的唯一销售来源。
/// # 参数
/// 数据库及经过运行时实例授权的提交资料。
/// # 返回
/// 同一事务中的销售版本、完整明细与合同凭证目录。
/// # 错误
/// 类型、申请版本、应收或销售来源不一致时拒绝。
pub(crate) async fn sales_basis(
    db: &Database,
    materials: &ApprovalMaterialsView,
) -> Result<InvoiceSalesBasis> {
    let db = db.clone();
    let materials = materials.clone();
    db.client()
        .clone()
        .with_transaction(move |executor| Box::pin(async move { load(&db, &materials, executor).await }))
        .await
}

async fn load(
    db: &Database,
    materials: &ApprovalMaterialsView,
    executor: &mut dyn Executor,
) -> Result<InvoiceSalesBasis> {
    if materials.document_type != "sales_invoice_request" {
        return Err(Error::ValidationError("当前审批不是开票申请".into()));
    }
    let request = request(db, &materials.document_id, materials.subject_version, executor).await?;
    if materials.display.root_document_id != request.sales_order_id.as_ref() {
        return Err(Error::ConflictError("本次审批与销售来源不一致".into()));
    }
    let account = db
        .receivable_accounts()
        .find_by_id(request.receivable_account_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("申请绑定的应收不存在".into()))?;
    source::validate_binding(&request, &account)?;
    let (_, revision, source) = source::load(db, &request, executor).await?;
    let sales_order = sources(db, std::slice::from_ref(&source), executor)
        .await?
        .pop()
        .ok_or_else(|| Error::ConflictError("来源销售版本不存在".into()))?;
    let (files, unavailable_reason) = match revision_materials(db, &revision, executor).await {
        Ok(materials) => (materials.files, None),
        Err(error) => {
            material_unavailable(error)?;
            (vec![], Some("销售版本关联的合同或凭证暂不可读取，请联系销售核对归档资料".into()))
        },
    };
    Ok(InvoiceSalesBasis {
        source,
        sales_order,
        attachments: files.iter().map(ApprovalMaterialView::from).collect(),
        unavailable_reason,
        files,
    })
}

/// 以审批资格重新证明当前销售版本及其精确文件允许清单。
/// # 参数
/// 数据库、审批运行服务、读者、实例、页面显示的销售版本与文件 ID。
/// # 返回
/// 仅供服务端比对的文件版本及内容指纹。
/// # 错误
/// 未授权、来源变更、非开票审批或文件不在来源清单时拒绝。
pub async fn invoice_sales_reference<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    actor: &AuditActor,
    instance_id: &str,
    revision_id: &str,
    file_id: &str,
) -> Result<ApprovalMaterialFile> {
    let materials = runtime.materials(actor, instance_id).await?;
    let basis = sales_basis(db, &materials).await?;
    let file = reference(&basis.source.revision_id, &basis.files, revision_id, file_id)?;
    runtime.materials(actor, instance_id).await?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use erp_core::ids::FileAssetId;

    use super::*;

    fn file() -> ApprovalMaterialFile {
        ApprovalMaterialFile {
            file_asset_id: FileAssetId::new("contract-pdf"),
            file_name: "合同.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 12,
            asset_version: 2,
            content_hmac: "a".repeat(64),
        }
    }

    #[test]
    fn current_source_file_keeps_exact_content_identity() {
        let file = file();
        let selected =
            reference("revision-1", std::slice::from_ref(&file), "revision-1", "contract-pdf").unwrap();
        assert_eq!(selected, file);
        let visible = serde_json::to_value(ApprovalMaterialView::from(&selected)).unwrap();
        assert_eq!(visible["file_name"], "合同.pdf");
        assert!(visible.get("content_hmac").is_none());
        assert!(visible.get("asset_version").is_none());
    }

    #[test]
    fn stale_versions_foreign_files_and_empty_evidence_are_rejected() {
        let files = vec![file()];
        assert!(matches!(
            reference("revision-2", &files, "revision-1", "contract-pdf"),
            Err(Error::ConflictError(_))
        ));
        assert!(matches!(
            reference("revision-1", &files, "revision-1", "another-file"),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            reference("revision-1", &[], "revision-1", "contract-pdf"),
            Err(Error::NotFound(_))
        ));
        assert!(reference("revision-1", &files, "", "contract-pdf").is_err());
    }
}
