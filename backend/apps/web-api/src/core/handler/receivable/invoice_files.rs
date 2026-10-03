//! 发票 multipart 边界复用通用上传校验，资产登记加入原发票事务。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, State};
use erp_finance::dto::receivable::{CommitInvoiceRequest, CreateInvoiceRequest, InvoiceView};
use erp_processes::attachments::PendingFileAssets;
use erp_processes::finance_posting::receivable::ReceivableProcess;
use erp_support::SensitivityClass;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::handler::file_asset::{
    delete_pending_asset_objects, extract_command_with_asset_files, should_compensate_pending_assets,
    store_pending_asset_files,
};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "登记发票草稿",
    resource = "invoice",
    action = "create"
)]
/// 登记发票草稿同时上传图片或 PDF。
/// # 参数
/// `command` JSON 与按临时引用命名的文件。
/// # 返回
/// 原发票草稿响应。
/// # 错误
/// 文件、业务字段或事务失败时拒绝并按确定回滚补偿存储对象。
pub async fn invoice_create_with_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<InvoiceView> {
    let (req, files) = extract_command_with_asset_files::<CreateInvoiceRequest>(&mut multipart).await?;
    let uploads = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let pending = match PendingFileAssets::prepare(uploads.clone(), &actor) {
        Ok(pending) => pending.shared(),
        Err(error) => {
            delete_pending_asset_objects(&state, &uploads).await;
            return Err(error.into());
        },
    };
    let outcome = ReceivableProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .create_invoice_with_assets(req, pending, &actor)
        .await;
    match outcome {
        Ok(view) => Ok(ApiResponse::ok_with_data(view)),
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &uploads).await;
            }
            Err(error.into())
        },
    }
}

#[permission_macros::permission(
    group = "客户往来",
    group_desc = "应收台账、回款与销项发票管理",
    desc = "原子登记销项发票并分配",
    resource = "invoice",
    action = "post"
)]
/// 原子开票时在同一事务登记文件和关联；重放删除本次未消费上传。
/// # 参数
/// `command` JSON、临时引用文件与认证账号。
/// # 返回
/// 原销项发票响应。
/// # 错误
/// 文件或事务失败时拒绝；提交结果未知时保留存储对象。
pub async fn invoice_commit_with_files(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    mut multipart: Multipart,
) -> Result<InvoiceView> {
    let (req, files) = extract_command_with_asset_files::<CommitInvoiceRequest>(&mut multipart).await?;
    let uploads = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let pending = match PendingFileAssets::prepare(uploads.clone(), &actor) {
        Ok(pending) => pending.shared(),
        Err(error) => {
            delete_pending_asset_objects(&state, &uploads).await;
            return Err(error.into());
        },
    };
    let outcome = ReceivableProcess::new(state.db())
        .with_object_read(state.approval_object_read())
        .commit_invoice_with_assets(req, pending, &actor)
        .await;
    match outcome {
        Ok(result) => {
            if !result.assets_committed {
                delete_pending_asset_objects(&state, &uploads).await;
            }
            Ok(ApiResponse::ok_with_data(result.view))
        },
        Err(error) => {
            if should_compensate_pending_assets(&error) {
                delete_pending_asset_objects(&state, &uploads).await;
            }
            Err(error.into())
        },
    }
}
