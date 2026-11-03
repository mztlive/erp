//! 线下服务履约 HTTP 协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Multipart, Path, State};
use erp_fulfillment::dto::{
    ConfirmServiceFulfillmentRequest, CreateServiceFulfillmentRequest, PageView,
    ServiceFulfillmentListParams, ServiceFulfillmentView,
};
use erp_fulfillment_execution::confirm_service_fulfillment_with_assets;
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

/// 查询线下服务履约记录列表（W01 履约任务聚合视图）。
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
pub async fn service_fulfillment_list(
    State(state): State<HttpState>,
    Query(params): Query<ServiceFulfillmentListParams>,
) -> Result<PageView<ServiceFulfillmentView>> {
    let page = service(&state).service_fulfillment_list(&params).await?;

    Ok(ApiResponse::ok_with_data(page))
}

/// 按主键查询线下服务履约记录。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 服务履约记录主键
///
/// # 返回
/// 返回服务履约记录视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn service_fulfillment_detail(
    State(state): State<HttpState>,
    Path(id): Path<String>,
) -> Result<ServiceFulfillmentView> {
    let view = service(&state).service_fulfillment_detail(&id).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 创建线下服务履约记录（草稿；服务地点与交付对象快照由边界传入）。
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
pub async fn service_fulfillment_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateServiceFulfillmentRequest>,
) -> Result<ServiceFulfillmentView> {
    let view = process(&state).create_service_fulfillment(req, &actor).await?;

    Ok(ApiResponse::ok_with_data(view))
}

/// 确认服务履约（草稿 → 已确认；门槛、现场事实与图片凭证同事务）。
///
/// 命令与现场图片一次 multipart 提交：`command` 为 JSON，图片字段名必须与
/// `evidence_attachment_id` 的 `pending-file:` 临时引用一致。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 记录主键
/// * `multipart` - 确认命令与现场图片
///
/// # 返回
/// 返回确认后的记录视图。
///
/// # 错误
/// 业务处理或数据读取失败时返回对应错误。
pub async fn service_fulfillment_confirm(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<ServiceFulfillmentView> {
    let (req, files) =
        extract_command_with_asset_files::<ConfirmServiceFulfillmentRequest>(&mut multipart).await?;
    validate_evidence_upload(req.evidence_attachment_id.as_ref(), &files)?;
    let pending = store_pending_asset_files(&state, files, |_| SensitivityClass::Sensitive).await?;
    let result =
        confirm_service_fulfillment_with_assets(process(&state), id, req, pending.clone(), actor).await;
    match result {
        Ok(view) => Ok(ApiResponse::ok_with_data(view)),
        Err(service_error) => {
            if should_compensate_pending_assets(&service_error) {
                delete_pending_asset_objects(&state, &pending).await;
            }
            Err(service_error.into())
        },
    }
}
