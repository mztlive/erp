//! API 供应商批量入库作业的协议适配。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_processes::supplier_catalog_adoption::{
    BatchSubmissionView, BatchVersionRequest, BatchView, CreateBatchRequest, SaveBatchRequest,
    SubmitBatchRequest,
};
use erp_supply::catalog_adoption::CatalogAdoptionBatch;

use crate::app_state::AppState;
use crate::core::errors::Result;
use crate::core::extractor::Json;
use crate::core::response::ApiResponse;

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "创建供应商批量入库作业",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 冻结明确勾选或全部筛选结果，并创建可恢复作业。
/// # 参数
/// 路径指定连接，载荷指定选择范围，操作人来自认证上下文。
/// # 返回
/// 服务端保存的固定成员及批次状态。
/// # 错误
/// 权限、供应商范围、连接或选择范围不合法时拒绝。
pub async fn create(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(connection): Path<String>,
    Json(request): Json<CreateBatchRequest>,
) -> Result<BatchView> {
    let batch = state.supplier_catalog_adoption_process()?.create_batch(&connection, request, &actor).await?;
    Ok(ApiResponse::ok_with_data(batch))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询可恢复的供应商批量入库作业",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 读取当前操作人最近保存的批次，重新校验连接所属供应商范围。
/// # 参数
/// 路径指定连接，操作人来自认证上下文。
/// # 返回
/// 可重新打开的最近批次列表。
/// # 错误
/// 权限已变更、连接不存在或超出供应商范围时拒绝。
pub async fn list(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(connection): Path<String>,
) -> Result<Vec<CatalogAdoptionBatch>> {
    let batches = state.supplier_catalog_adoption_process()?.batches(&connection, &actor).await?;
    Ok(ApiResponse::ok_with_data(batches))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查询供应商批量入库作业",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 根据当前权限和数据范围读取草稿或执行结果。
/// # 参数
/// 路径指定连接与批次，操作人来自认证上下文。
/// # 返回
/// 已保存输入、逐项结果及互斥汇总。
/// # 错误
/// 批次不存在、权限已变更或超出数据范围时拒绝。
pub async fn detail(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((connection, batch)): Path<(String, String)>,
) -> Result<BatchView> {
    let view = state.supplier_catalog_adoption_process()?.batch(&connection, &batch, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "保存供应商批量入库草稿",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 保存规则、补值、单项例外和排除项，使受影响预览失效。
/// # 参数
/// 路径指定连接与批次，载荷包含保存版本及修改项。
/// # 返回
/// 已持久化草稿和新版本。
/// # 错误
/// 版本冲突、已冻结执行范围或非法字段时拒绝。
pub async fn save(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((connection, batch)): Path<(String, String)>,
    Json(request): Json<SaveBatchRequest>,
) -> Result<BatchView> {
    let view =
        state.supplier_catalog_adoption_process()?.save_batch(&connection, &batch, request, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "核对供应商批量入库结果",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 由服务器重新匹配、计算并保存确认预览。
/// # 参数
/// 路径指定连接与批次，载荷指定当前草稿版本。
/// # 返回
/// 逐项匹配、最终价格、待补原因和就绪范围。
/// # 错误
/// 无权限、版本冲突或连接不可用时拒绝。
pub async fn preview(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((connection, batch)): Path<(String, String)>,
    Json(request): Json<BatchVersionRequest>,
) -> Result<BatchView> {
    let view = state
        .supplier_catalog_adoption_process()?
        .preview_batch(&connection, &batch, request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "确认供应商批量入库作业",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 冻结明确确认的就绪行，交由统一后台执行器持续处理。
/// # 参数
/// 路径指定连接与批次，载荷指定预览版本及本次确认范围。
/// # 返回
/// 已保存确认记录的批次；逐项执行结果通过查询恢复。
/// # 错误
/// 空提交、异常行、过期预览或未确认价格提示时拒绝。
pub async fn submit(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((connection, batch)): Path<(String, String)>,
    Json(request): Json<SubmitBatchRequest>,
) -> Result<BatchView> {
    let view =
        state.supplier_catalog_adoption_process()?.submit_batch(&connection, &batch, request, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "核实供应商批量入库原操作结果",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 查询原操作回执，未知结果不得作为明确失败直接重放。
/// # 参数
/// 路径指定连接与批次，操作人来自认证上下文。
/// # 返回
/// 核实后的原操作结果及仍需核实项。
/// # 错误
/// 无权限、越数据范围或原回执不一致时拒绝。
pub async fn reconcile(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((connection, batch)): Path<(String, String)>,
) -> Result<BatchView> {
    let view =
        state.supplier_catalog_adoption_process()?.reconcile_batch(&connection, &batch, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

#[permission_macros::permission(
    group = "API 供应商连接",
    group_desc = "供应商 API 连接与能力治理（W20）",
    desc = "查证供应商批量入库原确认",
    resource = "supplier_api_connection",
    action = "catalog_sync"
)]
/// 查证原确认；未受理时封存原操作，防止迟到请求执行。
/// # 参数
/// 路径指定连接与批次，载荷为浏览器保存的原确认。
/// # 返回
/// 确定的受理结果及当前批次，不触发商品入库。
/// # 错误
/// 越权、原确认不符、数据库冲突或结果未知时拒绝解除待核实。
pub async fn submission_result(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path((connection, batch)): Path<(String, String)>,
    Json(request): Json<SubmitBatchRequest>,
) -> Result<BatchSubmissionView> {
    let result = state
        .supplier_catalog_adoption_process()?
        .query_batch_submission(&connection, &batch, request, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(result))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::Request;
    use serde_json::{Value, json};

    use super::*;

    fn request(value: Value) -> Request<Body> {
        Request::builder()
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn batch_pricing_requires_decimal_strings_and_rejects_forced_targets() {
        let body = json!({"expected_version":1,"rules":{
            "mode":"SEPARATE", "dropship":{"method":"PERCENTAGE","value":"15"},
            "bulk":{"method":"FIXED","value":"110.00"},"minimum_quantity":"10"
        }});
        assert!(Json::<SaveBatchRequest>::from_request(request(body.clone()), &()).await.is_ok());
        let mut numeric = body.clone();
        numeric["rules"]["dropship"]["value"] = json!(15);
        assert!(Json::<SaveBatchRequest>::from_request(request(numeric), &()).await.is_err());
        let mut forced = body;
        forced["items"] = json!([{"item_id":"source","sku_id":"forced"}]);
        assert!(Json::<SaveBatchRequest>::from_request(request(forced), &()).await.is_err());
    }

    #[tokio::test]
    async fn item_adjustment_distinguishes_clear_and_keep_without_accepting_value_origins() {
        let body = json!({"expected_version":2,"items":[{
            "item_id":"source", "sales_edit":{
                "dropship":{"action":"SET","value":"118.00"},
                "bulk":{"action":"KEEP"},"market_price":{"action":"CLEAR"}
            },"delivery_template_id":{"action":"CLEAR"}
        }]});
        assert!(Json::<SaveBatchRequest>::from_request(request(body.clone()), &()).await.is_ok());
        let mut forged = body;
        forged["items"][0]["sales_edit"]["dropship"]["origin"] = json!("SOURCE");
        assert!(Json::<SaveBatchRequest>::from_request(request(forged), &()).await.is_err());
    }

    #[tokio::test]
    async fn confirmation_accepts_only_identity_and_explicit_reviewed_scope() {
        let body = json!({"expected_version":3,"preview_version":3,
            "item_ids":["first","second"],"idempotency_key":"confirmed-operation"});
        let Json(decoded) =
            Json::<SubmitBatchRequest>::from_request(request(body.clone()), &()).await.unwrap();
        assert_eq!(decoded.item_ids, ["first", "second"]);
        let mut stale_price = body;
        stale_price["sales_prices"] = json!({"sales_visible_price_gross":"0.00"});
        assert!(Json::<SubmitBatchRequest>::from_request(request(stale_price), &()).await.is_err());
    }
}
