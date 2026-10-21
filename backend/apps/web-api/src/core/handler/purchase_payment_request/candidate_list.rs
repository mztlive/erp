//! 可申请采购单分页及跨页选择只读入口。

use application_core::{AuditActor, PageView};
use axum::Extension;
use axum::extract::State;
use erp_read_models::finance::payment_request::{PaymentCandidateQuery, PurchasePaymentRequestCandidate};

use super::reads;
use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Query;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "查询可申请采购来源",
    resource = "purchase_payment_request",
    action = "create"
)]
/// 查询通过当前来源资格与独立条件的候选页。
/// # 参数
/// 当前账号、应用状态及分页查询条件。
/// # 返回
/// 返回当前页及全部匹配候选的准确总数。
/// # 错误
/// 参数、权限或候选读取失败时拒绝。
pub async fn page(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PaymentCandidateQuery>,
) -> Result<PageView<PurchasePaymentRequestCandidate>> {
    Ok(ApiResponse::ok_with_data(reads(&state).candidate_page(&actor, query).await?))
}

#[permission_macros::permission(
    group = "采购付款申请",
    group_desc = "采购申请、财务审核和批准额度内付款",
    desc = "查询可申请采购来源",
    resource = "purchase_payment_request",
    action = "create"
)]
/// 查询跨页全选与发起前复验所需的当前候选身份。
/// # 参数
/// 当前账号、应用状态及独立筛选条件。
/// # 返回
/// 返回全部匹配的应付身份，忽略分页。
/// # 错误
/// 参数、权限或候选读取失败时拒绝。
pub async fn ids(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Query(query): Query<PaymentCandidateQuery>,
) -> Result<Vec<String>> {
    Ok(ApiResponse::ok_with_data(reads(&state).candidate_ids(&actor, query).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_reads_require_create_permission() {
        assert_eq!(page_permission_key().to_string(), "purchase_payment_request:create");
        assert_eq!(ids_permission_key().to_string(), "purchase_payment_request:create");
    }

    #[test]
    fn query_rejects_unknown_fields_and_unsupported_sort() {
        assert!(
            serde_json::from_value::<PaymentCandidateQuery>(serde_json::json!({"q": "旧混合词"})).is_err()
        );
        assert!(
            serde_json::from_value::<PaymentCandidateQuery>(serde_json::json!({"sort_by": "amount"}))
                .is_err()
        );
        let query = serde_json::from_value::<PaymentCandidateQuery>(
            serde_json::json!({"purchase_summary":"茶叶", "supplier_id":"supplier", "payer_party_id":"payer", "page":2, "page_size":50}),
        )
        .unwrap();
        assert_eq!(query.purchase_summary.as_deref(), Some("茶叶"));
        assert_eq!(query.supplier_id.as_deref(), Some("supplier"));
        assert_eq!(query.payer_party_id.as_deref(), Some("payer"));
        assert_eq!(query.page, Some(2));
    }
}
