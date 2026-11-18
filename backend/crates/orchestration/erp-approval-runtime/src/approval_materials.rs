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
                return Err(Error::Forbidden("抄送仅允许本次提交附件".into()));
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

/// 建立关联前验证文件来源资格，冻结提交时仍须在事务内重新验证。
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
///
/// 身份数据库、权限服务、当前账号及目标文件 ID。
/// # 返回
/// 文件由本人创建或当前具有文件读取资格时成功。
/// # 错误
/// 文件不存在、账号失效或无文件来源资格时拒绝。
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
                    .ok_or_else(|| Error::NotFound("文件资产不存在".into()))?;
                authorize_material_transfer(&identity_service, &db, &actor, &[file], executor).await?;
                Ok(())
            })
        })
        .await
}

/// 取得已授权、仍匹配提交版本且可供读取的附件元数据。
///
/// # 参数
/// * `runtime` / `files` - 审批授权与文件领域服务。
/// * `actor` - 当前认证账号。
/// * `db` / `target` - 业务来源数据库及精确实例、文件和可选销售版本。
/// # 返回
/// 返回服务器内部文件视图；存储键不得序列化到 HTTP 响应。
/// # 错误
/// 未指派、来源不匹配、文件改变、销毁、隔离或审计失败时拒绝。
pub async fn readable<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    files: &FileAssetService,
    actor: &AuditActor,
    target: ApprovalMaterialTarget<'_>,
) -> Result<FileAssetView> {
    checked(db, runtime, files, actor, target, ReadPhase::Initial).await
}

/// 存储读取后重验当前文件治理状态和实例资格，不重复写入读取审计。
/// # 参数
/// 数据库、审批与文件服务、认证账号、同一文件目标及读取前的文件版本。
/// # 返回
/// 仍允许发送文件内容时成功。
/// # 错误
/// 文件被撤回、版本变化或读者资格失效时拒绝。
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

/// 文件读取前后复用相同冻结内容与当前治理状态检查。
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
