use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_identity::Permission;
use erp_supplier_orders::supplier_api_orders::{
    ExecutionAddressCandidates, ExecutionAddressQuery, ExecutionQueryRequest, ExecutionSetupRequest,
    ExecutionSetupView,
};
use persistence_core::NoTransaction;

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::middleware::RbacSubject;
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;

/// 读取真实接口执行阶段和可补充资料。
/// # 参数
/// 当前账号和路径订单通过同一订单数据范围核验。
/// # 返回
/// 后台真实执行资料。
/// # 错误
/// 越权、非自动订单或未配置协议时拒绝。
pub async fn supplier_order_execution_setup(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Path(id): Path<String>,
) -> Result<ExecutionSetupView> {
    state
        .application()
        .supplier_fulfillment_process()
        .require_scoped_order(&id, &actor, "detail", &mut NoTransaction)
        .await?;
    let process = state.application().supplier_api_order_process().ok_or_else(|| {
        crate::wiring::errors::Error::NotFound("此订单没有已配置的 API 自动执行入口".into())
    })?;
    let view = mask_execution_costs(&state, &subject, process.execution_setup(&id).await?).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 保存经核验的配送补充资料，后台负责后续执行。
/// # 参数
/// 当前账号须具备订单 submit 权限与对象写范围。
/// # 返回
/// 保存后的执行资料，不能解释为已下单成功。
/// # 错误
/// 版本冲突、无效坐标或不允许编辑时拒绝。
pub async fn save_supplier_order_execution_setup(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Path(id): Path<String>,
    Json(request): Json<ExecutionSetupRequest>,
) -> Result<ExecutionSetupView> {
    state
        .application()
        .supplier_fulfillment_process()
        .require_scoped_order(&id, &actor, "submit", &mut NoTransaction)
        .await?;
    let process = state
        .application()
        .supplier_api_order_process()
        .ok_or_else(|| crate::wiring::errors::Error::BusinessLogicError("供应商协议尚未配置".into()))?;
    let view = process
        .save_execution_setup(
            &id,
            request,
            &actor,
            &state.application().supplier_fulfillment_service().access(),
        )
        .await?;
    Ok(ApiResponse::ok_with_data(mask_execution_costs(&state, &subject, view).await?))
}

/// 按原执行地址返回下一层供应商候选，前端不能指定外部编码。
/// # 参数
/// 订单路径与当前账号执行 submit 数据范围核验。
/// # 返回
/// 与当前版本绑定的候选及加密选择凭据。
/// # 错误
/// 越权、父级不符、版本变化或来源读取失败时拒绝。
pub async fn supplier_order_execution_address_candidates(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(query): Query<ExecutionAddressQuery>,
) -> Result<ExecutionAddressCandidates> {
    state
        .application()
        .supplier_fulfillment_process()
        .require_scoped_order(&id, &actor, "submit", &mut NoTransaction)
        .await?;
    let process = state
        .application()
        .supplier_api_order_process()
        .ok_or_else(|| crate::wiring::errors::Error::NotFound("供应商协议尚未配置".into()))?;
    Ok(ApiResponse::ok_with_data(process.execution_address_candidates(&id, query).await?))
}

/// 请求查询原订单及确认结果，不重新发送下单或确认。
/// # 参数
/// 原订单及预期执行版本，权限在服务端事务内重新校验。
/// # 返回
/// 原调查和跟踪已唤醒后的执行资料。
/// # 错误
/// 越权、版本变化或没有原请求证据时拒绝。
pub async fn query_supplier_order_execution_result(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Extension(subject): Extension<RbacSubject>,
    Path(id): Path<String>,
    Json(request): Json<ExecutionQueryRequest>,
) -> Result<ExecutionSetupView> {
    state
        .application()
        .supplier_fulfillment_process()
        .require_scoped_order(&id, &actor, "submit", &mut NoTransaction)
        .await?;
    let process = state
        .application()
        .supplier_api_order_process()
        .ok_or_else(|| crate::wiring::errors::Error::NotFound("供应商协议尚未配置".into()))?;
    let view = process
        .query_execution_result(
            &id,
            request,
            &actor,
            &state.application().supplier_fulfillment_service().access(),
        )
        .await?;
    Ok(ApiResponse::ok_with_data(mask_execution_costs(&state, &subject, view).await?))
}

async fn mask_execution_costs(
    state: &HttpState,
    subject: &RbacSubject,
    mut view: ExecutionSetupView,
) -> std::result::Result<ExecutionSetupView, Error> {
    let permission = Permission::parse("supplier_offering_cost:detail")?;
    if !state.application().rbac().enforce(&subject.0, &permission).await? {
        view.without_costs();
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;

    use super::*;

    #[test]
    fn address_query_accepts_only_level_and_server_parent_ticket() {
        let valid: Uri = "/?level=CITY&parent_ticket=issued-proof".parse().unwrap();
        let Query(query) = Query::<ExecutionAddressQuery>::try_from_uri(&valid).unwrap();
        assert_eq!(query.parent_ticket.as_deref(), Some("issued-proof"));
        for value in [
            "/?level=CITY&parent_code=client-invented",
            "/?level=UNKNOWN",
            "/?level=TOWN&connection_id=other",
        ] {
            let uri: Uri = value.parse().unwrap();
            assert!(Query::<ExecutionAddressQuery>::try_from_uri(&uri).is_err());
        }
    }

    #[test]
    fn original_result_query_does_not_accept_new_order_or_confirmation_parameters() {
        assert!(serde_json::from_str::<ExecutionQueryRequest>(r#"{"expected_version":1}"#).is_ok());
        assert!(
            serde_json::from_str::<ExecutionQueryRequest>(r#"{"expected_version":1,"retry":true}"#).is_err()
        );
        assert!(
            serde_json::from_str::<ExecutionQueryRequest>(
                r#"{"expected_version":1,"external_order_no":"changed"}"#
            )
            .is_err()
        );
    }
}
