use std::result::Result as HttpResult;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use axum::http::HeaderValue;
use axum::http::header::CACHE_CONTROL;
use axum::response::{IntoResponse, Response};
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::service::organization::leader::own_leader_preview;
use erp_identity::{AccountProfile, AccountProfileService};

use crate::app_state::AppState;
use crate::core::errors::{Error, Result};
use crate::core::extractor::{AccountKind, Query, UserID};
use crate::core::response::ApiResponse;

/// 获取当前账号信息。
///
/// # 参数
/// * `state` - 应用状态
/// * `user_id` - 当前账号ID
/// * `account_kind` - 当前账号类型
///
/// # 返回值
/// 返回当前账号信息
///
/// # 错误
/// 当账号不存在或查询失败时返回错误。
pub async fn account_profile(
    State(state): State<AppState>,
    Extension(UserID(user_id)): Extension<UserID>,
    Extension(account_kind): Extension<AccountKind>,
) -> Result<AccountProfile> {
    let profile =
        AccountProfileService::new(state.db(), state.rbac()).account_profile(&user_id, account_kind).await?;

    Ok(ApiResponse::ok_with_data(profile))
}

/// 本人审批负责人查询参数。
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaderQuery {
    kind: OrgUnitKind,
}

/// 预览当前内部账号的团队或部门审批负责人。
/// # 参数
/// 应用状态、认证账号和组织类型。
/// # 返回
/// 负责人及组织可读名称。
/// # 错误
/// 非内部账号、组织或负责人缺失、资格失效及读取错误沿用错误信封。
pub async fn own_approval_leader(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<LeaderQuery>,
) -> HttpResult<Response, Error> {
    let view = own_leader_preview(state.db(), actor, query.kind).await?;
    let mut response = ApiResponse::ok_with_data(view).into_response();
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(response)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn leader_preview_only_accepts_own_supported_kind() {
        assert!(serde_json::from_value::<LeaderQuery>(json!({"kind":"team"})).is_ok());
        assert!(serde_json::from_value::<LeaderQuery>(json!({"kind":"department"})).is_ok());
        assert!(serde_json::from_value::<LeaderQuery>(json!({"kind":"team", "user_id":"other"})).is_err());
        assert!(serde_json::from_value::<LeaderQuery>(json!({"kind":"company"})).is_err());
    }
}
