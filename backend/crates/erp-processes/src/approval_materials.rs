//! 审批冻结附件读取：限定实例允许清单，重新校验文件内容，保留敏感读取审计。

use application_core::AuditActor;
use erp_core::ids::FileAssetId;
use erp_identity::SharedRbacService;
use erp_read_models::workbench::authorize_material_transfer;
use erp_support::repository::FileAssetExt;
use erp_support::{FileAssetService, FileAssetView, SecurityScanStatus};
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use erp_workflow::service::approval::execution::ApprovalRuntimeService;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::adapters::workflow::workflow_auth;
use crate::{Error, Result};

/// 建立关联前验证文件来源资格，冻结提交时仍须在事务内重新验证。
/// # 参数
/// 身份数据库、权限服务、当前账号及目标文件 ID。
/// # 返回
/// 文件由本人创建或当前具有文件读取资格时成功。
/// # 错误
/// 文件不存在、账号失效或无文件来源资格时拒绝。
pub async fn authorize_attachment(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    file_id: &str,
) -> Result<()> {
    let file = db
        .file_assets()
        .find_by_id(file_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("文件资产不存在".into()))?;
    let auth = workflow_auth(db.clone(), rbac.clone());
    authorize_material_transfer(&auth, actor, &[file], &mut NoTransaction).await?;
    Ok(())
}

/// 取得已授权、仍匹配提交版本且可供读取的附件元数据。
///
/// # 参数
/// * `runtime` / `files` - 审批授权与文件领域服务。
/// * `actor` - 当前认证账号。
/// * `instance_id` / `file_id` - 精确审批实例及请求文件。
/// # 返回
/// 返回服务器内部文件视图；存储键不得序列化到 HTTP 响应。
/// # 错误
/// 未指派、来源不匹配、文件改变、销毁、隔离或审计失败时拒绝。
pub async fn readable<A: WorkflowAuthorizationPort>(
    runtime: &ApprovalRuntimeService<A>,
    files: &FileAssetService,
    actor: &AuditActor,
    instance_id: &str,
    file_id: &str,
) -> Result<FileAssetView> {
    checked(runtime, files, actor, instance_id, file_id, true).await
}

/// 存储读取后重验当前文件治理状态和实例资格，不重复写入读取审计。
/// # 参数
/// 审批与文件服务、认证账号及同一实例和文件 ID。
/// # 返回
/// 仍允许发送文件内容时成功。
/// # 错误
/// 文件被撤回、版本变化或读者资格失效时拒绝。
pub async fn revalidate<A: WorkflowAuthorizationPort>(
    runtime: &ApprovalRuntimeService<A>,
    files: &FileAssetService,
    actor: &AuditActor,
    instance_id: &str,
    file_id: &str,
) -> Result<()> {
    checked(runtime, files, actor, instance_id, file_id, false).await?;
    Ok(())
}

/// 文件读取前后复用相同冻结内容与当前治理状态检查。
async fn checked<A: WorkflowAuthorizationPort>(
    runtime: &ApprovalRuntimeService<A>,
    files: &FileAssetService,
    actor: &AuditActor,
    instance_id: &str,
    file_id: &str,
    audit: bool,
) -> Result<FileAssetView> {
    let frozen = runtime.material_reference(actor, instance_id, file_id).await?;
    let view = if audit {
        files.file_asset_preview(file_id, actor).await?
    } else {
        files.file_asset_detail(file_id).await?
    };
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
    let latest = runtime.material_reference(actor, instance_id, file_id).await?;
    if latest != frozen {
        return Err(Error::ConflictError("审批资料已变化，请重新读取".into()));
    }
    Ok(view)
}
