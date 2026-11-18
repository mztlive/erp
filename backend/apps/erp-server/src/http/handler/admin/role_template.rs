//! 内建岗位角色的预览和显式生成入口。
use application_core::AuditActor;
use axum::extract::{Extension, State};
use erp_identity::{BuiltinRoleCatalog, GenerateBuiltinRolesRequest, GeneratedBuiltinRole};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询岗位模板及生成条件。
/// 模板权限、配置要求及当前角色状态。
/// 无查看资格或模板加载失败时拒绝。
pub async fn list_role_templates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<BuiltinRoleCatalog> {
    Ok(ApiResponse::ok_with_data(
        state.application().rbac().builtin_role_catalog(&actor, state.application().authorization()).await?,
    ))
}

/// 按所选岗位原子生成普通角色。
/// 各岗位的新建或保留结果。
/// 越权、参数无效、预览过期或写入失败时整批拒绝。
pub async fn generate_builtin_roles(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<GenerateBuiltinRolesRequest>,
) -> Result<Vec<GeneratedBuiltinRole>> {
    Ok(ApiResponse::ok_with_data(
        state
            .application()
            .rbac()
            .generate_builtin_roles(request, actor, state.application().authorization())
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use erp_permissions as permissions;

    // 核对岗位模板预览和生成分别复用角色查看与创建资格，避免引入更弱的管理端入口。
    #[test]
    fn templates_use_existing_role_permissions() {
        assert_eq!(permissions::admin::ROLE_TEMPLATE_LIST_ROLE_TEMPLATES.to_string(), "role:list");
        assert_eq!(permissions::admin::ROLE_TEMPLATE_GENERATE_BUILTIN_ROLES.to_string(), "role:create");
    }
}
