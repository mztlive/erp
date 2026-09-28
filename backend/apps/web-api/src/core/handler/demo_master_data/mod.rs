//! 演示主数据接口。

use application_core::AuditActor;
use axum::Json;
use axum::extract::{Extension, Query, State};
use erp_processes::demo_master_data::{
    ApplyDemoMasterDataRequest, DemoChunkReport, DemoFoundationReport, DemoStatus,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "演示主数据",
    group_desc = "准备演示主数据、岗位账号和审批流程，并删除由此产生的单据",
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
    group_desc = "准备演示主数据、岗位账号和审批流程，并删除由此产生的单据",
    desc = "准备岗位账号和审批流程",
    resource = "demo_master_data",
    action = "apply"
)]
/// 补齐演示用的岗位账号和尚未发布的审批流程。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的操作人
///
/// # 返回
/// 返回新建和已存在的数量。已有账号不改密码。
pub async fn demo_master_data_foundation(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
) -> Result<DemoFoundationReport> {
    let report = state.demo_master_data_service().ensure_foundation(&actor).await?;
    Ok(ApiResponse::ok_with_data(report))
}

#[permission_macros::permission(
    group = "演示主数据",
    group_desc = "准备演示主数据、岗位账号和审批流程，并删除由此产生的单据",
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

/// 删除请求是否先清理衍生单据。一次删除的第一批为 true。
#[derive(Debug, serde::Deserialize)]
pub struct RemoveDemoMasterDataParams {
    /// 为 true 时按外键图删除衍生单据。
    #[serde(default)]
    purge: bool,
}

#[permission_macros::permission(
    group = "演示主数据",
    group_desc = "准备演示主数据、岗位账号和审批流程，并删除由此产生的单据",
    desc = "删除演示主数据",
    resource = "demo_master_data",
    action = "remove"
)]
/// 删除下一批仍在列表中的演示主数据。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的操作人
/// * `query` - `purge=true` 时先清理衍生单据
///
/// # 返回
/// 返回本批删除结果。
pub async fn demo_master_data_remove(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<RemoveDemoMasterDataParams>,
) -> Result<DemoChunkReport> {
    let report = state.demo_master_data_service().remove_chunk(&actor, query.purge).await?;
    Ok(ApiResponse::ok_with_data(report))
}
