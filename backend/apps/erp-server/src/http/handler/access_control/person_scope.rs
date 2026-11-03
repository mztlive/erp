//! 人员唯一业务范围接口。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_identity::dto::person_scope::{PersonScopeView, SavePersonScopeRequest};

use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::scope_configuration;

/// 读取人员配置及有效操作。
/// # 参数
/// 人员、当前身份及应用依赖。
/// # 返回
/// 配置与策略版本。
/// # 错误
/// 越权或读取失败使用统一信封。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(user): Path<String>,
) -> Result<PersonScopeView> {
    Ok(ApiResponse::ok_with_data(
        scope_configuration(state.application().db(), state.application().rbac())
            .person_scopes(&user, &actor)
            .await?,
    ))
}

/// 保存所选操作的完整附加授权；旧表达式须显式确认转换。
/// # 参数
/// 人员、完整选择、预期版本及当前身份。
/// # 返回
/// 原子保存成功。
/// # 错误
/// 越权、无资格、目标失效及冲突拒绝整次操作。
pub async fn save(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(user): Path<String>,
    Json(req): Json<SavePersonScopeRequest>,
) -> Result<()> {
    scope_configuration(state.application().db(), state.application().rbac())
        .save_person_scope(&user, req, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(()))
}
