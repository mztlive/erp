//! 电子交付 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, State};
use erp_fulfillment::dto::{
    ConfirmElectronicDeliveryRequest, CreateElectronicDeliveryRequest, ElectronicDeliveryListParams,
    ElectronicDeliveryView, PageView,
};
use erp_fulfillment_execution::confirm_electronic_delivery_with_assets;
use erp_support::SensitivityClass;

use super::evidence_upload::validate_evidence_upload;
use super::{process, service};
use crate::http::errors::Result;
use crate::http::extractor::{Json, Query};
use crate::http::handler::file_asset::{
    delete_pending_asset_objects, extract_command_with_asset_files, should_compensate_pending_assets,
    store_pending_asset_files,
};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询电子交付记录列表（W01 履约任务聚合视图）。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`sales_order_line_id`/`status` 扁平传递）
///
/// # 返回
/// 返回契约形状的分页视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn electronic_delivery_list(
    State(state): State<HttpState>,
    Query(params): Query<ElectronicDeliveryListParams>,
) -> Result<PageView<ElectronicDeliveryView>> {
    let page = service(&state).electronic_delivery_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 按主键查询电子交付记录。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 电子交付记录主键
///
/// # 返回
/// 返回电子交付记录视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn electronic_delivery_detail(
    State(state): State<HttpState>,
    Path(id): Path<String>,
) -> Result<ElectronicDeliveryView> {
    let view = service(&state).electronic_delivery_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 创建电子交付记录（草稿；交付对象快照由边界传入，服务端计算查询指纹）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建记录的响应视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn electronic_delivery_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateElectronicDeliveryRequest>,
) -> Result<ElectronicDeliveryView> {
    let view = process(&state).create_electronic_delivery(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 确认电子交付（草稿 → 已确认；门槛与分配有效性校验同事务）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 记录主键
///
/// # 返回
/// 返回确认后的记录视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn electronic_delivery_confirm(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<ElectronicDeliveryView> {
    let (req, files) =
        extract_command_with_asset_files::<ConfirmElectronicDeliveryRequest>(&mut multipart).await?;
    validate_evidence_upload(req.evidence_attachment_id.as_ref(), &files)?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let result =
        confirm_electronic_delivery_with_assets(process(&state), id, req, pending.clone(), actor).await;
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
