//! 岗位所选操作范围的原子替换入口。

use application_core::AuditActor;
use axum::extract::State;
use axum::{Extension, Json};
use erp_identity::{DataScopeView, ReplaceDataScopeRequest};
use erp_processes::adapters::scope_configuration;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "权限与审计",
    group_desc = "权限目录、数据范围、用户授权与审计查询",
    desc = "创建数据范围",
    resource = "data_scope",
    action = "create"
)]
/// 替换岗位所选操作的范围，服务层同时校验删除配置资格。
///
/// # 参数
/// * `state` - 应用状态。
/// * `actor` - 已认证操作人。
/// * `req` - 替换范围及期望策略版本。
/// # 返回
/// 返回保存后的范围。
/// # 错误
/// 权限不足、版本冲突及写入失败返回统一错误信封。
pub async fn replace(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ReplaceDataScopeRequest>,
) -> Result<DataScopeView> {
    let view = scope_configuration(state.db(), state.rbac()).replace_data_scope(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn replacement_requires_snapshot_and_rejects_unknown_fields() {
        let value = json!({
            "scope": {
                "schema_version": 2, "resource": "sales_order", "actions": ["list"],
                "target_dimension": "internal_org", "enabled": true,
                "subject_type": "role", "subject_id": "sales", "scope_type": "self_owned",
                "scope_targets": []
            },
            "expected_policy_version": 1
        });
        assert!(serde_json::from_value::<ReplaceDataScopeRequest>(value.clone()).is_ok());
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("expected_policy_version");
        assert!(serde_json::from_value::<ReplaceDataScopeRequest>(missing).is_err());
        let mut unknown = value;
        unknown["force"] = true.into();
        assert!(serde_json::from_value::<ReplaceDataScopeRequest>(unknown).is_err());
    }
}
