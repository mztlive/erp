//! 销售单附加费用的 HTTP 协议适配（§7.5.1）。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, State};
use erp_finance::dto::extra_cost::{
    ExtraCostCommandResult, ReduceExtraCostRequest, RegisterExtraCostRequest, SalesExtraCostListView,
};
use erp_processes::finance_posting::extra_cost::SalesExtraCostProcess;
use erp_read_models::finance::extra_cost::SalesExtraCostReadModel;
use erp_support::SensitivityClass;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{
    PendingAssetFile, delete_pending_asset_objects, extract_command_with_asset_files,
    should_compensate_pending_assets, store_pending_asset_files,
};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "查询销售单附加费用",
    resource = "cost_entry",
    action = "list"
)]
/// 查询销售单上的附加费用。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单主键
///
/// # 返回
/// 返回附加费用列表及合计。
///
/// # 错误
/// 销售单不可见或读取失败时返回错误。
pub async fn sales_extra_cost_list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<SalesExtraCostListView> {
    let view = SalesExtraCostReadModel::new(state.db(), state.rbac()).list(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "登记销售单附加费用",
    resource = "cost_entry",
    action = "create"
)]
/// 按凭证登记销售单附加费用。
///
/// 命令与凭证一次 multipart 提交：`command` 为 JSON，凭证字段名须与
/// `evidence_attachment_id` 的 `pending-file:` 临时引用一致。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 销售单主键
/// * `multipart` - 登记命令与凭证文件
///
/// # 返回
/// 返回成本事实主键与是否重放。
///
/// # 错误
/// 凭证缺失或类型不符、业务校验或写入失败时返回错误。
pub async fn sales_extra_cost_register(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<ExtraCostCommandResult> {
    let (req, files) = extract_command_with_asset_files::<RegisterExtraCostRequest>(&mut multipart).await?;
    validate_cost_evidence(req.evidence_attachment_id.as_ref(), &files)?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let process = SalesExtraCostProcess::new(state.db(), state.rbac());
    let result = process.register(&id, req, pending.clone(), &actor).await;
    finish(&state, &pending, result).await
}

#[permission_macros::permission(
    group = "实际经营盈亏",
    group_desc = "成本事实与成本分配管理（W16）",
    desc = "冲减销售单附加费用",
    resource = "cost_entry",
    action = "create"
)]
/// 冲减一笔财务登记的附加费用。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `path` - 销售单主键与原附加费用成本事实
/// * `multipart` - 冲减命令与凭证文件
///
/// # 返回
/// 返回冲减成本事实主键与是否重放。
///
/// # 错误
/// 凭证缺失或类型不符、冲减超额、业务校验或写入失败时返回错误。
pub async fn sales_extra_cost_reduce(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, cost_entry_id)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<ExtraCostCommandResult> {
    let (req, files) = extract_command_with_asset_files::<ReduceExtraCostRequest>(&mut multipart).await?;
    validate_cost_evidence(req.evidence_attachment_id.as_ref(), &files)?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let process = SalesExtraCostProcess::new(state.db(), state.rbac());
    let result = process.reduce(&id, &cost_entry_id, req, pending.clone(), &actor).await;
    finish(&state, &pending, result).await
}

/// 业务失败或重放时删除本次已写入对象存储、未登记的凭证。
async fn finish(
    state: &AppState,
    pending: &[erp_support::PendingFileAssetRequest],
    result: erp_processes::Result<ExtraCostCommandResult>,
) -> Result<ExtraCostCommandResult> {
    match result {
        Ok(view) => {
            if view.replayed {
                // 重放命中原事实，本次上传的凭证未登记，删除对象避免遗留。
                delete_pending_asset_objects(state, pending).await;
            }
            Ok(ApiResponse::ok_with_data(view))
        },
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(state, pending).await;
            }
            Err(error.into())
        },
    }
}

/// 校验费用凭证：恰好一个与命令引用一致的图片或 PDF。
///
/// # 参数
/// * `reference` - 命令中的凭证引用
/// * `files` - 本次解析出的具名文件
///
/// # 返回
/// 引用与上传文件一一对应且类型允许时成功。
///
/// # 错误
/// 文件数量、临时引用或类型不符时返回 400。
fn validate_cost_evidence(reference: &str, files: &[PendingAssetFile]) -> std::result::Result<(), Error> {
    let [file] = files else {
        return Err(Error::BadRequest("请上传一个费用凭证文件".to_string()));
    };
    if !reference.starts_with("pending-file:") || file.reference != reference {
        return Err(Error::BadRequest("费用凭证临时引用无效".to_string()));
    }
    if !matches!(
        file.file.content_type.as_str(),
        "image/jpeg" | "image/png" | "image/webp" | "application/pdf"
    ) {
        return Err(Error::BadRequest("费用凭证仅支持 JPG、PNG、WebP 图片或 PDF".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::handler::file_asset::AssetFile;

    fn file(reference: &str, content_type: &str) -> PendingAssetFile {
        PendingAssetFile {
            reference: reference.into(),
            file: AssetFile {
                file_name: "evidence".into(),
                content_type: content_type.into(),
                content: vec![1],
            },
        }
    }

    #[test]
    fn cost_evidence_requires_one_matching_image_or_pdf() {
        let reference = "pending-file:evidence";
        assert!(validate_cost_evidence(reference, &[file(reference, "application/pdf")]).is_ok());
        assert!(validate_cost_evidence(reference, &[file(reference, "image/png")]).is_ok());
        assert!(validate_cost_evidence(reference, &[]).is_err());
        assert!(validate_cost_evidence(reference, &[file("pending-file:other", "image/png")]).is_err());
        assert!(validate_cost_evidence(reference, &[file(reference, "text/plain")]).is_err());
        assert!(validate_cost_evidence("file-1", &[file("file-1", "image/png")]).is_err());
        let two = [file(reference, "image/png"), file("pending-file:b", "image/png")];
        assert!(validate_cost_evidence(reference, &two).is_err());
    }
}
