//! 域 D08 `customer` 的 HTTP handler。
//!
//! Handler 只做协议适配：`Validate`（DTO 内联）→ Service 调用 → `ApiResponse`，
//! 直接复用 `erp_customer` 的 DTO，禁止重复定义同构类型、禁止直连数据库。

use application_core::AuditActor;
use axum::Extension;
use axum::extract::{Path, State};
use erp_customer::{
    CreateCustomerRequest, CustomerAssignmentListParams, CustomerAssignmentRequest, CustomerAssignmentView,
    CustomerDetailView, CustomerListParams, CustomerListView, CustomerProfileDetailView,
    CustomerProfileMutationView, CustomerScope, CustomerSensitiveRevealView, CustomerView, PageView,
    RevealCustomerSensitiveRequest, SaveCustomerProfileRequest, UpdateCustomerRequest,
};
use erp_master_data::delete_customer;
use erp_read_models::{CustomerCenterReadService, CustomerCenterReceivableView, CustomerCenterRelatedView};

use crate::http::errors::{Error, Result};
use crate::http::extractor::{Json, Query};
use crate::http::response::ApiResponse;
use crate::http::state::HttpState;
use crate::wiring::{contract_access, customer_access, funds_access};

/// 原子创建完整客户资料。
pub async fn customer_profile_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<SaveCustomerProfileRequest>,
) -> Result<CustomerProfileMutationView> {
    let view = state.application().customer_profile_process().create(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 原子修订完整客户资料。
pub async fn customer_profile_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<SaveCustomerProfileRequest>,
) -> Result<CustomerProfileMutationView> {
    let view = state.application().customer_profile_process().update(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询完整客户资料，并按当前字段权限裁剪从属事实与允许动作。
pub async fn customer_profile_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerProfileDetailView> {
    let view = state.application().customer_profile_read_service().detail(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询客户中心合同/销售最近摘要与跨页指标。
pub async fn customer_center_related(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerCenterRelatedView> {
    let view = CustomerCenterReadService::new(
        state.application().db(),
        state.application().authorization(),
        contract_access(state.application().db(), state.application().authorization()),
        customer_access(state.application().db(), state.application().authorization()),
        funds_access(state.application().db(), state.application().authorization()),
    )
    .related(&id, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询客户中心跨应收账户的定点金额汇总。
pub async fn customer_center_receivable(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerCenterReceivableView> {
    let view = CustomerCenterReadService::new(
        state.application().db(),
        state.application().authorization(),
        contract_access(state.application().db(), state.application().authorization()),
        customer_access(state.application().db(), state.application().authorization()),
        funds_access(state.application().db(), state.application().authorization()),
    )
    .receivable(&id, &actor)
    .await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 按幂等键查询已成功客户资料命令结果。
pub async fn customer_profile_command_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(idempotency_key): Path<String>,
) -> Result<Option<CustomerProfileMutationView>> {
    let view =
        state.application().customer_profile_process().command_result(&idempotency_key, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 按短时令牌揭示单个敏感字段，并同时校验客户范围、字段详情与揭示权限。
pub async fn customer_sensitive_reveal(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<RevealCustomerSensitiveRequest>,
) -> Result<CustomerSensitiveRevealView> {
    let view = state.application().customer_profile_process().reveal_sensitive(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询客户列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `query` - 分页与筛选参数（`keyword`/`party_id`/`status` 扁平传递）
///
/// # 返回
/// 返回带范围版本的分页视图，不附带负责销售候选。
///
/// # 关键业务约束
/// 负责销售筛选只收窄客户。行负责人姓名来自当前页归属，不由此接口产生候选。
pub async fn customer_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(params): Query<CustomerListParams>,
) -> Result<CustomerListView> {
    reject_all_authorized_on_regular_list(params.scope)?;
    let page = state.application().customer_service().customer_list(&params, &actor).await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 查询不受个人归属限制的全部有权客户。
///
/// 与常规列表共用同一响应，不附带负责销售候选。
/// `all_authorized` 只放宽客户归属收窄，不按客户归属定义销售人员全集。
pub async fn customer_all_authorized_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Query(mut params): Query<CustomerListParams>,
) -> Result<CustomerListView> {
    params.scope = CustomerScope::AllAuthorized;
    let page = state.application().customer_service().customer_list(&params, &actor).await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 创建客户（同事务建立客户角色 + 首条 OWNER 归属）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `req` - 创建请求
///
/// # 返回
/// 返回新建客户角色的响应视图。
pub async fn customer_create(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Json(req): Json<CreateCustomerRequest>,
) -> Result<CustomerView> {
    let view = state.application().customer_service().create_customer(req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 查询客户详情（客户 + 主体身份 + 当前生效 OWNER）。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 客户角色 ID
///
/// # 返回
/// 返回客户详情视图。
pub async fn customer_detail(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<CustomerDetailView> {
    let view = state.application().customer_service().customer_detail(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 更新客户角色（乐观锁）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
/// * `req` - 更新请求（含期望版本）
///
/// # 返回
/// 返回更新后客户角色的响应视图。
pub async fn customer_update(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<UpdateCustomerRequest>,
) -> Result<CustomerView> {
    let view = state.application().customer_service().update_customer(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(view))
}

/// 软删除客户角色。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
///
/// # 返回
/// 返回统一成功信封。
pub async fn customer_delete(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<()> {
    delete_customer(
        state.application().db(),
        state.application().customer_service(),
        customer_access(state.application().db(), state.application().authorization()),
        id,
        actor,
    )
    .await?;
    Ok(ApiResponse::ok())
}

/// 查询客户归属列表。
///
/// # 参数
/// * `state` - 应用状态
/// * `id` - 客户角色 ID
/// * `query` - 分页与筛选参数
///
/// # 返回
/// 返回契约形状的分页视图。
pub async fn customer_assignment_list(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Query(params): Query<CustomerAssignmentListParams>,
) -> Result<PageView<CustomerAssignmentView>> {
    let page = state
        .application()
        .customer_assignment_service()
        .customer_assignment_list(&actor, &id, &params)
        .await?;
    Ok(ApiResponse::ok_with_data(page))
}

/// 调整客户归属（Assign 建立新归属并结束重叠旧归属；End 提前结束有效期）。
///
/// # 参数
/// * `state` - 应用状态
/// * `actor` - 已通过鉴权的审计操作人
/// * `id` - 客户角色 ID
/// * `req` - 归属变更请求
///
/// # 返回
/// 返回本次变更涉及的归属行。
pub async fn customer_assignment_apply(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
    Json(req): Json<CustomerAssignmentRequest>,
) -> Result<Vec<CustomerAssignmentView>> {
    let views = state.application().customer_assignment_service().apply_assignment(&id, req, &actor).await?;
    Ok(ApiResponse::ok_with_data(views))
}

/// 常规列表拒绝全量授权口径，避免只凭 `customer:list` 绕过专用入口。
///
/// # 参数
/// * `scope` - 请求中的目录范围标签
///
/// # 返回
/// 非 `all_authorized` 时成功。
///
/// # 错误
/// 常规入口携带全量授权时返回校验错误。
///
/// # 关键业务约束
/// 全部有权客户必须走 `customer_scope:detail` 专用入口。
fn reject_all_authorized_on_regular_list(scope: CustomerScope) -> std::result::Result<(), Error> {
    if scope == CustomerScope::AllAuthorized {
        return Err(Error::BadRequest("全部有权客户请使用专用查询入口".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::extract::FromRequest;
    use axum::http::{Request, StatusCode, Uri};
    use erp_customer::{CustomerProfileOperation, Error as CustomerError};
    use serde_json::json;

    use super::*;
    use crate::http::extractor::Query;

    #[tokio::test]
    async fn customer_profile_http_requires_explicit_boolean_reuse_consent() {
        let body = json!({
            "idempotency_key": "customer-http-reuse", "legal_name": "已有供应商",
            "effective_from": "2026-10-21", "change_reason": "建立客户角色"
        });
        for consent in [None, Some(json!(false)), Some(json!(true)), Some(json!("true"))] {
            let mut payload = body.clone();
            if let Some(value) = &consent {
                payload["reuse_existing_party"] = value.clone();
            }
            let request = Request::builder()
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap();
            let decoded = Json::<SaveCustomerProfileRequest>::from_request(request, &()).await;
            if consent == Some(json!("true")) {
                assert_eq!(decoded.unwrap_err().status(), StatusCode::UNPROCESSABLE_ENTITY);
                continue;
            }
            let Json(mut request) = decoded.unwrap();
            assert_eq!(request.reuse_existing_party, consent == Some(json!(true)));
            assert!(request.validate_structure(CustomerProfileOperation::Create).is_ok());
            if request.reuse_existing_party {
                request.expected_customer_version = Some(1);
                request.expected_party_version = Some(1);
                assert!(matches!(
                    request.validate_structure(CustomerProfileOperation::Update),
                    Err(CustomerError::ValidationError(_))
                ));
            }
        }
    }

    /// 客户 Query 解码必须消费范围版本与组织筛选，并拒绝旧姓名参数。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 未知参数不得被忽略后返回全部客户。
    #[test]
    fn customer_scope_version_and_org_filters_decode_from_url() {
        let uri: Uri = "/?page=2&page_size=25&scope_version=v1&owner_user_ids=a,b&org_unit_ids=org-1&include_descendants=true"
            .parse()
            .unwrap();
        let Query(params) = Query::<CustomerListParams>::try_from_uri(&uri).unwrap();
        assert_eq!(params.page, Some(2));
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert!(params.owner_user_ids.is_some());
        assert_eq!(params.org_unit_ids.unwrap().as_slice(), &["org-1".to_string()]);
        assert_eq!(params.include_descendants, Some(true));
        let legacy: Uri = "/?owner=张三".parse().unwrap();
        assert!(Query::<CustomerListParams>::try_from_uri(&legacy).is_err());
    }

    /// 常规列表不得接受 all_authorized，专用入口才覆盖该口径。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 组织范围内账号不能靠常规列表看到全部有权客户。
    #[test]
    fn regular_list_rejects_all_authorized_scope() {
        assert!(reject_all_authorized_on_regular_list(CustomerScope::Mine).is_ok());
        assert!(reject_all_authorized_on_regular_list(CustomerScope::Collaborating).is_ok());
        assert!(reject_all_authorized_on_regular_list(CustomerScope::Assigned).is_ok());
        assert!(reject_all_authorized_on_regular_list(CustomerScope::AllAuthorized).is_err());
    }
}
