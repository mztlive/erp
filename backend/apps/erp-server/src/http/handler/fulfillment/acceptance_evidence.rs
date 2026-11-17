//! 客户签收凭证读取：通过验收单重新校验销售来源范围。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use erp_read_models::fulfillment_center::access::AcceptanceReadService;

use crate::http::errors::Error;
use crate::http::handler::file_asset::{asset_download_response, read_asset, revalidate_asset};
use crate::http::state::HttpState;

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
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> std::result::Result<Response, Error> {
    let service = AcceptanceReadService::new(state.application().db(), state.application().authorization());
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
