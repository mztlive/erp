use axum::{extract::State, Extension};
use services::iam::{AccountProfile, AccountProfileService};

use crate::{
    app_state::AppState,
    core::{
        errors::Result,
        extractor::{AccountKind, UserID},
        response::ApiResponse,
    },
};

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
    let profile = AccountProfileService::new(state.db(), state.rbac())
        .account_profile(&user_id, account_kind)
        .await?;

    Ok(ApiResponse::ok_with_data(profile))
}
