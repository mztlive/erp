//! 演示主数据接口。

use application_core::AuditActor;
use axum::Json;
use axum::extract::{Extension, State};
use erp_processes::demo_master_data::{
    ApplyDemoMasterDataRequest, DemoChunkReport, DemoFoundationReport, DemoResetReport, DemoStatus,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "演示主数据",
    group_desc = "生成演示资料；清空数据库时仅保留 admin",
    desc = "查看演示主数据",
    resource = "demo_master_data",
    action = "read"
)]
/// 查看演示主数据是否开放，以及各类还剩多少。
///
/// # 参数
/// * `state` - 应用状态
///
/// # 返回
/// 返回计划数量和当前数量。
pub async fn demo_master_data_status(State(state): State<AppState>) -> Result<DemoStatus> {
    let status = state.demo_master_data_service().status().await?;
    Ok(ApiResponse::ok_with_data(status))
}

#[permission_macros::permission(
    group = "演示主数据",
    group_desc = "生成演示资料；清空数据库时仅保留 admin",
    desc = "准备岗位账号、审批流程和默认责任规则",
    resource = "demo_master_data",
    action = "apply"
)]
/// 补齐演示用的岗位账号、尚未发布的审批流程，以及缺失的默认责任规则。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的操作人
///
/// # 返回
/// 返回新建和已存在的数量。已有账号不改密码，但姓名会同步为演示人名。已启用的默认责任规则保留现有负责人。
///
/// # 错误
/// 演示功能未开放、授权不足或基础数据准备失败时返回错误。
pub async fn demo_master_data_foundation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<DemoFoundationReport> {
    let report = state.demo_master_data_service().ensure_foundation(&actor).await?;
    Ok(ApiResponse::ok_with_data(report))
}

#[permission_macros::permission(
    group = "演示主数据",
    group_desc = "生成演示资料；清空数据库时仅保留 admin",
    desc = "生成演示主数据",
    resource = "demo_master_data",
    action = "apply"
)]
/// 生成或恢复下一批演示主数据。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的操作人
/// * `req` - 清单继续位置
///
/// # 返回
/// 返回本批写入结果。
pub async fn demo_master_data_apply(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<ApplyDemoMasterDataRequest>,
) -> Result<DemoChunkReport> {
    let report = state.demo_master_data_service().apply_chunk(&actor, req.cursor).await?;
    Ok(ApiResponse::ok_with_data(report))
}

#[permission_macros::permission(
    group = "演示主数据",
    group_desc = "生成演示资料；清空数据库时仅保留 admin",
    desc = "清空演示数据库（仅保留 admin）",
    resource = "demo_master_data",
    action = "remove"
)]
/// 一次清空数据库，只保留 admin 账号和必要超管授权。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的操作人
///
/// # 返回
/// 返回事务删除的文档总数。
///
/// # 错误
/// 非 admin 超管、环境未开放或事务失败时拒绝清空。
pub async fn demo_master_data_remove(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<DemoResetReport> {
    let report = state.demo_master_data_service().reset_database(&actor).await?;
    tracing::warn!(
        account = actor.account(),
        actor_id = actor.id(),
        deleted_documents = report.deleted_documents,
        "demo database reset committed; only admin authorization retained"
    );
    Ok(ApiResponse::ok_with_data(report))
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use serde_json::{Value, json};

    use super::*;

    #[tokio::test]
    async fn reset_response_is_a_single_completed_database_operation() {
        for deleted_documents in [0, 1024] {
            let response = ApiResponse::ok_with_data(DemoResetReport { deleted_documents }).into_response();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["success"], true);
            assert_eq!(value["data"], json!({"deleted_documents": deleted_documents}));
        }
    }
}
