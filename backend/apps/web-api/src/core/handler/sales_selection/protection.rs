//! 密码维护、个人提货码与公开访问凭证的 HTTP 适配。

use std::net::SocketAddr;

use application_core::AuditActor;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::{Extension, Json};
use erp_sales::dto::sales_selection::{
    PublicSelectionAccessView, SalesSelectionBookletView, SalesSelectionPasswordRequest, SelectionDetailView,
    SelectionVoucherView, UnlockSelectionRequest,
};
use serde::Deserialize;

use super::{access, process};
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

/// 在管理端维护公开访问密码。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 选品册身份
/// * `req` - 密码和预期版本
///
/// # 返回
/// 返回更新后的册详情，不返回密码或其哈希。
///
/// # 错误
/// 无维护权限、版本冲突或密码格式不合法时拒绝。
#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "维护选品册访问密码",
    resource = "sales_selection_booklet",
    action = "maintain"
)]
pub async fn booklet_password(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SalesSelectionPasswordRequest>,
) -> Result<SalesSelectionBookletView> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).set_access_password(id, req, actor).await?))
}

/// 读取仅供销售发放的个人提货码。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 选品册身份
///
/// # 返回
/// 返回该册提货码及办理状态。
///
/// # 错误
/// 无复制链接权限或无客户访问资格时拒绝。
#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "读取选品册个人提货码",
    resource = "sales_selection_booklet",
    action = "copy_link"
)]
pub async fn booklet_vouchers(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<SelectionVoucherView>> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).vouchers(&id, &actor).await?))
}

/// 读取该册所有已提交人的选品和收件明细。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已认证操作人
/// * `id` - 选品册身份
///
/// # 返回
/// 返回完整方案明细，供详情展示和表格导出。
///
/// # 错误
/// 无册读取权限或无客户访问资格时拒绝。
#[permission_macros::permission(
    group = "销售选品",
    group_desc = "选品册与销售方案",
    desc = "查询选品册个人选品明细",
    resource = "sales_selection_booklet",
    action = "get"
)]
pub async fn booklet_selection_details(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<SelectionDetailView>> {
    access::booklet(&state, &actor, &id).await?;
    Ok(ApiResponse::ok_with_data(process(&state).selection_details(&id, &actor).await?))
}

/// 验证密码和个人提货码，签发限于当前册与个人的访问凭证。
///
/// # 参数
/// * `state` - 应用状态
/// * `token` - 当前选品链接令牌
/// * `addr` - 来源地址
/// * `req` - 密码和可选提货码
///
/// # 返回
/// 返回访问凭证与当前个人公开页。
///
/// # 错误
/// 密码错误、提货码不属于该册、链接失效或超过限流时拒绝。
pub async fn public_unlock(
    State(state): State<AppState>,
    Path(token): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<UnlockSelectionRequest>,
) -> Result<PublicSelectionAccessView> {
    Ok(ApiResponse::ok_with_data(process(&state).public_unlock(token, req, addr.ip().to_string()).await?))
}

/// 图片 URL 的短期授权参数，不派生 Debug 避免记录凭证。
#[derive(Deserialize)]
pub struct ImageAccessQuery {
    /// 由密码验证签发的访问凭证。
    pub access_token: Option<String>,
}

pub(super) fn selection_access(headers: &HeaderMap) -> Option<&str> {
    headers.get("x-selection-access")?.to_str().ok().filter(|value| !value.is_empty() && value.len() <= 4096)
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::{ImageAccessQuery, selection_access};

    #[test]
    fn reads_access_header_case_insensitively_and_rejects_empty_or_oversized_values() {
        let mut headers = HeaderMap::new();
        assert_eq!(selection_access(&headers), None);
        headers.insert("X-Selection-Access", HeaderValue::from_static("current-person-grant"));
        assert_eq!(selection_access(&headers), Some("current-person-grant"));
        headers.insert("X-Selection-Access", HeaderValue::from_static(""));
        assert_eq!(selection_access(&headers), None);
        headers.insert("X-Selection-Access", HeaderValue::from_str(&"a".repeat(4097)).unwrap());
        assert_eq!(selection_access(&headers), None);
    }

    #[test]
    fn image_access_can_be_absent_and_is_read_only_from_explicit_parameter() {
        let anonymous: ImageAccessQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(anonymous.access_token, None);
        let authorized: ImageAccessQuery =
            serde_json::from_str(r#"{"access_token":"personal-grant"}"#).unwrap();
        assert_eq!(authorized.access_token.as_deref(), Some("personal-grant"));
    }
}
