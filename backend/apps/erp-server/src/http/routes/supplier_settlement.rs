//! 域 D33 `supplier_settlement` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-settlement-statements`、
//! `/admin/supplier-settlement-items`、`/admin/supplier-settlement-differences`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::supplier_settlement;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-settlement-statements/{id}/reviewer-options",
            with_permission(
                get(supplier_settlement::reviewer_options::supplier_settlement_reviewer_options),
                rbac,
                &permissions::supplier_settlement::STATEMENT_SUBMIT_REVIEW,
            ),
        )
        .route(
            "/supplier-settlement-statements",
            with_permission(
                get(supplier_settlement::supplier_settlement_statement_list),
                rbac,
                &permissions::supplier_settlement::STATEMENT_LIST,
            ),
        )
        .route(
            "/supplier-settlement-statements",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_create),
                rbac,
                &permissions::supplier_settlement::STATEMENT_CREATE,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}",
            with_permission(
                get(supplier_settlement::supplier_settlement_statement_detail),
                rbac,
                &permissions::supplier_settlement::STATEMENT_DETAIL,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/review-submissions",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_submit_review),
                rbac,
                &permissions::supplier_settlement::STATEMENT_SUBMIT_REVIEW,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/refreshes",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_refresh),
                rbac,
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
        .route(
            "/supplier-settlement-source-evidence",
            with_permission(
                get(supplier_settlement::supplier_settlement_source_evidence_latest),
                rbac,
                &permissions::supplier_settlement::SOURCE_EVIDENCE_LATEST,
            ),
        )
        .route(
            "/supplier-settlement-source-evidence",
            with_permission(
                post(supplier_settlement::supplier_settlement_source_evidence_record),
                rbac,
                &permissions::supplier_settlement::SOURCE_EVIDENCE_RECORD,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/review-decisions",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_review_decide),
                rbac,
                &permissions::supplier_settlement::STATEMENT_REVIEW_DECIDE,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/void",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_void),
                rbac,
                &permissions::supplier_settlement::STATEMENT_VOID,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/handover",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_handover),
                rbac,
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/handover-candidates",
            with_permission(
                get(supplier_settlement::supplier_settlement_statement_handover_candidates),
                rbac,
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/difference-handler",
            with_permission(
                post(supplier_settlement::supplier_settlement_statement_reassign_difference_handler),
                rbac,
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
        .route(
            "/supplier-settlement-items",
            with_permission(
                get(supplier_settlement::supplier_settlement_item_list),
                rbac,
                &permissions::supplier_settlement::ITEM_LIST,
            ),
        )
        .route(
            "/supplier-settlement-differences",
            with_permission(
                get(supplier_settlement::supplier_settlement_difference_list),
                rbac,
                &permissions::supplier_settlement::DIFFERENCE_LIST,
            ),
        )
        .route(
            "/supplier-settlement-differences/{id}/decisions",
            with_permission(
                post(supplier_settlement::supplier_settlement_difference_decide),
                rbac,
                &permissions::supplier_settlement::DIFFERENCE_DECIDE,
            ),
        )
        .route(
            "/supplier-settlement-differences/{id}/evidence",
            with_permission(
                post(supplier_settlement::supplier_settlement_difference_evidence_append),
                rbac,
                &permissions::supplier_settlement::DIFFERENCE_DECIDE,
            ),
        )
}
