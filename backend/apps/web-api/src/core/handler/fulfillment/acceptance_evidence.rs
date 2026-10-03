//! 客户签收凭证读取：通过验收单重新校验销售来源范围。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_read_models::fulfillment_center::access::AcceptanceReadService;

use crate::app_state::AppState;
use crate::core::errors::Error;
use crate::core::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};

#[permission_macros::permission(
    group = "履约",
    group_desc = "采购入库、发货、交付、服务与客户验收管理",
    desc = "查询客户验收单详情",
    resource = "customer_acceptance",
    action = "detail"
)]
/// 下载销售范围内的签收单凭证。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证的当前账号
/// * `id` - 签收单身份
/// # 返回
/// 返回图片或 PDF 文件内容。
/// # 错误
/// 无来源读取权限、缺少历史凭证、文件销毁或读取失败时拒绝。
pub async fn customer_acceptance_evidence(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let service = AcceptanceReadService::new(state.db(), state.rbac());
    let detail = service.detail(&id, &actor).await?;
    let file_id = detail
        .acceptance
        .evidence_attachment_id
        .ok_or_else(|| Error::NotFound("这份签收记录尚未上传凭证".into()))?;
    let (asset, bytes) = read_asset(&state, &actor, &file_id).await?;
    let current = service.detail(&id, &actor).await?;
    if current.acceptance.evidence_attachment_id.as_deref() != Some(file_id.as_str()) {
        return Err(Error::Conflict("签收单凭证已变化，请刷新后重新下载".into()));
    }
    revalidate_asset(&state, &asset).await?;
    asset_download_response(&asset, bytes)
}
