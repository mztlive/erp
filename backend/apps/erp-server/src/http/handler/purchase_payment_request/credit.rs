//! 已批准采购付款申请内的留抵查询与使用。
use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_finance::dto::payable::credit::UseSupplierCreditRequest;
use erp_finance::entity::payable::supplier_credit::SupplierCredit;

use super::process;
use crate::http::errors::Result;
use crate::http::extractor::Json;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 查询本付款任务可用留抵。
/// 同主体同供应商可用余额。
/// 非本任务经办人或任务失效时拒绝。
pub async fn list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<SupplierCredit>> {
    Ok(ApiResponse::ok_with_data(process(&state).supplier_credits(&id, &actor).await?))
}
/// 按批准额度抵扣留抵。
/// 抵扣记录身份。
/// 版本、责任、主体或余额不符时拒绝。
pub async fn apply(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UseSupplierCreditRequest>,
) -> Result<String> {
    Ok(ApiResponse::ok_with_data(process(&state).use_supplier_credit(&id, req, &actor).await?))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::UseSupplierCreditRequest;
    #[test]
    fn credit_body_requires_frozen_task_scope_and_positive_amount() {
        let body = json!({"work_item_id":"task", "task_version":"1", "expected_version":1, "credit_id":"credit", "payable_account_id":"account", "payable_entry_id":"entry", "amount":"175.00", "idempotency_key":"key"});
        serde_json::from_value::<UseSupplierCreditRequest>(body.clone()).unwrap().validate().unwrap();
        for (key, value) in
            [("amount", json!("0")), ("task_version", json!("bad")), ("idempotency_key", json!(""))]
        {
            let mut invalid = body.clone();
            invalid[key] = value;
            assert!(serde_json::from_value::<UseSupplierCreditRequest>(invalid).unwrap().validate().is_err());
        }
        let mut invalid = body;
        invalid["supplier_id"] = json!("other");
        assert!(serde_json::from_value::<UseSupplierCreditRequest>(invalid).is_err());
    }
}
