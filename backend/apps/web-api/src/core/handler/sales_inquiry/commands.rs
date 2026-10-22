//! 销售及采购询价命令的 HTTP 适配，分别执行对应操作权限。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::sales_inquiry::dto::{
    AcceptInquiryCommand, CreateInquiryCommand, InquiryMutationView, InquiryReasonCommand,
    InquiryTaskReasonCommand, PublishInquiryCommand, ReassignInquiryCommand, ReinquireInquiryCommand,
    SaveInquiryCommand, SaveInquiryFeedbackCommand, SubmitInquiryCommand,
};

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "创建销售询价",
    resource = "sales_inquiry",
    action = "create"
)]
/// 创建销售询价。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn sales_create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Json(request): Json<CreateInquiryCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().create(&actor, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "保存询价需求",
    resource = "sales_inquiry",
    action = "update"
)]
/// 保存询价需求。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn sales_save(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<SaveInquiryCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().save(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "提交询价需求",
    resource = "sales_inquiry",
    action = "submit"
)]
/// 提交询价需求。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn sales_submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<SubmitInquiryCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().submit(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "接受询价反馈",
    resource = "sales_inquiry",
    action = "accept"
)]
/// 接受询价反馈。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn sales_accept(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<AcceptInquiryCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().accept(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "要求重新询价",
    resource = "sales_inquiry",
    action = "reinquire"
)]
/// 要求重新询价。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn sales_reinquire(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<ReinquireInquiryCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().reinquire(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "销售询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "关闭销售询价",
    resource = "sales_inquiry",
    action = "close"
)]
/// 关闭销售询价。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn sales_close(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<InquiryReasonCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().close(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "要求补充询价需求",
    resource = "procurement_inquiry",
    action = "supplement"
)]
/// 要求补充询价需求。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn procurement_supplement(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<InquiryTaskReasonCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().request_supplement(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "改派询价责任",
    resource = "procurement_inquiry",
    action = "reassign"
)]
/// 改派询价责任。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn procurement_reassign(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<ReassignInquiryCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().reassign(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "保存询价反馈草稿",
    resource = "procurement_inquiry",
    action = "save_feedback"
)]
/// 保存询价反馈草稿。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn procurement_save_feedback(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<SaveInquiryFeedbackCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().save_feedback(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[permission_macros::permission(
    group = "采购询价",
    group_desc = "询价需求与采购反馈协作",
    desc = "发布询价完整反馈",
    resource = "procurement_inquiry",
    action = "publish"
)]
/// 发布询价完整反馈。
/// # 参数
/// `state` 为应用，`actor` 为认证账号，`request` 为带命令身份及版本的请求。
/// # 返回
/// 返回已提交状态及当前责任任务。
/// # 错误
/// 权限、当前责任、版本、状态或业务校验失败时拒绝。
pub async fn procurement_publish(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(request): Json<PublishInquiryCommand>,
) -> Result<InquiryMutationView> {
    let result = state.sales_inquiry_process().publish(&actor, &id, request).await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::extract::FromRequest;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use serde_json::{Value, json};

    use super::*;

    fn json_request(payload: Value) -> Request<Body> {
        Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn submit_and_reinquire_accept_requests_without_buyer_selection() {
        let submit = json!({"command_key": "submit", "expected_version": 1, "reason": "客户需求"});
        let Json(request) =
            Json::<SubmitInquiryCommand>::from_request(json_request(submit), &()).await.unwrap();
        assert_eq!(request.reason, "客户需求");
        let reinquire = json!({
            "command_key": "reinquire", "expected_version": 2,
            "data": {"reason": "调整交期", "line_ids": ["line"]}
        });
        let Json(request) =
            Json::<ReinquireInquiryCommand>::from_request(json_request(reinquire), &()).await.unwrap();
        assert_eq!(request.data.line_ids, vec!["line"]);
    }

    #[tokio::test]
    async fn sales_submit_and_reinquire_reject_client_supplied_responsibility() {
        for (field, value) in [
            ("assignments", json!([{"owner_user_id": "forged", "line_ids": ["line"]}])),
            ("shared_fee_owner_user_id", json!("forged")),
        ] {
            let mut submit = json!({"command_key": "submit", "expected_version": 1, "reason": "提交"});
            submit[field] = value.clone();
            assert!(Json::<SubmitInquiryCommand>::from_request(json_request(submit), &()).await.is_err());
            let mut reinquire = json!({
                "command_key": "reinquire", "expected_version": 2, "data": {"reason": "重询"}
            });
            reinquire[field] = value;
            assert!(
                Json::<ReinquireInquiryCommand>::from_request(json_request(reinquire), &()).await.is_err()
            );
        }
    }

    #[tokio::test]
    async fn save_request_accepts_deadline_and_keeps_omitted_field_out_of_receipt_payload() {
        let original = json!({
            "command_key": "save", "expected_version": 1,
            "data": {"lines": [], "note": "需求说明"}
        });
        for deadline in [None, Some(json!(0)), Some(json!(1900000000))] {
            let mut payload = original.clone();
            if let Some(value) = &deadline {
                payload["deadline_at"] = value.clone();
            }
            let request = Request::builder()
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();
            let Json(command) = Json::<SaveInquiryCommand>::from_request(request, &()).await.unwrap();
            assert_eq!(
                command.deadline_at.map(|value| value.unix_secs()),
                deadline.and_then(|value| value.as_i64())
            );
            assert_eq!(serde_json::to_value(command).unwrap(), payload);
        }
    }

    #[tokio::test]
    async fn save_request_rejects_invalid_deadline_and_unknown_fields() {
        for (key, value) in [("deadline_at", json!("tomorrow")), ("status", Value::from("confirmed"))] {
            let mut payload = json!({
                "command_key": "save", "expected_version": 1,
                "data": {"lines": [], "note": "需求说明"}
            });
            payload[key] = value;
            let request = Request::builder()
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();
            assert!(Json::<SaveInquiryCommand>::from_request(request, &()).await.is_err());
        }
    }
    fn feedback_draft_request() -> Value {
        json!({
            "command_key": "save-feedback", "expected_version": 1, "task_id": "task", "task_version": 1,
            "data": {"lines": [{"line_id": "line", "conclusion": "available", "available_quantity": "10",
                "prices": [{"price_id": "price", "unit_price_gross": null, "tax_rate": null, "valid_until": 0}]}]}
        })
    }

    #[tokio::test]
    async fn feedback_fee_empty_amount_reports_exact_path_and_zero_remains_explicit() {
        let mut payload = feedback_draft_request();
        payload["data"]["fees"] = json!([{
            "fee_id": "fee", "name": "", "amount_gross": "",
            "tax_rate": "0", "line_ids": ["line"],
            "valid_until": 1792684799, "public_note": ""
        }]);
        let response = Json::<SaveInquiryFeedbackCommand>::from_request(json_request(payload.clone()), &())
            .await
            .err()
            .expect("empty fee amount must be rejected")
            .into_response();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["code"], "REQUEST_FIELD_INVALID");
        assert!(body["fieldErrors"]["data.fees[0].amount_gross"].is_string());
        payload["data"]["fees"][0]["name"] = json!("配送费用");
        payload["data"]["fees"][0]["amount_gross"] = json!("0");
        let Json(request) =
            Json::<SaveInquiryFeedbackCommand>::from_request(json_request(payload), &()).await.unwrap();
        assert_eq!(serde_json::to_value(request.data.fees[0].amount_gross).unwrap(), "0");
        assert_eq!(request.data.lines[0].prices[0].unit_price_gross, None);
    }

    #[tokio::test]
    async fn feedback_rejects_removed_fee_fields() {
        for field in [
            "category",
            "allocation_ids",
            "included_in_goods_price",
            "confirmation",
            "requires_erp_payment",
            "valid_conditions",
        ] {
            let mut payload = feedback_draft_request();
            payload["data"]["fees"] = json!([{
                "fee_id": "fee", "name": "运费", "amount_gross": "10", "tax_rate": "0",
                "line_ids": [], "valid_until": 0, "public_note": ""
            }]);
            payload["data"]["fees"][0][field] = Value::Null;
            let rejection = Json::<SaveInquiryFeedbackCommand>::from_request(json_request(payload), &())
                .await
                .err()
                .expect("removed fee fields must be rejected");
            assert_eq!(rejection.into_response().status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[tokio::test]
    async fn feedback_draft_request_accepts_missing_values_and_preserves_zero() {
        for value in [Value::Null, json!("0"), json!("0.13")] {
            let mut payload = feedback_draft_request();
            payload["data"]["lines"][0]["prices"][0]["unit_price_gross"] = value.clone();
            payload["data"]["lines"][0]["prices"][0]["tax_rate"] = value.clone();
            let Json(request) =
                Json::<SaveInquiryFeedbackCommand>::from_request(json_request(payload), &()).await.unwrap();
            let price = &request.data.lines[0].prices[0];
            assert_eq!(serde_json::to_value(price.unit_price_gross).unwrap(), value);
            assert_eq!(serde_json::to_value(price.tax_rate).unwrap(), value);
        }
    }

    #[tokio::test]
    async fn feedback_draft_request_rejects_invalid_numbers_and_private_fields() {
        for (field, value) in [
            ("unit_price_gross", json!("abc")),
            ("unit_price_gross", json!("1.23456")),
            ("unit_price_gross", json!(1.23)),
            ("tax_rate", json!("0.1234567")),
            ("supplier_id", json!("private-supplier")),
            ("internal_quote_id", json!("private-quote")),
            ("quantity", json!("10")),
            ("delivery_due_at", json!(5000)),
            ("allocations", json!([])),
            ("min_quantity", json!("1")),
            ("max_quantity", json!("10")),
            ("allow_cumulative", json!(true)),
            ("included_cost_note", json!("含配送")),
        ] {
            let mut payload = feedback_draft_request();
            payload["data"]["lines"][0]["prices"][0][field] = value;
            assert!(
                Json::<SaveInquiryFeedbackCommand>::from_request(json_request(payload), &()).await.is_err(),
                "{field}"
            );
        }
    }
}
