//! 建单凭证下载在对象存储读取前后按来源销售单复验。
use std::result::Result;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_read_models::sales_center::order::SalesOrderReadService;

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};

#[permission_macros::permission(
    group = "销售单",
    group_desc = "销售单（W05）管理",
    desc = "下载销售单建单凭证",
    resource = "sales_order",
    action = "detail"
)]
/// 下载来源销售单自身的建单凭证。
///
/// # 参数
/// * `state` / `actor` - 应用状态及当前用户
/// * `order_id` / `asset_id` - 当前销售单及其建单凭证
/// # 返回
/// 返回经源对象及文件治理复验的 PDF 或图片内容。
/// # 错误
/// 来源越权、关系变化、文件销毁或隔离时拒绝。
pub async fn sales_order_evidence_download(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((order_id, asset_id)): Path<(String, String)>,
) -> Result<Response, Error> {
    let service = SalesOrderReadService::with_rbac(state.db(), state.rbac());
    let version = service.require_sales_evidence(&actor, &order_id, &asset_id).await?;
    let (view, bytes) = read_asset(&state, &actor, &asset_id).await?;
    let current = service.require_sales_evidence(&actor, &order_id, &asset_id).await?;
    if version != current {
        return Err(Error::Conflict("销售单或凭证关联已变化，请刷新后重试".into()));
    }
    revalidate_asset(&state, &view).await?;
    asset_download_response(&view, bytes)
}
