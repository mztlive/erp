//! 审批冻结附件读取：限定实例允许清单，重新校验文件内容，保留敏感读取审计。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::ids::FileAssetId;
use erp_identity::AuthorizationService;
use erp_read_models::workbench::{authorize_material_transfer, invoice_sales_reference};
use erp_support::repository::FileAssetExt;
use erp_support::{FileAssetService, FileAssetView, SecurityScanStatus};
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use erp_workflow::service::approval::execution::ApprovalRuntimeService;
use mongodb::Database;
use persistence_core::{ReadExecutor, Transactional};

use crate::{Error, Result};

/// 精确审批文件目标；销售版本只用于独立的当前来源资料入口。
#[derive(Clone, Copy)]
pub struct ApprovalMaterialTarget<'a> {
    pub copy_receipt_id: Option<&'a str>,
    pub instance_id: &'a str,
    pub file_id: &'a str,
    pub source_revision_id: Option<&'a str>,
}

impl ApprovalMaterialTarget<'_> {
    async fn reference<A: WorkflowAuthorizationPort>(
        self,
        db: &Database,
        runtime: &ApprovalRuntimeService<A>,
        actor: &AuditActor,
    ) -> Result<ApprovalMaterialFile> {
        if let Some(id) = self.copy_receipt_id {
            if self.source_revision_id.is_some() {
                return Err(Error::Forbidden("抄送人员只能查看本次提交所附的文件，请向提交人核对".into()));
            }
            return Ok(runtime.copy_material_reference(actor, id, self.file_id).await?);
        }

        match self.source_revision_id {
            Some(revision) => {
                Ok(invoice_sales_reference(db, runtime, actor, self.instance_id, revision, self.file_id)
                    .await?)
            },
            None => Ok(runtime.material_reference(actor, self.instance_id, self.file_id).await?),
        }
    }
}

/// 首次读取记录敏感审计；对象存储返回后的重验只重新读取元数据。
enum ReadPhase {
    Initial,
    Revalidate,
}

/// 关联已有文件前，核验当前账号是有权转授该资产的实际上传人。
/// 文件不存在、账号失效或转授不允许时拒绝；冻结材料时仍须在提交事务内重验。
pub async fn authorize_attachment(
    identity_service: &Arc<AuthorizationService>,
    db: &Database,
    actor: &AuditActor,
    file_id: &str,
) -> Result<()> {
    let identity_service = identity_service.clone();
    let db = db.clone();
    let actor = actor.clone();
    let file_id = file_id.to_owned();

    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                let file = db
                    .file_assets()
                    .find_by_id(&file_id, executor)
                    .await?
                    .ok_or_else(|| Error::NotFound("审批附件不存在，请刷新后重新打开".into()))?;
                authorize_material_transfer(&identity_service, &db, &actor, &[file], executor).await?;
                Ok(())
            })
        })
        .await
}

/// 按当前账号和精确审批目标核验访问资格，在读取元数据时记录敏感审计。
/// 只有当前文件仍与冻结资料一致且可读取，才返回供对象存储使用的元数据。
/// 文件变化、销毁、隔离、权限或审计失败时拒绝；返回的存储键不得写入 HTTP 响应。
pub async fn readable<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    files: &FileAssetService,
    actor: &AuditActor,
    target: ApprovalMaterialTarget<'_>,
) -> Result<FileAssetView> {
    checked(db, runtime, files, actor, target, ReadPhase::Initial).await
}

/// 对象存储读取成功后、交付字节前，重验审批资格和文件治理状态。文件版本、内容摘要或存储键变化时拒绝，不重复记录首次读取审计。
pub async fn revalidate<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    files: &FileAssetService,
    actor: &AuditActor,
    target: ApprovalMaterialTarget<'_>,
    expected: &FileAssetView,
) -> Result<()> {
    let latest = checked(db, runtime, files, actor, target, ReadPhase::Revalidate).await?;
    if expected.version != latest.version
        || expected.content_hmac != latest.content_hmac
        || expected.storage_object_key != latest.storage_object_key
    {
        return Err(Error::ConflictError("审批附件在读取期间已变化，请重新打开".into()));
    }
    Ok(())
}

// 读取前后共用审批目标、冻结指纹和当前治理状态校验；只有首次读取写审计，避免交付前重验重复写审计。
async fn checked<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    files: &FileAssetService,
    actor: &AuditActor,
    target: ApprovalMaterialTarget<'_>,
    phase: ReadPhase,
) -> Result<FileAssetView> {
    let frozen = target.reference(db, runtime, actor).await?;
    let view = match phase {
        ReadPhase::Initial => files.preview_asset(target.file_id, actor, &mut ReadExecutor::default()).await,
        ReadPhase::Revalidate => files.asset_view(target.file_id, &mut ReadExecutor::default()).await,
    }?;

    let current = ApprovalMaterialFile {
        file_asset_id: FileAssetId::new(&view.id),
        file_name: view.file_name.clone(),
        content_type: view.content_type.clone(),
        byte_size: view.byte_size,
        asset_version: view.version,
        content_hmac: view.content_hmac.clone(),
    };
    if !frozen.matches_current(&current) {
        return Err(Error::ConflictError("审批附件已变化，请重新核对提交资料".into()));
    }

    if view.destroyed_at.is_some()
        || matches!(view.security_scan_status, SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined)
    {
        return Err(Error::Forbidden("审批附件当前不可读取".into()));
    }

    // 读取文件和写审计后重验任务资格，撤权不能借第一次检查继续读取。
    let latest = target.reference(db, runtime, actor).await?;
    if latest != frozen {
        return Err(Error::ConflictError("审批资料已变化，请重新读取".into()));
    }
    Ok(view)
}
