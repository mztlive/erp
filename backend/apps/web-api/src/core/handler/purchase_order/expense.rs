//! 费用采购单创建与费用行确认发生的 HTTP 协议适配（§7.5.1）。

use application_core::AuditActor;
use axum::extract::{Multipart, Path, State};
use axum::{Extension, Json};
use erp_processes::procure_to_pay::PurchaseOrderProcess;
use erp_procurement::dto::purchase_order::{
    ConfirmExpenseOccurrenceRequest, ConfirmExpenseOccurrenceResult, CreateExpensePurchaseRequest,
    CreatePurchaseOrderResult,
};
use erp_support::SensitivityClass;

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{
    PendingAssetFile, delete_pending_asset_objects, extract_command_with_asset_files,
    should_compensate_pending_assets, store_pending_asset_files,
};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "从销售单追加费用采购单",
    resource = "purchase_order",
    action = "create"
)]
/// 从已生效销售单追加费用采购单并提交审批。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 来源销售单
/// * `req` - 收款供应商、付款条件、费用行与幂等键
///
/// # 返回
/// 返回已提交审批的采购单结果。
///
/// # 错误
/// 业务校验、审批绑定或写入失败时返回错误。
pub async fn expense_purchase_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CreateExpensePurchaseRequest>,
) -> Result<CreatePurchaseOrderResult> {
    let view = PurchaseOrderProcess::with_rbac(state.db(), state.rbac())
        .with_object_read(state.approval_object_read())
        .create_expense_purchase(&id, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "确认采购费用行已发生",
    resource = "purchase_order",
    action = "confirm_expense"
)]
/// 确认采购单上的一条费用行已发生，形成实际发生成本。
///
/// 命令与凭证一次 multipart 提交：`command` 为 JSON，凭证字段名须与
/// `evidence_attachment_id` 的 `pending-file:` 临时引用一致。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `path` - 采购单与稳定费用行身份
/// * `multipart` - 确认命令与凭证文件
///
/// # 返回
/// 返回费用行身份与实际成本事实主键。
///
/// # 错误
/// 凭证缺失或类型不符、业务校验或写入失败时返回错误。
pub async fn expense_occurrence_confirm(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, expense_line_id)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<ConfirmExpenseOccurrenceResult> {
    let (req, files) =
        extract_command_with_asset_files::<ConfirmExpenseOccurrenceRequest>(&mut multipart).await?;
    validate_expense_evidence(req.evidence_attachment_id.as_ref(), &files)?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let result = PurchaseOrderProcess::with_rbac(state.db(), state.rbac())
        .confirm_expense_occurrence(&id, &expense_line_id, req, pending.clone(), &actor)
        .await;
    match result {
        Ok(view) => Ok(ApiResponse::ok_with_data(view)),
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(error.into())
        },
    }
}

/// 校验费用凭证：恰好一个与命令引用一致的图片或 PDF。
fn validate_expense_evidence(reference: &str, files: &[PendingAssetFile]) -> std::result::Result<(), Error> {
    let [file] = files else {
        return Err(Error::BadRequest("请上传一个费用发生凭证文件".to_string()));
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

    #[test]
    fn expense_evidence_requires_one_matching_image_or_pdf() {
        let file = |reference: &str, content_type: &str| PendingAssetFile {
            reference: reference.into(),
            file: AssetFile { file_name: "e".into(), content_type: content_type.into(), content: vec![1] },
        };
        let reference = "pending-file:expense";
        assert!(validate_expense_evidence(reference, &[file(reference, "image/jpeg")]).is_ok());
        assert!(validate_expense_evidence(reference, &[]).is_err());
        assert!(validate_expense_evidence(reference, &[file(reference, "application/zip")]).is_err());
        assert!(validate_expense_evidence(reference, &[file("pending-file:x", "image/png")]).is_err());
    }
}
