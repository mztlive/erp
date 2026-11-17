//! 域 D33 `supplier_settlement` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-settlement-statements`、
//! `/admin/supplier-settlement-items`、`/admin/supplier-settlement-differences`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::supplier_settlement;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回声明公共权限目录信息的路由集合。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(statement_routes())
        .merge(statement_command_routes())
        .merge(evidence_routes())
        .merge(review_routes())
        .merge(handover_routes())
        .merge(difference_routes())
}

/// 供应商结算单读取、创建及审核候选路由。
fn statement_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-settlement-statements/{id}/reviewer-options",
            permission_route(
                get(supplier_settlement::reviewer_options::supplier_settlement_reviewer_options),
                &permissions::supplier_settlement::STATEMENT_SUBMIT_REVIEW,
            ),
        )
        .route(
            "/supplier-settlement-statements",
            permission_route(
                get(supplier_settlement::supplier_settlement_statement_list),
                &permissions::supplier_settlement::STATEMENT_LIST,
            ),
        )
        .route(
            "/supplier-settlement-statements",
            permission_route(
                post(supplier_settlement::supplier_settlement_statement_create),
                &permissions::supplier_settlement::STATEMENT_CREATE,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}",
            permission_route(
                get(supplier_settlement::supplier_settlement_statement_detail),
                &permissions::supplier_settlement::STATEMENT_DETAIL,
            ),
        )
}

/// 供应商结算提交与刷新路由。
fn statement_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-settlement-statements/{id}/review-submissions",
            permission_route(
                post(supplier_settlement::supplier_settlement_statement_submit_review),
                &permissions::supplier_settlement::STATEMENT_SUBMIT_REVIEW,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/refreshes",
            permission_route(
                post(supplier_settlement::supplier_settlement_statement_refresh),
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
}

/// 供应商结算来源凭据路由。
fn evidence_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-settlement-source-evidence",
            permission_route(
                get(supplier_settlement::supplier_settlement_source_evidence_latest),
                &permissions::supplier_settlement::SOURCE_EVIDENCE_LATEST,
            ),
        )
        .route(
            "/supplier-settlement-source-evidence",
            permission_route(
                post(supplier_settlement::supplier_settlement_source_evidence_record),
                &permissions::supplier_settlement::SOURCE_EVIDENCE_RECORD,
            ),
        )
}

/// 供应商结算审核与作废路由。
fn review_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-settlement-statements/{id}/review-decisions",
            permission_route(
                post(supplier_settlement::supplier_settlement_statement_review_decide),
                &permissions::supplier_settlement::STATEMENT_REVIEW_DECIDE,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/void",
            permission_route(
                post(supplier_settlement::supplier_settlement_statement_void),
                &permissions::supplier_settlement::STATEMENT_VOID,
            ),
        )
}

/// 供应商结算责任交接路由。
fn handover_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-settlement-statements/{id}/handover",
            permission_route(
                post(supplier_settlement::supplier_settlement_statement_handover),
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/handover-candidates",
            permission_route(
                get(supplier_settlement::supplier_settlement_statement_handover_candidates),
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
        .route(
            "/supplier-settlement-statements/{id}/difference-handler",
            permission_route(
                post(supplier_settlement::supplier_settlement_statement_reassign_difference_handler),
                &permissions::supplier_settlement::STATEMENT_REFRESH,
            ),
        )
}

/// 供应商结算项与差异处理路由。
fn difference_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-settlement-items",
            permission_route(
                get(supplier_settlement::supplier_settlement_item_list),
                &permissions::supplier_settlement::ITEM_LIST,
            ),
        )
        .route(
            "/supplier-settlement-differences",
            permission_route(
                get(supplier_settlement::supplier_settlement_difference_list),
                &permissions::supplier_settlement::DIFFERENCE_LIST,
            ),
        )
        .route(
            "/supplier-settlement-differences/{id}/decisions",
            permission_route(
                post(supplier_settlement::supplier_settlement_difference_decide),
                &permissions::supplier_settlement::DIFFERENCE_DECIDE,
            ),
        )
        .route(
            "/supplier-settlement-differences/{id}/evidence",
            permission_route(
                post(supplier_settlement::supplier_settlement_difference_evidence_append),
                &permissions::supplier_settlement::DIFFERENCE_DECIDE,
            ),
        )
}
