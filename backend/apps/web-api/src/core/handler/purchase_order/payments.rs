//! 采购详情内的付款记录与回单访问；不授予供应商付款台账权限。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_read_models::finance::document_files::matching_file;
use erp_read_models::purchase_center::PurchasePaymentView;

use super::purchase_reads;
use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查询采购单对象中心",
    resource = "purchase_order",
    action = "detail"
)]
/// 按采购详情资格读取本单付款记录。
/// # 参数
/// 当前账号与采购单身份。
/// # 返回
/// 当前采购单的付款核销金额及回单。
/// # 错误
/// 采购单不可见或读取失败时拒绝。
pub async fn purchase_payments(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<PurchasePaymentView>> {
    Ok(ApiResponse::ok_with_data(purchase_reads(&state).payment_records(&actor, &id).await?))
}

#[permission_macros::permission(
    group = "采购单",
    group_desc = "采购单、采购提交与采购变更管理",
    desc = "查询采购单对象中心",
    resource = "purchase_order",
    action = "detail"
)]
/// 读取回单前后重验采购资格、有效付款分配及精确文件身份。
/// # 参数
/// 当前账号、采购单与付款单身份。
/// # 返回
/// 经审计的私有文件字节，不提供通用资产访问资格。
/// # 错误
/// 跨单、分配被冲销、资格失效或资产被隔离时拒绝。
pub async fn purchase_payment_receipt(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((id, payment_id)): Path<(String, String)>,
) -> std::result::Result<Response, Error> {
    let service = purchase_reads(&state);
    let files = service
        .payment_records(&actor, &id)
        .await?
        .into_iter()
        .filter_map(|row| row.receipt)
        .collect::<Vec<_>>();
    let file = matching_file(&files, &payment_id, None)?;
    let (view, bytes) = read_asset(&state, &actor, &file.file_asset_id).await?;
    let current = service
        .payment_records(&actor, &id)
        .await?
        .into_iter()
        .filter_map(|row| row.receipt)
        .collect::<Vec<_>>();
    matching_file(&current, &payment_id, Some(&view.id))?;
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}
