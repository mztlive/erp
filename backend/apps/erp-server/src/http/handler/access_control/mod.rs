//! 域 D06 `access_control` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_identity` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

pub mod inspection;
pub mod person_directory;
pub mod person_scope;
pub mod policy_bundle;
pub mod resource_grants;

use application_core::AuditActor;
use axum::Extension;
use axum::extract::State;
use erp_identity::{AuditEventListParams, AuditEventView, PageView};

use crate::http::errors::Result;
use crate::http::extractor::Query;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::scope_configuration;

/// 查询审计事件列表（事实型留痕，不可编辑不可删除）。
/// `query` 为分页与筛选参数（`actor_id`/`action_type`/`object_type`/`result`）。
pub async fn audit_event_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<AuditEventListParams>,
) -> Result<PageView<AuditEventView>> {
    let page = scope_configuration(
        state.application().db(),
        state.application().rbac(),
        state.application().authorization(),
    )
    .audit_event_list(&params, &actor)
    .await?;

    Ok(ApiResponse::ok_with_data(page))
}
